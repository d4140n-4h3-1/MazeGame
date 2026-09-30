//! Team battle: red droids against cyan droids in the combat arena, `data/arena/combat_map.glb`.
//!
//! Five a side, all played by the AI, each one of the game's own droids (see
//! `maze::player::avatar`): red in the hostile droid's colours, `data/droid_hostile.glb`, and cyan
//! in the full deform droid's, `data/droid_full_deform.glb`, with every one of the game's droid's
//! animations: walking with no enemy about, jogging to one, running for cover and sprinting when
//! falling back hurt - skidding as they stop and turn - strafing and crouch-strafing while they
//! face a target, a jump now and then as they break for cover, the pistol drawn, raised, aimed up,
//! down and to the side at the target and fired - a glowing green bolt from its muzzle, the
//! game's own (see `maze::player::pistol`), which does its harm when it gets there - and
//! holstered once the fighting is over. Shot
//! down, a droid falls limp, knocked back by the shot, and may lose what it was hit in; it comes
//! back as a new droid. They cross the arena along the walk grid, see only in front
//! of them (hydroxus-ai's sight), and fight from cover: a crouched droid behind a low block or a
//! pillar is out of the line of fire, and stands up or steps round it to shoot. Hurt, a droid
//! falls back to cover further from the enemy. The dead come back at their own end of the arena
//! a few seconds later, and the first team to reach the kill limit wins the match.
//!
//! Each side fights as a team: it picks its targets - whoever is shooting at it, hurt, or out in
//! the open before whoever is merely nearest - takes cover from every enemy it knows of, not
//! just one, keeps out of sight of them as it moves and spreads out, sends some of its droids
//! round the side, leaves cover only while a teammate is firing, and fires at where an enemy
//! was just seen to keep their head down; a droid shot at like that stays down a moment.
//! `BATTLE_OLD=red` (or `cyan`, or `both`) has that side fight as they did before all that, one
//! droid at a time, to see the difference: each match ends by saying which side won.
//!
//! Run with `cargo run --example team_battle`.
//!
//! The camera is free: WASD to fly, Q and E down and up, Shift to go faster, the right mouse
//! button held to look round. Tab follows the next droid, F lets it go, Space pauses, R starts a
//! new match, Escape quits.

use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector2, Vector3},
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
        screen::ScreenBuilder,
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
        mesh::Mesh,
        node::Node,
        pivot::PivotBuilder,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        EnvironmentLightingSource, Scene,
    },
    window::WindowAttributes,
};
use hydroxus_ai::{prelude::*, route::route_to_weighted};
use maze::{
    dismember,
    player::{
        avatar::{Avatar, Going},
        pistol::{Bolts, BOLT_SPEED},
        posture::{Gait, Posture},
    },
    ragdoll::{self, Ragdoll},
};

const MAP: &str = "data/arena/combat_map.glb";
/// How often a droid breaking for cover jumps as it goes, out of 1; how fast it leaves the
/// ground, in meters per second, for a short and a high jump; and how fast it falls.
const JUMP_CHANCE: f32 = 0.25;
const JUMP_SPEED: (f32, f32) = (3.2, 4.4);
const GRAVITY: f32 = 9.81;
/// How far off the droid it is to hit a pistol can point, in radians, and the bolt still go onto
/// them; and pointed further off, how near their chest its way has to pass to hit them, in meters.
const AIM_SNAP: f32 = 0.1;
/// How much of how far over the barrel pointed at a shot each droid takes off what it asks for
/// next time, and the most it takes off either way, in radians.
const AIM_LEARNING: f32 = 0.5;
const AIM_BIAS_MOST: f32 = 0.6;
const HIT_RADIUS: f32 = 0.4;
/// How long, in seconds, a droid keeps its pistol out after it last had a target.
const HOLSTER_AFTER: f32 = 6.0;
/// The droids each team is made from: the same droid, in red and in cyan.
const MODELS: [&str; 2] = ["data/droid_hostile.glb", "data/droid_full_deform.glb"];
const PER_TEAM: usize = 5;
const KILL_LIMIT: u32 = 30;

/// Walk grid spacing, and how far from any wall a droid's middle keeps.
const CELL: f32 = 0.5;
const BODY_RADIUS: f32 = 0.4;

/// How fast a droid goes advancing and running for cover, in meters per second, if its model has
/// no jog or run to go by; with them it goes at their own pace, so its feet keep to the floor.
const WALK_SPEED: f32 = 1.8;
const RUN_SPEED: f32 = 3.0;
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
/// How far apart, in grid cells, the team works out which ground its enemies can see, and how
/// often, in seconds; how dear a route counts each step into ground one enemy can see, in
/// steps; and how far off an enemy sees that far, in meters.
const EXPOSURE_STEP: usize = 4;
const EXPOSURE_EVERY: f32 = 1.0;
const EXPOSURE_COST: f32 = 6.0;
const EXPOSURE_RANGE: f32 = 45.0;
/// How far round from the rest of its team a flanker goes, in radians, and how far from the
/// enemy it makes for, in meters.
const FLANK_ANGLE: f32 = 1.2;
const FLANK_DISTANCE: f32 = 14.0;
/// How near a teammate, in meters, makes cover crowded; and how much worse it counts cover that
/// is crowded, or seen by an enemy other than the one it is taken from, in the cover's terms.
const CROWDED: f32 = 3.0;
const CROWDED_COST: f32 = 4.0;
const SEEN_COST: f32 = 8.0;
/// How recently an enemy has to have been seen, in seconds, to fire at where it was; and how near
/// a bolt has to land or pass, in meters, to keep a droid down, and for how long, in seconds.
const SUPPRESS_SEEN: f32 = 3.0;
const SUPPRESS_NEAR: f32 = 2.0;
const SUPPRESSED_FOR: f32 = 1.5;
/// How far a droid has to get in how long, in meters and seconds, not to count as stuck.
const STUCK: (f32, f32) = (0.5, 3.0);
/// How far a droid caught without cover sidesteps, in meters.
const SIDESTEP: f32 = 2.5;

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
    /// The droid it is shown as.
    avatar: Avatar,
    position: Vector3<f32>,
    /// Where it was last frame, for how fast it is going, and how its feet are going there.
    was_at: Vector3<f32>,
    gait: Gait,
    /// Whether it is falling back, hurt, which it does at a sprint.
    retreating: bool,
    /// Whether it pulled the trigger this frame, and the shot it is waiting to fire.
    fired: bool,
    pending: Option<Shot>,
    /// How far its pistol's barrel has been pointing above and to the left of where it asked it
    /// to aim, in radians, as its last shots have shown: it asks that much the other way.
    aim_bias: (f32, f32),
    /// Where the chest of the target it has is, if one.
    target_at: Option<Vector3<f32>>,
    /// How long since it last had a target, in seconds: it holsters its pistol a while after.
    calm_for: f32,
    /// How high off the floor it is and how fast it is going up, while in a jump; whether it
    /// has just jumped, and whether a short jump.
    air: Option<(f32, f32)>,
    jumped: bool,
    low_jump: bool,
    /// Its body gone limp, while it lies where it fell; and whether its body is spent, to be made
    /// again when it comes back.
    ragdoll: Option<Ragdoll>,
    fallen: bool,
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
    /// How much longer it keeps its head down, shot at, in seconds.
    suppressed: f32,
    /// Which enemy it fires at to keep their head down, and where, while it does.
    suppress_at: Option<(usize, Vector3<f32>)>,
    /// Whether it goes round the side, and which side: 1 or -1.
    flank: Option<f32>,
    /// Where it was, and how long ago, to tell if it is stuck.
    stuck_from: (Vector3<f32>, f32),
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

/// A shot a droid has decided on, waiting for its pistol to fire: which droid it is at, where it
/// is going - onto them, into the cover in the way, or wide - whether it hits them, and for how
/// much; and how long it has waited.
#[derive(Debug, Clone, Copy)]
struct Shot {
    target: usize,
    end: Vector3<f32>,
    hits: bool,
    damage: f32,
    waited: f32,
}

/// A bolt on its way to the droid it hits: who fired it, at whom, for how much, from where to
/// where, and how long till it gets there.
#[derive(Debug, Clone, Copy)]
struct Arrival {
    shooter: usize,
    target: usize,
    damage: f32,
    from: Vector3<f32>,
    to: Vector3<f32>,
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
    /// The droids' models, red's and cyan's.
    models: [Option<ModelResource>; 2],
    level: Handle<Collider>,
    grid: Option<(WalkGrid, Vector3<f32>)>,
    /// Walkable cells at each end of the arena: red's and cyan's.
    spawns: [Vec<(usize, usize)>; 2],
    covers: Vec<CoverSpot>,
    droids: Vec<Droid>,
    /// Whether each team, red and cyan, fights as a team (see the top), or as it used to.
    smart: [bool; 2],
    /// For each team, how many of the enemies it knows of can see each coarse cell of the
    /// ground - every [`EXPOSURE_STEP`]th cell each way - and how long till it is worked out again.
    exposure: [Vec<u8>; 2],
    exposure_in: f32,
    /// The bolts everyone fires, and those on their way to someone they will hit.
    bolts: Option<Bolts>,
    arrivals: Vec<Arrival>,
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

        // Inside the arena, under its ceiling - the roof hides everything from above - in the
        // middle of one long side, looking across it with a team to either side.
        self.camera_position = Vector3::new(0.0, 4.6, -29.0);
        self.pitch = 22f32.to_radians();
        self.camera = CameraBuilder::new(BaseBuilder::new()).build(&mut scene.graph).to_base();
        self.scene = ctx.scenes.add(scene);
        self.map = Some(ctx.resource_manager.request::<Model>(MAP));
        let old = std::env::var("BATTLE_OLD").unwrap_or_default();
        self.smart = [
            !matches!(old.as_str(), "red" | "both"),
            !matches!(old.as_str(), "cyan" | "both"),
        ];
        Log::info(format!("Team battle: fighting as a team - red {}, cyan {}", self.smart[0], self.smart[1]));
        self.models = MODELS.map(|path| Some(ctx.resource_manager.request::<Model>(path)));

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
                .with_margin(Thickness::top(96.0))
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_font_size(24.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(&mut ui.build_ctx());
        self.help = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::uniform(12.0))
                .with_vertical_alignment(fyrox::gui::VerticalAlignment::Bottom)
                .with_foreground(Brush::Solid(Color::opaque(200, 200, 200)).into()),
        )
        .with_font_size(16.0.into())
        .with_vertical_text_alignment(fyrox::gui::VerticalAlignment::Bottom)
        .with_text(
            "WASD fly   Q/E down/up   Shift faster   Right mouse look\n\
             Tab follow a droid   F free camera   Space pause   R new match   Esc quit",
        )
        .build(&mut ui.build_ctx());
        // The UI's root gives its children only the size they ask for, in its corner; a screen is
        // the size of the window, so in one the scoreboard is at the top in the middle and the help
        // along the bottom.
        let children = [self.scoreboard, self.banner, self.help].map(|text| text.to_base());
        let ctx = &mut ui.build_ctx();
        ScreenBuilder::new(WidgetBuilder::new().with_children(children)).build(ctx);
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
                let models = self.models.clone().map(|model| model.filter(|m| !m.is_failed_to_load()));
                if map.is_failed_to_load() {
                    self.set_text(ctx, self.scoreboard, format!("Could not load {MAP}"));
                    self.map = None;
                } else if models.iter().any(Option::is_none) {
                    self.set_text(ctx, self.scoreboard, format!("Could not load {MODELS:?}"));
                    self.map = None;
                } else if map.is_ok() && models.iter().flatten().all(|m| m.is_ok()) {
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
                self.start_match(&mut ctx.scenes[self.scene]);
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
                    Log::info(format!(
                        "Team battle: match over, {} wins, RED {} : {} CYAN (as a team: red {}, cyan {})",
                        team.name(), self.score[0], self.score[1], self.smart[0], self.smart[1]
                    ));
                }
            }
            Phase::Won(team, left) if !self.paused => {
                if left - dt <= 0.0 {
                    self.start_match(&mut ctx.scenes[self.scene]);
                    self.phase = Phase::Playing;
                } else {
                    self.phase = Phase::Won(team, left - dt);
                }
            }
            _ => (),
        }
        if matches!(self.phase, Phase::Playing | Phase::Won(..)) {
            self.raise_the_fallen(&mut ctx.scenes[self.scene]);
        }
        for i in 0..self.droids.len() {
            let at = self.droids[i]
                .target
                .map(|j| self.droids[j].chest())
                .or(self.droids[i].suppress_at.map(|(_, at)| at));
            self.droids[i].target_at = at;
        }
        let graph = &mut ctx.scenes[self.scene].graph;
        let dt = if self.paused { 0.0 } else { dt };
        self.show_droids(graph, dt);
        self.fire_bolts(graph, dt);
        self.bolts_arrive(graph, dt);
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
        // Only the arena itself stands in the way: not a droid lying there, nor what it lost.
        let arena = |collider: Handle<Collider>| {
            graph
                .try_get::<Node>(collider.to_base())
                .ok()
                .and_then(|c: &Node| graph.try_get_of_type::<RigidBody>(c.parent()).ok())
                .is_some_and(|body| body.body_type() == RigidBodyType::Static)
        };
        hits.iter()
            .find(|hit| arena(hit.collider))
            .map(|hit| (hit.position.coords - from).norm())
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

    fn start_match(&mut self, scene: &mut Scene) {
        ragdoll::prepare(&mut scene.graph);
        match self.bolts.as_mut() {
            Some(bolts) => bolts.clear(&mut scene.graph),
            None => self.bolts = Some(Bolts::new(&mut scene.graph)),
        }
        self.arrivals.clear();
        for mut droid in self.droids.drain(..) {
            Self::clear_body(&mut scene.graph, &mut droid);
        }
        self.score = [0, 0];
        self.following = None;
        for team in [Team::Red, Team::Cyan] {
            for _ in 0..PER_TEAM {
                if let Some(droid) = self.make_droid(scene, team) {
                    self.droids.push(droid);
                }
            }
        }
        for i in 0..self.droids.len() {
            self.respawn(i);
        }
        self.intel = [vec![None; self.droids.len()], vec![None; self.droids.len()]];
    }

    /// One of the team's droids, made from its model, standing where it is put. None if the
    /// model is not the droid it should be.
    fn make_droid(&mut self, scene: &mut Scene, team: Team) -> Option<Droid> {
        let model = self.models[team.index()].clone()?;
        let root = PivotBuilder::new(BaseBuilder::new()).build(&mut scene.graph).to_base();
        // Its feet at the root; only the first droid says what it finds in the model.
        let quiet = !self.droids.is_empty();
        let Some(avatar) = Avatar::spawn(&model, scene, root, 0.0, quiet) else {
            scene.graph.remove_node(root);
            return None;
        };
        avatar.set_eyes(Some(team.color()));
        Some(Droid {
            team,
            root,
            avatar,
            position: Vector3::zeros(),
            was_at: Vector3::zeros(),
            gait: Gait::Jogging,
            retreating: false,
            fired: false,
            pending: None,
            aim_bias: (0.0, 0.0),
            target_at: None,
            calm_for: HOLSTER_AFTER,
            air: None,
            jumped: false,
            low_jump: false,
            ragdoll: None,
            fallen: false,
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
            suppressed: 0.0,
            suppress_at: None,
            flank: None,
            stuck_from: (Vector3::zeros(), 0.0),
        })
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
        droid.suppressed = 0.0;
        droid.suppress_at = None;
        droid.stuck_from = (droid.position, 0.0);
        // Two of each team's five go round the side, one each way.
        droid.flank = match i % PER_TEAM {
            1 => Some(1.0),
            3 => Some(-1.0),
            _ => None,
        };
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
        self.exposure_in -= dt;
        if self.exposure_in <= 0.0 {
            self.exposure_in = EXPOSURE_EVERY;
            self.survey_exposure(graph);
        }
        for droid in &mut self.droids {
            droid.suppressed = (droid.suppressed - dt).max(0.0);
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
        if !self.smart(i) {
            self.droids[i].target = visible.into_iter().min_by(|&a, &b| {
                let d = |j: usize| (self.droids[j].position - me).norm();
                d(a).total_cmp(&d(b))
            });
            return;
        }
        // Whoever is shooting at it first, then the hurt, the exposed and the near; and it keeps
        // to the one it has unless another is clearly better.
        let worth = |j: usize| {
            let them = &self.droids[j];
            let mut score = (them.position - me).norm() / 10.0 + them.health / HEALTH * 1.5;
            if them.target == Some(i) {
                score -= 1.5;
            }
            if !them.crouched && them.cover_spot().is_none() {
                score -= 0.8;
            }
            score
        };
        let best = visible.iter().copied().min_by(|&a, &b| worth(a).total_cmp(&worth(b)));
        let current = self.droids[i].target.filter(|j| visible.contains(j));
        self.droids[i].target = match (current, best) {
            (Some(current), Some(best)) if worth(current) <= worth(best) + 0.5 => Some(current),
            (_, best) => best,
        };
    }

    /// Whether droid `i`'s team fights as a team.
    fn smart(&self, i: usize) -> bool {
        self.smart[self.droids[i].team.index()]
    }

    /// Where each team's enemies, as it knows of them, can see: works out anew, for every coarse
    /// cell of the ground, how many of them could see a droid standing there.
    fn survey_exposure(&mut self, graph: &Graph) {
        let Some((grid, origin)) = &self.grid else { return };
        let (cols, rows) = (grid.width.div_ceil(EXPOSURE_STEP), grid.depth.div_ceil(EXPOSURE_STEP));
        for team in [Team::Red, Team::Cyan] {
            let t = team.index();
            let mut exposure = vec![0u8; cols * rows];
            if self.smart[t] {
                let seen: Vec<Vector3<f32>> = self.intel[t]
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| self.droids.get(*j).is_some_and(|d| d.alive()))
                    .filter_map(|(_, intel)| intel.filter(|(_, age)| *age < INTEL_TIME).map(|(p, _)| p))
                    .collect();
                for (c, value) in exposure.iter_mut().enumerate() {
                    let cell = ((c % cols) * EXPOSURE_STEP, (c / cols) * EXPOSURE_STEP);
                    if !grid.is_walkable(cell.0, cell.1) {
                        continue;
                    }
                    let at = grid.on_floor(*origin, cell) + Vector3::y() * CHEST_STANDING;
                    let seeing = seen
                        .iter()
                        .filter(|enemy| (**enemy - at).norm() < EXPOSURE_RANGE)
                        .filter(|enemy| Self::clear(graph, **enemy + Vector3::y() * EYES_STANDING, at))
                        .count();
                    *value = seeing.min(255) as u8;
                }
            }
            self.exposure[t] = exposure;
        }
    }

    /// How many enemies droid `i`'s team knows of can see the cell `(x, z)`, going by its nearest
    /// coarse cell.
    fn exposure_at(&self, team: usize, (x, z): (usize, usize)) -> u8 {
        let Some((grid, _)) = &self.grid else { return 0 };
        let cols = grid.width.div_ceil(EXPOSURE_STEP);
        let (cx, cz) = ((x + EXPOSURE_STEP / 2) / EXPOSURE_STEP, (z + EXPOSURE_STEP / 2) / EXPOSURE_STEP);
        self.exposure[team].get(cz * cols + cx).copied().unwrap_or(0)
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
                        if self.smart(i) {
                            self.sidestep(i, threat);
                        }
                    }
                    return;
                }
                // Move up: to where an enemy was seen - or, going round the side, off to one side
                // of it - or somewhere in the enemy half.
                let goal = match threat {
                    Some(threat) if self.smart(i) && self.droids[i].flank.is_some() => {
                        self.flank_point(i, threat).unwrap_or(threat)
                    }
                    Some(threat) => threat,
                    None => self.objective(i),
                };
                self.walk_to(i, goal);
                if self.droids[i].route.is_empty() {
                    self.droids[i].objective = None;
                }
                // Getting nowhere: somewhere else.
                if self.smart(i) {
                    let (from, since) = self.droids[i].stuck_from;
                    let here = self.droids[i].position;
                    if flat(here - from).norm() > STUCK.0 {
                        self.droids[i].stuck_from = (here, 0.0);
                    } else if since + dt > STUCK.1 {
                        self.droids[i].stuck_from = (here, 0.0);
                        self.droids[i].objective = None;
                        self.droids[i].going_to = None;
                        self.droids[i].route.clear();
                    } else {
                        self.droids[i].stuck_from = (from, since + dt);
                    }
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
                // Shot at, it keeps its head down.
                let down = self.smart(i) && self.droids[i].suppressed > 0.0;
                if left - dt > 0.0 || down {
                    self.droids[i].task = Task::Hidden { spot, peek, left: (left - dt).max(0.0) };
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
                let smart = self.smart(i);
                // With nobody in sight, it fires at where one was just seen, to keep them down.
                self.droids[i].suppress_at = if smart && self.droids[i].target.is_none() {
                    self.recently_seen(i)
                } else {
                    None
                };
                // Shot at, it ducks back down.
                if smart && self.droids[i].suppressed > 0.0 {
                    self.droids[i].suppress_at = None;
                    self.droids[i].crouched = true;
                    self.droids[i].task = Task::Hidden { spot, peek, left: 0.4 };
                    return;
                }
                if left - dt > 0.0 {
                    self.droids[i].task = Task::Peeking { spot, peek, left: left - dt };
                    return;
                }
                self.droids[i].suppress_at = None;
                if self.droids[i].target.is_none() && threat.is_none() {
                    self.droids[i].task = Task::Advance;
                    return;
                }
                if self.droids[i].target.is_none() {
                    // Nobody to shoot from here: push on to the next cover - fighting as a team,
                    // only while a teammate is firing to cover it, or it is on its own.
                    if smart && !self.covered(i) {
                        let left = self.random((0.6, 1.2));
                        self.droids[i].crouched = true;
                        self.droids[i].task = Task::Hidden { spot, peek, left };
                        return;
                    }
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

    /// Where droid `i`, going round the side, makes for against a `threat`: off to its side of the
    /// line from the rest of its team to the threat, [`FLANK_DISTANCE`] from it, on the ground.
    fn flank_point(&self, i: usize, threat: Vector3<f32>) -> Option<Vector3<f32>> {
        let side = self.droids[i].flank?;
        let team = self.droids[i].team;
        let mates: Vec<Vector3<f32>> = self
            .droids
            .iter()
            .filter(|d| d.team == team && d.alive())
            .map(|d| d.position)
            .collect();
        let middle = mates.iter().sum::<Vector3<f32>>() / mates.len().max(1) as f32;
        let from_threat = heading_of(middle - threat)?;
        let point = threat + forward(from_threat + side * FLANK_ANGLE) * FLANK_DISTANCE;
        let (grid, origin) = self.grid.as_ref()?;
        let cell = grid.nearest_walkable(*origin, point)?;
        // Close enough already: straight at them from here.
        let there = grid.on_floor(*origin, cell);
        (flat(there - self.droids[i].position).norm() > 3.0).then_some(there)
    }

    /// Has droid `i`, with no cover to be had, step aside across the line from `threat`, one way
    /// or the other, rather than stand there.
    fn sidestep(&mut self, i: usize, threat: Vector3<f32>) {
        let me = self.droids[i].position;
        let Some(across) = flat(threat - me).try_normalize(1.0e-3).map(right_of) else { return };
        let side = if self.random((0.0, 1.0)) < 0.5 { 1.0 } else { -1.0 };
        let Some((grid, origin)) = &self.grid else { return };
        let to = me + across * side * SIDESTEP;
        if let Some(cell) = grid.walkable_cell(*origin, to) {
            let to = grid.on_floor(*origin, cell);
            self.walk_to(i, to);
        }
    }

    /// The enemy droid `i`'s team last saw lately enough to fire at, and near enough, and where:
    /// the chest of one standing there.
    fn recently_seen(&self, i: usize) -> Option<(usize, Vector3<f32>)> {
        let me = self.droids[i].position;
        self.intel[self.droids[i].team.index()]
            .iter()
            .enumerate()
            .filter(|(j, _)| self.droids.get(*j).is_some_and(|d| d.alive()))
            .filter_map(|(j, intel)| intel.filter(|(_, age)| *age < SUPPRESS_SEEN).map(|(p, _)| (j, p)))
            .filter(|(_, p)| (p - me).norm() < EXPOSURE_RANGE)
            .min_by(|a, b| (a.1 - me).norm().total_cmp(&(b.1 - me).norm()))
            .map(|(j, p)| (j, p + Vector3::y() * CHEST_STANDING))
    }

    /// Whether droid `i` has a teammate firing to cover it as it moves - or no teammate near
    /// enough to, or it has waited long enough: it goes a quarter of the times it asks anyway.
    fn covered(&mut self, i: usize) -> bool {
        let me = self.droids[i].position;
        let team = self.droids[i].team;
        let mates: Vec<&Droid> = self
            .droids
            .iter()
            .enumerate()
            .filter(|(j, d)| *j != i && d.team == team && d.alive())
            .map(|(_, d)| d)
            .filter(|d| (d.position - me).norm() < 30.0)
            .collect();
        let firing = mates.iter().any(|d| {
            matches!(d.task, Task::Peeking { .. } | Task::Standing { .. })
                && (d.target.is_some() || d.suppress_at.is_some())
        });
        mates.is_empty() || firing || self.random((0.0, 1.0)) < 0.25
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

        // Fighting as a team, the other enemies it knows of, and where its teammates are: cover
        // they can see into, or crowded by a teammate, counts for less.
        let smart = self.smart(i);
        let others: Vec<Vector3<f32>> = if smart {
            self.intel[self.droids[i].team.index()]
                .iter()
                .enumerate()
                .filter(|(j, _)| self.droids.get(*j).is_some_and(|d| d.alive()))
                .filter_map(|(_, intel)| intel.filter(|(_, age)| *age < INTEL_TIME).map(|(p, _)| p))
                .filter(|p| flat(p - threat).norm() > 2.0)
                .map(|p| p + Vector3::y() * EYES_STANDING)
                .collect()
        } else {
            Vec::new()
        };
        let mates: Vec<Vector3<f32>> = if smart {
            let team = self.droids[i].team;
            self.droids
                .iter()
                .enumerate()
                .filter(|(j, d)| *j != i && d.team == team && d.alive())
                .map(|(_, d)| d.cover_spot().map_or(d.position, |s| self.covers[s].position))
                .collect()
        } else {
            Vec::new()
        };
        let mut best: Option<(f32, usize, Vector3<f32>)> = None;
        for &(rank, s) in candidates.iter().take(24) {
            let spot = self.covers[s].position;
            if !Self::blocked(graph, spot + Vector3::y() * EYES_CROUCHED, threat_eyes).is_some() {
                continue;
            }
            let crouched = spot + Vector3::y() * EYES_CROUCHED;
            let seen = others.iter().filter(|eyes| Self::clear(graph, **eyes, crouched)).count() as f32;
            let crowded = mates.iter().any(|m| flat(m - spot).norm() < CROWDED);
            let worth = rank + seen * SEEN_COST + if crowded { CROWDED_COST } else { 0.0 };
            if best.is_some_and(|(b, _, _)| b <= worth) {
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
            let Some(peek) = peek else { continue };
            best = Some((worth, s, peek));
            // As it used to, the first that will do.
            if !smart {
                break;
            }
        }
        if let Some((_, s, peek)) = best {
            self.droids[i].task = Task::ToCover { spot: s, peek };
            self.droids[i].retreating = retreat;
            // Now and then it jumps as it breaks for cover: a short hop or a high one.
            if self.droids[i].air.is_none() && self.random((0.0, 1.0)) < JUMP_CHANCE {
                let low = self.random((0.0, 1.0)) < 0.5;
                let speed = if low { JUMP_SPEED.0 } else { JUMP_SPEED.1 };
                let droid = &mut self.droids[i];
                droid.air = Some((0.0, speed));
                droid.jumped = true;
                droid.low_jump = low;
                droid.crouched = false;
            }
            return true;
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
        // Fighting as a team, the way that keeps out of the enemy's sight where it can.
        let team = droid.team.index();
        let route = if self.smart[team] && !self.exposure[team].is_empty() {
            route_to_weighted((grid, *origin), droid.position, goal, f32::INFINITY, |x, z| {
                self.exposure_at(team, (x, z)) as f32 * EXPOSURE_COST
            })
        } else {
            route_to((grid, *origin), droid.position, goal, f32::INFINITY)
        };
        let droid = &mut self.droids[i];
        droid.route = route;
        droid.going_to = Some(goal);
    }

    fn shoot(&mut self, graph: &mut Graph, i: usize) {
        // At its target, or with none in sight, at where an enemy was just seen, to keep them
        // down: that goes into their cover, or wide.
        let (j, keeping_down) = match (self.droids[i].target, self.droids[i].suppress_at) {
            (Some(j), _) => (j, None),
            (None, Some((j, at))) => (j, Some(at)),
            (None, None) => return,
        };
        if self.droids[i].reload > 0.0 || self.droids[i].crouched || !self.droids[j].alive() {
            return;
        }
        let (from, to) = (
            self.droids[i].eyes() - Vector3::y() * 0.35,
            keeping_down.unwrap_or_else(|| self.droids[j].chest()),
        );
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
        let hit = keeping_down.is_none() && hit_cover.is_none() && self.random((0.0, 1.0)) < chance;
        // A miss goes a little wide.
        let end = match hit_cover {
            Some(along) => from + (to - from).normalize() * along,
            None if hit => to,
            None => {
                let wide = Vector3::new(self.random((-0.8, 0.8)), self.random((-0.4, 0.6)), self.random((-0.8, 0.8)));
                from + (to + wide - from).normalize() * (distance + 6.0)
            }
        };
        // It pulls the trigger; the bolt leaves when the pistol fires (see `fire_bolts`), and
        // does its harm when it gets there (see `bolts_arrive`).
        let damage = self.random(DAMAGE);
        self.droids[i].fired = true;
        self.droids[i].pending = Some(Shot { target: j, end, hits: hit, damage, waited: 0.0 });
    }

    /// Lets go the bolt of each droid whose pistol fired this frame, from its muzzle, and flies
    /// every bolt on for `dt`: into what it hits of the arena, or onto the droid it is to hit.
    fn fire_bolts(&mut self, graph: &mut Graph, dt: f32) {
        let Some(mut bolts) = self.bolts.take() else { return };
        for i in 0..self.droids.len() {
            let Some(mut shot) = self.droids[i].pending else { continue };
            // From the muzzle as the pistol fires, the way it points; if it never does, from about
            // where it is held, the way the droid faces.
            let (from, pointing) = match self.droids[i].avatar.shot() {
                Some(shot) => shot,
                None if shot.waited > 0.6 || !self.droids[i].alive() => {
                    let from = self.droids[i].eyes() - Vector3::y() * 0.35 + forward(self.droids[i].heading) * 0.4;
                    (from, (shot.end - from).try_normalize(1.0e-3).unwrap_or_else(|| forward(self.droids[i].heading)))
                }
                None => {
                    shot.waited += dt;
                    self.droids[i].pending = Some(shot);
                    continue;
                }
            };
            self.droids[i].pending = None;
            if !self.droids[i].alive() {
                continue;
            }
            // The bolt goes the way the pistol points. Pointed near enough at the droid it was to
            // hit, it goes onto them; pointed further off, it hits them only if that way passes
            // them, and nothing of the arena is in between.
            let Some(pointing) = pointing.try_normalize(1.0e-3) else { continue };
            let chest = self.droids[shot.target].chest();
            let Some(aimed) = (chest - from).try_normalize(1.0e-3) else { continue };
            // How far over the barrel was, for it to ask that much less next time.
            if let (Some(pointed), Some(wanted)) = (heading_of(pointing), heading_of(aimed)) {
                let over = (
                    pointing.y.clamp(-1.0, 1.0).asin() - aimed.y.clamp(-1.0, 1.0).asin(),
                    wrap(pointed - wanted),
                );
                let bias = &mut self.droids[i].aim_bias;
                *bias = (
                    (bias.0 + AIM_LEARNING * over.0).clamp(-AIM_BIAS_MOST, AIM_BIAS_MOST),
                    (bias.1 + AIM_LEARNING * over.1).clamp(-AIM_BIAS_MOST, AIM_BIAS_MOST),
                );
            }
            let onto = |to: Vector3<f32>| Some((to - from).norm()).filter(|_| Self::clear(graph, from, to));
            let landing = if !shot.hits || !self.droids[shot.target].alive() {
                None
            } else if pointing.dot(&aimed) >= AIM_SNAP.cos() {
                onto(chest).map(|length| (aimed, length))
            } else {
                // Where that way passes nearest their chest, if it is near enough to hit them.
                let along = (chest - from).dot(&pointing);
                let nearest = from + pointing * along;
                (along > 0.0 && (nearest - chest).norm() < HIT_RADIUS)
                    .then(|| onto(nearest))
                    .flatten()
                    .map(|length| (pointing, length))
            };
            match landing {
                Some((way, length)) => {
                    // Onto them, glowing there as it lands.
                    bolts.fire_for(graph, from, way, length, true);
                    self.arrivals.push(Arrival {
                        shooter: i,
                        target: shot.target,
                        damage: shot.damage,
                        from,
                        to: from + way * length,
                        left: length / BOLT_SPEED,
                    });
                }
                None => {
                    // Wide by as much as the miss it was to be, or straight on if it was to hit:
                    // on till it hits the arena.
                    let wide = (shot.end - from).try_normalize(1.0e-3).map_or(Vector3::zeros(), |w| w - aimed);
                    let wide = if shot.hits { Vector3::zeros() } else { wide };
                    let way = (pointing + wide).try_normalize(1.0e-3).unwrap_or(pointing);
                    bolts.fire(graph, from, way);
                    // Anyone it passes or lands near keeps their head down a moment.
                    let reach = Self::blocked(graph, from, from + way * 100.0).unwrap_or(100.0);
                    let team = self.droids[i].team;
                    for them in self.droids.iter_mut().filter(|d| d.team != team && d.alive()) {
                        let chest = them.chest();
                        let along = (chest - from).dot(&way).clamp(0.0, reach);
                        if (from + way * along - chest).norm() < SUPPRESS_NEAR {
                            them.suppressed = SUPPRESSED_FOR;
                        }
                    }
                }
            }
        }
        // Only the arena stops a bolt: a droid it misses, it passes.
        bolts.fly(graph, dt, |graph, from, way, reach| {
            let end = from + way * reach;
            Self::blocked(graph, from, end).map(|along| (along, Handle::NONE))
        });
        self.bolts = Some(bolts);
    }

    /// Does the harm of each bolt that has got to the droid it hits.
    fn bolts_arrive(&mut self, graph: &mut Graph, dt: f32) {
        let mut arrived = Vec::new();
        self.arrivals.retain_mut(|arrival| {
            arrival.left -= dt;
            let here = arrival.left <= 0.0;
            if here {
                arrived.push(*arrival);
            }
            !here
        });
        for arrival in arrived {
            let Arrival { shooter: i, target: j, damage, from, to, .. } = arrival;
            if self.droids[j].alive() {
                self.hit(graph, i, j, damage, from, to);
            }
        }
    }

    /// Droid `i`'s bolt, fired from `from`, gets to droid `j` at `to`, for `damage`.
    fn hit(&mut self, graph: &mut Graph, i: usize, j: usize, damage: f32, from: Vector3<f32>, to: Vector3<f32>) {
        let distance = (to - from).norm();
        let target = &mut self.droids[j];
        target.health -= damage;
        target.hurt_flash = 0.12;
        if target.health <= 0.0 {
            target.dead_for = Some(RESPAWN_TIME);
            // It goes limp, knocked back by the shot, and may lose what it was hit in.
            let way = (to - from).normalize();
            target.avatar.set_eyes(Some(Color::BLACK));
            let going = if target.route.is_empty() { Vector3::zeros() } else { forward(target.heading) * 2.0 };
            target.ragdoll = Ragdoll::start(
                graph,
                target.avatar.root(),
                going,
                Some((way * ragdoll::STOPPING_BLOW, to)),
            );
            target.fallen = true;
            target.air = None;
            if let Some(ragdoll) = target.ragdoll.as_mut() {
                if let Some(body) = ragdoll.body_struck(graph, to, way) {
                    if let Some(part) = dismember::break_for(body) {
                        if target.avatar.break_off(graph, part) {
                            ragdoll.let_loose(graph, part);
                        }
                    }
                }
            }
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



    // ------------------------------------------------------------------ moving and showing

    fn move_droids(&mut self, dt: f32) {
        for i in 0..self.droids.len() {
            if !self.droids[i].alive() {
                continue;
            }
            // At the pace of what its feet are doing: jogging as it advances, running for cover,
            // and crouched, the crouch walk.
            // Walking with no enemy known of, jogging to one, running for cover, sprinting when
            // falling back hurt.
            let running = matches!(self.droids[i].task, Task::ToCover { .. });
            let knows = self.nearest_known_enemy(i).is_some() || self.droids[i].target.is_some();
            let (posture, gait, fallback) = if self.droids[i].crouched {
                (Posture::Crouching, Gait::Walking, WALK_SPEED * 0.5)
            } else if running && self.droids[i].retreating {
                (Posture::Standing, Gait::Sprinting, RUN_SPEED * 1.4)
            } else if running {
                (Posture::Standing, Gait::Running, RUN_SPEED)
            } else if knows {
                (Posture::Standing, Gait::Jogging, WALK_SPEED)
            } else {
                (Posture::Standing, Gait::Walking, WALK_SPEED * 0.5)
            };
            let droid = &mut self.droids[i];
            droid.gait = gait;
            let speed = droid.avatar.pace(posture, gait).unwrap_or(fallback);
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
            let face = match (target, droid.suppress_at) {
                (Some(j), _) => heading_of(self.droids[j].position - self.droids[i].position),
                (None, Some((_, at))) => heading_of(at - self.droids[i].position),
                (None, None) => heading_of(way),
            };
            let droid = &mut self.droids[i];
            if let Some(face) = face {
                let turn = wrap(face - droid.heading);
                droid.heading += turn.clamp(-TURN_SPEED * dt, TURN_SPEED * dt);
            }
        }
    }

    /// Takes away a droid's body - its model, and its limp bodies and what it lost, if it fell.
    fn clear_body(graph: &mut Graph, droid: &mut Droid) {
        if let Some(ragdoll) = droid.ragdoll.take() {
            ragdoll.remove(graph);
        }
        droid.avatar.sweep_up(graph);
        if graph.is_valid_handle(droid.root) {
            graph.remove_node(droid.root);
        }
    }

    /// Gives each droid that has come back, but still lies where it fell, a new body.
    fn raise_the_fallen(&mut self, scene: &mut Scene) {
        for i in 0..self.droids.len() {
            if !(self.droids[i].fallen && self.droids[i].alive()) {
                continue;
            }
            let team = self.droids[i].team;
            let Some(new) = self.make_droid(scene, team) else { continue };
            let droid = &mut self.droids[i];
            Self::clear_body(&mut scene.graph, droid);
            droid.root = new.root;
            droid.avatar = new.avatar;
            droid.fallen = false;
            droid.was_at = droid.position;
        }
    }

    fn show_droids(&mut self, graph: &mut Graph, dt: f32) {
        for droid in &mut self.droids {
            // Lying where it fell, the physics has it.
            if let Some(ragdoll) = droid.ragdoll.as_mut().filter(|_| droid.fallen) {
                ragdoll.update(graph);
                continue;
            }
            droid.hurt_flash -= dt;
            // Its eyes in its team's colour, and white for a moment when hit.
            let eyes = if droid.hurt_flash > 0.0 { Color::WHITE } else { droid.team.color() };
            droid.avatar.set_eyes(Some(eyes));
            // In a jump, up and back down onto the floor.
            let mut falling = 0.0;
            if let Some((height, up)) = droid.air {
                let (height, up) = (height + up * dt, up - GRAVITY * dt);
                falling = (-up).max(0.0);
                droid.air = (height > 0.0).then_some((height, up));
            }
            let lift = droid.air.map_or(0.0, |(height, _)| height);
            // Its root turned the way it faces, as the player's body is turned the way they look:
            // the droid takes which way it goes, and faces, from there.
            graph[droid.root]
                .local_transform_mut()
                .set_position(droid.position + Vector3::y() * lift)
                .set_rotation(UnitQuaternion::from_axis_angle(&Vector3::y_axis(), droid.heading));
            // How fast it went since last frame, which is what its feet keep pace with.
            let moved = flat(droid.position - droid.was_at);
            let speed = if dt > 0.0 { moved.norm() / dt } else { 0.0 };
            // Coming back to life is a jump across the arena, not a step.
            let (speed, moved) = if speed > RUN_SPEED * 2.0 { (0.0, Vector3::zeros()) } else { (speed, moved) };
            droid.was_at = droid.position;
            // The pistol out while fighting, raised and aimed at the target it has, and put away
            // a while after the last.
            droid.calm_for = if droid.target.is_some() { 0.0 } else { droid.calm_for + dt };
            let target = droid.target_at;
            let raised = target.is_some();
            // Up, and to the left of ahead, to the target.
            let look = target.map_or((0.0, 0.0), |at| {
                let to = at - (droid.position + Vector3::y() * if droid.crouched { EYES_CROUCHED } else { EYES_STANDING });
                let up = (to.y / to.norm().max(1.0e-3)).clamp(-1.0, 1.0).asin();
                let left = heading_of(to).map_or(0.0, |h| wrap(h - droid.heading));
                // Less what its barrel has been pointing over by.
                (up - droid.aim_bias.0, left - droid.aim_bias.1)
            });
            // Facing a target, it strafes: going which way it goes, facing where it faces. Which
            // way it goes is from the way it faces.
            let strafing = raised && speed > 0.1;
            let way = heading_of(moved).map_or(0.0, |h| wrap(h - droid.heading));
            let going = Going {
                heading: Some(if strafing || speed > 0.1 { way } else { 0.0 }),
                speed,
                posture: if droid.crouched { Posture::Crouching } else { Posture::Standing },
                gait: droid.gait,
                grounded: droid.air.is_none(),
                jumped: std::mem::take(&mut droid.jumped),
                low: droid.low_jump,
                falling,
                cover: false,
                pushing: speed > 0.1,
                strafing,
                armed: droid.calm_for < HOLSTER_AFTER,
                trigger: std::mem::take(&mut droid.fired),
                raised,
                look,
                way,
            };
            droid.avatar.animate(graph, going, dt);
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
                            self.start_match(&mut ctx.scenes[self.scene]);
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
