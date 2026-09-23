//! The game itself: loading a level, playing rounds in it, and the player's input.

use crate::{
    diagnostics::{self, FrameStats},
    generate::Maze,
    hud::{self, Hud, Status},
    inhabitants::Inhabitants,
    layout::Rng,
    level::Level,
    menu::{Choice, PauseMenu},
    player::{Player, DROID_MODEL},
    survey,
    tiles::{self, Measured, Prefabs},
};
use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
        reflect::prelude::*,
        visitor::prelude::*,
    },
    engine::GraphicsContext,
    event::{ElementState, Event, WindowEvent},
    graph::SceneGraph,
    gui::{message::UiMessage, UserInterface},
    keyboard::{KeyCode, PhysicalKey},
    material::{Material, MaterialResource},
    plugin::{error::GameResult, Plugin, PluginContext},
    resource::model::{Model, ModelResource},
    scene::{
        base::BaseBuilder,
        collider::{ColliderBuilder, ColliderShape},
        graph::Graph,
        light::{directional::DirectionalLightBuilder, point::PointLightBuilder, BaseLightBuilder},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            MeshBuilder,
        },
        node::Node,
        rigidbody::{RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        EnvironmentLightingSource, Scene,
    },
    window::CursorGrabMode,
};

/// How many junctions wide and deep a maze is, unless MAZE_SIZE says otherwise.
const MAZE_SIZE: (usize, usize) = (20, 20);
/// The chance of a dead end being opened into a neighbouring corridor, which makes loops.
const LOOP_CHANCE: f32 = 0.15;
/// How close to the exit counts as reaching it.
const EXIT_RADIUS: f32 = 1.5;
/// The ambient light. Low: the lamps do the lighting, and a flat ambient term lights corners as
/// much as open floor, which is what makes a room look like untextured geometry.
const AMBIENT: Color = Color::opaque(24, 26, 34);
/// The ambient light with the lights off: next to none, so that what the flashlight is not
/// pointed at is as good as black.
const DARK_AMBIENT: Color = Color::opaque(3, 3, 5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// Waiting for the maze model.
    #[default]
    Loading,
    /// The model is in the scene; its colliders exist from the next physics step on.
    Settling(u8),
    Playing,
    Won,
    /// The maze could not be used; the reason is on screen.
    Broken,
}

#[derive(Default, Debug, PartialEq, Visit, Reflect)]
#[reflect(non_cloneable, type_uuid = "0d3b1c55-7e0a-4f1e-9b53-2f6a8c1d4e90")]
pub struct MazeGame {
    /// A fixed maze model to play instead of random mazes, from MAZE_MODEL.
    #[visit(skip)]
    #[reflect(hidden)]
    model: Option<ModelResource>,
    #[visit(skip)]
    #[reflect(hidden)]
    prefabs: Option<Prefabs>,
    /// The tiles, once measured.
    #[visit(skip)]
    #[reflect(hidden)]
    measured: Option<Measured>,
    /// The level being played.
    #[visit(skip)]
    #[reflect(hidden)]
    level: Level,
    /// Surface data already made double-sided. Tiles share theirs between all their copies, and
    /// in every level, so each must be done only once.
    #[visit(skip)]
    #[reflect(hidden)]
    doubled: fyrox::fxhash::FxHashSet<u64>,
    scene: Handle<Scene>,
    #[visit(skip)]
    #[reflect(hidden)]
    player: Player,
    /// What moves in the scene, for the graphics effects: the droid.
    #[visit(skip)]
    #[reflect(hidden)]
    moving: fyrox_gfx::MovingThings,
    /// The droid the player is seen as, until it has loaded and joined the player.
    #[visit(skip)]
    #[reflect(hidden)]
    droid: Option<ModelResource>,
    /// The same droid, loaded, for the maze's inhabitants.
    #[visit(skip)]
    #[reflect(hidden)]
    droid_model: Option<ModelResource>,
    /// Droids going about the maze by themselves.
    #[visit(skip)]
    #[reflect(hidden)]
    inhabitants: Inhabitants,
    exit: Handle<Node>,
    sun: Handle<Node>,
    /// Whether the player has switched the maze's lights off, leaving the flashlight to see by.
    /// It stays that way from one maze to the next.
    lights_off: bool,
    #[visit(skip)]
    #[reflect(hidden)]
    rng: Option<Rng>,
    #[visit(skip)]
    #[reflect(hidden)]
    phase: Phase,
    round_time: f32,
    best_time: Option<f32>,
    #[visit(skip)]
    #[reflect(hidden)]
    hud: Hud,
    /// The pause menu. While it is open the world stands still.
    #[visit(skip)]
    #[reflect(hidden)]
    menu: PauseMenu,
    #[visit(skip)]
    #[reflect(hidden)]
    stats: FrameStats,
    mouse_captured: bool,
    /// Whether the mouse should be captured. Capturing can fail while the window is still
    /// appearing, so it is retried until it works.
    want_mouse: bool,
    focused: bool,
}

impl MazeGame {
    /// The game, telling the graphics effects what moves through `moving`.
    pub fn new(moving: fyrox_gfx::MovingThings) -> Self {
        Self {
            moving,
            ..Default::default()
        }
    }

    fn build_scene(&mut self, ctx: &mut PluginContext) {
        let mut scene = Scene::new();
        scene.rendering_options.ambient_lighting_color = AMBIENT;
        scene.rendering_options.environment_lighting_source =
            EnvironmentLightingSource::AmbientColor;

        // A low sun, so the walls throw long shadows into the corridors.
        self.sun = DirectionalLightBuilder::new(BaseLightBuilder::new(
            BaseBuilder::new().with_local_transform(
                TransformBuilder::new()
                    .with_local_rotation(
                        UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 35f32.to_radians())
                            * UnitQuaternion::from_axis_angle(
                                &Vector3::x_axis(),
                                50f32.to_radians(),
                            ),
                    )
                    .build(),
            ),
        ))
        .build(&mut scene.graph)
        .to_base();

        // The exit: a glowing ball with a light of its own, visible over the walls.
        let mut material = Material::standard();
        material.set_property("diffuseColor", Color::opaque(80, 255, 120));
        self.exit = PointLightBuilder::new(
            BaseLightBuilder::new(
                BaseBuilder::new().with_child(
                    // The ball must not shadow its own light.
                    MeshBuilder::new(BaseBuilder::new().with_cast_shadows(false))
                        .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(
                            SurfaceData::make_sphere(16, 16, 0.4, &Default::default()),
                        ))
                        .with_material(MaterialResource::new_embedded(material))
                        .build()])
                        .build(&mut scene.graph),
                ),
            )
            .with_color(Color::opaque(80, 255, 120))
            .with_scatter_enabled(true),
        )
        .with_radius(8.0)
        .build(&mut scene.graph)
        .to_base();

        // A floor under everything, in case a level leaves gaps at ground level. It sits a little
        // below the level's own floor: level with it, rays dropped onto the floor would hit this
        // one as often as the level's, and the survey would find no floor that is the maze's.
        let floor = ColliderBuilder::new(
            BaseBuilder::new().with_local_transform(
                TransformBuilder::new()
                    .with_local_position(Vector3::new(0.0, -0.55, 0.0))
                    .build(),
            ),
        )
        .with_shape(ColliderShape::cuboid(500.0, 0.5, 500.0))
        .build(&mut scene.graph);
        RigidBodyBuilder::new(BaseBuilder::new().with_child(floor))
            .with_body_type(RigidBodyType::Static)
            .build(&mut scene.graph);

        self.player = Player::spawn(&mut scene.graph);
        self.scene = ctx.scenes.add(scene);
        let resources = &ctx.resource_manager;
        self.droid = Some(resources.request::<Model>(DROID_MODEL));
        match std::env::var("MAZE_MODEL") {
            Ok(path) => self.model = Some(resources.request::<Model>(path)),
            Err(_) => self.prefabs = Some(Prefabs::request(resources)),
        }
    }

    fn rng(&mut self) -> &mut Rng {
        self.rng.get_or_insert_with(|| {
            // MAZE_SEED makes every maze and round the same, which is what comparing two runs
            // needs.
            if let Some(seed) = std::env::var("MAZE_SEED").ok().and_then(|s| s.parse().ok()) {
                return Rng::new(seed);
            }
            Rng::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(1),
            )
        })
    }

    /// Puts the level into the scene, once its models have loaded: the fixed maze model, or a new
    /// random maze.
    fn place_level(&mut self, scene: &mut Scene) -> Result<(), String> {
        if let Some(model) = &self.model {
            let fbx = std::env::var("MAZE_MODEL")
                .is_ok_and(|path| path.to_ascii_lowercase().ends_with(".fbx"));
            self.level = Level::from_model(model, fbx, scene, &mut self.doubled);
            return Ok(());
        }
        let Some(prefabs) = self.prefabs.clone() else {
            return Err("no tiles".into());
        };
        if self.measured.is_none() {
            let measured = tiles::measure(&prefabs, scene)?;
            Log::info(format!(
                "Maze: tiles are {:.1} m cells, {:.1} m high: {:?}",
                measured.cell, measured.height, measured.shapes
            ));
            self.measured = Some(measured);
        }
        let (width, depth) = std::env::var("MAZE_SIZE")
            .ok()
            .and_then(|s| {
                let (w, d) = s.split_once('x')?;
                Some((w.trim().parse().ok()?, d.trim().parse().ok()?))
            })
            .unwrap_or(MAZE_SIZE);
        let maze = Maze::generate(width, depth, LOOP_CHANCE, self.rng());
        let measured = self.measured.as_ref().unwrap();
        self.level = Level::from_tiles(&prefabs, measured, &maze, scene, &mut self.doubled)?;
        Ok(())
    }

    /// The exit's ball, which is a mesh in the scene but no part of the maze.
    fn exit_mesh(&self, graph: &Graph) -> Handle<Node> {
        graph
            .try_get(self.exit)
            .ok()
            .and_then(|exit| exit.children().first().copied())
            .unwrap_or_default()
    }

    fn start_round(&mut self, ctx: &mut PluginContext) {
        // Made (and seeded) first: the grid below borrows the game.
        self.rng();
        let Some((grid, origin)) = self.level.grid.as_ref() else {
            return;
        };
        let Some(rng) = self.rng.as_mut() else {
            return;
        };
        let round = match self.level.goal {
            // Something in the model is the thing to find, so the walk to it should be as long as
            // the maze allows.
            Some(goal) => {
                let exit = survey::nearest_walkable(grid, *origin, goal);
                exit.and_then(|exit| grid.farthest_from(exit).map(|(start, _)| (start, exit)))
            }
            None => grid.plan_round(|n| rng.below(n)),
        };
        let Some((start, exit)) = round else {
            Log::err("Maze: found no walkable ground to play on");
            self.set_banner(ctx, "No walkable floor found in the maze model.");
            self.phase = Phase::Broken;
            return;
        };
        if std::env::var_os("MAZE_DEBUG").is_some() {
            Log::info(format!(
                "Maze grid (x right, z down, origin {origin:?}):\n{}",
                survey::draw_map(grid, start, exit)
            ));
        }
        let start_position = survey::cell_center(*origin, start.0, start.1);
        let exit_position = survey::cell_center(*origin, exit.0, exit.1);

        let scene = &mut ctx.scenes[self.scene];
        // Light whatever marks the end: the model's own landmark if it has one, otherwise the
        // glowing ball, which then has to be visible.
        let (marker, show_ball) = match self.level.goal {
            Some(goal) => (goal, false),
            None => (exit_position + Vector3::new(0.0, 1.2, 0.0), true),
        };
        scene.graph[self.exit]
            .local_transform_mut()
            .set_position(marker);
        if let Some(&ball) = scene.graph[self.exit].children().first() {
            scene.graph[ball].set_visibility(show_ball);
        }
        // Face into the maze: towards the open floor nearby. Starts are at the ends of the
        // longest route, often at an opening in the outer wall, and facing the sky is no start.
        let into_maze = survey::open_direction(grid, *origin, start);
        // Everyone is put down afresh for the new round, away from where the player starts.
        self.inhabitants.clear(&mut scene.graph);
        self.player.teleport(
            &mut scene.graph,
            start_position + Vector3::new(0.0, 1.2, 0.0),
            into_maze.x.atan2(into_maze.z),
        );

        self.round_time = 0.0;
        self.phase = Phase::Playing;
        self.set_banner(ctx, "");
    }

    fn set_banner(&self, ctx: &mut PluginContext, text: &str) {
        self.hud.set_banner(ctx.user_interfaces.first(), text);
    }

    fn update_hud(&mut self, ctx: &mut PluginContext) {
        let status = match self.phase {
            Phase::Loading | Phase::Settling(_) => Status::Loading,
            Phase::Broken => Status::Blank,
            Phase::Playing | Phase::Won => Status::Round {
                time: self.round_time,
                best: self.best_time,
                breath: self.player.breath(),
                // The menu says what to do next, so the hint to click would only be in the way.
                mouse_captured: self.mouse_captured || self.menu.is_open(),
            },
        };
        self.hud.update(ctx.user_interfaces.first(), ctx.dt, status);
    }

    /// Puts the current view settings on screen for a few seconds.
    fn show_look_settings(&mut self) {
        let (sensitivity, fov) = self.player.look_settings();
        self.hud.show_note(format!(
            "View: {fov:.0} degrees ([ ] turn speed {:.1} mrad, - = width)",
            sensitivity * 1000.0
        ));
        Log::info(format!("Maze: {}", self.hud.note()));
    }

    fn set_mouse_captured(&mut self, ctx: &mut PluginContext, captured: bool) {
        let GraphicsContext::Initialized(graphics_context) = &*ctx.graphics_context else {
            return;
        };
        let window = &graphics_context.window;
        if captured {
            // Locked is right for mouse look; X11 cannot lock, so fall back to confining.
            if window.set_cursor_grab(CursorGrabMode::Locked).is_err()
                && window.set_cursor_grab(CursorGrabMode::Confined).is_err()
            {
                return;
            }
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
        }
        window.set_cursor_visible(!captured);
        self.mouse_captured = captured;
    }

    fn on_key(&mut self, ctx: &mut PluginContext, code: KeyCode) {
        match code {
            // The view settings are a matter of taste, so they are tuned here rather than
            // guessed: brackets for how fast the view turns, minus and equals for how wide it is.
            KeyCode::BracketLeft | KeyCode::BracketRight => {
                let factor = if code == KeyCode::BracketLeft {
                    0.8
                } else {
                    1.25
                };
                self.player.nudge_sensitivity(factor);
                self.show_look_settings();
            }
            KeyCode::Minus | KeyCode::Equal => {
                let step = if code == KeyCode::Minus { -5.0 } else { 5.0 };
                let scene = &mut ctx.scenes[self.scene];
                self.player.nudge_fov(step, &mut scene.graph);
                self.show_look_settings();
            }
            KeyCode::Escape => self.set_paused(ctx, !self.menu.is_open()),
            KeyCode::KeyR => self.restart(ctx),
            _ => (),
        }
    }

    /// A new maze, or with a fixed model a new round in it. Only once there is a level to start
    /// again from.
    fn restart(&mut self, ctx: &mut PluginContext) {
        if self.level.grid.is_none() {
            return;
        }
        self.set_paused(ctx, false);
        if self.prefabs.is_some() {
            // Out of the way first: the new maze's survey would take them for walls.
            self.inhabitants
                .clear(&mut ctx.scenes[self.scene].graph);
            self.level.clear(&mut ctx.scenes[self.scene]);
            self.set_banner(ctx, "");
            self.phase = Phase::Loading;
        } else {
            self.start_round(ctx);
        }
    }

    /// Switches the maze's lights as the player has them: the lamps and the glow of their glass,
    /// the sun, and nearly all of the ambient light. The exit keeps its own light, so that there
    /// is still something to make for in the dark.
    fn apply_lights(&mut self, ctx: &mut PluginContext) {
        let on = !self.lights_off;
        let scene = &mut ctx.scenes[self.scene];
        scene.rendering_options.ambient_lighting_color = if on { AMBIENT } else { DARK_AMBIENT };
        scene.graph[self.sun].set_visibility(on);
        self.level.set_lights(&mut scene.graph, on);
        self.menu.set_lights(ctx.user_interfaces.first(), on);
    }

    /// Puts the maze's inhabitants into it once there is a droid to make them from, and moves
    /// them along.
    fn update_inhabitants(&mut self, ctx: &mut PluginContext) {
        let (Some(model), Some((grid, origin)), Some(rng)) =
            (&self.droid_model, &self.level.grid, self.rng.as_mut())
        else {
            return;
        };
        let scene = &mut ctx.scenes[self.scene];
        let player = self.player.feet(&scene.graph);
        if !self.inhabitants.is_populated() {
            self.inhabitants
                .populate(scene, model, (grid, *origin), player, rng);
        }
        self.inhabitants
            .update(&mut scene.graph, (grid, *origin), player, rng, ctx.dt);
    }

    /// Opens the pause menu and stops the world, or closes it and carries on.
    fn set_paused(&mut self, ctx: &mut PluginContext, paused: bool) {
        if paused == self.menu.is_open() {
            return;
        }
        let can_restart = self.level.grid.is_some();
        self.menu
            .set_open(ctx.user_interfaces.first(), paused, can_restart);
        // The round's clock and the player stop in `update`; this stops everything else that
        // moves, and holds the player where they are.
        ctx.scenes[self.scene]
            .graph
            .physics
            .enabled
            .set_value_and_mark_modified(!paused);
        if paused {
            // Nothing should still be walking when the game carries on, and the mouse is needed
            // for the menu.
            self.player.release_keys();
            self.want_mouse = false;
            self.set_mouse_captured(ctx, false);
        } else {
            self.want_mouse = true;
        }
    }
}

impl Plugin for MazeGame {
    fn init(&mut self, _scene_path: Option<&str>, mut ctx: PluginContext) -> GameResult {
        self.focused = true;
        self.build_scene(&mut ctx);
        self.hud = Hud::build(&mut ctx);
        let restart = if self.prefabs.is_some() {
            "New maze"
        } else {
            "New round"
        };
        self.menu = PauseMenu::build(ctx.user_interfaces.first_mut(), restart);
        Ok(())
    }

    fn on_graphics_context_initialized(&mut self, ctx: PluginContext) -> GameResult {
        let GraphicsContext::Initialized(graphics_context) = &*ctx.graphics_context else {
            return Ok(());
        };
        graphics_context.window.set_title("Maze");
        if !diagnostics::is_vulkan(graphics_context) {
            ctx.loop_controller.exit();
            return Ok(());
        }
        self.want_mouse = true;
        Ok(())
    }

    fn update(&mut self, ctx: &mut PluginContext) -> GameResult {
        // The droid joins the player whenever it has loaded; the game goes on without it if it
        // cannot, seen through the player's own eyes.
        if let Some(droid) = self.droid.take_if(|droid| droid.is_ok()) {
            self.player.attach_avatar(&mut ctx.scenes[self.scene], &droid);
            self.droid_model = Some(droid);
        } else if self.droid.as_ref().is_some_and(|d| d.is_failed_to_load()) {
            Log::err(format!("Could not load {DROID_MODEL}; playing in first person"));
            self.droid = None;
        }

        match self.phase {
            // While the menu is open nothing happens: no loading, no clock, no player.
            _ if self.menu.is_open() => (),
            Phase::Loading => {
                let models: Vec<(String, ModelResource)> = match (&self.model, &self.prefabs) {
                    (Some(model), _) => vec![(
                        std::env::var("MAZE_MODEL").unwrap_or_default(),
                        model.clone(),
                    )],
                    (None, Some(prefabs)) => prefabs
                        .all()
                        .into_iter()
                        .map(|(path, model)| (path.to_string(), model.clone()))
                        .collect(),
                    (None, None) => Vec::new(),
                };
                if let Some((path, _)) = models.iter().find(|(_, m)| m.is_failed_to_load()) {
                    let text = format!("Could not load {path}");
                    Log::err(&text);
                    self.set_banner(ctx, &text);
                    self.phase = Phase::Broken;
                } else if !models.is_empty() && models.iter().all(|(_, m)| m.is_ok()) {
                    match self.place_level(&mut ctx.scenes[self.scene]) {
                        Ok(()) => self.phase = Phase::Settling(2),
                        Err(error) => {
                            let text = format!("Could not build a maze from the tiles: {error}");
                            Log::err(&text);
                            self.set_banner(ctx, &text);
                            self.phase = Phase::Broken;
                        }
                    }
                }
            }
            Phase::Settling(frames) => {
                if frames > 0 {
                    self.phase = Phase::Settling(frames - 1);
                } else {
                    let graph = &mut ctx.scenes[self.scene].graph;
                    let exit_mesh = self.exit_mesh(graph);
                    self.level.finish(graph, exit_mesh);
                    // A new level's lamps start out on; they follow the player's choice.
                    self.apply_lights(ctx);
                    self.start_round(ctx);
                }
            }
            Phase::Playing => {
                self.round_time += ctx.dt;
                let scene = &mut ctx.scenes[self.scene];
                self.player.update(&mut scene.graph, ctx.dt, self.focused);
                self.update_inhabitants(ctx);
                let scene = &mut ctx.scenes[self.scene];
                let exit = scene.graph[self.exit].global_position();
                let player = self.player.position(&scene.graph);
                let flat = Vector3::new(exit.x - player.x, 0.0, exit.z - player.z);
                if flat.norm() < EXIT_RADIUS {
                    self.phase = Phase::Won;
                    let best = self
                        .best_time
                        .map_or(self.round_time, |b| b.min(self.round_time));
                    let record = self.best_time.is_none_or(|b| self.round_time < b);
                    self.best_time = Some(best);
                    let text = format!(
                        "You escaped in {}{}\nPress R for another maze",
                        hud::format_time(self.round_time),
                        if record { " - a new best!" } else { "" }
                    );
                    self.set_banner(ctx, &text);
                }
            }
            Phase::Broken => (),
            Phase::Won => {
                let scene = &mut ctx.scenes[self.scene];
                self.player.update(&mut scene.graph, ctx.dt, false);
                self.update_inhabitants(ctx);
            }
        }

        // Only what can be seen from where the player stands is drawn and lit. Not while the
        // level is being readied: the survey measures the tiles, which must all be showing.
        if matches!(self.phase, Phase::Playing | Phase::Won) {
            let scene = &mut ctx.scenes[self.scene];
            let player = self.player.position(&scene.graph);
            self.level.cull(&mut scene.graph, player);
            self.inhabitants.show(&mut scene.graph, &self.level);
        }

        // The exit bobs so it catches the eye.
        if let Ok(exit) = ctx.scenes[self.scene].graph.try_get_mut(self.exit) {
            let t = ctx.elapsed_time;
            if let Some(&mesh) = exit.children().first() {
                let offset = Vector3::new(0.0, (t * 2.0).sin() * 0.15, 0.0);
                ctx.scenes[self.scene].graph[mesh]
                    .local_transform_mut()
                    .set_position(offset);
            }
        }

        // The effects follow only so many things: the player's droid, and the nearest of the rest.
        let moving = match self.phase {
            Phase::Playing | Phase::Won => {
                let graph = &ctx.scenes[self.scene].graph;
                let player = self.player.position(graph);
                let droid = self.player.moving(graph);
                let inhabitant = self.inhabitants.moving(graph, player);
                droid.into_iter().chain(inhabitant).collect()
            }
            _ => Vec::new(),
        };
        self.moving.set(moving);

        if self.want_mouse && !self.mouse_captured && self.focused {
            self.set_mouse_captured(ctx, true);
        }

        self.update_hud(ctx);
        self.stats.update(ctx);
        Ok(())
    }

    fn on_ui_message(
        &mut self,
        ctx: &mut PluginContext,
        message: &UiMessage,
        _ui: Handle<UserInterface>,
    ) -> GameResult {
        match self.menu.choice(message) {
            Some(Choice::Resume) => self.set_paused(ctx, false),
            Some(Choice::Lights) => {
                self.lights_off = !self.lights_off;
                self.apply_lights(ctx);
            }
            Some(Choice::Restart) => self.restart(ctx),
            Some(Choice::Quit) => ctx.loop_controller.exit(),
            None => (),
        }
        Ok(())
    }

    fn on_os_event(&mut self, event: &Event<()>, mut ctx: PluginContext) -> GameResult {
        match event {
            Event::DeviceEvent {
                event: fyrox::event::DeviceEvent::MouseMotion { delta },
                ..
            } => {
                if self.mouse_captured && self.focused {
                    self.player.look(delta.0 as f32, delta.1 as f32);
                }
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput { event: input, .. } => {
                    if let PhysicalKey::Code(code) = input.physical_key {
                        let pressed = input.state == ElementState::Pressed;
                        // The menu holds the player still; letting go of a key still counts.
                        if !self.menu.is_open() || !pressed {
                            self.player.on_key(code, pressed);
                        }
                        if pressed && !input.repeat {
                            self.on_key(&mut ctx, code);
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    // With the menu open, a click is for the menu.
                    if !self.menu.is_open() {
                        self.want_mouse = true;
                    }
                }
                WindowEvent::Focused(focused) => {
                    self.focused = *focused;
                    if !focused {
                        // Nothing should keep walking while the player is in another window, and a
                        // round in play waits for them to come back.
                        self.player.release_keys();
                        self.want_mouse = false;
                        self.set_mouse_captured(&mut ctx, false);
                        if self.phase == Phase::Playing {
                            self.set_paused(&mut ctx, true);
                        }
                    }
                }
                _ => (),
            },
            _ => (),
        }
        Ok(())
    }
}
