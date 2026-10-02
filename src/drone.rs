//! A security drone, patrolling the maze. One is put down each round well away from where the
//! player starts, and hovers [`HOVER`] above the floor along the corridors, going from one spot a
//! trip away to the next and waiting a while at each - paying the player no heed, calm. More wait
//! out of sight, to be called in (see [`Drone::call_in`]): a failed hack brings one.
//!
//! It turns hostile when the alarm is sounded (see [`Drone::alarm`]) or when the player's pistol
//! hits a droid - or the drone itself (see [`Drone::shot`]). Then it goes about the player as a
//! sentry does, but by air:
//!
//! - **Searching**: it flies to where the player was and scans there, then to one spot after
//!   another nearby, scanning at each, for [`SEARCH`] seconds; and then it is calm again, and
//!   patrols as before. It sees in front of it as a sentry that is not on Alert does - less far
//!   the lower the player is, and with the lights off.
//! - **Alert**: it can see the player. It comes on to [`ENGAGE`] from them, keeps facing them and
//!   fires every [`FIRE_EVERY`] seconds while it can see them (see [`crate::drone_shot`]), each
//!   shot hurting them where it hits. Losing them, it searches.
//!
//! [`HITS`] bolts from the pistol bring it down: it sputters, its rings break into four pieces
//! each, which fly apart, and it drops with them to the floor, where it lies for the rest of the
//! round. Its body is solid, up and down: a ball the player cannot walk through, nor the droids
//! lying in the maze tumble through, nor bolts fly through, the wreck on the floor too. Flying, it
//! never pushes into anyone: someone in its way, it hovers where it is until they move.
//!
//! Its model, [`DRONE_MODEL`], is made in Blender: `drone_root`, which the animations move about,
//! with the eye, the shell and two rings under it, and the ring pieces beside it. The rings' spin
//! is in the animations, but not its glow, which is played here to go with each: what its green
//! materials glow with, over the animation's frames at 24 a second ([`glow`]); and what colour,
//! which is its mood's ([`Drone::mood`]), as a droid's eyes show theirs - its own green calm,
//! orange searching and red on Alert - turning from one to the next over a moment.
//!
//! It lights what is round it too, in its colour, as brightly as it glows: a lamp just in front
//! of its eye, whose shadows are traced as every light's are. Not inside it, where the drone's own
//! shell and eye, which are traced with the rest of the scene, would shut the light in.
//!
//! It speaks System Latin as things happen to it - hearing the alarm, spotting the player,
//! starting to search, giving up and going down - as [`DRONE_LINES`] has it, said as beeps, hums
//! and buzzes (see [`crate::formants::chirps`]), which no one would take for speech.

use crate::{
    ctf::Side,
    player::pistol::Pass,
    ragdoll::CHARACTERS,
    dialogue::{screen, Mood},
    fixtures::{glow_strength, DIFFUSE_COLOR, EMISSION_STRENGTH},
    formants::Curve,
    inhabitants::{self, Alert},
    layout::{Rng, WalkGrid},
    player::posture::Posture,
    survey,
};
use fyrox::{
    core::{
        algebra::{Isometry3, Point3, UnitQuaternion, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{MaterialProperty, MaterialResource},
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        animation::{Animation, AnimationPlayer},
        base::BaseBuilder,
        collider::{BitMask, Collider, ColliderBuilder, ColliderShape, InteractionGroups},
        graph::{
            physics::{geometry::Ball, QueryFilter, RayCastOptions},
            Graph,
        },
        light::{
            point::{PointLight, PointLightBuilder},
            BaseLightBuilder,
        },
        mesh::Mesh,
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        Scene,
    },
};
use hydroxus_ai::sight::Sight;
use serde::Deserialize;
use std::{collections::HashMap, f32::consts::PI};

/// The drone's model, and what drones say.
pub const DRONE_MODEL: &str = "data/drone.glb";
pub const DRONE_LINES: &str = "data/dialogue/drone.json";

/// A line a drone says: in System Latin, and what that means in English.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Said {
    pub says: String,
    pub means: String,
}

/// What drones say, by what they say it about: see [`Drone::update`].
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct DroneLines {
    pub lines: HashMap<String, Vec<Said>>,
}

impl DroneLines {
    /// The lines in the file at `path`.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = crate::platform::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))
    }

    /// The `n`th line, going round, for what is called `name`; None for something with none.
    pub fn line(&self, name: &str, n: usize) -> Option<&Said> {
        let lines = self.lines.get(name)?;
        lines.get(n % lines.len().max(1))
    }
}

/// How much the model is scaled: as made it is 6.5 m across its rings, and 1.3 m in the game.
const SCALE: f32 = 0.2;
/// How high it hovers, in meters above the floor. The death animation drops it this far.
pub const HOVER: f32 = 1.7;
/// How far from where the player starts it is put down, in steps across the grid's half-meter
/// cells, at least: out of sight.
const AWAY_FROM_PLAYER: f32 = 40.0;
/// How far off one called in is put down, in the same steps, at most: out of sight, but near
/// enough to be heard coming, and to come.
const CALLED_FROM: f32 = 80.0;
/// How near where it is called to a patrolling drone has to be, in meters as the crow flies, to
/// answer the call itself, rather than one being called in from nearer.
pub const ANSWERS_WITHIN: f32 = 25.0;
/// How far it goes on patrol each time it sets off, and how far between the spots it searches,
/// in steps across the grid, from least to most; and how long it waits between trips, in seconds.
const TRIP: (f32, f32) = (30.0, 120.0);
/// On a side in capture the flag, how far from its flag it patrols, in meters.
const POST_REACH: f32 = 9.0;
const SEARCH_TRIP: (f32, f32) = (10.0, 40.0);
const REST: (f32, f32) = (3.0, 8.0);
/// How fast it flies, in meters per second: patrolling, and hostile; how quickly it gets up to
/// speed, in meters per second per second; and how quickly it turns, in radians a second.
const PATROL_SPEED: f32 = 1.4;
const HURRY_SPEED: f32 = 3.5;
const ACCELERATION: f32 = 3.0;
const TURN_RATE: f32 = 3.0;
/// How near the next point on its route it has to get, in meters, before making for the one after,
/// and how near the last counts as there.
const REACHED: f32 = 0.6;
const ARRIVED: f32 = 0.15;
/// How quickly it follows the floor up and down.
const FLOOR_EASING: f32 = 4.0;
/// How long it searches once it has lost the player, or been sent to look for them, in seconds.
pub const SEARCH: f32 = 30.0;
/// On Alert: how near the player it comes, in meters, how far off it fires from, how often, and
/// how soon after spotting them the first time; and how often it works out its way to them again.
const ENGAGE: f32 = 6.0;
const FIRE_RANGE: f32 = 20.0;
const FIRE_EVERY: f32 = 1.5;
const FIRST_SHOT: f32 = 0.8;
const REPLAN: f32 = 0.5;
/// How many of the pistol's bolts bring it down.
pub const HITS: u32 = 3;
/// Its body: a ball this big across its middle, in meters.
const BODY_RADIUS: f32 = 0.3;
/// How near anyone it comes, flying, in meters between its body and theirs.
const CLEARANCE: f32 = 0.05;
/// How near a bolt has to fly past its body to be taken as shot at, in meters.
const NEAR_MISS: f32 = 1.0;
/// Its lamp: where it is, in the model's own terms along the way it faces - just clear of the
/// front of its eye, which reaches 1.4 out from the middle; how bright it is for each strength of
/// [`glow`]; and how far it reaches, in meters.
const LAMP_AT: f32 = 1.7;
const LAMP_BRIGHTNESS: f32 = 1.5;
const LAMP_REACH: f32 = 5.0;
/// Where its shots leave from, in the model's own terms along the way it faces: just clear of the
/// front of its eye and of its body; and the frame of `drone_fire` they leave on, as the eye
/// flashes.
const MUZZLE: f32 = 1.7;
const FIRE_AT: f32 = 2.0;
/// How long `drone_fire` and `drone_scan` last, in seconds.
const FIRE_LENGTH: f32 = 25.0 / FPS;
const SCAN_LENGTH: f32 = 97.0 / FPS;
/// How quickly its colour turns to its mood's: most of the way in a third of a second.
const MOOD_RATE: f32 = 6.0;
/// Its node that the animations move about, which the lamp goes along with.
const BODY: &str = "drone_root";
/// The material property that says how much like metal a surface is, from 0 to 1.
const METALLIC_FACTOR: &str = "metallicFactor";
/// The animations' frames a second.
const FPS: f32 = 24.0;
/// How many times as strong its glow is in the game as [`glow`] has it, as made in Blender: the
/// engine only glows round what is brighter than 1.01, and green light counts for less than
/// three quarters of how bright it is, so that as made its green would only be coloured.
const BRIGHTNESS: f32 = 3.0;
/// Its animations: the loops it hovers in, still and on the move; and the rest, each played once.
const IDLE: &str = "drone_idle";
const PATROL: &str = "drone_patrol";
const SCAN: &str = "drone_scan";
const FIRE: &str = "drone_fire";
const DEATH: &str = "drone_death";
const ANIMATIONS: [&str; 5] = [IDLE, PATROL, SCAN, FIRE, DEATH];

/// How strongly its green materials glow `frame` frames into the animation called `name`: as
/// many times as bright as green light of strength 1.
pub fn glow(name: &str, frame: f32) -> f32 {
    let curve = match name {
        // A slow breath, in and out once a loop of 97 frames.
        "drone_idle" => {
            return 1.2 - 0.3 * (frame / 97.0 * std::f32::consts::TAU).cos();
        }
        "drone_scan" => Curve(vec![
            [77.0, 1.0],
            [78.0, 5.0],
            [81.0, 2.0],
            [84.0, 5.0],
            [87.0, 2.0],
            [90.0, 3.0],
        ]),
        "drone_fire" => Curve(vec![[0.0, 1.0], [2.0, 8.0], [14.0, 1.0]]),
        // Flickering out, and then fading.
        "drone_death" => {
            let mut points = vec![[0.0, 1.0]];
            points.extend((0..=10).map(|k| [1.0 + 2.0 * k as f32, if k % 2 == 0 { 4.0 } else { 0.0 }]));
            points.push([40.0, 0.0]);
            Curve(points)
        }
        _ => return 1.0,
    };
    curve.at(frame)
}

/// The colour a glow whose own colour is `own` has in `mood`, strength 1 in its brightest part.
pub(crate) fn mood_colour(own: Vector3<f32>, mood: Mood) -> Vector3<f32> {
    screen::eyes(mood).map_or(own, |colour| {
        let colour = Vector3::new(colour.r, colour.g, colour.b).cast::<f32>();
        colour / colour.max().max(1.0)
    })
}

/// A colour of strength 1 at its brightest, as a colour to light with.
pub(crate) fn light_colour(colour: Vector3<f32>) -> Color {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::opaque(byte(colour.x), byte(colour.y), byte(colour.z))
}

/// `angle` brought round to between -π and π.
fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(2.0 * PI) - PI
}

/// `vector` along the ground.
fn flat(vector: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(vector.x, 0.0, vector.z)
}

/// How the drone is going about the player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    /// Calm: it patrols, paying the player no heed.
    Patrol,
    /// It can see the player, and fires at them.
    Alert,
    /// It is looking for the player, `left` seconds more.
    Search { left: f32 },
    /// Brought down.
    Down,
}

/// Where the player is, for the drone to look for them: their feet and the middle of their body,
/// what their body is to anything that hits it, how they hold themselves, and whether it is too
/// dark to see them far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub feet: Vector3<f32>,
    pub middle: Vector3<f32>,
    pub collider: Handle<Collider>,
    pub posture: Posture,
    pub in_the_dark: bool,
}

/// What the drone did this frame: what it has something to say about, as [`DRONE_LINES`] names it; and
/// where a shot leaves from, and its colour, if it has fired one.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Doing {
    pub says: Option<&'static str>,
    pub fired: Option<(Vector3<f32>, Vector3<f32>)>,
}

/// The drone in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Drone {
    root: Handle<Node>,
    player: Handle<Node>,
    /// What the animations move about, and its lamp.
    body: Handle<Node>,
    lamp: Handle<Node>,
    /// Its body, and the ball that is, while it is in the round.
    hull: Handle<Node>,
    collider: Handle<Collider>,
    /// Its animations by name.
    animations: Vec<(String, Handle<Animation>)>,
    /// Its own copies of the materials that glow, and which way their glow's colour goes.
    glows: Vec<(MaterialResource, Vector3<f32>)>,
    /// The colour it glows now, on its way to its mood's, as [`mood_colour`] has it.
    colour: Vector3<f32>,
    /// The animation it is playing, and for how long it has, in seconds; and whether it has fired
    /// this time through `drone_fire`.
    playing: &'static str,
    since: f32,
    fired: bool,
    /// How it is going about the player.
    state: State,
    /// Whether it has been put down in this round's maze.
    placed: bool,
    /// Where it hovers - the model's root - which way it faces, in radians, left positive from
    /// the world's +z, and how fast it is flying, in meters per second.
    at: Vector3<f32>,
    heading: f32,
    speed: f32,
    /// The rest of its route, as points on the floor, the next one last.
    route: Vec<Vector3<f32>>,
    /// How long it has left to wait before its next trip, or to scan where it is, in seconds.
    resting: f32,
    scanning: f32,
    /// Where it last saw the player's feet, or was sent to look for them; and how many spots it has
    /// searched since.
    lost_at: Vector3<f32>,
    searched: u32,
    /// On Alert, how long until it fires again, and until it works out its way to them again.
    fire_in: f32,
    replan: f32,
    /// How many of the pistol's bolts have hit it.
    hits: u32,
    /// In capture the flag, its side, and where it patrols round: its side's flag. On a side,
    /// it watches all the while, patrolling too, and what it goes after is whichever of the other
    /// side the game shows it (see [`crate::ctf`]).
    side: Option<Side>,
    post: Option<Vector3<f32>>,
}

impl Drone {
    /// Puts the drone into `scene` from its `model`, out of sight until it is placed. None if the
    /// model has no animations.
    pub fn spawn(model: &ModelResource, scene: &mut Scene) -> Option<Self> {
        let root = model.instantiate(scene);
        let graph = &mut scene.graph;
        graph[root]
            .local_transform_mut()
            .set_scale(Vector3::repeat(SCALE));
        graph[root].set_visibility(false);
        let nodes: Vec<Handle<Node>> = graph.traverse_handle_iter(root).collect();
        let Some(player) = nodes
            .iter()
            .copied()
            .find(|&node| graph[node].cast::<AnimationPlayer>().is_some())
        else {
            Log::err(format!("Drone: {DRONE_MODEL} has no animations"));
            graph.remove_node(root);
            return None;
        };
        // Its own shell and rings, just behind its lamp, would throw shadows across the corridor
        // from shadow maps. Traced shadows leave it out, as they are gathered once; so do these.
        for &node in &nodes {
            if graph[node].cast::<Mesh>().is_some() {
                graph[node].set_cast_shadows(false);
            }
        }
        let body = graph.find_by_name(root, BODY).map_or(root, |(body, _)| body);
        // Not scattering into a haze in the air: the drone is what glows.
        let lamp = PointLightBuilder::new(
            BaseLightBuilder::new(BaseBuilder::new().with_visibility(false))
                .with_intensity(LAMP_BRIGHTNESS)
                .with_scatter_enabled(false),
        )
        .with_radius(LAMP_REACH)
        .build(graph)
        .to_base();
        let glows = claim_glows(graph, &nodes);
        Log::info(format!("Drone: {} glowing materials", glows.len()));
        let animations = graph
            .try_get_mut_of_type::<AnimationPlayer>(player)
            .ok()?
            .animations_mut()
            .get_value_mut_silent()
            .pair_iter_mut()
            .map(|(handle, animation)| {
                animation.set_enabled(false);
                (animation.name().to_owned(), handle)
            })
            .collect::<Vec<_>>();
        for name in ANIMATIONS {
            if !animations.iter().any(|(had, _)| had == name) {
                Log::warn(format!("Drone: {DRONE_MODEL} has no {name}"));
            }
        }
        let mut drone = Self {
            root,
            player,
            body,
            lamp,
            // Given a body once it is put down.
            hull: Handle::NONE,
            collider: Handle::NONE,
            animations,
            colour: glows.first().map_or(Vector3::new(0.0, 1.0, 0.0), |(_, own)| *own),
            glows,
            playing: DEATH,
            since: 0.0,
            fired: false,
            state: State::Patrol,
            placed: false,
            at: Vector3::zeros(),
            heading: 0.0,
            speed: 0.0,
            route: Vec::new(),
            resting: 0.0,
            scanning: 0.0,
            lost_at: Vector3::zeros(),
            searched: 0,
            fire_in: 0.0,
            replan: 0.0,
            hits: 0,
            side: None,
            post: None,
        };
        drone.play(graph, IDLE);
        Some(drone)
    }

    /// Puts it down afresh, calm and whole, over the floor of `grid` whose corner is at `origin`,
    /// well away from the player's `feet`. Whether there was anywhere to put it.
    pub fn place(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        feet: Vector3<f32>,
        rng: &mut Rng,
    ) -> bool {
        self.side = None;
        self.post = None;
        self.put_down(graph, (grid, origin), feet, f32::INFINITY, rng)
    }

    /// Puts it down for `side` in capture the flag, by `post`, its side's flag, to patrol round
    /// it. Whether there was anywhere to put it.
    pub fn place_for(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        side: Side,
        post: Vector3<f32>,
        rng: &mut Rng,
    ) -> bool {
        let Some(at) = inhabitants::spot_near((grid, origin), post, POST_REACH, rng) else {
            return false;
        };
        // Put down anywhere, and then where it should be.
        if !self.put_down(graph, (grid, origin), at, f32::INFINITY, rng) {
            return false;
        }
        self.side = Some(side);
        self.post = Some(post);
        self.at = at + Vector3::new(0.0, HOVER, 0.0);
        if let Ok(hull) = graph.try_get_mut_of_type::<RigidBody>(self.hull) {
            hull.local_transform_mut().set_position(self.at);
        }
        self.pose(graph);
        Log::info(format!("Drone: {}'s, by its flag", side.name()));
        true
    }

    /// Its side in capture the flag, if it has one.
    pub fn side(&self) -> Option<Side> {
        self.side
    }

    /// What it would go after, as a target, for another drone: its body, where it hovers.
    pub fn as_target(&self) -> Option<Target> {
        (self.placed && self.state != State::Down).then(|| Target {
            feet: self.at - Vector3::new(0.0, HOVER, 0.0),
            middle: self.at,
            collider: self.collider,
            posture: Posture::Standing,
            in_the_dark: false,
        })
    }

    /// Puts it down as [`Drone::place`] does, no more than `within` steps from the player's
    /// `feet` if there is room.
    fn put_down(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        feet: Vector3<f32>,
        within: f32,
        rng: &mut Rng,
    ) -> bool {
        let Some(start) = survey::cell_at(grid, origin, feet)
            .filter(|&(x, z)| grid.is_walkable(x, z))
            .or_else(|| survey::nearest_walkable(grid, origin, feet))
        else {
            return false;
        };
        let routes = grid.routes_from(start, f32::INFINITY);
        let reached: Vec<(usize, f32)> = routes
            .costs
            .iter()
            .enumerate()
            .filter_map(|(i, cost)| cost.map(|cost| (i, cost)))
            .collect();
        let away: Vec<usize> = reached
            .iter()
            .filter(|&&(_, cost)| cost >= AWAY_FROM_PLAYER && cost <= within.max(AWAY_FROM_PLAYER))
            .map(|&(i, _)| i)
            .collect();
        // Out of the way if there is room; in a small maze, as far off as it can be.
        let cell = match away.is_empty() {
            false => away[rng.below(away.len())],
            true => match reached.iter().max_by(|a, b| a.1.total_cmp(&b.1)) {
                Some(&(i, _)) => i,
                None => return false,
            },
        };
        let (x, z) = (cell % grid.width, cell / grid.width);
        self.at = survey::cell_center(origin, x, z) + Vector3::new(0.0, grid.floor(x, z) + HOVER, 0.0);
        self.heading = inhabitants::between(rng, (-PI, PI));
        self.state = State::Patrol;
        self.placed = true;
        self.speed = 0.0;
        self.route.clear();
        self.resting = inhabitants::between(rng, REST);
        self.scanning = 0.0;
        self.hits = 0;
        if !graph.is_valid_handle(self.hull) {
            (self.hull, self.collider) = hull(graph);
        }
        self.pose(graph);
        if let Ok(hull) = graph.try_get_mut_of_type::<RigidBody>(self.hull) {
            hull.local_transform_mut().set_position(self.at);
        }
        self.play(graph, IDLE);
        graph[self.root].set_visibility(true);
        graph[self.lamp].set_visibility(true);
        Log::info(format!("Drone: patrolling from {:.1} m away", flat(self.at - feet).norm()));
        true
    }

    /// Puts it down afresh as [`Drone::place`] does, well away from the player's `feet`, and sends
    /// it straight to search where they are, `at`, as the alarm would: called in. Whether there
    /// was anywhere to put it.
    pub fn call_in(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        feet: Vector3<f32>,
        at: Vector3<f32>,
        rng: &mut Rng,
    ) -> bool {
        if !self.put_down(graph, (grid, origin), feet, CALLED_FROM, rng) {
            return false;
        }
        self.search(at);
        true
    }

    /// Takes it out of the round: out of sight, and out of the way of bolts, until it is put down
    /// again.
    pub fn hide(&mut self, graph: &mut Graph) {
        self.placed = false;
        self.state = State::Patrol;
        self.route.clear();
        graph[self.root].set_visibility(false);
        graph[self.lamp].set_visibility(false);
        if graph.is_valid_handle(self.hull) {
            graph.remove_node(self.hull);
        }
    }

    /// Whether it is in this round, hovering where it is.
    pub fn is_placed(&self) -> bool {
        self.placed
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Where it is hovering.
    pub fn at(&self) -> Vector3<f32> {
        self.at
    }

    /// What the animations move about: where it speaks from.
    pub fn body(&self) -> Handle<Node> {
        self.body
    }

    /// Its body, while it is in the round.
    pub fn collider(&self) -> Handle<Collider> {
        self.collider
    }

    /// Where its health bar goes - over the top of it - and how much health it has left, from 0
    /// to 1, while it is after the player or has been hit, and is up.
    pub fn health_bar(&self, graph: &Graph) -> Option<(Vector3<f32>, f32)> {
        let shown = match self.state {
            State::Down => false,
            State::Alert | State::Search { .. } => true,
            State::Patrol => self.hits > 0,
        };
        (shown && self.placed).then(|| {
            let top = graph[self.body].global_position() + Vector3::new(0.0, BODY_RADIUS, 0.0);
            (top, 1.0 - self.hits as f32 / HITS as f32)
        })
    }

    /// How it feels, which colours its glow: calm, searching, or after the player.
    pub fn mood(&self) -> Mood {
        match self.state {
            State::Patrol => Mood::Normal,
            State::Search { .. } | State::Down => Mood::Agitated,
            State::Alert => Mood::Hostile,
        }
    }

    /// The alarm sounded, or a droid shot, with the player's feet at `at`: calm or searching, it
    /// searches there. What it has to say about it, if anything.
    pub fn alarm(&mut self, at: Vector3<f32>) -> Option<&'static str> {
        match self.state {
            State::Down | State::Alert => None,
            _ if !self.placed => None,
            // The player's own side's pays the alarm no heed.
            _ if self.side.is_some_and(Side::is_players) => None,
            was => {
                self.search(at);
                (was == State::Patrol).then_some("alarm")
            }
        }
    }

    /// Bolts that flew `passes` this frame, fired by `fired_by`'s side - or the player in the maze,
    /// with none: one that went within [`NEAR_MISS`] of its body, not its own side's, has it search
    /// where it was fired from, unless it is after the player already. What it says, if anything.
    pub fn near_miss(&mut self, passes: &[Pass], fired_by: Option<Side>) -> Option<&'static str> {
        if !self.placed || matches!(self.state, State::Down | State::Alert) {
            return None;
        }
        if fired_by.is_some() && fired_by == self.side {
            return None;
        }
        let pass = passes.iter().find(|pass| pass.nearest(self.at) <= BODY_RADIUS + NEAR_MISS)?;
        let was = self.state;
        self.search(pass.fired_at);
        (was == State::Patrol).then_some("searching")
    }

    /// A bolt that hit `collider`, fired by the player from `from`: if it hit the drone, it takes
    /// the hit, and searches where it came from if it was not after the player already. Whether it
    /// was the drone, and whether that brought it down.
    pub fn shot(&mut self, graph: &mut Graph, collider: Handle<Collider>, from: Vector3<f32>) -> Option<bool> {
        if collider != self.collider || self.state == State::Down || !self.placed {
            return None;
        }
        self.hits += 1;
        if self.hits >= HITS {
            self.state = State::Down;
            self.route.clear();
            self.speed = 0.0;
            // Its body goes down with it (see `update`), and lies on the floor as solid as ever.
            self.play(graph, DEATH);
            return Some(true);
        }
        if self.state != State::Alert {
            self.search(from);
        }
        Some(false)
    }

    /// Searches from `at`, going there first.
    fn search(&mut self, at: Vector3<f32>) {
        self.state = State::Search { left: SEARCH };
        self.lost_at = at;
        self.searched = 0;
        self.route.clear();
        self.scanning = 0.0;
    }

    /// Starts the animation called `name` from its start, looping those it hovers in, unless it
    /// is playing it already.
    fn play(&mut self, graph: &mut Graph, name: &'static str) {
        if self.playing == name {
            return;
        }
        let find = |name: &str| {
            self.animations
                .iter()
                .find(|(had, _)| had == name)
                .map(|&(_, handle)| handle)
        };
        let (before, now) = (find(self.playing), find(name));
        self.playing = name;
        self.since = 0.0;
        self.fired = false;
        let Ok(player) = graph.try_get_mut_of_type::<AnimationPlayer>(self.player) else {
            return;
        };
        let animations = player.animations_mut().get_value_mut_silent();
        if let Some(before) = before.and_then(|h| animations.try_get_mut(h).ok()) {
            before.set_enabled(false);
        }
        if let Some(now) = now.and_then(|h| animations.try_get_mut(h).ok()) {
            now.set_loop(name == IDLE || name == PATROL)
                .set_enabled(true)
                .rewind();
        }
    }

    /// Where it is and which way it faces, as it is.
    fn pose(&self, graph: &mut Graph) {
        graph[self.root]
            .local_transform_mut()
            .set_position(self.at)
            .set_rotation(UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.heading));
    }

    /// Whether it can see the `player`: in front of it, as a sentry sees - from as far as it sees
    /// at all on Alert - and with nothing in the way from its eye to the middle of their body.
    /// Whether it can see `player` - or whoever else it is shown as a target - from where it is.
    pub fn sees(&self, graph: &Graph, player: &Target) -> bool {
        if !self.placed || self.state == State::Down {
            return false;
        }
        let alert = match self.state {
            State::Alert => Alert::Alert,
            _ => Alert::Evasion,
        };
        let under = self.at - Vector3::new(0.0, HOVER, 0.0);
        let stance = player.posture.into();
        if !Sight::default().could_see(Some(alert), under, self.heading, player.feet, stance, player.in_the_dark) {
            return false;
        }
        let from = graph[self.body].global_position();
        let way = player.middle - from;
        let length = way.norm();
        if length < 1.0e-3 {
            return true;
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
        hits.iter()
            .find(|hit| hit.collider != self.collider)
            .is_some_and(|hit| hit.collider == player.collider)
    }

    /// Goes about its business for another `dt` over the floor of `grid` whose corner is at
    /// `origin`, with the `player` where they are - if `live`, as while a round is being played;
    /// otherwise it only glows, hovering where it is. What it did.
    pub fn update(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: &Target,
        rng: &mut Rng,
        dt: f32,
        live: bool,
    ) -> Doing {
        let mut doing = Doing::default();
        if live && self.placed {
            self.since += dt;
            doing.says = self.think(graph, (grid, origin), player, rng, dt);
            self.fly(graph, (grid, origin), player, dt);
            self.pose(graph);
            // Its body goes where the animations take it: bobbing as it hovers, and down to the
            // floor when it is shot down. As of the last frame, which is as near as matters.
            let at = match self.state {
                State::Down => graph[self.body].global_position(),
                _ => self.at,
            };
            // Moved by hand, it would push anyone in its way with no end of force - down through
            // the floor, even, settling or falling onto them. So it never moves into anyone: it
            // stays put until they are out of the way.
            if graph.is_valid_handle(self.hull) {
                let from = graph[self.hull].global_position();
                if !blocked(graph, self.hull, from, at - from) {
                    if let Ok(hull) = graph.try_get_mut_of_type::<RigidBody>(self.hull) {
                        hull.set_next_kinematic_translation(at);
                    }
                }
            }
            // Which animation goes with what it is doing: firing and scanning play through.
            let next = match self.state {
                State::Down => DEATH,
                _ if self.playing == FIRE && self.since < FIRE_LENGTH => FIRE,
                State::Alert if self.fire_in <= 0.0 => {
                    self.fire_in = FIRE_EVERY;
                    FIRE
                }
                State::Search { .. } if self.scanning > 0.0 => SCAN,
                _ if self.speed > 0.2 => PATROL,
                _ => IDLE,
            };
            self.play(graph, next);
        }
        self.glow(graph, dt);
        // It fires as the eye flashes, at wherever the player is by then.
        let time = self.time(graph);
        if self.playing == FIRE && !self.fired && time * FPS >= FIRE_AT && live {
            self.fired = true;
            let from = graph[self.body]
                .global_transform()
                .transform_point(&Point3::new(0.0, 0.0, MUZZLE))
                .coords;
            doing.fired = Some((from, self.colour));
        }
        doing
    }

    /// Where the animation it is playing is, in seconds.
    fn time(&self, graph: &Graph) -> f32 {
        let handle = self
            .animations
            .iter()
            .find(|(had, _)| had == self.playing)
            .map(|&(_, handle)| handle);
        handle
            .and_then(|handle| {
                let player = graph.try_get_of_type::<AnimationPlayer>(self.player).ok()?;
                Some(player.animations().try_get(handle).ok()?.time_position())
            })
            .unwrap_or(0.0)
    }

    /// Looks for the player and decides where to go. What it has to say, if anything.
    fn think(
        &mut self,
        graph: &Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: &Target,
        rng: &mut Rng,
        dt: f32,
    ) -> Option<&'static str> {
        // On a side, it watches all the while.
        let hostile = matches!(self.state, State::Alert | State::Search { .. }) || self.side.is_some();
        let sees = hostile && self.sees(graph, player);
        let under = self.at - Vector3::new(0.0, HOVER, 0.0);
        match self.state {
            State::Down => None,
            State::Patrol if sees => {
                self.state = State::Alert;
                self.fire_in = FIRST_SHOT;
                self.replan = 0.0;
                self.scanning = 0.0;
                Some("spotted")
            }
            State::Patrol => {
                if self.route.is_empty() {
                    self.resting -= dt;
                    if self.resting <= 0.0 {
                        self.route = match self.post {
                            // On a side, round its flag.
                            Some(post) => inhabitants::spot_near((grid, origin), post, POST_REACH, rng)
                                .map(|to| inhabitants::route_to((grid, origin), under, to, f32::INFINITY))
                                .unwrap_or_default(),
                            None => inhabitants::plan((grid, origin), under, TRIP, rng),
                        };
                        self.resting = inhabitants::between(rng, REST);
                    }
                }
                None
            }
            State::Search { .. } if sees => {
                self.state = State::Alert;
                self.fire_in = FIRST_SHOT;
                self.replan = 0.0;
                self.scanning = 0.0;
                Some("spotted")
            }
            State::Alert if !sees => {
                let at = self.lost_at;
                self.search(at);
                Some("searching")
            }
            State::Alert => {
                self.lost_at = player.feet;
                self.fire_in -= dt;
                // Facing them, coming on to within its range of them, and no nearer.
                let to = flat(player.feet - under);
                self.heading = to.x.atan2(to.z);
                self.replan -= dt;
                if to.norm() <= ENGAGE {
                    self.route.clear();
                } else if self.replan <= 0.0 || self.route.is_empty() {
                    self.replan = REPLAN;
                    self.route = inhabitants::route_to((grid, origin), under, player.feet, f32::INFINITY);
                }
                // Out of range, it holds its fire till it is nearer.
                if to.norm() > FIRE_RANGE {
                    self.fire_in = self.fire_in.max(0.1);
                }
                None
            }
            State::Search { left } => {
                // Its time runs from when it gets to where it was sent, not while it is on its way.
                let on_its_way = self.searched <= 1 && self.scanning == 0.0 && !self.route.is_empty();
                let left = if on_its_way { left } else { left - dt };
                if left <= 0.0 {
                    self.state = State::Patrol;
                    self.route.clear();
                    self.scanning = 0.0;
                    self.resting = REST.0;
                    return Some("calm");
                }
                self.state = State::Search { left };
                let mut says = None;
                if self.scanning > 0.0 {
                    self.scanning -= dt;
                    // Done scanning: on to another spot nearby.
                    if self.scanning <= 0.0 {
                        self.scanning = 0.0;
                        self.searched += 1;
                        self.route = inhabitants::plan((grid, origin), under, SEARCH_TRIP, rng);
                    }
                } else if self.route.is_empty() {
                    if self.searched == 0 && flat(self.lost_at - under).norm() > REACHED {
                        // First where the player was.
                        self.searched = 1;
                        self.route = inhabitants::route_to((grid, origin), under, self.lost_at, f32::INFINITY);
                        says = Some("searching");
                    } else {
                        // Got there, or has nowhere to go: it scans.
                        self.searched = self.searched.max(1);
                        self.scanning = SCAN_LENGTH;
                    }
                }
                says
            }
        }
    }

    /// Flies on along its route for another `dt`, turning to face the way it goes - or the player,
    /// on Alert - and hovering over the floor.
    fn fly(&mut self, graph: &Graph, (grid, origin): (&WalkGrid, Vector3<f32>), player: &Target, dt: f32) {
        if self.state == State::Down {
            return;
        }
        let under = self.at - Vector3::new(0.0, HOVER, 0.0);
        // Past the points it has got near, the last one only once it is there.
        while let Some(&next) = self.route.last() {
            let near = if self.route.len() == 1 { ARRIVED } else { REACHED };
            if flat(next - under).norm() > near {
                break;
            }
            self.route.pop();
        }
        let hurry = matches!(self.state, State::Alert | State::Search { .. });
        let top = if hurry { HURRY_SPEED } else { PATROL_SPEED };
        let (way, wanted) = match self.route.last() {
            Some(&next) => {
                let to = flat(next - under);
                let distance = to.norm();
                // Slowing down for the end of its route.
                let wanted = match self.route.len() {
                    1 => top.min((2.0 * ACCELERATION * distance).sqrt()),
                    _ => top,
                };
                (to / distance.max(1.0e-4), wanted)
            }
            None => (Vector3::zeros(), 0.0),
        };
        self.speed = match wanted > self.speed {
            true => (self.speed + ACCELERATION * dt).min(wanted),
            false => (self.speed - 2.0 * ACCELERATION * dt).max(wanted),
        };
        let mut next = self.at + way * self.speed * dt;
        // Facing the way it goes, or the player on Alert.
        let facing = match self.state {
            State::Alert => {
                let to = flat(player.feet - under);
                (to.norm() > 1.0e-3).then(|| to.x.atan2(to.z))
            }
            _ => (self.speed > 0.1).then(|| way.x.atan2(way.z)),
        };
        if let Some(facing) = facing {
            let turn = wrap(facing - self.heading);
            self.heading = wrap(self.heading + turn.clamp(-TURN_RATE * dt, TURN_RATE * dt));
        }
        // Hovering over the floor where it is, if there is floor there.
        if let Some((x, z)) = survey::cell_at(grid, origin, next).filter(|&(x, z)| grid.is_walkable(x, z)) {
            let height = grid.floor(x, z) + HOVER;
            next.y += (height - next.y) * (1.0 - (-FLOOR_EASING * dt).exp());
        }
        // Never into anyone, sideways, up or down: it waits for them to move.
        if blocked(graph, self.hull, self.at, next - self.at) {
            self.speed = 0.0;
        } else {
            self.at = next;
        }
    }

    /// Glows along with the animation it is playing, in its mood's colour, and lights what is
    /// round it as brightly.
    fn glow(&mut self, graph: &mut Graph, dt: f32) {
        let time = self.time(graph);
        let turned = 1.0 - (-MOOD_RATE * dt).exp();
        let own = self.glows.first().map_or(self.colour, |(_, own)| *own);
        self.colour += (mood_colour(own, self.mood()) - self.colour) * turned;
        let strength = glow(self.playing, time * FPS);
        // Its colour as well as its glow, as a droid's eyes: the eye and the inside of the shell
        // are made green, and would stay green under any glow.
        let colour = light_colour(self.colour);
        for (material, _) in &self.glows {
            let mut material = material.data_ref();
            material.set_property(DIFFUSE_COLOR, colour);
            material.set_property(
                EMISSION_STRENGTH,
                MaterialProperty::Vector3(self.colour * BRIGHTNESS * strength),
            );
        }
        // The lamp, in front of the eye wherever the animation has it, as bright as the glow.
        let at = graph[self.body]
            .global_transform()
            .transform_point(&Point3::new(0.0, 0.0, LAMP_AT))
            .coords;
        graph[self.lamp].local_transform_mut().set_position(at);
        if let Ok(lamp) = graph.try_get_mut_of_type::<PointLight>(self.lamp) {
            lamp.base_light_mut().set_intensity(LAMP_BRIGHTNESS * strength);
            lamp.base_light_mut().set_color(light_colour(self.colour));
        }
    }
}

/// Its body: a solid ball on a body moved by hand, which bumps into everyone and everything but
/// is not one of the characters, whom it looks out for as it flies (see [`blocked`]). The body,
/// and the ball.
fn hull(graph: &mut Graph) -> (Handle<Node>, Handle<Collider>) {
    let collider: Handle<Collider> = ColliderBuilder::new(BaseBuilder::new())
        .with_shape(ColliderShape::ball(BODY_RADIUS))
        .with_collision_groups(InteractionGroups::new(BitMask(!CHARACTERS), BitMask(u32::MAX)))
        .build(graph);
    let body = RigidBodyBuilder::new(BaseBuilder::new().with_child(collider))
        .with_body_type(RigidBodyType::KinematicPositionBased)
        .build(graph)
        .to_base();
    (body, collider)
}

/// Whether the drone whose body is `hull`, its middle at `at`, would run into anyone - the player
/// or a droid - going `step`, or come nearer them than [`CLEARANCE`]. Going away from someone it
/// touches is never blocked, so it can always get clear.
fn blocked(graph: &Graph, hull: Handle<Node>, at: Vector3<f32>, step: Vector3<f32>) -> bool {
    let Some(way) = step.try_normalize(1.0e-6) else {
        return false;
    };
    graph
        .physics
        .cast_shape(
            graph,
            &Ball::new(BODY_RADIUS),
            &Isometry3::translation(at.x, at.y, at.z),
            &(way * (step.norm() + CLEARANCE)),
            1.0,
            false,
            QueryFilter {
                exclude_rigid_body: Some(hull),
                // The characters' capsules alone: the maze and most else are in every group,
                // the characters' among them.
                predicate: Some(&|_, collider: &Collider| {
                    collider.collision_groups().memberships.0 == CHARACTERS
                }),
                ..Default::default()
            },
        )
        .is_some()
}

/// Gives the drone its own copy of each material under `nodes` that glows, so that its glow can
/// change without changing the model's, with which way the colour of its glow goes, at strength 1.
pub(crate) fn claim_glows(graph: &mut Graph, nodes: &[Handle<Node>]) -> Vec<(MaterialResource, Vector3<f32>)> {
    let mut claimed: Vec<(u64, MaterialResource, Vector3<f32>)> = Vec::new();
    for &node in nodes {
        let Some(mesh) = graph[node].cast_mut::<Mesh>() else {
            continue;
        };
        for surface in mesh.surfaces_mut() {
            let original = surface.material().clone();
            let key = original.key();
            if let Some((_, copy, _)) = claimed.iter().find(|(had, _, _)| *had == key) {
                surface.set_material(copy.clone());
                continue;
            }
            let state = original.state();
            let Some(material) = state.data_ref() else {
                continue;
            };
            let colour = match glow_strength(material) {
                Some(MaterialProperty::Vector3(glow)) => glow / glow.max(),
                Some(_) => Vector3::new(0.0, 1.0, 0.0),
                None => continue,
            };
            let mut copy = material.clone();
            // Not metal: the engine lights a metal only by what it reflects, and puts out any glow
            // it has with the rest of its own colour. The model says nothing, which glTF takes
            // for all metal.
            copy.set_property(METALLIC_FACTOR, MaterialProperty::Float(0.0));
            let copy = MaterialResource::new_embedded(copy);
            drop(state);
            surface.set_material(copy.clone());
            claimed.push((key, copy, colour));
        }
    }
    claimed.into_iter().map(|(_, copy, colour)| (copy, colour)).collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_green_glows_past_the_engines_threshold_whenever_it_is_lit() {
        // As the engine weighs green light (Rec. 709).
        let brightest = |strength: f32| 0.7152 * BRIGHTNESS * strength;
        for f in 0..97 {
            assert!(brightest(glow(IDLE, f as f32)) > 1.01, "idle at {f}");
        }
        assert!(brightest(glow(PATROL, 0.0)) > 1.01);
    }

    #[test]
    fn its_glow_follows_each_animation() {
        // Idle breathes between 0.9 and 1.5.
        let idle: Vec<f32> = (0..97).map(|f| glow(IDLE, f as f32)).collect();
        let (low, high) = idle.iter().fold((f32::MAX, f32::MIN), |(l, h), &g| (l.min(g), h.max(g)));
        assert!((low - 0.9).abs() < 0.01 && (high - 1.5).abs() < 0.01, "{low} {high}");
        assert_eq!(glow(PATROL, 30.0), 1.0);
        // Scan pulses at the end.
        assert_eq!(glow(SCAN, 40.0), 1.0);
        assert_eq!(glow(SCAN, 78.0), 5.0);
        assert_eq!(glow(SCAN, 81.0), 2.0);
        assert_eq!(glow(SCAN, 84.0), 5.0);
        assert_eq!(glow(SCAN, 96.0), 3.0);
        // Fire flashes.
        assert_eq!(glow(FIRE, 2.0), 8.0);
        assert_eq!(glow(FIRE, 14.0), 1.0);
        // Death flickers, then goes out.
        assert_eq!(glow(DEATH, 1.0), 4.0);
        assert_eq!(glow(DEATH, 3.0), 0.0);
        assert_eq!(glow(DEATH, 21.0), 4.0);
        assert_eq!(glow(DEATH, 40.0), 0.0);
        assert_eq!(glow(DEATH, 60.0), 0.0);
    }

    #[test]
    fn its_mood_colours_it_as_a_droids_eyes() {
        let green = Vector3::new(0.0, 1.0, 0.0);
        assert_eq!(mood_colour(green, Mood::Normal), green);
        let red = mood_colour(green, Mood::Hostile);
        assert!(red.x == 1.0 && red.y < 0.5 && red.z < 0.5, "{red}");
        let orange = mood_colour(green, Mood::Agitated);
        assert!(orange.x == 1.0 && orange.y > 0.3 && orange.y < 0.8, "{orange}");
        assert_eq!(light_colour(Vector3::new(1.0, 0.5, 0.0)), Color::opaque(255, 128, 0));
    }

    #[test]
    fn angles_come_round() {
        assert!((wrap(3.0 * PI / 2.0) + PI / 2.0).abs() < 1.0e-5);
        assert!((wrap(-3.0 * PI / 2.0) - PI / 2.0).abs() < 1.0e-5);
        assert!((wrap(0.5) - 0.5).abs() < 1.0e-6);
    }

    /// What it says, and when: hearing the alarm or a droid shot, spotting the player, starting
    /// to search where they were, giving up, and going down.
    const SAYS: [&str; 5] = ["alarm", "spotted", "searching", "calm", "down"];

    #[test]
    fn it_has_something_to_say_about_everything_it_does() {
        let lines = DroneLines::load(DRONE_LINES).expect("the drones' lines");
        for name in SAYS {
            let said = lines.line(name, 0).unwrap_or_else(|| panic!("nothing for {name}"));
            assert!(!said.says.is_empty() && !said.means.is_empty());
        }
        // Going round its lines in turn.
        let spotted = lines.lines["spotted"].len();
        assert_eq!(lines.line("spotted", spotted), lines.line("spotted", 0));
        assert_eq!(lines.line("nothing", 0), None);
    }

    /// Someone standing with their feet at the origin, as the player's capsule is - 1.7 m tall,
    /// 0.35 m across - and a wall across the way 3 m ahead along z; and a drone's body hovering at `at`. The
    /// drone's body.
    fn standing_by(graph: &mut Graph, at: Vector3<f32>) -> Handle<Node> {
        use fyrox::scene::transform::TransformBuilder;
        let capsule = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(ColliderShape::capsule_y(0.5, 0.35))
            .with_collision_groups(crate::ragdoll::character_groups())
            .build(graph);
        RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_child(capsule)
                .with_local_transform(TransformBuilder::new().with_local_position(Vector3::new(0.0, 0.85, 0.0)).build()),
        )
        .with_body_type(RigidBodyType::KinematicPositionBased)
        .build(graph);
        let wall = ColliderBuilder::new(BaseBuilder::new()).with_shape(ColliderShape::cuboid(2.0, 2.0, 0.1)).build(graph);
        RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_child(wall)
                .with_local_transform(TransformBuilder::new().with_local_position(Vector3::new(0.0, 1.0, 3.0)).build()),
        )
        .with_body_type(RigidBodyType::Static)
        .build(graph);
        let (body, _) = hull(graph);
        graph[body].local_transform_mut().set_position(at);
        // The physics has its colliders by the second update.
        for _ in 0..2 {
            graph.update(
                fyrox::core::algebra::Vector2::new(800.0, 600.0),
                1.0 / 60.0,
                Default::default(),
            );
        }
        body
    }

    #[test]
    fn a_drone_waits_for_someone_in_its_way_and_never_pushes_into_them() {
        let mut graph = Graph::new();
        let at = Vector3::new(-1.0, HOVER, 0.0);
        let body = standing_by(&mut graph, at);
        let toward = Vector3::new(0.05, 0.0, 0.0);
        // Far off, it flies on; up against them - 0.6 m from their middle, its body 4.5 cm from
        // the top of theirs, which curves down from the height it hovers at - it waits.
        assert!(!blocked(&graph, body, at, toward));
        assert!(blocked(&graph, body, Vector3::new(-0.6, HOVER, 0.0), toward));
        // It gets clear, and goes past them where there is room.
        assert!(!blocked(&graph, body, Vector3::new(-0.6, HOVER, 0.0), -toward));
        assert!(!blocked(&graph, body, Vector3::new(-0.6, HOVER, 0.0), Vector3::new(0.0, 0.0, -0.05)));
        // Over a crouching or crawling head it passes, and the maze is not someone.
        assert!(!blocked(&graph, body, Vector3::new(-0.6, 2.4, 0.0), toward));
        assert!(!blocked(&graph, body, Vector3::new(0.0, HOVER, 2.55), Vector3::new(0.0, 0.0, 0.05)));
    }

    #[test]
    fn a_drone_is_solid() {
        let mut graph = Graph::new();
        let (_, collider) = hull(&mut graph);
        let ball = &graph[collider];
        assert!(!ball.is_sensor());
        let player = crate::ragdoll::character_groups();
        let groups = ball.collision_groups();
        // It and the player's capsule meet, but it is not one of the characters.
        assert!(groups.memberships.0 & player.filter.0 != 0 && player.memberships.0 & groups.filter.0 != 0);
        assert_eq!(groups.memberships.0 & CHARACTERS, 0);
    }

    /// How low the player's feet get, standing and maybe walking forward on a thin floor of the
    /// maze - a triangle mesh - for four seconds while a drone's body is moved by hand along
    /// `path` over time, holding still while it would run into anyone, as `Drone::update` moves
    /// it.
    fn lowest_feet(path: impl Fn(f32) -> Vector3<f32>, walk: bool) -> f32 {
        use crate::player::Player;
        use fyrox::core::algebra::{Matrix4, Vector2};
        use fyrox::scene::{
            collider::GeometrySource,
            mesh::{
                surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
                MeshBuilder,
            },
            transform::TransformBuilder,
        };
        let mut graph = Graph::new();
        let mesh = MeshBuilder::new(
            BaseBuilder::new().with_local_transform(
                TransformBuilder::new().with_local_position(Vector3::new(0.0, -0.1, 0.0)).build(),
            ),
        )
        .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(SurfaceData::make_cube(
            Matrix4::new_nonuniform_scaling(&Vector3::new(20.0, 0.2, 20.0)),
        )))
        .build()])
        .build(&mut graph);
        graph.update_hierarchical_data();
        let floor = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(ColliderShape::trimesh(vec![GeometrySource(mesh.to_base())]))
            .build(&mut graph);
        RigidBodyBuilder::new(BaseBuilder::new().with_child(floor))
            .with_body_type(RigidBodyType::Static)
            .build(&mut graph);
        let mut player = Player::spawn(&mut graph);
        player.teleport(&mut graph, Vector3::new(0.0, 0.86, 0.0), 0.0);
        if walk {
            player.on_key(fyrox::keyboard::KeyCode::KeyW, true);
        }
        let (hull, _) = hull(&mut graph);
        graph[hull].local_transform_mut().set_position(path(0.0));
        let dt = 1.0 / 60.0;
        let mut lowest = f32::MAX;
        for k in 0..240 {
            player.update(&mut graph, dt, true);
            let to = path(k as f32 * dt);
            let from = graph[hull].global_position();
            if !blocked(&graph, hull, from, to - from) {
                if let Ok(body) = graph.try_get_mut_of_type::<RigidBody>(hull) {
                    body.set_next_kinematic_translation(to);
                }
            }
            graph.update(Vector2::new(800.0, 600.0), dt, Default::default());
            lowest = lowest.min(player.feet(&graph).y);
        }
        lowest
    }

    #[test]
    fn a_drone_never_pushes_the_player_down_through_the_floor() {
        // Standing, with no drone near, the feet sink this far into the floor at most.
        let alone = lowest_feet(|_| Vector3::new(10.0, HOVER, 10.0), false);
        let cases: [(&str, Box<dyn Fn(f32) -> Vector3<f32>>, bool); 4] = [
            // Settling onto their head from above, as it would coming down to hover over a step,
            // or shot down overhead: moved by hand, it would press them down through the floor.
            ("sinking onto them", Box::new(|t| Vector3::new(0.0, (2.3 - t).max(0.3), 0.0)), false),
            ("flying through them", Box::new(|t| Vector3::new(-2.0 + 2.0 * t, HOVER, 0.0)), false),
            // Walking into it, under it, at its height and lower.
            ("walked into", Box::new(|_| Vector3::new(0.0, HOVER, 1.0)), true),
            ("walked into low", Box::new(|_| Vector3::new(0.0, 1.5, 0.6)), true),
        ];
        for (name, path, walk) in cases {
            let lowest = lowest_feet(path, walk);
            assert!(lowest > alone - 0.01, "{name}: down to {lowest} (alone {alone})");
        }
    }
}
