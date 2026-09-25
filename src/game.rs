//! The game itself: loading a level, playing rounds in it, and the player's input.

use crate::{
    diagnostics::{self, FrameStats},
    dialogue::{
        screen::{DialogueScreen, Pointer},
        Conversation, Facts, Script, SCRIPT,
    },
    formants::{
        self,
        speech::{Voices, VOICES},
        synth,
    },
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
    event::{ElementState, Event, MouseButton, WindowEvent},
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
        sound::{SoundBuilder, Status as SoundStatus},
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
/// How much higher or lower each droid speaks than the rest of its kind, at most, as a part of
/// their pitch.
const VOICE_SPREAD: f32 = 0.06;
/// How near the exit is, in meters as the crow flies, for a droid to call it near, and to call
/// it not far off; any further is far.
const EXIT_NEAR: f32 = 25.0;
const EXIT_NOT_FAR: f32 = 60.0;
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

/// A conversation under way.
#[derive(Debug, PartialEq)]
struct Talking {
    /// The droid being talked to, as an index into the inhabitants.
    droid: usize,
    conversation: Conversation,
    /// What is true where it is happening, for its lines.
    facts: Facts,
    /// Who is talking, as the screen names them.
    who: String,
    /// What the droid is saying out loud, while it is.
    voice: Handle<Node>,
    /// The line it is about to say, while it is being made.
    making: Option<Making>,
}

/// A line being made into a voice, away from the game so as not to hold it up: the samples to
/// come, and how far off they are heard at full volume.
#[derive(Debug)]
struct Making(std::sync::mpsc::Receiver<Vec<f32>>, f32);

impl PartialEq for Making {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
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
    /// What the droids say, if it could be read, and how they sound saying it.
    #[visit(skip)]
    #[reflect(hidden)]
    script: Option<Script>,
    #[visit(skip)]
    #[reflect(hidden)]
    voices: Option<Voices>,
    /// The conversation under way, if there is one; while it is, the clock and the player
    /// stand still.
    #[visit(skip)]
    #[reflect(hidden)]
    talking: Option<Talking>,
    /// The droid the player could talk to right now, as an index into the inhabitants, and who
    /// it is as the hint to talk names it.
    #[visit(skip)]
    #[reflect(hidden)]
    talkable: Option<(usize, String)>,
    #[visit(skip)]
    #[reflect(hidden)]
    dialogue: DialogueScreen,
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
        self.stop_talking(ctx);
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
                // The menu and the conversation say what to do next, so the hint to click would
                // only be in the way.
                mouse_captured: self.mouse_captured
                    || self.menu.is_open()
                    || self.talking.is_some(),
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
            _ if self.talking.is_some() && !self.menu.is_open() => self.on_talking_key(ctx, code),
            KeyCode::KeyE if !self.menu.is_open() => self.start_talking(ctx),
            KeyCode::KeyN => self.restart(ctx),
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
        self.stop_talking(ctx);
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
            let characters = self.script.as_ref().map_or(0, |s| s.characters.len());
            let ahead = self.player.ahead();
            self.inhabitants.populate(
                scene,
                model,
                (grid, *origin),
                player,
                ahead,
                characters,
                rng,
            );
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
            // Talking, the mouse is for picking what to say.
            self.want_mouse = self.talking.is_none();
        }
    }

    /// Finds the droid the player could talk to, if any, and puts the hint to talk to it on
    /// screen. Nobody, while the player cannot talk.
    fn look_for_someone(&mut self, ctx: &mut PluginContext) {
        let can_talk = self.phase == Phase::Playing
            && self.talking.is_none()
            && !self.menu.is_open()
            && self.script.is_some();
        let found = if can_talk {
            let graph = &ctx.scenes[self.scene].graph;
            let (player, feet, ahead) = (&self.player, self.player.feet(graph), self.player.ahead());
            self.inhabitants
                .to_talk_to(feet, ahead, |there| player.can_see(graph, there))
        } else {
            None
        };
        let talkable = found.and_then(|n| Some((n, self.name_of(n)?)));
        if talkable != self.talkable {
            let who = talkable.as_ref().map(|(_, who)| who.as_str());
            self.dialogue.set_prompt(ctx.user_interfaces.first(), who);
            self.talkable = talkable;
        }
    }

    /// What the `n`th inhabitant is called on screen: what kind of droid it is, and its code.
    fn name_of(&self, n: usize) -> Option<String> {
        let (character, code) = self.inhabitants.who(n)?;
        let name = &self.script.as_ref()?.characters.get(character)?.name;
        Some(format!("{} {code}", name.to_uppercase()))
    }

    /// What a conversation with the `n`th inhabitant can talk about: its code, and how far off
    /// the exit is and which way, as the crow flies, for a player facing it.
    fn facts(&self, graph: &Graph, n: usize) -> Option<Facts> {
        let (_, code) = self.inhabitants.who(n)?;
        let player = self.player.feet(graph);
        let flat = |v: Vector3<f32>| Vector3::new(v.x, 0.0, v.z);
        let ahead = flat(self.inhabitants.feet(n)? - player)
            .try_normalize(1.0e-4)
            .unwrap_or_else(|| self.player.ahead());
        // Facing +z, the right is -x.
        let right = Vector3::new(-ahead.z, 0.0, ahead.x);
        let exit = flat(graph[self.exit].global_position() - player);
        let far = match exit.norm() {
            d if d < EXIT_NEAR => ("prope", "near"),
            d if d < EXIT_NOT_FAR => ("non longe", "not far off"),
            _ => ("longe", "far off"),
        };
        let off = exit.dot(&right).atan2(exit.dot(&ahead)).to_degrees();
        let way = match off {
            o if o.abs() <= 45.0 => ("rectum", "straight ahead"),
            o if o.abs() >= 135.0 => ("retro", "back behind you"),
            o if o > 0.0 => ("dextrum", "to your right"),
            _ => ("sinistrum", "to your left"),
        };
        Some(
            Facts::default()
                .with("code", crate::dialogue::digits(code), code.to_string())
                .with("exit_far", far.0, far.1)
                .with("exit_way", way.0, way.1),
        )
    }

    /// Starts talking to the droid the player could talk to, if there is one: it stops and
    /// faces them, the camera closes in on its face, and the mouse is let go to pick replies.
    fn start_talking(&mut self, ctx: &mut PluginContext) {
        if self.phase != Phase::Playing || self.talking.is_some() {
            return;
        }
        let Some((droid, who)) = self.talkable.clone() else {
            return;
        };
        let graph = &ctx.scenes[self.scene].graph;
        let (Some(script), Some((character, _)), Some(facts), Some(face)) = (
            &self.script,
            self.inhabitants.who(droid),
            self.facts(graph, droid),
            self.inhabitants.face(graph, droid),
        ) else {
            return;
        };
        let Some(conversation) = Conversation::new(script, character) else {
            return;
        };
        let ui = ctx.user_interfaces.first();
        self.dialogue.set_open(ui, true);
        self.dialogue
            .show(ui, &who, &conversation.view(script, &facts));
        self.dialogue.set_prompt(ui, None);
        self.talkable = None;
        self.inhabitants.set_talking(droid, true);
        self.player.release_keys();
        self.player.talk_to(Some(face));
        self.want_mouse = false;
        self.set_mouse_captured(ctx, false);
        self.talking = Some(Talking {
            droid,
            conversation,
            facts,
            who,
            voice: Handle::NONE,
            making: None,
        });
        self.speak(ctx);
    }

    /// Has the droid being talked to say its line out loud, cutting off whatever it was saying
    /// before. The line is made into a voice away from the game, and said once it is ready: see
    /// [`MazeGame::keep_talking`].
    fn speak(&mut self, ctx: &mut PluginContext) {
        self.hush(ctx);
        let (Some(talking), Some(script), Some(voices)) =
            (self.talking.as_mut(), &self.script, &self.voices)
        else {
            return;
        };
        let Some((character, code)) = self.inhabitants.who(talking.droid) else {
            return;
        };
        let Some(voice) = voices.voice(&script.characters[character].name) else {
            return;
        };
        let says = talking.conversation.view(script, &talking.facts).says;
        // Each a little higher or lower than the rest of its kind, and always the same.
        let pitch = 1.0 + VOICE_SPREAD * ((code % 7) as f32 / 3.0 - 1.0);
        let sound = voices.speak(&says, voice, pitch);
        let (sender, receiver) = std::sync::mpsc::channel();
        let (rate, reach) = (voices.sample_rate, sound.reach);
        std::thread::spawn(move || sender.send(synth::make(&sound, rate)));
        talking.making = Some(Making(receiver, reach));
    }

    /// Says the line that has been made into a voice, if it is ready, from the droid's face.
    fn say_when_made(&mut self, ctx: &mut PluginContext) {
        let (Some(talking), Some(voices)) = (self.talking.as_mut(), &self.voices) else {
            return;
        };
        let Some(Making(receiver, reach)) = &talking.making else {
            return;
        };
        let Ok(samples) = receiver.try_recv() else {
            return;
        };
        let reach = *reach;
        talking.making = None;
        let graph = &mut ctx.scenes[self.scene].graph;
        let (Some(face), Some(buffer)) = (
            self.inhabitants.face(graph, talking.droid),
            formants::playable(samples, voices.sample_rate),
        ) else {
            return;
        };
        talking.voice = SoundBuilder::new(BaseBuilder::new().with_local_transform(
            TransformBuilder::new().with_local_position(face).build(),
        ))
        .with_buffer(Some(buffer))
        .with_radius(reach)
        .with_play_once(true)
        .with_status(SoundStatus::Playing)
        .build(graph)
        .to_base();
    }

    /// Stops the droid being talked to from saying any more of its line.
    fn hush(&mut self, ctx: &mut PluginContext) {
        let Some(talking) = self.talking.as_mut() else {
            return;
        };
        let graph = &mut ctx.scenes[self.scene].graph;
        if graph.is_valid_handle(talking.voice) {
            graph.remove_node(talking.voice);
        }
        talking.voice = Handle::NONE;
        // Whatever was being made is not wanted any more.
        talking.making = None;
    }

    /// Says the `choice`th reply on offer, and shows what comes of it, or ends the conversation.
    fn say(&mut self, ctx: &mut PluginContext, choice: usize) {
        let roll = self.rng().below(100) as u32;
        let (Some(talking), Some(script)) = (self.talking.as_mut(), self.script.as_ref()) else {
            return;
        };
        if talking.conversation.choose(script, choice, roll) {
            let view = talking.conversation.view(script, &talking.facts);
            self.dialogue
                .show(ctx.user_interfaces.first(), &talking.who, &view);
            self.speak(ctx);
        } else {
            self.stop_talking(ctx);
        }
    }

    /// Ends the conversation, if there is one: the droid goes on its way, the camera goes back
    /// and the mouse is taken again to look around.
    fn stop_talking(&mut self, ctx: &mut PluginContext) {
        self.hush(ctx);
        let Some(talking) = self.talking.take() else {
            return;
        };
        self.inhabitants.set_talking(talking.droid, false);
        self.player.talk_to(None);
        self.dialogue.set_open(ctx.user_interfaces.first(), false);
        self.want_mouse = !self.menu.is_open();
    }

    /// Keeps the camera on the face of the droid being talked to, as it moves, and has it say its
    /// line once that is ready.
    fn keep_talking(&mut self, ctx: &mut PluginContext) {
        let Some(droid) = self.talking.as_ref().map(|talking| talking.droid) else {
            return;
        };
        match self.inhabitants.face(&ctx.scenes[self.scene].graph, droid) {
            Some(face) => self.player.talk_to(Some(face)),
            None => self.stop_talking(ctx),
        }
        self.say_when_made(ctx);
    }

    /// A key pressed while talking: W and S or the arrows to go through the replies, E, Enter or
    /// Space to say the one picked, a number to say that one, and Tab to walk away.
    fn on_talking_key(&mut self, ctx: &mut PluginContext, code: KeyCode) {
        let ui = ctx.user_interfaces.first();
        let number = [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ]
        .iter()
        .position(|&digit| digit == code);
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.dialogue.step(ui, -1),
            KeyCode::KeyS | KeyCode::ArrowDown => self.dialogue.step(ui, 1),
            KeyCode::KeyE | KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                let choice = self.dialogue.selected();
                self.say(ctx, choice);
            }
            KeyCode::Tab => self.stop_talking(ctx),
            _ => {
                if let Some(n) = number.filter(|&n| n < self.dialogue.count()) {
                    self.say(ctx, n);
                }
            }
        }
    }
}

impl Plugin for MazeGame {
    fn init(&mut self, _scene_path: Option<&str>, mut ctx: PluginContext) -> GameResult {
        self.focused = true;
        self.build_scene(&mut ctx);
        self.hud = Hud::build(&mut ctx);
        // Under the menu, which is built after it.
        self.dialogue = DialogueScreen::build(ctx.user_interfaces.first_mut());
        self.script = match Script::load(SCRIPT) {
            Ok(script) => Some(script),
            Err(error) => {
                Log::err(format!("Maze: the droids have nothing to say: {error}"));
                None
            }
        };
        self.voices = Voices::load(VOICES)
            .inspect_err(|error| Log::err(format!("Maze: the droids are silent: {error}")))
            .ok();
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
                // Talking stops the clock, and holds the player where they are.
                let talking = self.talking.is_some();
                if !talking {
                    self.round_time += ctx.dt;
                }
                self.keep_talking(ctx);
                let scene = &mut ctx.scenes[self.scene];
                self.player
                    .update(&mut scene.graph, ctx.dt, self.focused && !talking);
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
                        "You escaped in {}{}\nPress N for another maze",
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

        self.look_for_someone(ctx);
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
        if !self.menu.is_open() {
            match self.dialogue.pointer(message) {
                Some(Pointer::Over(n)) => self.dialogue.select(ctx.user_interfaces.first(), n),
                Some(Pointer::Picked(n)) => self.say(ctx, n),
                None => (),
            }
        }
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
                        // The menu and talking hold the player still; letting go of a key still
                        // counts.
                        if (!self.menu.is_open() && self.talking.is_none()) || !pressed {
                            self.player.on_key(code, pressed);
                        }
                        if pressed && !input.repeat {
                            self.on_key(&mut ctx, code);
                        }
                    }
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Middle,
                    state,
                    ..
                } => {
                    // Held, the mouse swings the camera round the droid. Let go counts even
                    // with the menu open, so the camera is not left swung round.
                    let held = *state == ElementState::Pressed;
                    if !held || (!self.menu.is_open() && self.mouse_captured) {
                        self.player.set_orbiting(held);
                    }
                    if held && !self.menu.is_open() && self.talking.is_none() {
                        self.want_mouse = true;
                    }
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Left,
                    state: ElementState::Pressed,
                    ..
                } => {
                    // The click that takes the mouse is only for that; after it, the left button
                    // is the pistol's. Talking, it picks what to say instead.
                    if !self.menu.is_open() && self.talking.is_none() {
                        if self.mouse_captured {
                            self.player.pull_trigger();
                        }
                        self.want_mouse = true;
                    }
                }
                WindowEvent::MouseInput {
                    button: MouseButton::Right,
                    state,
                    ..
                } => {
                    // Held, the droid strafes. Let go counts even with the menu open, so it is
                    // not left strafing.
                    let held = *state == ElementState::Pressed;
                    if !held || (!self.menu.is_open() && self.mouse_captured) {
                        self.player.set_strafing(held);
                    }
                    if held && !self.menu.is_open() && self.talking.is_none() {
                        self.want_mouse = true;
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    // With the menu open, or talking, a click is for that.
                    if !self.menu.is_open() && self.talking.is_none() {
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
