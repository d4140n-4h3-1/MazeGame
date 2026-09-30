//! Team battle: red droids against cyan droids in the combat arena, `examples/combat_map.glb`.
//!
//! Five a side, all played by the AI. They cross the arena along the walk grid, see only in front
//! of them (hydroxus-ai's sight), and fight from cover: a crouched droid behind a low block or a
//! pillar is out of the line of fire, and stands up or steps round it to shoot. Hurt, a droid
//! falls back to cover further from the enemy. The dead come back at their own end of the arena
//! a few seconds later, and the first team to reach the kill limit wins the match.
//!
//! Run with `cargo run --example team_battle`.
//!
//! The camera is free: WASD to fly, Q and E down and up, Shift to go faster, the right mouse
//! button held to look round. Tab follows the next droid, F lets it go, Space pauses, R starts a
//! new match, Escape quits.

use fyrox::{
    core::{
        algebra::{Matrix4, Point3, UnitQuaternion, Vector2, Vector3},
        color::Color,
        log::Log,
        math::aabb::AxisAlignedBoundingBox,
        pool::Handle,
        reflect::prelude::*,
        sstorage::ImmutableString,
        visitor::prelude::*,
    },
    engine::{executor::Executor, GraphicsContextParams},
    event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent},
    event_loop::EventLoop,
    graph::SceneGraph,
    gui::{
        brush::Brush,
        text::{Text, TextBuilder, TextMessage},
        widget::WidgetBuilder,
        HorizontalAlignment, Thickness, UserInterface,
    },
    keyboard::{KeyCode, PhysicalKey},
    material::{Material, MaterialProperty, MaterialResource, MaterialResourceBinding},
    plugin::{error::GameResult, Plugin, PluginContext},
    resource::model::{Model, ModelResource, ModelResourceExtension},
    scene::{
        base::BaseBuilder,
        camera::CameraBuilder,
        collider::{Collider, ColliderBuilder, ColliderShape, GeometrySource},
        graph::{physics::RayCastOptions, Graph},
        light::{point::PointLightBuilder, BaseLightBuilder},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            Mesh, MeshBuilder,
        },
        node::Node,
        pivot::PivotBuilder,
        rigidbody::{RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        EnvironmentLightingSource, Scene,
    },
    window::WindowAttributes,
};
use hydroxus_ai::prelude::*;

const MAP: &str = "examples/combat_map.glb";
const PER_TEAM: usize = 5;
const KILL_LIMIT: u32 = 30;

/// Walk grid spacing, and how far from any wall a droid's middle keeps.
const CELL: f32 = 0.5;
const BODY_RADIUS: f32 = 0.4;

const WALK_SPEED: f32 = 3.5;
const RUN_SPEED: f32 = 5.5;
const TURN_SPEED: f32 = 8.0;

/// Eye and chest heights, standing and crouched. Low cover is about 1.1 m tall, so a crouched
/// droid behind it is hidden and a standing one can shoot over it.
const EYES_STANDING: f32 = 1.55;
const EYES_CROUCHED: f32 = 0.8;
const CHEST_STANDING: f32 = 1.2;
const CHEST_CROUCHED: f32 = 0.65;

const HEALTH: f32 = 100.0;
const DAMAGE: (f32, f32) = (9.0, 16.0);
const FIRE_INTERVAL: f32 = 0.16;
const RESPAWN_TIME: f32 = 5.0;
/// Hurt this badly, a droid falls back instead of pushing on.
const RETREAT_HEALTH: f32 = 35.0;
/// How far a droid will go to reach cover, in grid steps (edges count double).
const COVER_REACH: f32 = 30.0;
/// How long what a team saw of an enemy stays worth acting on.
const INTEL_TIME: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Team {
    Red,
    Cyan,
}

impl Team {
    fn color(self) -> Color {
        match self {
            Team::Red => Color::opaque(225, 40, 40),
            Team::Cyan => Color::opaque(0, 225, 235),
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Team::Red => "RED",
            Team::Cyan => "CYAN",
        }
    }
}

/// A place next to an obstacle where a crouched droid is hidden from some directions.
#[derive(Debug, Clone)]
struct CoverSpot {
    position: Vector3<f32>,
    /// The directions (as unit vectors along the ground) that an obstacle shields it from.
    shields: Vec<Vector3<f32>>,
}

/// What a droid is doing.
#[derive(Debug, Clone, PartialEq)]
enum Task {
    /// Heading for the enemy: where one was last seen, or into their half of the arena.
    Advance,
    /// Running to cover, to shoot from `peek` - the same spot standing, or a step to the side.
    ToCover { spot: usize, peek: Vector3<f32> },
    /// Crouched in cover.
    Hidden { spot: usize, peek: Vector3<f32>, left: f32 },
    /// Up out of cover and shooting.
    Peeking { spot: usize, peek: Vector3<f32>, left: f32 },
    /// No cover to be had: standing and fighting where it is.
    Standing { left: f32 },
}

impl Task {
    fn label(&self) -> &'static str {
        match self {
            Task::Advance => "advancing",
            Task::ToCover { .. } => "running to cover",
            Task::Hidden { .. } => "in cover",
            Task::Peeking { .. } => "firing from cover",
            Task::Standing { .. } => "fighting in the open",
        }
    }
}

#[derive(Debug)]
struct Droid {
    team: Team,
    root: Handle<Node>,
    /// The part that squashes down when crouching.
    body: Handle<Node>,
    material: MaterialResource,
    position: Vector3<f32>,
    heading: f32,
    health: f32,
    /// Seconds until it comes back, while dead.
    dead_for: Option<f32>,
    route: Vec<Vector3<f32>>,
    /// Where the route ends, so it is only planned again when that changes.
    going_to: Option<Vector3<f32>>,
    task: Task,
    crouched: bool,
    /// The enemy it is fighting, while it can see them.
    target: Option<usize>,
    /// Where it heads when there is nothing better to do.
    objective: Option<Vector3<f32>>,
    reload: f32,
    look_timer: f32,
    /// A moment of red after a hit.
    hurt_flash: f32,
    kills: u32,
    deaths: u32,
}

impl Droid {
    fn alive(&self) -> bool {
        self.dead_for.is_none()
    }

    fn eyes(&self) -> Vector3<f32> {
        self.position + Vector3::y() * if self.crouched { EYES_CROUCHED } else { EYES_STANDING }
    }

    fn chest(&self) -> Vector3<f32> {
        self.position + Vector3::y() * if self.crouched { CHEST_CROUCHED } else { CHEST_STANDING }
    }

    fn cover_spot(&self) -> Option<usize> {
        match self.task {
            Task::ToCover { spot, .. } | Task::Hidden { spot, .. } | Task::Peeking { spot, .. } => {
                Some(spot)
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Tracer {
    node: Handle<Node>,
    left: f32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum Phase {
    #[default]
    Loading,
    /// The level is in the scene, waiting for its collider to reach the physics world.
    Settling(u32),
    Playing,
    /// The match is over; a new one starts when the time runs out.
    Won(Team, f32),
}

#[derive(Debug, Default)]
struct Battle {
    scene: Handle<Scene>,
    map: Option<ModelResource>,
    level: Handle<Collider>,
    grid: Option<(WalkGrid, Vector3<f32>)>,
    /// Walkable cells at each end of the arena: red's and cyan's.
    spawns: [Vec<(usize, usize)>; 2],
    covers: Vec<CoverSpot>,
    droids: Vec<Droid>,
    tracers: Vec<Tracer>,
    /// Where each team last saw each enemy, and how long ago.
    intel: [Vec<Option<(Vector3<f32>, f32)>>; 2],
    score: [u32; 2],
    phase: Phase,
    paused: bool,
    rng: Option<Rng>,
    // The spectator camera.
    camera: Handle<Node>,
    camera_position: Vector3<f32>,
    yaw: f32,
    pitch: f32,
    looking: bool,
    keys: Vec<KeyCode>,
    following: Option<usize>,
    // The interface.
    scoreboard: Handle<Text>,
    banner: Handle<Text>,
    help: Handle<Text>,
}

/// The plugin compares itself for the editor; a battle is only ever equal to itself.
impl PartialEq for Battle {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

#[derive(Default, Debug, PartialEq, Visit, Reflect)]
#[reflect(non_cloneable, type_uuid = "6c1f5a7e-3b8d-4e2a-9f41-7d0c2b9e8a13")]
struct TeamBattle {
    #[visit(skip)]
    #[reflect(hidden)]
    battle: Battle,
}

fn main() {
    // Assets are looked up next to this crate, wherever it is started from.
    let _ = std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"));
    let mut executor = Executor::from_params(
        Some(EventLoop::new().unwrap()),
        GraphicsContextParams {
            window_attributes: WindowAttributes::default()
                .with_title("Team battle")
                .with_resizable(true),
            vsync: true,
            msaa_sample_count: None,
            graphics_server_constructor: Default::default(),
            named_objects: false,
        },
    );
    executor.add_plugin(TeamBattle::default());
    executor.run()
}

impl Plugin for TeamBattle {
    fn init(&mut self, _scene_path: Option<&str>, mut ctx: PluginContext) -> GameResult {
        self.battle.init(&mut ctx);
        Ok(())
    }

    fn update(&mut self, ctx: &mut PluginContext) -> GameResult {
        self.battle.update(ctx);
        Ok(())
    }

    fn on_os_event(&mut self, event: &Event<()>, mut ctx: PluginContext) -> GameResult {
        self.battle.on_event(event, &mut ctx);
        Ok(())
    }
}

impl Battle {
    fn init(&mut self, ctx: &mut PluginContext) {
        let mut scene = Scene::new();
        scene.rendering_options.ambient_lighting_color = Color::opaque(70, 72, 80);
        scene.rendering_options.environment_lighting_source = EnvironmentLightingSource::AmbientColor;

        self.camera_position = Vector3::new(0.0, 28.0, -38.0);
        self.pitch = 38f32.to_radians();
        self.camera = CameraBuilder::new(BaseBuilder::new()).build(&mut scene.graph).to_base();
        self.scene = ctx.scenes.add(scene);
        self.map = Some(ctx.resource_manager.request::<Model>(MAP));

        ctx.user_interfaces.add(UserInterface::new(Vector2::new(1280.0, 720.0)));
        let ui = ctx.user_interfaces.first_mut();
        self.scoreboard = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::uniform(12.0))
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_font_size(30.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .with_text("Loading the arena...")
        .build(&mut ui.build_ctx());
        self.banner = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::top(60.0))
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_font_size(44.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(&mut ui.build_ctx());
        self.help = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::uniform(12.0))
                .with_vertical_alignment(fyrox::gui::VerticalAlignment::Bottom)
                .with_foreground(Brush::Solid(Color::opaque(200, 200, 200)).into()),
        )
        .with_font_size(16.0.into())
        .with_text(
            "WASD fly   Q/E down/up   Shift faster   Right mouse look\n\
             Tab follow a droid   F free camera   Space pause   R new match   Esc quit",
        )
        .build(&mut ui.build_ctx());
    }

    fn rng(&mut self) -> &mut Rng {
        self.rng.get_or_insert_with(|| {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |t| t.as_nanos() as u64);
            Rng::new(seed)
        })
    }

    fn random(&mut self, range: (f32, f32)) -> f32 {
        between(self.rng(), range)
    }

    fn update(&mut self, ctx: &mut PluginContext) {
        let dt = ctx.dt;
        match self.phase {
            Phase::Loading => {
                let Some(map) = self.map.clone() else { return };
                if map.is_failed_to_load() {
                    self.set_text(ctx, self.scoreboard, format!("Could not load {MAP}"));
                    self.map = None;
                } else if map.is_ok() {
                    self.place_level(&mut ctx.scenes[self.scene], &map);
                    self.phase = Phase::Settling(0);
                }
            }
            // Colliders only reach the physics world with its next step.
            Phase::Settling(frames) if frames < 2 => self.phase = Phase::Settling(frames + 1),
            Phase::Settling(_) => {
                let graph = &ctx.scenes[self.scene].graph;
                self.survey(graph);
                self.find_cover(graph);
                self.start_match(&mut ctx.scenes[self.scene].graph);
                self.phase = Phase::Playing;
            }
            Phase::Playing if !self.paused => {
                let graph = &mut ctx.scenes[self.scene].graph;
                self.think(graph, dt);
                self.move_droids(dt);
                if let Some(team) = [Team::Red, Team::Cyan]
                    .into_iter()
                    .find(|team| self.score[team.index()] >= KILL_LIMIT)
                {
                    self.phase = Phase::Won(team, 6.0);
                }
            }
            Phase::Won(team, left) if !self.paused => {
                if left - dt <= 0.0 {
                    self.start_match(&mut ctx.scenes[self.scene].graph);
                    self.phase = Phase::Playing;
                } else {
                    self.phase = Phase::Won(team, left - dt);
                }
            }
            _ => (),
        }
        let graph = &mut ctx.scenes[self.scene].graph;
        if !self.paused {
            self.update_tracers(graph, dt);
        }
        self.show_droids(graph, dt);
        self.move_camera(graph, dt);
        self.update_text(ctx);
    }

    // ------------------------------------------------------------------ the level

    fn place_level(&mut self, scene: &mut Scene, map: &ModelResource) {
        let root = map.instantiate(scene);
        scene.graph.update_hierarchical_data();

        // The map marks its light strips pure magenta: they glow, and each gets a lamp.
        let mut glass = Material::standard();
        glass.set_property("diffuseColor", Color::opaque(255, 240, 215));
        glass.set_property("emissionStrength", MaterialProperty::Vector3(Vector3::repeat(4.0)));
        let glass = MaterialResource::new_embedded(glass);
        let mut lamps = Vec::new();
        let meshes: Vec<Handle<Node>> = scene
            .graph
            .traverse_handle_iter(root)
            .filter(|&h| scene.graph[h].is_mesh())
            .collect();
        for &handle in &meshes {
            let bounds = scene.graph[handle].world_bounding_box();
            let Some(mesh) = scene.graph[handle].cast_mut::<Mesh>() else { continue };
            for surface in mesh.surfaces_mut() {
                let magenta = diffuse_color(&surface.material().data_ref())
                    == Some(Color::opaque(255, 0, 255));
                if magenta {
                    surface.set_material(glass.clone());
                    let center = bounds.center();
                    lamps.push(Vector3::new(center.x, bounds.min.y - 0.4, center.z));
                }
            }
        }
        for position in lamps {
            PointLightBuilder::new(
                BaseLightBuilder::new(
                    BaseBuilder::new()
                        .with_cast_shadows(false)
                        .with_local_transform(TransformBuilder::new().with_local_position(position).build()),
                )
                .with_color(Color::opaque(255, 235, 205))
                .with_intensity(1.2)
                .with_scatter_enabled(false),
            )
            .with_radius(10.0)
            .build(&mut scene.graph);
        }

        let collider = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(ColliderShape::trimesh(meshes.into_iter().map(GeometrySource).collect()))
            .build(&mut scene.graph);
        RigidBodyBuilder::new(BaseBuilder::new().with_child(collider))
            .with_body_type(RigidBodyType::Static)
            .build(&mut scene.graph);
        self.level = collider;
    }

    /// The first thing in the way from `from` to `to`, as how far along it is.
    fn blocked(graph: &Graph, from: Vector3<f32>, to: Vector3<f32>) -> Option<f32> {
        let way = to - from;
        let length = way.norm();
        if length < 1.0e-4 {
            return None;
        }
        let mut hits = Vec::new();
        graph.physics.cast_ray(
            RayCastOptions {
                ray_origin: Point3::from(from),
                ray_direction: way / length,
                max_len: length,
                groups: Default::default(),
                sort_results: true,
            },
            &mut hits,
        );
        hits.first().map(|hit| (hit.position.coords - from).norm())
    }

    fn clear(graph: &Graph, from: Vector3<f32>, to: Vector3<f32>) -> bool {
        Self::blocked(graph, from, to).is_none()
    }

    /// Samples the arena for floor a droid can stand on, with room round it.
    fn survey(&mut self, graph: &Graph) {
        let mut bounds = AxisAlignedBoundingBox::default();
        for node in graph.linear_iter() {
            if node.is_mesh() {
                bounds.add_box(node.world_bounding_box());
            }
        }
        let width = ((bounds.max.x - bounds.min.x) / CELL).ceil() as usize;
        let depth = ((bounds.max.z - bounds.min.z) / CELL).ceil() as usize;
        let origin = Vector3::new(bounds.min.x, 0.0, bounds.min.z);
        let mut grid = WalkGrid::new(width, depth, CELL);
        for z in 0..depth {
            for x in 0..width {
                let spot = grid.center(origin, (x, z));
                let probe = spot + Vector3::y() * 1.0;
                let Some(down) = Self::blocked(graph, probe, probe - Vector3::y() * 1.5) else {
                    continue;
                };
                let floor = 1.0 - down;
                // The arena floor is flat: anything higher is the top of a block.
                if floor > 0.3 {
                    continue;
                }
                let waist = Vector3::new(spot.x, floor + 0.5, spot.z);
                let head = Vector3::new(spot.x, floor + 1.6, spot.z);
                let roomy = [Vector3::x(), -Vector3::x(), Vector3::z(), -Vector3::z()]
                    .iter()
                    .all(|dir| {
                        Self::clear(graph, waist, waist + dir * BODY_RADIUS)
                            && Self::clear(graph, head, head + dir * BODY_RADIUS)
                    });
                if roomy && Self::clear(graph, waist, head) {
                    grid.set(x, z, true);
                    grid.set_floor(x, z, floor);
                }
            }
        }

        // Keep only the biggest connected area: anything else is outside the walls.
        let mut best: Vec<(usize, usize)> = Vec::new();
        let mut seen = vec![false; width * depth];
        let cells: Vec<_> = grid.walkable_cells().collect();
        for (x, z) in cells {
            if seen[z * width + x] {
                continue;
            }
            let area: Vec<_> = grid
                .distances_from((x, z))
                .iter()
                .enumerate()
                .filter(|(_, d)| d.is_some())
                .map(|(i, _)| (i % width, i / width))
                .collect();
            for &(ax, az) in &area {
                seen[az * width + ax] = true;
            }
            if area.len() > best.len() {
                best = area;
            }
        }
        let mut kept = WalkGrid::new(width, depth, CELL);
        for &(x, z) in &best {
            kept.set(x, z, true);
            kept.set_floor(x, z, grid.floor(x, z));
        }

        // Each team starts in the tenth of the arena at its own end.
        let (min_x, max_x) = best.iter().fold((usize::MAX, 0), |(lo, hi), &(x, _)| (lo.min(x), hi.max(x)));
        let band = (max_x - min_x) / 10;
        self.spawns[Team::Red.index()] = best.iter().copied().filter(|&(x, _)| x <= min_x + band).collect();
        self.spawns[Team::Cyan.index()] = best.iter().copied().filter(|&(x, _)| x >= max_x - band).collect();
        Log::info(format!(
            "Team battle: {} walkable cells, {} and {} spawn cells",
            best.len(),
            self.spawns[0].len(),
            self.spawns[1].len()
        ));
        self.grid = Some((kept, origin));
    }

    /// Finds every place right beside an obstacle that hides a crouched droid from some side.
    fn find_cover(&mut self, graph: &Graph) {
        let Some((grid, origin)) = &self.grid else { return };
        let directions: Vec<Vector3<f32>> = (0..16)
            .map(|i| forward(i as f32 * std::f32::consts::TAU / 16.0))
            .collect();
        let mut covers = Vec::new();
        // Every other cell is plenty, and keeps droids from crowding onto neighbouring spots.
        for (x, z) in grid.walkable_cells().filter(|&(x, z)| x % 2 == 0 && z % 2 == 0) {
            let position = grid.on_floor(*origin, (x, z));
            let eyes = position + Vector3::y() * EYES_CROUCHED;
            let shields: Vec<_> = directions
                .iter()
                .copied()
                .filter(|dir| !Self::clear(graph, eyes, eyes + dir * 1.2))
                .collect();
            if !shields.is_empty() {
                covers.push(CoverSpot { position, shields });
            }
        }
        Log::info(format!("Team battle: {} cover spots", covers.len()));
        self.covers = covers;
    }

    // ------------------------------------------------------------------ the match

    fn start_match(&mut self, graph: &mut Graph) {
        for droid in self.droids.drain(..) {
            graph.remove_node(droid.root);
        }
        self.score = [0, 0];
        self.following = None;
        for team in [Team::Red, Team::Cyan] {
            for _ in 0..PER_TEAM {
                let droid = self.make_droid(graph, team);
                self.droids.push(droid);
            }
        }
        for i in 0..self.droids.len() {
            self.respawn(i);
        }
        self.intel = [vec![None; self.droids.len()], vec![None; self.droids.len()]];
    }

    fn make_droid(&mut self, graph: &mut Graph, team: Team) -> Droid {
        let mut material = Material::standard();
        material.set_property("diffuseColor", team.color());
        let material = MaterialResource::new_embedded(material);
        let mut dark = Material::standard();
        dark.set_property("diffuseColor", Color::opaque(30, 30, 34));
        let dark = MaterialResource::new_embedded(dark);
        let mut visor = Material::standard();
        visor.set_property("diffuseColor", Color::WHITE);
        visor.set_property(
            "emissionStrength",
            MaterialProperty::Vector3(Vector3::new(team.color().r as f32, team.color().g as f32, team.color().b as f32) / 255.0 * 3.0),
        );
        let visor = MaterialResource::new_embedded(visor);

        let part = |data: SurfaceData, material: &MaterialResource| {
            SurfaceBuilder::new(SurfaceResource::new_embedded(data))
                .with_material(material.clone())
                .build()
        };
        let at = |x: f32, y: f32, z: f32| Matrix4::new_translation(&Vector3::new(x, y, z));
        let scaled = |x: f32, y: f32, z: f32, sx: f32, sy: f32, sz: f32| {
            Matrix4::new_translation(&Vector3::new(x, y, z)) * Matrix4::new_nonuniform_scaling(&Vector3::new(sx, sy, sz))
        };
        let body = MeshBuilder::new(BaseBuilder::new())
            .with_surfaces(vec![
                // Torso and legs.
                part(SurfaceData::make_cylinder(12, 0.32, 1.25, true, &at(0.0, 0.0, 0.0)), &material),
                // Head, visor and gun.
                part(SurfaceData::make_sphere(12, 12, 0.24, &at(0.0, 1.45, 0.0)), &material),
                part(SurfaceData::make_cube(scaled(0.0, 1.47, 0.18, 0.34, 0.1, 0.12)), &visor),
                part(SurfaceData::make_cube(scaled(0.28, 1.1, 0.35, 0.1, 0.12, 0.7)), &dark),
            ])
            .build(graph)
            .to_base();
        let root = PivotBuilder::new(BaseBuilder::new().with_child(body)).build(graph).to_base();
        Droid {
            team,
            root,
            body,
            material,
            position: Vector3::zeros(),
            heading: 0.0,
            health: HEALTH,
            dead_for: None,
            route: Vec::new(),
            going_to: None,
            task: Task::Advance,
            crouched: false,
            target: None,
            objective: None,
            reload: 0.0,
            look_timer: 0.0,
            hurt_flash: 0.0,
            kills: 0,
            deaths: 0,
        }
    }

    fn respawn(&mut self, i: usize) {
        let team = self.droids[i].team.index();
        if self.spawns[team].is_empty() {
            return;
        }
        let count = self.spawns[team].len();
        let pick = self.rng().below(count);
        let cell = self.spawns[team][pick];
        let Some((grid, origin)) = &self.grid else { return };
        let position = grid.on_floor(*origin, cell);
        let droid = &mut self.droids[i];
        droid.position = position;
        // Facing the other end.
        droid.heading = if droid.team == Team::Red { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
        droid.health = HEALTH;
        droid.dead_for = None;
        droid.route.clear();
        droid.going_to = None;
        droid.task = Task::Advance;
        droid.crouched = false;
        droid.target = None;
        droid.objective = None;
    }

    // ------------------------------------------------------------------ the AI

    fn enemies_of(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let team = self.droids[i].team;
        (0..self.droids.len()).filter(move |&j| self.droids[j].team != team && self.droids[j].alive())
    }

    /// Whether droid `i` sees droid `j`: in front of it, near enough, and nothing in the way.
    fn sees(&self, graph: &Graph, i: usize, j: usize) -> bool {
        let (me, them) = (&self.droids[i], &self.droids[j]);
        let stance = if them.crouched { Stance::Crouching } else { Stance::Standing };
        // In a fight, everything nearby is noticed; calm, only what is in front.
        let sight = Sight { range: 45.0, cone: 70f32.to_radians(), ..Default::default() };
        let alert = me.target.map(|_| Alert::Alert);
        sight.could_see(alert, me.position, me.heading, them.position, stance, false)
            && (Self::clear(graph, me.eyes(), them.eyes()) || Self::clear(graph, me.eyes(), them.chest()))
    }

    fn think(&mut self, graph: &mut Graph, dt: f32) {
        for intel in self.intel.iter_mut().flatten().flatten() {
            intel.1 += dt;
        }
        for i in 0..self.droids.len() {
            if let Some(left) = self.droids[i].dead_for {
                if left - dt <= 0.0 {
                    self.respawn(i);
                } else {
                    self.droids[i].dead_for = Some(left - dt);
                }
                continue;
            }
            self.droids[i].reload -= dt;
            self.droids[i].look_timer -= dt;
            if self.droids[i].look_timer <= 0.0 {
                self.droids[i].look_timer = 0.15;
                self.look(graph, i);
            }
            self.decide(graph, i, dt);
            self.shoot(graph, i);
        }
    }

    /// Picks the nearest enemy droid `i` can see, and tells its team where they are.
    fn look(&mut self, graph: &Graph, i: usize) {
        let visible: Vec<usize> = self.enemies_of(i).filter(|&j| self.sees(graph, i, j)).collect();
        let team = self.droids[i].team.index();
        for &j in &visible {
            self.intel[team][j] = Some((self.droids[j].position, 0.0));
        }
        let me = self.droids[i].position;
        self.droids[i].target = visible
            .into_iter()
            .min_by(|&a, &b| {
                let d = |j: usize| (self.droids[j].position - me).norm();
                d(a).total_cmp(&d(b))
            });
    }

    /// Where droid `i` believes the nearest enemy is, from what its team has seen lately.
    fn nearest_known_enemy(&self, i: usize) -> Option<Vector3<f32>> {
        let me = &self.droids[i];
        self.intel[me.team.index()]
            .iter()
            .enumerate()
            .filter(|(j, _)| self.droids[*j].alive())
            .filter_map(|(_, intel)| intel.filter(|(_, age)| *age < INTEL_TIME).map(|(p, _)| p))
            .min_by(|a, b| (a - me.position).norm().total_cmp(&(b - me.position).norm()))
    }

    fn decide(&mut self, graph: &Graph, i: usize, dt: f32) {
        let threat = self.droids[i]
            .target
            .map(|j| self.droids[j].position)
            .or_else(|| self.nearest_known_enemy(i));
        let task = self.droids[i].task.clone();
        match task {
            Task::Advance => {
                if let (Some(_), Some(threat)) = (self.droids[i].target, threat) {
                    self.droids[i].crouched = false;
                    if !self.take_cover(graph, i, threat, false) {
                        let left = self.random((1.0, 2.0));
                        self.droids[i].task = Task::Standing { left };
                    }
                    return;
                }
                // Move up: to where an enemy was seen, or somewhere in the enemy half.
                let goal = match threat {
                    Some(threat) => threat,
                    None => self.objective(i),
                };
                self.walk_to(i, goal);
                if self.droids[i].route.is_empty() {
                    self.droids[i].objective = None;
                }
            }
            Task::ToCover { spot, peek } => {
                let position = self.covers[spot].position;
                self.walk_to(i, position);
                if (self.droids[i].position - position).norm() < 0.3 {
                    self.droids[i].crouched = true;
                    let left = self.random((0.6, 1.4));
                    self.droids[i].task = Task::Hidden { spot, peek, left };
                }
            }
            Task::Hidden { spot, peek, left } => {
                self.droids[i].crouched = true;
                let position = self.covers[spot].position;
                self.walk_to(i, position);
                // Flanked: an enemy can see it where it crouches. Find better cover.
                let exposed = self.enemies_of(i).any(|j| {
                    (self.droids[j].position - position).norm() < 40.0
                        && self.sees_spot(graph, j, position)
                });
                if exposed {
                    if let Some(threat) = threat {
                        self.take_cover(graph, i, threat, self.droids[i].health < RETREAT_HEALTH);
                    }
                    return;
                }
                if left - dt > 0.0 {
                    self.droids[i].task = Task::Hidden { spot, peek, left: left - dt };
                    return;
                }
                if threat.is_none() {
                    // Nobody about: back to looking for them.
                    self.droids[i].task = Task::Advance;
                    self.droids[i].crouched = false;
                } else {
                    let left = self.random((1.2, 2.2));
                    self.droids[i].task = Task::Peeking { spot, peek, left };
                    self.droids[i].crouched = false;
                }
            }
            Task::Peeking { spot, peek, left } => {
                self.droids[i].crouched = false;
                self.walk_to(i, peek);
                if left - dt > 0.0 {
                    self.droids[i].task = Task::Peeking { spot, peek, left: left - dt };
                    return;
                }
                if self.droids[i].target.is_none() && threat.is_none() {
                    self.droids[i].task = Task::Advance;
                    return;
                }
                if self.droids[i].target.is_none() {
                    // Nobody to shoot from here: push on to the next cover.
                    self.droids[i].task = Task::Advance;
                    return;
                }
                let left = self.random((0.5, 1.2));
                self.droids[i].crouched = true;
                self.droids[i].task = Task::Hidden { spot, peek, left };
            }
            Task::Standing { left } => {
                self.droids[i].route.clear();
                self.droids[i].going_to = None;
                if left - dt > 0.0 && self.droids[i].target.is_some() {
                    self.droids[i].task = Task::Standing { left: left - dt };
                } else {
                    self.droids[i].task = Task::Advance;
                }
            }
        }
    }

    /// Whether droid `j`, from where it stands, could see a crouched droid at `spot`.
    fn sees_spot(&self, graph: &Graph, j: usize, spot: Vector3<f32>) -> bool {
        let eyes = self.droids[j].eyes();
        Self::clear(graph, eyes, spot + Vector3::y() * EYES_CROUCHED)
    }

    /// Somewhere to head for in the enemy's half, kept until it is reached.
    fn objective(&mut self, i: usize) -> Vector3<f32> {
        if let Some(objective) = self.droids[i].objective {
            return objective;
        }
        let enemy = match self.droids[i].team {
            Team::Red => Team::Cyan,
            Team::Cyan => Team::Red,
        };
        let (theirs, own) = (enemy.index(), self.droids[i].team.index());
        if self.spawns[theirs].is_empty() || self.spawns[own].is_empty() {
            return self.droids[i].position;
        }
        // Somewhere between the middle and the enemy's end.
        let a = { let n = self.spawns[theirs].len(); self.rng().below(n) };
        let b = { let n = self.spawns[own].len(); self.rng().below(n) };
        let t = self.random((0.4, 0.9));
        let (spawns, own) = (&self.spawns[theirs], &self.spawns[own]);
        let Some((grid, origin)) = &self.grid else { return self.droids[i].position };
        let (theirs, ours) = (grid.center(*origin, spawns[a]), grid.center(*origin, own[b]));
        let point = ours + (theirs - ours) * t;
        let objective = grid
            .walkable_cell(*origin, point)
            .map_or(point, |cell| grid.on_floor(*origin, cell));
        self.droids[i].objective = Some(objective);
        objective
    }

    /// Picks cover for droid `i` against a threat at `threat`: a spot the threat cannot see a
    /// crouched droid at, from which it can shoot standing or a step to the side. `retreat` takes
    /// cover further away rather than nearer.
    fn take_cover(&mut self, graph: &Graph, i: usize, threat: Vector3<f32>, retreat: bool) -> bool {
        let Some((grid, origin)) = &self.grid else { return false };
        let me = self.droids[i].position;
        let Some(from) = grid.walkable_cell(*origin, me) else { return false };
        let routes = grid.routes_from(from, COVER_REACH);
        let taken: Vec<usize> = self
            .droids
            .iter()
            .enumerate()
            .filter(|(j, d)| *j != i && d.alive())
            .filter_map(|(_, d)| d.cover_spot())
            .collect();
        let threat_eyes = threat + Vector3::y() * EYES_STANDING;
        let now = self.droids[i].cover_spot();

        // Rank the reachable spots that face the threat, then check the best with rays.
        let mut candidates: Vec<(f32, usize)> = self
            .covers
            .iter()
            .enumerate()
            .filter(|(s, _)| !taken.contains(s) && Some(*s) != now)
            .filter_map(|(s, cover)| {
                let cell = grid.cell_at(*origin, cover.position)?;
                let cost = routes.costs[cell.1 * grid.width + cell.0]?;
                let to_threat = flat(threat - cover.position);
                let distance = to_threat.norm();
                if distance < 4.0 {
                    return None;
                }
                let dir = to_threat / distance;
                if !cover.shields.iter().any(|s| s.dot(&dir) > 0.92) {
                    return None;
                }
                // Best fought from 10-22 m; retreating, the further the better.
                let range = if retreat { -distance } else { (distance - 16.0).abs() };
                Some((cost * 0.5 + range, s))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));

        for &(_, s) in candidates.iter().take(24) {
            let spot = self.covers[s].position;
            if !Self::blocked(graph, spot + Vector3::y() * EYES_CROUCHED, threat_eyes).is_some() {
                continue;
            }
            // Low cover: shoot over it, standing.
            let standing = spot + Vector3::y() * EYES_STANDING;
            let peek = if Self::clear(graph, standing, threat_eyes) {
                Some(spot)
            } else {
                // High cover: step out to one side of it.
                let side = right_of(flat(threat - spot).normalize());
                [side, -side].into_iter().find_map(|side| {
                    let out = spot + side * 1.0;
                    let cell = grid.cell_at(*origin, out)?;
                    let out = grid.on_floor(*origin, cell);
                    (grid.is_walkable(cell.0, cell.1)
                        && Self::clear(graph, spot + Vector3::y() * EYES_STANDING, out + Vector3::y() * EYES_STANDING)
                        && Self::clear(graph, out + Vector3::y() * EYES_STANDING, threat_eyes))
                        .then_some(out)
                })
            };
            if let Some(peek) = peek {
                self.droids[i].task = Task::ToCover { spot: s, peek };
                return true;
            }
        }
        false
    }

    /// Sets droid `i` on its way to `goal`, planning a route only when the goal moves.
    fn walk_to(&mut self, i: usize, goal: Vector3<f32>) {
        let droid = &self.droids[i];
        if droid.going_to.is_some_and(|g| (g - goal).norm() < 1.0) && !droid.route.is_empty() {
            return;
        }
        if (droid.position - goal).norm() < 0.1 {
            self.droids[i].route.clear();
            return;
        }
        let Some((grid, origin)) = &self.grid else { return };
        let route = route_to((grid, *origin), droid.position, goal, f32::INFINITY);
        let droid = &mut self.droids[i];
        droid.route = route;
        droid.going_to = Some(goal);
    }

    fn shoot(&mut self, graph: &mut Graph, i: usize) {
        let Some(j) = self.droids[i].target else { return };
        if self.droids[i].reload > 0.0 || self.droids[i].crouched || !self.droids[j].alive() {
            return;
        }
        let (from, to) = (self.droids[i].eyes() - Vector3::y() * 0.35, self.droids[j].chest());
        // Only while facing them.
        let facing = heading_of(to - from).is_some_and(|h| angle_between(h, self.droids[i].heading) < 0.35);
        if !facing {
            return;
        }
        self.droids[i].reload = FIRE_INTERVAL * self.random((0.8, 1.6));
        let distance = (to - from).norm();
        let moving = !self.droids[i].route.is_empty();
        let mut chance = 0.75 - distance / 70.0;
        if moving {
            chance *= 0.5;
        }
        if !self.droids[j].route.is_empty() {
            chance *= 0.75;
        }
        let hit_cover = Self::blocked(graph, from, to);
        let hit = hit_cover.is_none() && self.random((0.0, 1.0)) < chance;
        // A miss goes a little wide.
        let end = match hit_cover {
            Some(along) => from + (to - from).normalize() * along,
            None if hit => to,
            None => {
                let wide = Vector3::new(self.random((-0.8, 0.8)), self.random((-0.4, 0.6)), self.random((-0.8, 0.8)));
                from + (to + wide - from).normalize() * (distance + 6.0)
            }
        };
        self.tracer(graph, from, end, self.droids[i].team);
        if hit {
            let damage = self.random(DAMAGE);
            let target = &mut self.droids[j];
            target.health -= damage;
            target.hurt_flash = 0.12;
            if target.health <= 0.0 {
                target.dead_for = Some(RESPAWN_TIME);
                target.deaths += 1;
                target.route.clear();
                self.droids[i].kills += 1;
                self.droids[i].target = None;
                self.score[self.droids[i].team.index()] += 1;
                Log::info(format!(
                    "Team battle: {} {} downs {} {} from {:.0} m, {} - {} : {}",
                    self.droids[i].team.name(),
                    i % PER_TEAM + 1,
                    self.droids[j].team.name(),
                    j % PER_TEAM + 1,
                    distance,
                    self.droids[i].task.label(),
                    self.score[0],
                    self.score[1]
                ));
                for intel in &mut self.intel {
                    intel[j] = None;
                }
            } else {
                // Shot at out in the open: get to cover. Badly hurt anywhere: fall back.
                let threat = self.droids[i].position;
                let hurt = self.droids[j].health < RETREAT_HEALTH;
                let exposed = matches!(self.droids[j].task, Task::Advance | Task::Standing { .. });
                if exposed || hurt {
                    self.take_cover(graph, j, threat, hurt);
                } else if let Task::Peeking { spot, peek, .. } = self.droids[j].task {
                    self.droids[j].crouched = true;
                    self.droids[j].task = Task::Hidden { spot, peek, left: 0.8 };
                }
            }
        }
    }

    fn tracer(&mut self, graph: &mut Graph, from: Vector3<f32>, to: Vector3<f32>, team: Team) {
        let way = to - from;
        let length = way.norm();
        if length < 0.01 {
            return;
        }
        let color = team.color();
        let mut material = Material::standard();
        material.set_property("diffuseColor", color);
        material.set_property(
            "emissionStrength",
            MaterialProperty::Vector3(Vector3::new(color.r as f32, color.g as f32, color.b as f32) / 255.0 * 6.0),
        );
        let rotation = UnitQuaternion::face_towards(&way, &Vector3::y());
        let node = MeshBuilder::new(
            BaseBuilder::new().with_cast_shadows(false).with_local_transform(
                TransformBuilder::new()
                    .with_local_position(from + way * 0.5)
                    .with_local_rotation(rotation)
                    .with_local_scale(Vector3::new(0.04, 0.04, length))
                    .build(),
            ),
        )
        .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(SurfaceData::make_cube(
            Matrix4::identity(),
        )))
        .with_material(MaterialResource::new_embedded(material))
        .build()])
        .build(graph)
        .to_base();
        self.tracers.push(Tracer { node, left: 0.07 });
    }

    fn update_tracers(&mut self, graph: &mut Graph, dt: f32) {
        self.tracers.retain_mut(|tracer| {
            tracer.left -= dt;
            if tracer.left <= 0.0 {
                graph.remove_node(tracer.node);
                false
            } else {
                true
            }
        });
    }

    // ------------------------------------------------------------------ moving and showing

    fn move_droids(&mut self, dt: f32) {
        for i in 0..self.droids.len() {
            if !self.droids[i].alive() {
                continue;
            }
            let running = matches!(self.droids[i].task, Task::ToCover { .. });
            let speed = if self.droids[i].crouched {
                WALK_SPEED * 0.5
            } else if running {
                RUN_SPEED
            } else {
                WALK_SPEED
            };
            let droid = &mut self.droids[i];
            let mut way = Vector3::zeros();
            let mut step = speed * dt;
            while let Some(&next) = droid.route.last() {
                let to = flat(next - droid.position);
                let distance = to.norm();
                if distance <= step {
                    droid.position = next;
                    droid.route.pop();
                    step -= distance;
                    way = to;
                } else {
                    droid.position += to / distance * step;
                    droid.position.y = next.y;
                    way = to;
                    break;
                }
            }
            // Face the target while there is one; otherwise, the way it is going.
            let target = droid.target.map(|j| j);
            let face = match target {
                Some(j) => heading_of(self.droids[j].position - self.droids[i].position),
                None => heading_of(way),
            };
            let droid = &mut self.droids[i];
            if let Some(face) = face {
                let turn = wrap(face - droid.heading);
                droid.heading += turn.clamp(-TURN_SPEED * dt, TURN_SPEED * dt);
            }
        }
    }

    fn show_droids(&mut self, graph: &mut Graph, dt: f32) {
        for droid in &mut self.droids {
            droid.hurt_flash -= dt;
            let color = if droid.hurt_flash > 0.0 { Color::WHITE } else { droid.team.color() };
            droid.material.data_ref().set_property("diffuseColor", color);
            let root = &mut graph[droid.root];
            root.set_visibility(droid.alive());
            root.local_transform_mut()
                .set_position(droid.position)
                .set_rotation(UnitQuaternion::from_axis_angle(&Vector3::y_axis(), droid.heading));
            let squash = if droid.crouched { 0.6 } else { 1.0 };
            graph[droid.body].local_transform_mut().set_scale(Vector3::new(1.0, squash, 1.0));
        }
    }

    fn move_camera(&mut self, graph: &mut Graph, dt: f32) {
        let rotation = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.yaw)
            * UnitQuaternion::from_axis_angle(&Vector3::x_axis(), self.pitch);
        if let Some(i) = self.following.filter(|&i| i < self.droids.len()) {
            // Behind and above the droid being followed, looking the way it faces.
            let droid = &self.droids[i];
            self.yaw = droid.heading;
            self.pitch = 20f32.to_radians();
            self.camera_position = droid.position + Vector3::y() * 3.0 - forward(droid.heading) * 5.0;
        } else {
            let pressed = |key| self.keys.contains(&key);
            let speed = if pressed(KeyCode::ShiftLeft) || pressed(KeyCode::ShiftRight) { 30.0 } else { 12.0 };
            let ahead = forward(self.yaw);
            let right = -right_of(ahead);
            let mut way = Vector3::zeros();
            let axes = [
                (KeyCode::KeyW, ahead),
                (KeyCode::KeyS, -ahead),
                (KeyCode::KeyD, -right),
                (KeyCode::KeyA, right),
                (KeyCode::KeyE, Vector3::y()),
                (KeyCode::KeyQ, -Vector3::y()),
            ];
            for (key, dir) in axes {
                if pressed(key) {
                    way += dir;
                }
            }
            if way.norm() > 0.0 {
                self.camera_position += way.normalize() * speed * dt;
            }
        }
        graph[self.camera]
            .local_transform_mut()
            .set_position(self.camera_position)
            .set_rotation(rotation);
    }

    fn set_text(&self, ctx: &mut PluginContext, text: Handle<Text>, value: String) {
        ctx.user_interfaces.first_mut().send(text, TextMessage::Text(value));
    }

    fn update_text(&self, ctx: &mut PluginContext) {
        if matches!(self.phase, Phase::Loading | Phase::Settling(_)) {
            return;
        }
        let alive = |team: Team| self.droids.iter().filter(|d| d.team == team && d.alive()).count();
        self.set_text(
            ctx,
            self.scoreboard,
            format!(
                "RED {}   —   {} CYAN\n{} / {} alive   first to {}",
                self.score[0],
                self.score[1],
                alive(Team::Red),
                alive(Team::Cyan),
                KILL_LIMIT
            ),
        );
        let banner = match self.phase {
            Phase::Won(team, _) => format!("{} TEAM WINS", team.name()),
            _ if self.paused => "PAUSED".to_string(),
            _ => match self.following.and_then(|i| self.droids.get(i)) {
                Some(d) => format!(
                    "Following {} droid {}   {:.0} hp   {} kills {} deaths   {}",
                    d.team.name(),
                    self.following.unwrap() % PER_TEAM + 1,
                    d.health.max(0.0),
                    d.kills,
                    d.deaths,
                    if d.alive() { d.task.label() } else { "down" }
                ),
                None => String::new(),
            },
        };
        self.set_text(ctx, self.banner, banner);
    }

    // ------------------------------------------------------------------ input

    fn on_event(&mut self, event: &Event<()>, ctx: &mut PluginContext) {
        match event {
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } if self.looking => {
                self.following = None;
                self.yaw -= delta.0 as f32 * 0.003;
                self.pitch = (self.pitch + delta.1 as f32 * 0.003).clamp(-1.5, 1.5);
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::MouseInput { button: MouseButton::Right, state, .. } => {
                    self.looking = *state == ElementState::Pressed;
                }
                WindowEvent::KeyboardInput { event: input, .. } => {
                    let PhysicalKey::Code(code) = input.physical_key else { return };
                    if input.state == ElementState::Released {
                        self.keys.retain(|&k| k != code);
                        return;
                    }
                    if !self.keys.contains(&code) {
                        self.keys.push(code);
                    }
                    if input.repeat {
                        return;
                    }
                    match code {
                        KeyCode::Escape => ctx.loop_controller.exit(),
                        KeyCode::Space => self.paused = !self.paused,
                        KeyCode::KeyF => self.following = None,
                        KeyCode::Tab if !self.droids.is_empty() => {
                            self.following = Some(self.following.map_or(0, |i| (i + 1) % self.droids.len()));
                        }
                        KeyCode::KeyR if self.grid.is_some() => {
                            self.start_match(&mut ctx.scenes[self.scene].graph);
                            self.phase = Phase::Playing;
                        }
                        _ => (),
                    }
                }
                WindowEvent::Focused(false) => {
                    self.keys.clear();
                    self.looking = false;
                }
                _ => (),
            },
            _ => (),
        }
    }
}

/// The colour a material is, if it says.
fn diffuse_color(material: &Material) -> Option<Color> {
    let key = ImmutableString::new("properties");
    let Some(MaterialResourceBinding::PropertyGroup(group)) = material.bindings().get(&key) else {
        return None;
    };
    match group.property_ref("diffuseColor") {
        Some(MaterialProperty::Color(color)) => Some(*color),
        _ => None,
    }
}

/// An angle brought into -π..π.
fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

fn angle_between(a: f32, b: f32) -> f32 {
    wrap(a - b).abs()
}

/// The right-hand side of `ahead`, along the ground.
fn right_of(ahead: Vector3<f32>) -> Vector3<f32> {
    hydroxus_ai::steer::right_of(ahead)
}
