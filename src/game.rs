//! The game itself: loading a level, playing rounds in it, and the player's input.

use crate::player::{
    pistol::{Bolts, Strike},
    posture::Posture,
};
use fyrox::{
    core::algebra::Point3,
    scene::{collider::Collider, graph::physics::RayCastOptions},
};
use crate::{
    credits::Credits,
    computer::{self, Beeps, Computer, ScreenTerminal, Terminal, COMPUTER_MODEL},
    ctf::{Bases, Side, MAPS, PLAYERS},
    firewall::{self, Firewalls},
    drone::{self, Drone, DroneLines, State, Target, DRONE_LINES, DRONE_MODEL},
    drone_shot::{Shots, SHOT_MODEL},
    alarm::AlarmSound,
    health::{Health, HealthSounds, Healing, Heard},
    hearts::{self, Hearts, HEART_MODEL},
    notes::{self, Notes, NOTES},
    diagnostics::{self, FrameStats},
    dialogue::{
        screen::{self, DialogueScreen, Pointer, Subtitles},
        Bark, Conversation, Facts, Mood, Provoked, Script, CTF_SCRIPT, SCRIPT,
    },
    formants::{
        self,
        chirps::{Chirps, DRONE_VOICE},
        speech::{Voices, VOICES},
        synth,
    },
    generate::Maze,
    hud::{self, Hud, Status},
    inhabitants::{Alert, Inhabitants, Livery, News, Threat, HOSTILE_MODEL},
    layout::{self, Rng},
    level::Level,
    menu::{Choice, Game, MainMenu, PauseMenu, Start},
    platform,
    player::{Player, DROID_MODEL},
    survey,
    tiles::{self, Measured, Prefabs},
};
use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector2, Vector3},
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
        sound::{Sound, SoundBuilder, Status as SoundStatus},
        transform::TransformBuilder,
        EnvironmentLightingSource, Scene,
    },
    window::CursorGrabMode,
};

/// How many junctions wide and deep a maze is, unless MAZE_SIZE says otherwise.
const MAZE_SIZE: (usize, usize) = (20, 20);
/// How many computers there are in a maze, to hack and read the notes on.
const COMPUTERS: usize = 6;
/// The chance of a dead end being opened into a neighbouring corridor, which makes loops.
const LOOP_CHANCE: f32 = 0.15;
/// How long a droid in capture the flag keeps its head turned to the player it says something
/// to, in seconds.
const CHAT_LOOK: f32 = 3.5;

/// In capture the flag, the side the player takes the flag from.
const ENEMY: Side = PLAYERS.other();

/// How close to the exit counts as reaching it.
const EXIT_RADIUS: f32 = 1.5;
/// In capture the flag, how close to the middle of the player's own flag counts as having
/// brought the enemy's home - from beside its plinth - and how high over a flag the marker shows where to go.
const HOME_REACH: f32 = 2.5;
const FLAG_MARKER: f32 = 2.0;
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
/// How long the player is shown they were deleted before the next maze, in seconds.
const DELETED_FOR: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    /// The main menu, before a game has been picked.
    #[default]
    Title,
    /// Waiting for the maze model.
    Loading,
    /// The model is in the scene; its colliders exist from the next physics step on.
    Settling(u8),
    Playing,
    Won,
    /// A hostile droid caught the player; the next maze comes once the time is up.
    Deleted,
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

/// Something a droid says out loud by itself, being made into a voice: which droid, as an index
/// into the inhabitants.
#[derive(Debug, PartialEq)]
struct Barking {
    droid: usize,
    making: Making,
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

/// How high the droid with `code` speaks, feeling `mood`, as a part of its kind's pitch: each a
/// little higher or lower than the rest of its kind, and always the same; and higher or lower
/// again with how it feels.
fn pitch(voices: &Voices, code: u32, mood: Mood) -> f32 {
    (1.0 + VOICE_SPREAD * ((code % 7) as f32 / 3.0 - 1.0)) * voices.mood_pitch(mood)
}

/// How many drones there are: one patrolling each round, the rest waiting out of sight to be
/// called in by a failed hack.
const DRONES: usize = 4;

/// The bars over the sentries' heads, and the drone's: how far above the face or the top of the
/// drone, and how wide, in meters, and how far off, in meters, they are still shown.
const BREATH_BAR_ABOVE: f32 = 0.3;
const BREATH_BAR_WIDTH: f32 = 0.6;
const BREATH_BAR_REACH: f32 = 25.0;

/// Whether droids of the `character`th kind in `script` are sentries: the kind that goes after
/// the player itself when provoked, rather than sounding the alarm.
fn is_sentry(script: &Script, character: usize) -> bool {
    script
        .characters
        .get(character)
        .and_then(|character| character.threatened)
        .is_some_and(|threatened| threatened.then == Provoked::Attacks)
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
    /// The glowing frames of the computers, for the effects to light with.
    #[visit(skip)]
    #[reflect(hidden)]
    area_lights: fyrox_gfx::AreaLights,
    /// The droid the player is seen as, until it has loaded and joined the player.
    #[visit(skip)]
    #[reflect(hidden)]
    droid: Option<ModelResource>,
    /// The same droid, loaded, for the maze's inhabitants of any kind whose own model is not.
    #[visit(skip)]
    #[reflect(hidden)]
    droid_model: Option<ModelResource>,
    /// Each kind of droid's own model, by its index into the conversations' characters, and the
    /// droid in the colours of one after the player.
    #[visit(skip)]
    #[reflect(hidden)]
    kind_models: Vec<Option<ModelResource>>,
    #[visit(skip)]
    #[reflect(hidden)]
    hostile_model: Option<ModelResource>,
    /// Droids going about the maze by themselves.
    #[visit(skip)]
    #[reflect(hidden)]
    inhabitants: Inhabitants,
    exit: Handle<Node>,
    sun: Handle<Node>,
    /// Whether the player has switched the maze's lights off, leaving the flashlight to see by.
    /// It stays that way from one maze to the next.
    lights_off: bool,
    /// Which of what the droids say the player has on screen: the System Latin, the English, or
    /// both, as usual.
    #[visit(skip)]
    #[reflect(hidden)]
    subtitles: Subtitles,
    #[visit(skip)]
    #[reflect(hidden)]
    rng: Option<Rng>,
    #[visit(skip)]
    #[reflect(hidden)]
    phase: Phase,
    round_time: f32,
    /// Whether MAZE_KNOCKDOWN has shot a droid down this round yet.
    knocked_down: bool,
    best_time: Option<f32>,
    /// The credits the player has taken from computers, kept from one maze to the next.
    #[visit(skip)]
    #[reflect(hidden)]
    credits: Credits,
    /// How long ago the player was deleted, in seconds.
    #[visit(skip)]
    #[reflect(hidden)]
    deleted: f32,
    #[visit(skip)]
    #[reflect(hidden)]
    hud: Hud,
    /// The pause menu. While it is open the world stands still.
    #[visit(skip)]
    #[reflect(hidden)]
    menu: PauseMenu,
    /// The main menu, where a game is picked; and the path of the fixed maze model being played,
    /// if one is.
    #[visit(skip)]
    #[reflect(hidden)]
    main_menu: MainMenu,
    #[visit(skip)]
    #[reflect(hidden)]
    model_path: String,
    /// What the droids say, if it could be read, and how they sound saying it.
    #[visit(skip)]
    #[reflect(hidden)]
    script: Option<Script>,
    /// Which file `script` is from.
    #[visit(skip)]
    #[reflect(hidden)]
    script_path: &'static str,
    #[visit(skip)]
    #[reflect(hidden)]
    voices: Option<Voices>,
    /// The conversation under way, if there is one; while it is, the clock and the player
    /// stand still.
    #[visit(skip)]
    #[reflect(hidden)]
    talking: Option<Talking>,
    /// What droids are saying by themselves, while it is being made into a voice.
    #[visit(skip)]
    #[reflect(hidden)]
    barks: Vec<Barking>,
    /// The droid the player could talk to right now, as an index into the inhabitants, and who
    /// it is as the hint to talk names it.
    #[visit(skip)]
    #[reflect(hidden)]
    talkable: Option<(usize, String)>,
    #[visit(skip)]
    #[reflect(hidden)]
    dialogue: DialogueScreen,
    /// The computers to hack, as their model loads and once they are in the scene - the first
    /// near the cell the player started from, the rest about the maze; whether they have been put
    /// down in this round; whether the player is using one; and which they are close enough to
    /// use, if any, and whether it is cleared, as the hint on screen says.
    #[visit(skip)]
    #[reflect(hidden)]
    computer_model: Option<ModelResource>,
    #[visit(skip)]
    #[reflect(hidden)]
    computers: Vec<Computer>,
    /// The firewalls round the flags, in a maze model for capture the flag, each opened by one
    /// of the computers.
    #[visit(skip)]
    #[reflect(hidden)]
    firewalls: Firewalls,
    /// In capture the flag, where the flags are - None in the maze; and each side's bolts, red's
    /// and blue's, from its droids' pistols.
    #[visit(skip)]
    #[reflect(hidden)]
    ctf: Option<Bases>,
    #[visit(skip)]
    #[reflect(hidden)]
    side_bolts: Option<[Bolts; 2]>,
    /// In capture the flag, which of its chatter the droid talked to last said.
    #[visit(skip)]
    #[reflect(hidden)]
    last_chat: Option<usize>,
    /// In capture the flag, whether the player has the enemy's flag, to bring home.
    #[visit(skip)]
    #[reflect(hidden)]
    carrying: bool,
    /// The drone, as its model loads and once it is in the scene, and whether it has been put in
    /// front of the player this round.
    #[visit(skip)]
    #[reflect(hidden)]
    drone_model: Option<ModelResource>,
    /// The hearts floating in the corridors: their model as it loads and once it has, the hearts
    /// themselves, and whether they have been put in this round's maze.
    #[visit(skip)]
    #[reflect(hidden)]
    heart_model: Option<ModelResource>,
    #[visit(skip)]
    #[reflect(hidden)]
    hearts: Hearts,
    /// The player's health this round, and how it is heard.
    #[visit(skip)]
    #[reflect(hidden)]
    health: Health,
    #[visit(skip)]
    #[reflect(hidden)]
    health_sounds: HealthSounds,
    /// The alarm's klaxon.
    #[visit(skip)]
    #[reflect(hidden)]
    alarm_sound: AlarmSound,
    #[visit(skip)]
    #[reflect(hidden)]
    hearts_placed: bool,
    #[visit(skip)]
    #[reflect(hidden)]
    drones: Vec<Drone>,
    /// In capture the flag, what each drone went after last.
    #[visit(skip)]
    #[reflect(hidden)]
    drone_targets: Vec<Option<Target>>,
    #[visit(skip)]
    #[reflect(hidden)]
    drone_placed: bool,
    /// Its shots, as their model loads and once they are made.
    #[visit(skip)]
    #[reflect(hidden)]
    shot_model: Option<ModelResource>,
    #[visit(skip)]
    #[reflect(hidden)]
    shots: Option<Shots>,
    /// How drones sound and what they say, how many lines it has said, and the one being made
    /// into sound to say, if any.
    #[visit(skip)]
    #[reflect(hidden)]
    drone_voice: Option<(Chirps, DroneLines)>,
    #[visit(skip)]
    #[reflect(hidden)]
    drone_said: usize,
    /// Whether MAZE_DRONE_ALARM has sent the drone after the player this round.
    #[visit(skip)]
    #[reflect(hidden)]
    drone_alarmed: bool,
    /// What each drone has to say about something that happened to it outside its own update, by
    /// its place among the drones; which is saying the line being made into sound; and where
    /// drones have been called in to since the last update, to be sent there.
    #[visit(skip)]
    #[reflect(hidden)]
    drone_says: Vec<(usize, &'static str)>,
    #[visit(skip)]
    #[reflect(hidden)]
    drone_speaking: usize,
    #[visit(skip)]
    #[reflect(hidden)]
    drone_calls: Vec<Vector3<f32>>,
    #[visit(skip)]
    #[reflect(hidden)]
    drone_saying: Option<Making>,
    /// Which the player is using, or last used, whose terminal shows.
    #[visit(skip)]
    #[reflect(hidden)]
    using: Option<usize>,
    /// The notes and diary entries shared out among them each round.
    #[visit(skip)]
    #[reflect(hidden)]
    notes: Option<Notes>,
    /// Its terminal, on its screen while the player uses it - or, with MAZE_TERMINAL_OVERLAY=1,
    /// over it.
    #[visit(skip)]
    #[reflect(hidden)]
    terminal: Terminal,
    #[visit(skip)]
    #[reflect(hidden)]
    screen_terminal: ScreenTerminal,
    #[visit(skip)]
    #[reflect(hidden)]
    computer_placed: bool,
    /// Whether MAZE_COMPUTER is to put the player at it, now that it has been placed.
    #[visit(skip)]
    #[reflect(hidden)]
    computer_test: bool,
    #[visit(skip)]
    #[reflect(hidden)]
    start_cell: Option<(usize, usize)>,
    #[visit(skip)]
    #[reflect(hidden)]
    hacking: bool,
    #[visit(skip)]
    #[reflect(hidden)]
    at_computer: Option<(usize, bool)>,
    /// Whether Shift is held, for telling what the player meant to type from what Caps Lock made
    /// of it.
    #[visit(skip)]
    #[reflect(hidden)]
    shift: bool,
    /// The sounds that were playing as the game was paused, to carry on with once it resumes.
    #[visit(skip)]
    #[reflect(hidden)]
    paused_sounds: Vec<Handle<Node>>,
    #[visit(skip)]
    #[reflect(hidden)]
    stats: FrameStats,
    mouse_captured: bool,
    /// Whether the browser had the mouse locked, the last time it was asked. Unused on the desktop.
    browser_locked: bool,
    /// Whether the mouse should be captured. Capturing can fail while the window is still
    /// appearing, so it is retried until it works.
    want_mouse: bool,
    focused: bool,
}

impl MazeGame {
    /// The game, telling the graphics effects what moves through `moving`, and what glows through
    /// `area_lights`.
    pub fn new(moving: fyrox_gfx::MovingThings, area_lights: fyrox_gfx::AreaLights) -> Self {
        Self {
            moving,
            area_lights,
            health_sounds: HealthSounds::make(),
            alarm_sound: AlarmSound::make(),
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
        self.computer_model = Some(resources.request::<Model>(COMPUTER_MODEL));
        self.drone_model = Some(resources.request::<Model>(DRONE_MODEL));
        self.shot_model = Some(resources.request::<Model>(SHOT_MODEL));
        self.heart_model = Some(resources.request::<Model>(HEART_MODEL));
    }

    /// Starts `game`, picked in the main menu: loads its level, and plays it once it has. The
    /// maze is random, unless MAZE_MODEL gives a model to play instead.
    fn play(&mut self, ctx: &mut PluginContext, game: Game) {
        let resources = &ctx.resource_manager;
        let model = match game {
            Game::CaptureTheFlag(map) => Some(MAPS[map].path.to_string()),
            Game::Maze => platform::var("MAZE_MODEL"),
        };
        match model {
            Some(path) => {
                self.model = Some(resources.request::<Model>(&path));
                self.firewalls = Firewalls::request(resources);
                self.model_path = path;
            }
            None => self.prefabs = Some(Prefabs::request(resources)),
        }
        Log::info(format!("Playing {game:?}"));
        self.main_menu.set_open(ctx.user_interfaces.first(), false);
        self.phase = Phase::Loading;
        self.want_mouse = true;
    }

    /// Leaves the game under way for the main menu: everything in it is taken out, and the
    /// next game picked starts afresh.
    fn to_main_menu(&mut self, ctx: &mut PluginContext) {
        self.set_paused(ctx, false);
        self.stop_talking(ctx);
        self.stop_hacking(ctx);
        self.barks.clear();
        let scene = &mut ctx.scenes[self.scene];
        self.inhabitants.clear(&mut scene.graph);
        self.firewalls.clear(&mut scene.graph);
        for computer in &mut self.computers {
            computer.hide(&mut scene.graph);
        }
        for drone in &mut self.drones {
            drone.hide(&mut scene.graph);
        }
        self.hearts.clear(&mut scene.graph);
        if let Some(shots) = self.shots.as_mut() {
            shots.clear(&mut scene.graph);
        }
        for bolts in self.side_bolts.iter_mut().flatten() {
            bolts.clear(&mut scene.graph);
        }
        self.level.clear(scene);
        self.model = None;
        self.prefabs = None;
        self.model_path.clear();
        self.ctf = None;
        self.set_banner(ctx, "");
        self.phase = Phase::Title;
        self.main_menu.set_open(ctx.user_interfaces.first(), true);
        self.want_mouse = false;
        self.set_mouse_captured(ctx, false);
    }

    fn rng(&mut self) -> &mut Rng {
        self.rng.get_or_insert_with(|| {
            // MAZE_SEED makes every maze and round the same, which is what comparing two runs
            // needs.
            if let Some(seed) = platform::var("MAZE_SEED").and_then(|s| s.parse().ok()) {
                return Rng::new(seed);
            }
            Rng::new(platform::nanos_now())
        })
    }

    /// Puts the level into the scene, once its models have loaded: the fixed maze model, or a new
    /// random maze.
    fn place_level(&mut self, scene: &mut Scene) -> Result<(), String> {
        if let Some(model) = &self.model {
            let fbx = self.model_path.to_ascii_lowercase().ends_with(".fbx");
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
        let (width, depth) = platform::var("MAZE_SIZE")
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
        self.stop_hacking(ctx);
        // The droids are put down afresh, and what they were about to say goes with them.
        self.barks.clear();
        // Made (and seeded) first: the grid below borrows the game.
        self.rng();
        // The firewalls first, closed again, so that the floor round them is out of the grid
        // before anything is put on it.
        if let Some((grid, origin)) = self.level.grid.as_mut() {
            let scene = &mut ctx.scenes[self.scene];
            self.firewalls.place(scene, &self.level.markers, grid, *origin);
        }
        // A map with both flags is played as capture the flag.
        let flag = |side: Side| {
            let name = format!("flag_{}", side.name());
            self.level.markers.iter().find(|m| m.name == name).map(|m| m.position)
        };
        // Each side's posts, by their number.
        let posts = |side: Side| {
            let prefix = format!("post_{}_", side.name());
            let mut posts: Vec<_> = self
                .level
                .markers
                .iter()
                .filter_map(|m| Some((m.name.strip_prefix(&prefix)?.parse::<u32>().ok()?, m.position)))
                .collect();
            posts.sort_by_key(|&(n, _)| n);
            posts.into_iter().map(|(_, at)| at).collect()
        };
        self.ctf = flag(Side::Red).zip(flag(Side::Blue)).map(|(red, blue)| Bases {
            red,
            blue,
            posts: [posts(Side::Red), posts(Side::Blue)],
        });
        self.carrying = false;
        // Capture the flag has droids of its own, which say other things.
        let script = if self.ctf.is_some() { CTF_SCRIPT } else { SCRIPT };
        if script != self.script_path {
            self.use_script(&ctx.resource_manager, script);
        }
        if let (Some(bolts), Ok(scene)) = (self.side_bolts.as_mut(), ctx.scenes.try_get_mut(self.scene)) {
            for bolts in bolts {
                bolts.clear(&mut scene.graph);
            }
        }
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
            None => layout::plan_round(grid, |n| rng.below(n)),
        };
        let Some((start, exit)) = round else {
            Log::err("Maze: found no walkable ground to play on");
            self.set_banner(ctx, "No walkable floor found in the maze model.");
            self.phase = Phase::Broken;
            return;
        };
        if platform::var("MAZE_DEBUG").is_some() {
            Log::info(format!(
                "Maze grid (x right, z down, origin {origin:?}):\n{}",
                survey::draw_map(grid, start, exit)
            ));
        }
        self.start_cell = Some(start);
        self.computer_placed = false;
        self.drone_placed = false;
        self.drone_alarmed = false;
        self.drone_calls.clear();
        self.hearts_placed = false;
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
        if let Some(shots) = self.shots.as_mut() {
            shots.clear(&mut scene.graph);
        }
        self.player.teleport(
            &mut scene.graph,
            start_position + Vector3::new(0.0, 1.2, 0.0),
            into_maze.x.atan2(into_maze.z),
        );

        self.round_time = 0.0;
        self.health = Health::default();
        self.knocked_down = false;
        self.phase = Phase::Playing;
        self.set_banner(ctx, "");
    }

    fn set_banner(&self, ctx: &mut PluginContext, text: &str) {
        self.hud.set_banner(ctx.user_interfaces.first(), text);
    }

    fn update_hud(&mut self, ctx: &mut PluginContext) {
        let status = match self.phase {
            Phase::Loading | Phase::Settling(_) => Status::Loading,
            Phase::Title | Phase::Broken => Status::Blank,
            Phase::Playing | Phase::Won | Phase::Deleted => Status::Round {
                time: self.round_time,
                best: self.best_time,
                breath: self.player.breath(),
                health: (self.health.left, self.health.flash),
                armed: self.player.armed(),
                // The menu and the conversation say what to do next, so the hint to click would
                // only be in the way.
                mouse_captured: self.mouse_captured
                    || self.menu.is_open()
                    || self.talking.is_some(),
                alarm: self.inhabitants.alarm(),
                credits: self.credits,
            },
        };
        // Healing only while playing; the flash of the last hit fades out after too.
        let round = matches!(self.phase, Phase::Playing | Phase::Won | Phase::Deleted);
        if round && !self.menu.is_open() {
            let resting = self.phase == Phase::Playing && self.player.resting();
            if let Some(healing) = self.health.update(ctx.dt, resting) {
                let graph = &mut ctx.scenes[self.scene].graph;
                let at = self.player.position(graph);
                let heard = match healing {
                    Healing::Started => Heard::Healing,
                    Healing::Whole => Heard::Whole,
                };
                self.health_sounds.play(graph, heard, at);
            }
        }
        self.hud.update(ctx.user_interfaces.first(), ctx.dt, status);
        self.show_overhead_bars(ctx);
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
        // In a browser the lock hides the cursor, and comes later if the browser grants it:
        // `update` sees it arrive.
        if cfg!(target_arch = "wasm32") {
            if captured {
                let _ = window.set_cursor_grab(CursorGrabMode::Locked);
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                self.mouse_captured = false;
            }
            return;
        }
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

    /// Takes in the browser locking the mouse, or letting it go. The lock arrives some time after
    /// it is asked for, if the browser grants it. Let go while the game still has it, it is
    /// because the player pressed Escape, which the page never hears: that pauses, as Escape does
    /// on the desktop.
    #[cfg(target_arch = "wasm32")]
    fn follow_browser_mouse_lock(&mut self, ctx: &mut PluginContext) {
        let locked = platform::mouse_locked();
        if locked == self.browser_locked {
            return;
        }
        self.browser_locked = locked;
        if locked {
            self.mouse_captured = true;
            // The game may have stopped wanting it in the meantime.
            if !self.want_mouse {
                self.set_mouse_captured(ctx, false);
            }
        } else if self.mouse_captured {
            self.mouse_captured = false;
            self.player.release_keys();
            self.want_mouse = false;
            if self.phase == Phase::Playing {
                self.set_paused(ctx, true);
            }
        }
    }

    fn on_key(&mut self, ctx: &mut PluginContext, code: KeyCode) {
        match code {
            // On the main menu, only its buttons do anything, and Escape goes back from the maps.
            KeyCode::Escape if self.phase == Phase::Title => {
                self.main_menu.show_maps(ctx.user_interfaces.first(), false)
            }
            _ if self.phase == Phase::Title => (),
            // At the computer every key but Escape is for it: minus and the rest are typed.
            _ if self.hacking && !self.menu.is_open() && code != KeyCode::Escape => {
                self.on_hacking_key(ctx, code)
            }
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
            KeyCode::Escape if self.menu.in_options() => {
                self.menu.set_in_options(ctx.user_interfaces.first(), false)
            }
            KeyCode::Escape => self.set_paused(ctx, !self.menu.is_open()),
            _ if self.talking.is_some() && !self.menu.is_open() => self.on_talking_key(ctx, code),
            KeyCode::KeyE if !self.menu.is_open() && self.talkable.is_some() => {
                self.start_talking(ctx)
            }
            KeyCode::KeyE if !self.menu.is_open() => self.start_hacking(ctx),
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
        self.barks.clear();
        if self.prefabs.is_some() {
            // Out of the way first: the new maze's survey would take them for walls.
            self.inhabitants.clear(&mut ctx.scenes[self.scene].graph);
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

    /// Shows what the droids say, and what it means, as the player has them.
    fn apply_subtitles(&mut self, ctx: &mut PluginContext) {
        let ui = ctx.user_interfaces.first();
        self.menu.set_subtitles(ui, self.subtitles);
        self.dialogue.set_subtitles(ui, self.subtitles);
    }

    /// What each kind of droid looks like, once every model there is to make them from has
    /// loaded, or failed to: in its own model, or the player's droid's if that failed.
    fn liveries(&self) -> Option<Vec<Livery>> {
        let droid = self.droid_model.as_ref()?;
        let settled = |model: &ModelResource| model.is_ok() || model.is_failed_to_load();
        if !self
            .kind_models
            .iter()
            .flatten()
            .chain(&self.hostile_model)
            .all(settled)
        {
            return None;
        }
        let hostile = self.hostile_model.as_ref().filter(|model| model.is_ok());
        let characters = self
            .script
            .as_ref()
            .map_or(0, |s| s.characters.len())
            .max(1);
        Some(
            (0..characters)
                .map(|n| {
                    let own = self.kind_models.get(n).and_then(Option::as_ref);
                    if let Some(own) = own.filter(|model| model.is_failed_to_load()) {
                        Log::err(format!(
                            "Could not load {}; using {DROID_MODEL}",
                            own.kind()
                        ));
                    }
                    let model = own.filter(|model| model.is_ok()).unwrap_or(droid);
                    let recoloured = self
                        .script
                        .as_ref()
                        .and_then(|script| script.characters.get(n))
                        .is_some_and(|c| c.hostile_colours);
                    Livery::new(model.clone(), hostile.filter(|_| recoloured))
                })
                .collect(),
        )
    }

    /// Has the droids say what the file at `path` has them say, each kind of droid made from its
    /// own model: loaded, for droids put into a maze from now on.
    fn use_script(&mut self, resources: &fyrox::asset::manager::ResourceManager, path: &'static str) {
        self.script_path = path;
        self.script = match Script::load(path) {
            Ok(script) => Some(script),
            Err(error) => {
                Log::err(format!("Maze: the droids have nothing to say: {error}"));
                None
            }
        };
        // Each kind of droid in its own colours, and any of them in the hostile droid's.
        self.kind_models = self.script.as_ref().map_or_else(Vec::new, |script| {
            let models = script
                .characters
                .iter()
                .map(|character| character.model.as_ref());
            models
                .map(|path| path.map(|path| resources.request::<Model>(path)))
                .collect()
        });
        // Only loaded for a kind that changes into its colours.
        let recoloured = self
            .script
            .as_ref()
            .is_some_and(|script| script.characters.iter().any(|c| c.hostile_colours));
        self.hostile_model = recoloured.then(|| resources.request::<Model>(HOSTILE_MODEL));
    }

    /// Puts the maze's inhabitants into it once there are droids to make them from, and moves
    /// them along. Whether any caught the player, and whose phase changed.
    fn update_inhabitants(&mut self, ctx: &mut PluginContext) -> News {
        let liveries = match self.inhabitants.is_populated() {
            true => None,
            false => match self.liveries() {
                Some(liveries) => Some(liveries),
                None => return News::default(),
            },
        };
        let (Some((grid, origin)), Some(rng)) = (&self.level.grid, self.rng.as_mut()) else {
            return News::default();
        };
        let scene = &mut ctx.scenes[self.scene];
        let player = self.player.feet(&scene.graph);
        if let Some(liveries) = liveries {
            match &self.ctf {
                // Each side's droids of its own kind: red's the first in the conversations, blue's
                // the second.
                Some(bases) => self.inhabitants.populate_sides(
                    scene,
                    liveries,
                    (grid, *origin),
                    bases,
                    |side| match side.is_players() {
                        true => 0,
                        false => 1,
                    },
                    rng,
                ),
                None => self
                    .inhabitants
                    .populate(scene, liveries, (grid, *origin), player, self.player.yaw(), rng),
            }
        }
        let graph = &scene.graph;
        // With the lights off, the player is hard to see, unless their flashlight gives them
        // away.
        let in_the_dark = self.lights_off && !self.player.flashlight_on();
        let posture = self.player.posture();
        self.inhabitants
            .look_for_player(player, posture, in_the_dark, |there| {
                self.player.can_see(graph, there)
            });
        // A sentry's eyes are no flashlight: with the lights off, the others see it only near.
        let script = self.script.as_ref();
        self.inhabitants
            .join_chases(graph, self.lights_off, |character| {
                script.is_some_and(|script| is_sentry(script, character))
            });
        self.inhabitants.update(
            &mut scene.graph,
            (grid, *origin),
            player,
            rng,
            ctx.dt,
            |character| script.is_some_and(|script| is_sentry(script, character)),
        )
    }

    /// Tells the droids what the player's bolts have hit, and says so when one goes down.
    fn land_shots(&mut self, ctx: &mut PluginContext) {
        let graph = &mut ctx.scenes[self.scene].graph;
        let player = self.player.feet(graph);
        // With MAZE_KNOCKDOWN=<seconds>, to try the droids' fall out: that far into the round,
        // the droid nearest the player is shot down, as if by the player where they stand.
        let knockdown = platform::var("MAZE_KNOCKDOWN").and_then(|s| s.trim().parse::<f32>().ok());
        if knockdown.is_some_and(|at| self.round_time >= at) && !self.knocked_down {
            self.knocked_down = true;
            if let Some(n) = self.inhabitants.knock_down(graph, player) {
                Log::info(format!("MAZE_KNOCKDOWN: droid {n} shot down"));
            }
        }
        let mut provoked = Vec::new();
        let mut droid_shot = false;
        // In capture the flag, each side's droids' bolts, which harm only the other side.
        if let Some(bolts) = self.side_bolts.as_mut() {
            let mut landed = Vec::new();
            for (index, bolts) in bolts.iter_mut().enumerate() {
                let side = if index == 0 { Side::Red } else { Side::Blue };
                // Through its own side, the droid that fired among them.
                let own: Vec<Handle<Collider>> = self
                    .inhabitants
                    .standing()
                    .into_iter()
                    .filter(|d| d.1 == Some(side))
                    .map(|d| d.4)
                    .chain(self.drones.iter().filter(|d| d.side() == Some(side)).map(Drone::collider))
                    .collect();
                let strikes = bolts.fly(graph, ctx.dt, |graph, from, way, reach| first_hit(graph, from, way, reach, &own));
                landed.extend(strikes.into_iter().map(|strike| (side, strike)));
            }
            for (side, strike) in landed {
                self.side_strike(ctx, side, strike);
            }
        }
        let graph = &mut ctx.scenes[self.scene].graph;
        for strike in self.player.struck() {
            let collider = strike.collider;
            // In capture the flag, the player's bolts pass their own side by.
            let friendly = self.inhabitants.hit(collider).and_then(|n| self.inhabitants.side(n))
                .or_else(|| self.drones.iter().find(|d| d.collider() == collider).and_then(Drone::side))
                .is_some_and(Side::is_players);
            if friendly {
                continue;
            }
            // A drone, shot: it takes the hit, and goes down after enough of them.
            let hit = self
                .drones
                .iter_mut()
                .enumerate()
                .find_map(|(n, drone)| Some((n, drone.shot(graph, collider, player)?)));
            match hit {
                Some((n, true)) => {
                    self.drone_says.push((n, "down"));
                    self.hud.show_note("A drone is down".into());
                    continue;
                }
                Some((_, false)) => continue,
                None => (),
            }
            droid_shot |= self.inhabitants.hit(collider).is_some();
            if let Some(n) = self.inhabitants.shot(graph, strike, player) {
                let name = self.name_of(n).unwrap_or_else(|| "The droid".into());
                self.hud.show_note(format!("{name} is down"));
            }
            // Shot at, a droid that was not after the player already is at once.
            else if let Some(n) = self.inhabitants.hit(collider) {
                if self.threatened(n).is_some() && self.inhabitants.provoke(n) {
                    provoked.push(n);
                }
            }
        }
        // A droid shot sets the drones on the player.
        if droid_shot {
            self.alert_drone(player);
        }
        for n in provoked {
            self.on_threat(ctx, n, Threat::Provoked);
        }
    }

    /// A bolt from `side`'s droid, or drone, has struck something: the player, one of the other
    /// side's droids or its drone takes the hit; anything of its own side, nothing.
    fn side_strike(&mut self, ctx: &mut PluginContext, side: Side, strike: Strike) {
        let graph = &mut ctx.scenes[self.scene].graph;
        let from = strike.at - strike.way * 5.0;
        if strike.collider == self.player.collider() {
            if side.is_players() || self.phase != Phase::Playing {
                return;
            }
            let last = self.health.hit();
            let at = self.player.position(graph);
            self.health_sounds.play(graph, Heard::Hit, at);
            if last {
                self.delete_player(ctx, &format!("{}'s side", side.name()));
            }
            return;
        }
        if let Some(n) = self.drones.iter().position(|d| d.collider() == strike.collider) {
            if self.drones[n].side() != Some(side) {
                if let Some(true) = self.drones[n].shot(graph, strike.collider, from) {
                    self.drone_says.push((n, "down"));
                }
            }
            return;
        }
        let victim = self.inhabitants.hit(strike.collider).and_then(|n| self.inhabitants.side(n));
        if victim == Some(side) {
            return;
        }
        if let Some(n) = self.inhabitants.shot(graph, strike, from) {
            Log::info(format!("Capture the flag: droid {n} is down"));
            let whose = self.inhabitants.side(n).map_or("A", |s| if s.is_players() { "One of yours" } else { "One of the enemy's" });
            self.hud.show_note(format!("{whose} droids is down"));
        }
    }

    /// Sends the drones in the maze to search where the player's feet are, `at`, as the alarm
    /// does.
    fn alert_drone(&mut self, at: Vector3<f32>) {
        for (n, drone) in self.drones.iter_mut().enumerate() {
            if let Some(says) = drone.alarm(at) {
                self.drone_says.push((n, says));
            }
        }
    }

    /// Calls a drone in to search where the player is, `at`: the one patrolling nearest there, if
    /// it is near enough to come soon, or failing that one from out of sight, not too far off - or
    /// failing that, with every drone already out, the patrolling one nearest there however far,
    /// or the searching one nearest, to look there instead. Some drone always comes.
    fn call_drone(&mut self, graph: &mut Graph, at: Vector3<f32>) {
        // The klaxon, from where the drone is called to.
        self.alarm_sound.sound(graph, at + Vector3::new(0.0, 1.5, 0.0));
        let (Some((grid, origin)), Some(rng)) = (self.level.grid.as_ref(), self.rng.as_mut()) else {
            return;
        };
        let from = |drone: &Drone| (drone.at() - at).xz().norm();
        let nearest = |drones: &[Drone], state: fn(State) -> bool| {
            drones
                .iter()
                .enumerate()
                .filter(|(_, drone)| drone.is_placed() && state(drone.state()))
                .min_by(|a, b| from(a.1).total_cmp(&from(b.1)))
                .map(|(n, _)| n)
        };
        let patrolling = nearest(&self.drones, |state| state == State::Patrol);
        if let Some(n) = patrolling.filter(|&n| from(&self.drones[n]) <= drone::ANSWERS_WITHIN) {
            if let Some(says) = self.drones[n].alarm(at) {
                self.drone_says.push((n, says));
            }
            Log::info(format!("Drone {n}: called to the computer"));
            return;
        }
        let feet = self.player.feet(graph);
        if let Some(n) = self.drones.iter().position(|drone| !drone.is_placed()) {
            if self.drones[n].call_in(graph, (grid, *origin), feet, at, rng) {
                self.drone_says.push((n, "alarm"));
                Log::info(format!("Drone {n}: called in"));
                return;
            }
        }
        // Every drone out already: the nearest patrolling or searching one comes, however far.
        if let Some(n) = patrolling {
            if let Some(says) = self.drones[n].alarm(at) {
                self.drone_says.push((n, says));
            }
            Log::info(format!("Drone {n}: called to the computer, from afar"));
            return;
        }
        if let Some(n) = nearest(&self.drones, |state| matches!(state, State::Search { .. })) {
            self.drones[n].alarm(at);
            Log::info(format!("Drone {n}: sent to the computer"));
        }
    }

    /// How the `n`th droid takes having the pistol pointed at it, if it minds at all.
    fn threatened(&self, n: usize) -> Option<crate::dialogue::Threatened> {
        let (character, _) = self.inhabitants.who(n)?;
        self.script.as_ref()?.characters.get(character)?.threatened
    }

    /// Has the droids feel the pistol pointed at them, or not, for another frame, and deals with
    /// what they do about it.
    fn threaten(&mut self, ctx: &mut PluginContext) {
        let graph = &ctx.scenes[self.scene].graph;
        let aim = self.player.pistol_aim(graph);
        let Some(script) = self.script.as_ref() else {
            return;
        };
        let patience = |character: usize| {
            let threatened = script.characters.get(character)?.threatened?;
            Some(threatened.patience)
        };
        let player = &self.player;
        let stages = self.inhabitants.feel_aimed_at(
            aim,
            |there| player.can_see(graph, there),
            patience,
            ctx.dt,
        );
        for (n, threat) in stages {
            self.on_threat(ctx, n, threat);
        }
    }

    /// The `n`th droid has gone on to `threat`, with the pistol pointed at it: its eyes and what
    /// it says show how it takes it, and provoked, it does what its kind does about it.
    fn on_threat(&mut self, ctx: &mut PluginContext, n: usize, threat: Threat) {
        let (mood, bark) = match threat {
            Threat::Warned => (Mood::Warning, "warned"),
            Threat::WarnedAgain => (Mood::Agitated, "warned_again"),
            Threat::Provoked => (Mood::Hostile, "provoked"),
            Threat::Calmed => (Mood::Normal, "calmed"),
        };
        self.bark(n, bark, mood);
        let eyes = match threat {
            Threat::Calmed => None,
            _ => screen::eyes(mood),
        };
        self.inhabitants.set_eyes(n, eyes);
        if threat != Threat::Provoked {
            return;
        }
        match self.threatened(n).map(|threatened| threatened.then) {
            Some(Provoked::Attacks) => self.inhabitants.set_hostile(n),
            Some(Provoked::Alarm) => {
                // It has done its part: the sentries see to the player.
                self.inhabitants.set_eyes(n, None);
                let player = self.player.feet(&ctx.scenes[self.scene].graph);
                let Some(script) = self.script.as_ref() else {
                    return;
                };
                self.inhabitants
                    .raise_alarm(n, player, |character| is_sentry(script, character));
                // The klaxon, from the droid that sounded it.
                let graph = &mut ctx.scenes[self.scene].graph;
                if let Some(face) = self.inhabitants.face(graph, n) {
                    self.alarm_sound.sound(graph, face);
                }
                // The drone answers the alarm too.
                self.alert_drone(player);
                let name = self.name_of(n).unwrap_or_else(|| "A droid".into());
                self.hud.show_note(format!("{name} sounded the alarm"));
            }
            None => (),
        }
    }

    /// Moves the droids along, and deals with what comes of it: the player caught, and droids
    /// spotting them, losing them and giving up. True if the player was caught.
    fn move_inhabitants(&mut self, ctx: &mut PluginContext) -> bool {
        // What the player made heard since the last time, for the droids to hear.
        let noises = self.player.noises();
        if let Some((grid, origin)) = &self.level.grid {
            for (at, loudness) in noises {
                self.inhabitants.hear((grid, *origin), at, loudness);
            }
        }
        let News {
            hit,
            alerts,
            heard,
            alarmed,
            engaged,
            shots,
        } = self.update_inhabitants(ctx);
        // In capture the flag, a droid going after one of the other side says so, and the
        // droids' shots leave their pistols.
        for n in engaged {
            let side = self.inhabitants.side(n).map_or("", Side::name);
            Log::info(format!("Capture the flag: {side}'s droid {n} goes after one of the other side"));
            self.bark(n, "engaged", Mood::Hostile);
        }
        if !shots.is_empty() {
            let graph = &mut ctx.scenes[self.scene].graph;
            let bolts = self.side_bolts.get_or_insert_with(|| [Bolts::new(graph), Bolts::new(graph)]);
            for (from, way, side) in shots {
                bolts[side_index(side)].fire(graph, from, way);
            }
        }
        for (n, alert) in alerts {
            // The eyes show the phase: red after the player, orange searching, yellow wary, and
            // their own colour once it is calm again.
            let (mood, bark) = match alert {
                Some(Alert::Alert) => (Mood::Hostile, "spotted"),
                Some(Alert::Evasion) if alarmed.contains(&n) => (Mood::Agitated, "alarmed"),
                Some(Alert::Evasion) if heard.contains(&n) => (Mood::Agitated, "heard"),
                Some(Alert::Evasion) => (Mood::Agitated, "lost"),
                Some(Alert::Caution) => (Mood::Warning, "gave_up"),
                None => {
                    self.inhabitants.set_eyes(n, None);
                    continue;
                }
            };
            self.inhabitants.set_eyes(n, screen::eyes(mood));
            self.bark(n, bark, mood);
        }
        match hit {
            // Hit, they lose some health, and with none left they are deleted.
            Some(n) if self.phase == Phase::Playing => {
                let last = self.health.hit();
                let graph = &mut ctx.scenes[self.scene].graph;
                let at = self.player.position(graph);
                self.health_sounds.play(graph, Heard::Hit, at);
                if last {
                    let name = self.name_of(n).unwrap_or_else(|| "A droid".into());
                    self.delete_player(ctx, &name);
                }
                last
            }
            _ => false,
        }
    }

    /// Has the `n`th droid say its bark called `name`, if its kind has one, feeling `mood`: out
    /// loud once the voice is made, and on screen straight away, as the player has subtitles.
    fn bark(&mut self, n: usize, name: &str, mood: Mood) {
        let bark = self
            .script
            .as_ref()
            .zip(self.inhabitants.who(n))
            .and_then(|(script, (character, _))| script.characters[character].barks.get(name).cloned());
        if let Some(bark) = bark {
            self.say_bark(n, &bark, mood);
        }
    }

    /// Has the `n`th droid say one of the things it says when the player tries to talk to it,
    /// in capture the flag, where there are no conversations: at random, but never the same
    /// twice running. Only to one of the player's own side.
    fn chat(&mut self, n: usize) {
        if !self.inhabitants.side(n).is_some_and(Side::is_players) {
            return;
        }
        let Some(chatter) = self
            .script
            .as_ref()
            .zip(self.inhabitants.who(n))
            .map(|(script, (character, _))| script.characters[character].chatter.clone())
            .filter(|chatter| !chatter.is_empty())
        else {
            return;
        };
        let mut pick = self.rng().below(chatter.len());
        if chatter.len() > 1 && Some(pick) == self.last_chat {
            pick = (pick + 1) % chatter.len();
        }
        self.last_chat = Some(pick);
        self.inhabitants.speak_to_player(n, CHAT_LOOK);
        self.say_bark(n, &chatter[pick], Mood::Normal);
    }

    /// Has the `n`th droid say `bark`, feeling `mood`: out loud once the voice is made, and on
    /// screen straight away, as the player has subtitles.
    fn say_bark(&mut self, n: usize, bark: &Bark, mood: Mood) {
        let (Some(script), Some((character, code))) = (&self.script, self.inhabitants.who(n))
        else {
            return;
        };
        let character = &script.characters[character];
        let mut lines = Vec::new();
        if self.subtitles.latin {
            lines.push(bark.says.clone());
        }
        if self.subtitles.english && !bark.means.is_empty() {
            lines.push(bark.means.clone());
        }
        if !lines.is_empty() {
            let who = format!("{} {code}", character.name.to_uppercase());
            self.hud.show_note(format!("{who}: {}", lines.join("\n")));
        }
        let Some(voices) = &self.voices else {
            return;
        };
        let Some(voice) = voices.voice(&character.name) else {
            return;
        };
        let sound = voices.speak(&bark.says, voice, pitch(voices, code, mood));
        let (rate, reach) = (voices.sample_rate, sound.reach);
        let receiver = platform::in_background(move || synth::make(&sound, rate));
        // A new one from the same droid cuts off whatever it had yet to say.
        self.barks.retain(|barking| barking.droid != n);
        self.barks.push(Barking {
            droid: n,
            making: Making(receiver, reach),
        });
    }

    /// Says each bark that has been made into a voice, from its droid's face.
    fn bark_when_made(&mut self, ctx: &mut PluginContext) {
        let Some(voices) = &self.voices else {
            return;
        };
        let graph = &mut ctx.scenes[self.scene].graph;
        let inhabitants = &self.inhabitants;
        self.barks.retain(
            |Barking {
                 droid,
                 making: Making(receiver, reach),
             }| {
                let samples = match receiver.try_recv() {
                    Ok(samples) => samples,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return true,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return false,
                };
                if let (Some(face), Some(buffer)) = (
                    inhabitants.face(graph, *droid),
                    formants::playable(samples, voices.sample_rate),
                ) {
                    SoundBuilder::new(BaseBuilder::new().with_local_transform(
                        TransformBuilder::new().with_local_position(face).build(),
                    ))
                    .with_buffer(Some(buffer))
                    .with_radius(*reach)
                    .with_play_once(true)
                    .with_status(SoundStatus::Playing)
                    .build(graph);
                }
                false
            },
        );
    }

    /// The player has been caught by the `n`th droid: they stop where they are, and are told so
    /// until the next maze.
    fn delete_player(&mut self, ctx: &mut PluginContext, name: &str) {
        self.stop_talking(ctx);
        self.phase = Phase::Deleted;
        self.deleted = 0.0;
        self.set_banner(ctx, &format!("Deleted by {name}"));
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
        // moves, and holds the player where they are, and every sound where it is - a droid's
        // line, the hum of a bolt - to carry on from there.
        let graph = &mut ctx.scenes[self.scene].graph;
        graph.physics.enabled.set_value_and_mark_modified(!paused);
        self.pause_sounds(graph, paused);
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

    /// Pauses every sound playing in `graph`, each where it is, or carries on with those it
    /// paused. The engine sets the scene's own pause every frame, from switches the game has no
    /// say in, so each sound is paused by itself.
    fn pause_sounds(&mut self, graph: &mut Graph, paused: bool) {
        if paused {
            self.paused_sounds = graph
                .pair_iter_mut()
                .filter_map(|(handle, node)| {
                    let sound = node.cast_mut::<Sound>()?;
                    (sound.status() == SoundStatus::Playing).then(|| {
                        sound.pause();
                        handle
                    })
                })
                .collect();
        } else {
            for handle in self.paused_sounds.drain(..) {
                if let Ok(sound) = graph.try_get_mut_of_type::<Sound>(handle) {
                    if sound.status() == SoundStatus::Paused {
                        sound.play();
                    }
                }
            }
        }
    }

    /// Finds the droid the player could talk to, if any, and puts the hint to talk to it on
    /// screen. Nobody, while the player cannot talk.
    fn look_for_someone(&mut self, ctx: &mut PluginContext) {
        let can_use_computer = self.phase == Phase::Playing && !self.busy() && !self.menu.is_open();
        // In capture the flag, the droids are not talked to.
        // In capture the flag, only to the player's own side, and they only say something.
        let can_talk = can_use_computer && self.script.is_some();
        let found = if can_talk {
            let graph = &ctx.scenes[self.scene].graph;
            let (player, feet, ahead) =
                (&self.player, self.player.feet(graph), self.player.ahead());
            self.inhabitants
                .to_talk_to(feet, ahead, |there| player.can_see(graph, there))
                .filter(|&n| self.ctf.is_none() || self.inhabitants.side(n).is_some_and(Side::is_players))
        } else {
            None
        };
        let talkable = found.and_then(|n| Some((n, self.name_of(n)?)));
        // With nobody to talk to, the computer, if the player is at it.
        let at_computer = match talkable.is_none() && can_use_computer && self.computer_placed {
            true => {
                let graph = &ctx.scenes[self.scene].graph;
                let feet = self.player.feet(graph);
                self.computers
                    .iter()
                    .position(|computer| computer.within_reach(graph, feet))
                    .map(|n| (n, self.computers[n].cleared()))
            }
            false => None,
        };
        if talkable != self.talkable || (talkable.is_none() && at_computer != self.at_computer) {
            let ui = ctx.user_interfaces.first();
            match (&talkable, at_computer) {
                (Some((_, who)), _) => self.dialogue.set_prompt(ui, Some(who)),
                (None, Some((_, cleared))) => self.dialogue.set_action_prompt(
                    ui,
                    Some(("Computer", if cleared { "Use" } else { "Hack" })),
                ),
                (None, None) => self.dialogue.set_prompt(ui, None),
            }
            self.talkable = talkable;
        }
        self.at_computer = at_computer;
    }

    /// Puts bars over the head of each sentry the player can see that is after them, has spent
    /// some of its breath or been hit - its health over its stamina - and over the drone, its
    /// health, while it is after them or has been hit; the size they would be there, as the
    /// player's are on screen.
    fn show_overhead_bars(&mut self, ctx: &mut PluginContext) {
        let ui = ctx.user_interfaces.first_mut();
        let playing = matches!(self.phase, Phase::Playing | Phase::Won | Phase::Deleted);
        if !playing || self.menu.is_open() || self.talking.is_some() {
            self.hud.show_overhead_bars(ui, &[]);
            return;
        }
        let size = ui.screen_size();
        let graph = &ctx.scenes[self.scene].graph;
        let player = self.player.feet(graph);
        // Where the bars go on screen over `top`, seen from `feet`, and how wide.
        let placed = |feet: Vector3<f32>, top: Vector3<f32>| {
            if (feet - player).norm() >= BREATH_BAR_REACH || !self.player.can_see(graph, feet) {
                return None;
            }
            let above = top + Vector3::new(0.0, BREATH_BAR_ABOVE, 0.0);
            let at = self.player.on_screen(graph, above, size)?;
            // How many pixels a meter is, there.
            let higher = self.player.on_screen(graph, above + Vector3::y(), size)?;
            Some((at, BREATH_BAR_WIDTH * (at - higher).norm()))
        };
        let mut bars: Vec<hud::OverheadBars> = self
            .inhabitants
            .breath(graph)
            .into_iter()
            .filter_map(|breath| {
                let (at, width) = placed(breath.feet, breath.face)?;
                Some(hud::OverheadBars {
                    at,
                    width,
                    health: breath.health,
                    breath: Some((breath.left, breath.winded)),
                })
            })
            .collect();
        for (top, health) in self.drones.iter().filter_map(|drone| drone.health_bar(graph)) {
            if let Some((at, width)) = placed(top, top) {
                bars.push(hud::OverheadBars {
                    at,
                    width,
                    health,
                    breath: None,
                });
            }
        }
        self.hud.show_overhead_bars(ui, &bars);
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
        // In capture the flag there is no conversation: it just says something.
        if self.ctf.is_some() {
            self.chat(droid);
            return;
        }
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
        let view = conversation.view(script, &facts, self.credits);
        self.dialogue.set_open(ui, true);
        self.dialogue.show(ui, &who, &view);
        self.dialogue.set_prompt(ui, None);
        self.talkable = None;
        self.inhabitants.set_talking(droid, true);
        self.inhabitants.set_eyes(droid, screen::eyes(view.mood));
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
        let view = talking.conversation.view(script, &talking.facts, self.credits);
        let sound = voices.speak(&view.says, voice, pitch(voices, code, view.mood));
        let (rate, reach) = (voices.sample_rate, sound.reach);
        let receiver = platform::in_background(move || synth::make(&sound, rate));
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
        talking.voice = SoundBuilder::new(
            BaseBuilder::new()
                .with_local_transform(TransformBuilder::new().with_local_position(face).build()),
        )
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
        // A bribe the player cannot pay for is not said.
        if talking
            .conversation
            .price(script, choice)
            .is_some_and(|price| price > self.credits)
        {
            self.hud.show_note("Not enough credits".into());
            return;
        }
        let before = self.credits;
        if talking.conversation.choose(script, choice, roll, &mut self.credits) {
            if self.credits != before {
                Log::info(format!("Credits: bribe paid, {} left", self.credits));
            }
            let view = talking.conversation.view(script, &talking.facts, self.credits);
            self.dialogue
                .show(ctx.user_interfaces.first(), &talking.who, &view);
            self.inhabitants
                .set_eyes(talking.droid, screen::eyes(view.mood));
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
        self.inhabitants.set_eyes(talking.droid, None);
        // However it ended, a conversation that got as far as a threat is carried out.
        if self
            .script
            .as_ref()
            .is_some_and(|script| talking.conversation.attacks(script))
        {
            self.inhabitants.set_hostile(talking.droid);
            self.inhabitants
                .set_eyes(talking.droid, screen::eyes(Mood::Hostile));
        }
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

    /// Whether the player is talking or at the computer, and so held still, with the keys and
    /// the mouse for that.
    fn busy(&self) -> bool {
        self.talking.is_some() || self.hacking
    }

    /// Puts the computers into the scene once their model has loaded, and about the maze once a
    /// round is under way, with the notes shared out among them; keeps their screens going.
    fn set_up_computer(&mut self, ctx: &mut PluginContext) {
        if let Some(model) = self.computer_model.take_if(|model| model.is_ok()) {
            let seed = platform::nanos_now();
            let beeps = Beeps::make();
            let scene = &mut ctx.scenes[self.scene];
            self.computers = (0..COMPUTERS as u64)
                .filter_map(|n| {
                    let seed = seed ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15);
                    Computer::spawn(&model, scene, seed, &beeps)
                })
                .collect();
        } else if self
            .computer_model
            .as_ref()
            .is_some_and(|model| model.is_failed_to_load())
        {
            Log::err(format!(
                "Could not load {COMPUTER_MODEL}; there is nothing to hack"
            ));
            self.computer_model = None;
        }
        if self.computers.is_empty() {
            return;
        }
        if !self.computer_placed && self.phase == Phase::Playing {
            self.rng();
            if let (Some((grid, origin)), Some(start), Some(rng)) =
                (self.level.grid.as_mut(), self.start_cell, self.rng.as_mut())
            {
                let graph = &mut ctx.scenes[self.scene].graph;
                let colliders: Vec<_> = self.computers.iter().map(Computer::collider).collect();
                let mut taken = Vec::new();
                let mut placed = Vec::new();
                // Each firewall answers to one of the last computers, put first after the one
                // near the start, where the model marks for it or else near the firewall.
                let count = self.computers.len();
                let tied = self.firewalls.iter().count().min(count.saturating_sub(1));
                let first_tied = count - tied;
                let order = std::iter::once(0).chain(first_tied..count).chain(1..first_tied);
                for n in order {
                    let spot = match n {
                        0 => computer::spot(grid, start),
                        n if n >= first_tied => {
                            let wall = self.firewalls.iter().nth(n - first_tied);
                            wall.and_then(|wall| {
                                let name = format!("computer_{}", wall.side);
                                let marker = self.level.markers.iter().find(|m| m.name == name);
                                match marker {
                                    Some(marker) => computer::spot_marked(grid, *origin, marker.position, marker.yaw),
                                    None => grid
                                        .nearest_walkable(*origin, wall.position)
                                        .and_then(|cell| computer::spot(grid, cell)),
                                }
                            })
                        }
                        _ => computer::spot_away(grid, start, &taken, rng),
                    };
                    let computer = &mut self.computers[n];
                    match spot {
                        Some(spot) => {
                            computer.place(graph, grid, *origin, spot, &colliders);
                            taken.push(spot.0);
                            placed.push(n);
                            if n >= first_tied {
                                if let Some(wall) = self.firewalls.get_mut(n - first_tied) {
                                    wall.computer = Some(n);
                                    Log::info(format!("Firewall: {}'s opened by computer {n}", wall.side));
                                }
                            }
                        }
                        None => computer.hide(graph),
                    }
                }
                if self.firewalls.iter().any(|wall| wall.computer.is_none()) {
                    Log::warn("Maze: a firewall has no computer to open it, and stays open");
                }
                if placed.first() != Some(&0) {
                    Log::warn("Maze: found no wall near the start to put a computer against");
                }
                // The notes, among those put somewhere.
                let entries = self.notes.as_ref().map_or(&[][..], |notes| &notes.entries);
                let shares = notes::share_out(entries, placed.len(), rng);
                for (&n, share) in placed.iter().zip(shares) {
                    let files = share.into_iter().map(|e| entries[e].clone()).collect();
                    self.computers[n].set_files(files);
                }
                Log::info(format!("Maze: {} computers", placed.len()));
                // Tried once a round, found or not.
                self.computer_placed = true;
                // MAZE_COMPUTER, next frame: where it is now is only worked out after this one.
                self.computer_test = platform::var("MAZE_COMPUTER").is_some();
                return;
            }
        }
        // With MAZE_COMPUTER=1, to try it out: the player is put at it, using it; with
        // MAZE_COMPUTER=breach, a breach under way too.
        if std::mem::take(&mut self.computer_test) {
            {
                if let (Some(test), Some(computer)) =
                    (platform::var("MAZE_COMPUTER"), self.computers.first_mut())
                {
                    let graph = &mut ctx.scenes[self.scene].graph;
                    let (middle, facing) = computer.screen(graph);
                    let mut feet = middle + facing * 1.1;
                    feet.y = computer.floor(graph);
                    self.player.teleport(
                        graph,
                        feet + Vector3::new(0.0, 1.2, 0.0),
                        (-facing.x).atan2(-facing.z),
                    );
                    if test == "breach" {
                        computer.enter(graph);
                    }
                    // With MAZE_COMPUTER=look, only put in front of it, to see it from there.
                    if test != "look" {
                        self.at_computer = Some((0, false));
                        self.start_hacking(ctx);
                    }
                    return;
                }
            }
        }
        if !self.menu.is_open() {
            let mut denied = false;
            let mut taken = Credits::default();
            for computer in &mut self.computers {
                denied |= computer.update(ctx.dt);
                // Cleared, it transfers its credits to the player.
                if let Some(credits) = computer.take_credits() {
                    taken += credits;
                }
            }
            if taken > Credits::default() {
                self.credits += taken;
                self.hud.show_note(format!("+{taken} transferred"));
                Log::info(format!("Credits: +{taken}, {} in all", self.credits));
            }
            // A hacked computer opens its firewall.
            self.firewalls.update(&mut ctx.scenes[self.scene].graph, &self.computers, ctx.elapsed_time);
            // A failed hack calls in a drone, to where the player is.
            if denied && self.phase == Phase::Playing {
                let feet = self.player.feet(&ctx.scenes[self.scene].graph);
                self.drone_calls.push(feet);
                self.hud.show_note("Trace complete: a drone is on its way".into());
            }
        }
        // The terminal of the one the player is using, or last used.
        let Some(computer) = self.using.and_then(|n| self.computers.get(n)) else {
            return;
        };
        // Over the screen, wherever the camera sees it, while the player is at it.
        let ui = ctx.user_interfaces.first();
        let size = ui.screen_size();
        let graph = &ctx.scenes[self.scene].graph;
        let corners = computer
            .screen_corners(graph)
            .map(|corner| self.player.on_screen(graph, corner, size));
        // Should the camera not say where the screen is, the middle of the view, about as much of
        // it as the screen takes up in the close-up.
        let middle = || {
            let (half_height, half_width) = (0.37 * size.y, 0.37 * size.y * 1.537);
            let centre = size * 0.5;
            [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
                .map(|(x, y)| centre + Vector2::new(x * half_width, y * half_height))
        };
        // Anywhere but in the window, it goes in the middle too.
        let in_view = |c: &Vector2<f32>| {
            c.x >= -1.0 && c.y >= -1.0 && c.x <= size.x + 1.0 && c.y <= size.y + 1.0
        };
        let overlay = platform::var("MAZE_TERMINAL_OVERLAY").as_deref() == Some("1");
        let shown = (self.hacking && overlay).then(|| {
            let (lines, stage, wrong) = computer.terminal();
            let corners = match corners {
                [Some(a), Some(b), Some(c), Some(d)] if [a, b, c, d].iter().all(in_view) => {
                    [a, b, c, d]
                }
                _ => middle(),
            };
            (lines, corners, stage, wrong)
        });
        self.terminal.show(ui, shown, ctx.dt);
        // Otherwise on the screen itself.
        let on_screen = (self.hacking && !overlay).then(|| computer.terminal());
        self.screen_terminal
            .show(ctx.user_interfaces, computer, on_screen, ctx.dt);
    }

    /// Puts the drones into the scene once their model has loaded - one to patrol each round,
    /// well away from the player, the rest out of sight until called in - and keeps them going,
    /// and their shots flying, which hurt the player they hit.
    fn set_up_drone(&mut self, ctx: &mut PluginContext) {
        if let Some(model) = self.drone_model.take_if(|model| model.is_ok()) {
            let scene = &mut ctx.scenes[self.scene];
            self.drones = (0..DRONES).filter_map(|_| Drone::spawn(&model, scene)).collect();
        } else if self.drone_model.as_ref().is_some_and(|model| model.is_failed_to_load()) {
            Log::err(format!("Could not load {DRONE_MODEL}; there are no drones"));
            self.drone_model = None;
        }
        if let Some(model) = self.shot_model.take_if(|model| model.is_ok()) {
            self.shots = Some(Shots::spawn(&model, &mut ctx.scenes[self.scene]));
        } else if self.shot_model.as_ref().is_some_and(|model| model.is_failed_to_load()) {
            Log::err(format!("Could not load {SHOT_MODEL}; the drones fire nothing"));
            self.shot_model = None;
        }
        // Shots fly, and hurt, only while the round is being played - through the drones.
        if self.phase == Phase::Playing && !self.menu.is_open() {
            let graph = &mut ctx.scenes[self.scene].graph;
            // In the maze, through every drone; in capture the flag, only through the one that
            // fired, so that the sides' drones can hit each other.
            let drones: Vec<_> = match self.ctf {
                Some(_) => Vec::new(),
                None => self.drones.iter().map(|drone| drone.collider()).collect(),
            };
            let hits = self.shots.as_mut().map_or_else(Vec::new, |shots| shots.update(graph, ctx.dt, &drones));
            for hit in hits {
                if self.phase != Phase::Playing {
                    break;
                }
                let side = self.drones.iter().find(|d| d.collider() == hit.by).and_then(Drone::side);
                match (&self.ctf, side) {
                    // A drone on a side harms whatever of the other side it hits.
                    (Some(_), Some(side)) => {
                        let strike = Strike { collider: hit.collider, at: hit.at, way: hit.way };
                        self.side_strike(ctx, side, strike);
                    }
                    _ if hit.collider == self.player.collider() => {
                        let graph = &mut ctx.scenes[self.scene].graph;
                        let last = self.health.hit();
                        let at = self.player.position(graph);
                        self.health_sounds.play(graph, Heard::Hit, at);
                        if last {
                            self.delete_player(ctx, "a security drone");
                            break;
                        }
                    }
                    _ => (),
                }
            }
        }
        if self.drones.is_empty() || self.level.grid.is_none() {
            return;
        }
        // The first patrols, the rest wait out of sight: put down once a round.
        if !self.drone_placed && self.phase == Phase::Playing {
            let graph = &mut ctx.scenes[self.scene].graph;
            let feet = self.player.feet(graph);
            if let (Some((grid, origin)), Some(rng)) = (self.level.grid.as_ref(), self.rng.as_mut()) {
                for drone in &mut self.drones {
                    drone.hide(graph);
                }
                match &self.ctf {
                    // One for each side, by its flag; the rest wait to be called in.
                    Some(bases) => {
                        for (drone, side) in self.drones.iter_mut().zip(Side::BOTH) {
                            drone.place_for(graph, (grid, *origin), side, bases.flag(side), rng);
                        }
                    }
                    None => {
                        self.drones[0].place(graph, (grid, *origin), feet, rng);
                    }
                }
            }
            // Tried once a round, found or not.
            self.drone_placed = true;
        }
        // With MAZE_DRONE_ALARM=<seconds>, to try the drones out: that far into the round, one is
        // called in as by a failed hack.
        let alarm = platform::var("MAZE_DRONE_ALARM").and_then(|s| s.trim().parse::<f32>().ok());
        if alarm.is_some_and(|at| self.round_time >= at) && !self.drone_alarmed && self.drone_placed {
            self.drone_alarmed = true;
            let feet = self.player.feet(&ctx.scenes[self.scene].graph);
            self.drone_calls.push(feet);
            Log::info("MAZE_DRONE_ALARM: a drone is called in");
        }
        for at in std::mem::take(&mut self.drone_calls) {
            self.call_drone(&mut ctx.scenes[self.scene].graph, at);
        }
        let (Some((grid, origin)), Some(rng)) = (self.level.grid.as_ref(), self.rng.as_mut()) else {
            return;
        };
        let graph = &mut ctx.scenes[self.scene].graph;
        let player = Target {
            feet: self.player.feet(graph),
            middle: self.player.position(graph),
            collider: self.player.collider(),
            posture: self.player.posture(),
            in_the_dark: self.lights_off && !self.player.flashlight_on(),
        };
        // In capture the flag, whoever of the other side each drone on a side sees, nearest
        // first - the player among them for blue's; or, seeing none, the one it went after last.
        let targets: Vec<Target> = (0..self.drones.len())
            .map(|n| {
                let drone = &self.drones[n];
                let Some(side) = drone.side().filter(|_| self.ctf.is_some()) else {
                    return player;
                };
                let droids = self.inhabitants.standing().into_iter().filter(|d| d.1 == Some(side.other())).map(|(_, _, feet, middle, collider)| Target {
                    feet,
                    middle,
                    collider,
                    posture: Posture::Standing,
                    in_the_dark: false,
                });
                let drones = self.drones.iter().filter(|d| d.side() == Some(side.other())).filter_map(Drone::as_target);
                let player = (!side.is_players()).then_some(player);
                let away = |t: &Target| (t.feet - drone.at()).norm();
                let seen = droids
                    .chain(drones)
                    .chain(player)
                    .filter(|t| drone.sees(graph, t))
                    .min_by(|a, b| away(a).total_cmp(&away(b)));
                seen.or(self.drone_targets.get(n).copied().flatten()).unwrap_or(NOBODY)
            })
            .collect();
        self.drone_targets = targets.iter().map(|&t| (t != NOBODY).then_some(t)).collect();
        let live = self.phase == Phase::Playing && !self.menu.is_open();
        let mut says = std::mem::take(&mut self.drone_says);
        for (n, drone) in self.drones.iter_mut().enumerate() {
            let target = targets[n];
            let doing = drone.update(graph, (grid, *origin), &target, rng, ctx.dt, live);
            says.extend(doing.says.map(|said| (n, said)));
            // At its target's middle, wherever they are as it fires.
            if let (Some((from, colour)), Some(shots)) = (doing.fired, self.shots.as_mut()) {
                shots.fire(graph, from, target.middle, colour, drone.collider());
            }
        }
        for &(n, said) in &says {
            Log::info(format!("Drone {n}: {said}"));
        }
        // Speaking as things happen to it, in beeps made in the background, from its body: one
        // line at a time, the latest.
        if let (Some(&(n, name)), Some((chirps, lines))) = (says.last(), &self.drone_voice) {
            if let Some(said) = lines.line(name, self.drone_said) {
                self.drone_said += 1;
                let sound = chirps.say(&said.says, 1.0);
                let (rate, reach) = (chirps.sample_rate, sound.reach);
                let receiver = platform::in_background(move || synth::make(&sound, rate));
                self.drone_saying = Some(Making(receiver, reach));
                self.drone_speaking = n;
            }
        }
        if let (Some(Making(receiver, reach)), Some((chirps, _))) =
            (&self.drone_saying, &self.drone_voice)
        {
            match receiver.try_recv() {
                Ok(samples) => {
                    let body = self.drones.get(self.drone_speaking).map(|drone| drone.body());
                    if let (Some(buffer), Some(body)) = (formants::playable(samples, chirps.sample_rate), body) {
                        let sound = SoundBuilder::new(BaseBuilder::new())
                            .with_buffer(Some(buffer))
                            .with_radius(*reach)
                            .with_play_once(true)
                            .with_status(SoundStatus::Playing)
                            .build(graph);
                        // Along with it, wherever it goes.
                        graph.link_nodes(sound, body);
                    }
                    self.drone_saying = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.drone_saying = None,
            }
        }
    }

    /// Scatters the hearts in the corridors once their model has loaded and a round is under way,
    /// and keeps them floating.
    fn set_up_hearts(&mut self, ctx: &mut PluginContext) {
        if self.heart_model.as_ref().is_some_and(|model| model.is_failed_to_load()) {
            Log::err(format!("Could not load {HEART_MODEL}; there are no hearts"));
            self.heart_model = None;
        }
        let Some(model) = self.heart_model.clone().filter(|model| model.is_ok()) else {
            return;
        };
        if !self.hearts_placed && self.phase == Phase::Playing {
            self.rng();
            if let (Some((grid, origin)), Some(start), Some(rng)) =
                (self.level.grid.as_ref(), self.start_cell, self.rng.as_mut())
            {
                let floor = (0..grid.depth)
                    .flat_map(|z| (0..grid.width).map(move |x| (x, z)))
                    .filter(|&(x, z)| grid.is_walkable(x, z))
                    .count();
                let spots = hearts::spots(grid, start, hearts::count(floor), rng);
                let scene = &mut ctx.scenes[self.scene];
                self.hearts.place(&model, scene, (grid, *origin), &spots);
                // Tried once a round, found room or not.
                self.hearts_placed = true;
            }
        }
        if !self.menu.is_open() {
            let graph = &mut ctx.scenes[self.scene].graph;
            self.hearts.update(graph, ctx.dt);
            // Walking into one with health to make up picks it up.
            if self.phase == Phase::Playing && self.health.hurt() {
                let player = self.player.position(graph);
                if self.hearts.take(graph, player) {
                    let whole = self.health.heart_and_whole();
                    self.health_sounds.play(graph, Heard::Heart, player);
                    if whole {
                        self.health_sounds.play(graph, Heard::Whole, player);
                    }
                }
            }
        }
    }

    /// Starts using the computer, if the player is at it: the view goes in to its screen, and the
    /// keys are for typing.
    fn start_hacking(&mut self, ctx: &mut PluginContext) {
        if self.phase != Phase::Playing || self.busy() {
            return;
        }
        let Some((n, _)) = self.at_computer else {
            return;
        };
        let Some(computer) = self.computers.get(n) else {
            return;
        };
        let screen = computer.screen(&ctx.scenes[self.scene].graph);
        self.hacking = true;
        // Off the screen of the one used last, should it be another.
        if self.using.is_some_and(|last| last != n) {
            if let Some(last) = self.using.and_then(|last| self.computers.get(last)) {
                last.show_on_screen(None);
            }
            self.screen_terminal.forget();
        }
        self.using = Some(n);
        self.player.release_keys();
        self.player.use_screen(Some(screen));
        self.dialogue.set_prompt(ctx.user_interfaces.first(), None);
        self.at_computer = None;
        self.want_mouse = false;
        self.set_mouse_captured(ctx, false);
    }

    /// Walks away from the computer, if the player is at it.
    fn stop_hacking(&mut self, ctx: &mut PluginContext) {
        if !self.hacking {
            return;
        }
        self.hacking = false;
        self.player.use_screen(None);
        self.want_mouse = !self.menu.is_open();
        let _ = ctx;
    }

    /// Keeps the view on the computer's screen while the player uses it.
    fn keep_hacking(&mut self, ctx: &mut PluginContext) {
        let computer = self.using.and_then(|n| self.computers.get(n));
        if let (true, Some(computer)) = (self.hacking, computer) {
            let screen = computer.screen(&ctx.scenes[self.scene].graph);
            self.player.use_screen(Some(screen));
        }
    }

    /// A key pressed at the computer: Enter to breach it, try again or open a file; up and down
    /// to go through its files or down the one open; Backspace to go back to the listing; and
    /// Tab to walk away. What is typed goes to it in `on_os_event`.
    fn on_hacking_key(&mut self, ctx: &mut PluginContext, code: KeyCode) {
        let graph = &mut ctx.scenes[self.scene].graph;
        let Some(computer) = self.using.and_then(|n| self.computers.get_mut(n)) else {
            return;
        };
        match code {
            KeyCode::Enter | KeyCode::NumpadEnter => computer.enter(graph),
            KeyCode::ArrowUp => computer.step(graph, -1),
            KeyCode::ArrowDown => computer.step(graph, 1),
            KeyCode::PageUp => computer.step(graph, -(computer::PAGE as i32)),
            KeyCode::PageDown => computer.step(graph, computer::PAGE as i32),
            KeyCode::Backspace => computer.back(graph),
            KeyCode::Tab => self.stop_hacking(ctx),
            _ => (),
        }
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
        self.terminal = Terminal::build(ctx.user_interfaces.first_mut());
        self.use_script(&ctx.resource_manager, SCRIPT);
        self.notes = Notes::load(NOTES)
            .inspect_err(|error| Log::err(format!("Maze: the computers hold no notes: {error}")))
            .ok();
        self.voices = Voices::load(VOICES)
            .inspect_err(|error| Log::err(format!("Maze: the droids are silent: {error}")))
            .ok();
        self.drone_voice = Chirps::load(DRONE_VOICE)
            .and_then(|chirps| Ok((chirps, DroneLines::load(DRONE_LINES)?)))
            .inspect_err(|error| Log::err(format!("Maze: the drones are silent: {error}")))
            .ok();
        self.menu = PauseMenu::build(ctx.user_interfaces.first_mut(), "Start again");
        // Over the pause menu. With a model to play from MAZE_MODEL, straight into the maze.
        self.main_menu = MainMenu::build(ctx.user_interfaces.first_mut());
        if platform::var("MAZE_MODEL").is_some() {
            self.play(&mut ctx, Game::Maze);
        } else {
            self.main_menu.set_open(ctx.user_interfaces.first(), true);
        }
        // An interface of its own, after the window's, which stays the first.
        self.screen_terminal = ScreenTerminal::build(ctx.user_interfaces);
        Ok(())
    }

    fn on_graphics_context_initialized(&mut self, ctx: PluginContext) -> GameResult {
        let GraphicsContext::Initialized(graphics_context) = &*ctx.graphics_context else {
            return Ok(());
        };
        graphics_context.window.set_title("Ruptura Systematis");
        if !diagnostics::has_expected_backend(graphics_context) {
            ctx.loop_controller.exit();
            return Ok(());
        }
        self.want_mouse = self.phase != Phase::Title;
        Ok(())
    }

    fn update(&mut self, ctx: &mut PluginContext) -> GameResult {
        // The droid joins the player whenever it has loaded; the game goes on without it if it
        // cannot, seen through the player's own eyes.
        if let Some(droid) = self.droid.take_if(|droid| droid.is_ok()) {
            self.player
                .attach_avatar(&mut ctx.scenes[self.scene], &droid);
            self.droid_model = Some(droid);
        } else if self.droid.as_ref().is_some_and(|d| d.is_failed_to_load()) {
            Log::err(format!(
                "Could not load {DROID_MODEL}; playing in first person"
            ));
            self.droid = None;
        }

        self.set_up_computer(ctx);
        self.set_up_drone(ctx);
        self.set_up_hearts(ctx);

        match self.phase {
            // While the menu is open nothing happens: no loading, no clock, no player.
            _ if self.menu.is_open() => (),
            Phase::Title => (),
            Phase::Loading => {
                let models: Vec<(String, ModelResource)> = match (&self.model, &self.prefabs) {
                    (Some(model), _) => {
                        let mut models = vec![(self.model_path.clone(), model.clone())];
                        models.extend(self.firewalls.models());
                        models
                    }
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
                // Talking stops the clock, and holds the player where they are; so does using the
                // computer, but the clock runs on.
                let talking = self.talking.is_some();
                if !talking {
                    self.round_time += ctx.dt;
                }
                self.keep_talking(ctx);
                self.keep_hacking(ctx);
                let scene = &mut ctx.scenes[self.scene];
                self.player.update(
                    &mut scene.graph,
                    ctx.dt,
                    self.focused && !talking && !self.hacking,
                );
                self.land_shots(ctx);
                if !talking {
                    self.threaten(ctx);
                }
                if !self.move_inhabitants(ctx) {
                    let scene = &mut ctx.scenes[self.scene];
                    let exit = scene.graph[self.exit].global_position();
                    let player = self.player.position(&scene.graph);
                    let flat = Vector3::new(exit.x - player.x, 0.0, exit.z - player.z);
                    // In capture the flag, blue's flag first - only there to take once its
                    // firewall is down, and reached from beside its plinth - and then home with
                    // it, to red's, which the marker moves to.
                    let won = match self.ctf.clone() {
                        Some(bases) if !self.carrying => {
                            let open = self.firewalls.of(ENEMY.name()).is_none_or(|wall| wall.is_open());
                            if open && flat.norm() < firewall::REACH {
                                self.carrying = true;
                                scene.graph[self.exit]
                                    .local_transform_mut()
                                    .set_position(bases.flag(PLAYERS) + Vector3::new(0.0, FLAG_MARKER, 0.0));
                                self.hud.show_note(format!("You have {}'s flag: bring it home", ENEMY.name()));
                                Log::info(format!("Capture the flag: the player has {}'s flag", ENEMY.name()));
                            }
                            false
                        }
                        Some(_) => flat.norm() < HOME_REACH,
                        None => flat.norm() < EXIT_RADIUS,
                    };
                    if won {
                        self.phase = Phase::Won;
                        let best = self
                            .best_time
                            .map_or(self.round_time, |b| b.min(self.round_time));
                        let record = self.best_time.is_none_or(|b| self.round_time < b);
                        self.best_time = Some(best);
                        let text = format!(
                            "{} in {}{}\nPress N for {}",
                            match self.ctf {
                                Some(_) => format!("You captured {}'s flag", ENEMY.name()),
                                None => "You escaped".to_string(),
                            },
                            hud::format_time(self.round_time),
                            if record { " - a new best!" } else { "" },
                            if self.model.is_some() { "another round" } else { "another maze" }
                        );
                        self.set_banner(ctx, &text);
                    }
                }
            }
            Phase::Broken => (),
            Phase::Won => {
                let scene = &mut ctx.scenes[self.scene];
                self.player.update(&mut scene.graph, ctx.dt, false);
                self.land_shots(ctx);
                self.move_inhabitants(ctx);
            }
            Phase::Deleted => {
                let scene = &mut ctx.scenes[self.scene];
                self.player.update(&mut scene.graph, ctx.dt, false);
                self.land_shots(ctx);
                self.move_inhabitants(ctx);
                self.deleted += ctx.dt;
                if self.deleted > DELETED_FOR {
                    self.restart(ctx);
                }
            }
        }

        // Only what can be seen from where the player stands is drawn and lit. Not while the
        // level is being readied: the survey measures the tiles, which must all be showing.
        if matches!(self.phase, Phase::Playing | Phase::Won | Phase::Deleted) {
            let scene = &mut ctx.scenes[self.scene];
            let player = self.player.position(&scene.graph);
            self.level.cull(&mut scene.graph, player);
            self.inhabitants.show(&mut scene.graph, &self.level);
            let level = &self.level;
            self.hearts.cull(&mut scene.graph, |at| level.can_see(at));
        }

        // Blue's flag, once the player has it, on their back.
        if self.carrying && matches!(self.phase, Phase::Playing | Phase::Won) {
            let graph = &mut ctx.scenes[self.scene].graph;
            let (back, ahead) = self.player.back(graph);
            if let Some(wall) = self.firewalls.of(ENEMY.name()) {
                wall.carry(graph, back, ahead);
            }
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
            Phase::Playing | Phase::Won | Phase::Deleted => {
                let graph = &ctx.scenes[self.scene].graph;
                let player = self.player.position(graph);
                let droid = self.player.moving(graph);
                let inhabitant = self.inhabitants.moving(graph, player);
                droid.into_iter().chain(inhabitant).collect()
            }
            _ => Vec::new(),
        };
        self.moving.set(moving);

        // The frames of the computers nearest the player glow on what is round them; the effects
        // light only so many strips.
        // MAZE_AREA_LIGHTS=0 leaves them dark, to compare.
        static LIT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let lit = *LIT.get_or_init(|| platform::var("MAZE_AREA_LIGHTS").as_deref() != Some("0"));
        let lights = if self.computer_placed && lit {
            let graph = &ctx.scenes[self.scene].graph;
            let player = self.player.position(graph);
            let mut nearest: Vec<&Computer> = self.computers.iter().collect();
            nearest.sort_by(|a, b| {
                let away = |c: &Computer| (c.position(graph) - player).norm_squared();
                away(a).total_cmp(&away(b))
            });
            nearest.iter().flat_map(|c| c.lights(graph)).collect()
        } else {
            Vec::new()
        };
        self.area_lights.set(lights);

        #[cfg(target_arch = "wasm32")]
        self.follow_browser_mouse_lock(ctx);
        // A browser only locks the mouse for a click or a key, so there it is asked for in
        // `on_os_event`.
        if self.want_mouse
            && !self.mouse_captured
            && self.focused
            && cfg!(not(target_arch = "wasm32"))
        {
            self.set_mouse_captured(ctx, true);
        }

        if !self.menu.is_open() {
            self.bark_when_made(ctx);
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
        match self.main_menu.choice(message) {
            Some(Start::Play(game)) if self.phase == Phase::Title => self.play(ctx, game),
            Some(Start::Maps) => self.main_menu.show_maps(ctx.user_interfaces.first(), true),
            Some(Start::Back) => self.main_menu.show_maps(ctx.user_interfaces.first(), false),
            Some(Start::Quit) => platform::quit(ctx),
            _ => (),
        }
        match self.menu.choice(message) {
            Some(Choice::MainMenu) => self.to_main_menu(ctx),
            Some(Choice::Resume) => self.set_paused(ctx, false),
            Some(Choice::Lights) => {
                self.lights_off = !self.lights_off;
                self.apply_lights(ctx);
            }
            Some(Choice::Options) => self.menu.set_in_options(ctx.user_interfaces.first(), true),
            Some(Choice::Back) => self.menu.set_in_options(ctx.user_interfaces.first(), false),
            Some(Choice::LatinSubtitles) => {
                self.subtitles.latin = !self.subtitles.latin;
                self.apply_subtitles(ctx);
            }
            Some(Choice::EnglishSubtitles) => {
                self.subtitles.english = !self.subtitles.english;
                self.apply_subtitles(ctx);
            }
            Some(Choice::Restart) => self.restart(ctx),
            Some(Choice::Quit) => platform::quit(ctx),
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
                WindowEvent::ModifiersChanged(modifiers) => {
                    self.shift = modifiers.state().shift_key();
                }
                WindowEvent::KeyboardInput { event: input, .. } => {
                    if let PhysicalKey::Code(code) = input.physical_key {
                        let pressed = input.state == ElementState::Pressed;
                        // The menu, talking and the computer hold the player still; letting go of a
                        // key still counts.
                        if (!self.menu.is_open() && !self.busy()) || !pressed {
                            self.player.on_key(code, pressed);
                        }
                        // At the computer, what is typed goes to it; Enter and Tab are keys.
                        if pressed && !input.repeat && self.hacking && !self.menu.is_open() {
                            if let (Some(text), Some(computer)) =
                                (&input.text, self.using.and_then(|n| self.computers.get_mut(n)))
                            {
                                let graph = &mut ctx.scenes[self.scene].graph;
                                for c in text.chars().filter(|c| !c.is_control()) {
                                    computer.type_char(graph, computer::without_caps_lock(c, self.shift));
                                }
                            }
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
                    if held && !self.menu.is_open() && !self.busy() {
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
                    if !self.menu.is_open() && !self.busy() {
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
                    if held && !self.menu.is_open() && !self.busy() {
                        self.want_mouse = true;
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    // With the menu open, or talking, a click is for that.
                    if !self.menu.is_open() && !self.busy() {
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
        // A browser only locks the mouse in answer to a click or a key.
        if cfg!(target_arch = "wasm32") && self.want_mouse && !self.mouse_captured {
            if let Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        ..
                    }
                    | WindowEvent::KeyboardInput { .. },
                ..
            } = event
            {
                self.set_mouse_captured(&mut ctx, true);
            }
        }
        Ok(())
    }
}

/// A target for a drone on the player's side with no one of the other side to go after: no one,
/// far out of sight.
const NOBODY: Target = Target {
    feet: Vector3::new(0.0, -1000.0, 0.0),
    middle: Vector3::new(0.0, -1000.0, 0.0),
    collider: Handle::NONE,
    posture: Posture::Standing,
    in_the_dark: true,
};

/// Which of the sides' bolts `side`'s are.
fn side_index(side: Side) -> usize {
    match side {
        Side::Red => 0,
        Side::Blue => 1,
    }
}

/// What a bolt from `from` along `way` hits first within `reach`, but for anything `passed`, if
/// anything: how far along, and what.
fn first_hit(
    graph: &Graph,
    from: Vector3<f32>,
    way: Vector3<f32>,
    reach: f32,
    passed: &[Handle<Collider>],
) -> Option<(f32, Handle<Collider>)> {
    let mut hits = Vec::new();
    graph.physics.cast_ray(
        RayCastOptions {
            ray_origin: Point3::from(from),
            ray_direction: way,
            max_len: reach,
            groups: Default::default(),
            sort_results: true,
        },
        &mut hits,
    );
    hits.iter()
        .find(|hit| !passed.contains(&hit.collider))
        .map(|hit| (hit.toi, hit.collider))
}
