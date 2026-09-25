//! The maze's inhabitants: droids like the player's, going about the maze by themselves.
//!
//! Each one wanders. It picks somewhere a good walk away, follows the cheapest route there over
//! the survey's grid - which keeps to the middle of the corridors (see
//! [`WalkGrid::routes_from`]) - and when it gets there stands idling a while before setting off
//! again. It walks at the droid's own walking pace, so its feet keep to the floor, and slows down
//! to turn.
//!
//! They have bodies, so the player cannot walk through them, and they make way for each other.
//! One walking veers to its right round anyone ahead of it, so that two meeting head on pass
//! each other on the left, and to the left instead if a wall is in the way; one standing about
//! that sees another coming straight at it steps out of its way. Only someone right in front of
//! it stops it, and kept waiting too long, it goes somewhere else instead.
//!
//! Only those the player could see are drawn. Like the player's droid, they cast no shadows.
//!
//! Each is one of the kinds of droid in the conversations (see [`crate::dialogue`]), with a code
//! of its own, and can be talked to by a player close by and facing it. While it is, it stands
//! still and turns to face them, and afterwards stands a moment before going on its way. One of
//! them does not wander at all: it stands just in front of where the player starts, and stays
//! there, so that there is always someone near to talk to.
//!
//! A conversation can turn a droid hostile (see [`Inhabitants::set_hostile`]), and a hostile
//! droid hunts the player the way Metal Gear's guards do, through its [`Alert`] phases:
//!
//! - **Alert**: it can see the player, and after a first moment runs at them. If it gets close
//!   enough, it has caught them.
//! - **Evasion**: it has lost them. It runs to where they were going when it last saw them, and
//!   looks about; then walks to one spot after another nearby, looking about at each, until
//!   [`EVASION`] seconds are up.
//! - **Caution**: it has given up, and wanders as before, but watching for the player still,
//!   for [`CAUTION`] seconds; then it is calm again, and can be talked to once more.
//!
//! Out of Alert, a droid sees only what is in front of it, and not as far as it could: a player
//! crouched is seen from half as far, and one crawling from less than a third. Anyone right next
//! to it, it notices whichever way it faces. With the lights off it sees a good deal less far in
//! any phase - unless the player's flashlight is on. Seeing the player again, it is back on
//! Alert, and a bolt from the pistol has it search where the shot came from.
//!
//! Out of Alert, it also listens (see [`Inhabitants::hear`]): a noise that carries as far as it
//! is, along the corridors, has it run to where the noise was and search from there.
//!
//! Hostile, it cannot be talked to any more, but after [`HITS`] bolts it goes down, crouched
//! with its eyes dark, and stays there.
//!
//! A droid that is not hostile minds having the player's pistol pointed at it (see
//! [`Inhabitants::feel_aimed_at`]): for as long as its kind stands for it, and then it stops,
//! faces the player and warns them. Each warning then holds for [`WARNING`] seconds more, time
//! to say it and for the player to take it in, before the next: the last warning, and then
//! being provoked. What it does then is up to its kind: it turns hostile, or sounds the alarm
//! (see [`Inhabitants::raise_alarm`]) for those near it that would. Looking away only holds the
//! next stage off; with the pistol off it for [`CALM`] seconds, it calms down again.

use crate::{
    layout::{Rng, WalkGrid},
    level::Level,
    player::{
        avatar::{self, Avatar, Going},
        posture::{Gait, Posture},
    },
    survey,
};
use fyrox::{
    core::{algebra::Vector3, color::Color, log::Log, pool::Handle},
    graph::SceneGraph,
    resource::model::ModelResource,
    scene::{
        base::BaseBuilder,
        collider::{Collider, ColliderBuilder, ColliderShape},
        graph::Graph,
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        Scene,
    },
};

/// How many droids live in a maze, unless MAZE_INHABITANTS says otherwise.
const COUNT: usize = 6;
/// Their bodies: a capsule this far out from its middle line, with its middle this far above the
/// feet - 1.7 m tall, like the player's.
const RADIUS: f32 = 0.3;
const MIDDLE: f32 = 0.85;
/// How far from the player they are put down, in steps across the grid's half-meter cells
/// (see [`WalkGrid::routes_from`]): out of sight to begin with.
const AWAY_FROM_PLAYER: f32 = 24.0;
/// How far a droid goes each time it sets off, in steps across the grid, from least to most:
/// never just round the corner, never across the whole maze.
const TRIP: (f32, f32) = (30.0, 160.0);
/// How long it stands still between trips, in seconds, from least to most.
const REST: (f32, f32) = (2.0, 10.0);
/// How near the next point on its route it has to get, in meters, before making for the one
/// after: that much of every corner is cut.
const REACHED: f32 = 0.6;
/// How near the end of its route counts as there, in meters.
const ARRIVED: f32 = 0.1;
/// How quickly it gets up to speed and slows down, in meters per second per second.
const ACCELERATION: f32 = 3.0;
/// How far ahead, in meters, a droid starts to veer round anyone in its way.
const AVOID_RANGE: f32 = 2.5;
/// How far to either side of its way, in meters, someone has to be not to be in it.
const PASSING: f32 = 1.0;
/// How hard it veers round someone just in front of it: how far to the side for every meter
/// ahead, less the further off they are.
const VEER: f32 = 1.2;
/// How close, in meters, someone straight ahead has to be for a droid to stop for them.
const KEEP_CLEAR: f32 = 0.8;
/// How far ahead, in meters, it makes sure there is floor before veering that way.
const LOOK_AHEAD: f32 = 0.6;
/// How near, in meters, a droid coming straight at one standing about has to be for that one to
/// step out of its way, and how far it steps.
const YIELD_RANGE: f32 = 2.2;
const STEP_ASIDE: f32 = 0.8;
/// How long, in seconds, a droid waits for someone in its way before going somewhere else.
const PATIENCE: f32 = 3.0;
/// How quickly its feet follow the floor up and down, like
/// [`EYE_EASING`](crate::player::posture::EYE_EASING).
const FLOOR_EASING: f32 = 10.0;
/// Its walking pace, in meters per second, if the droid has no walk to go by.
const FALLBACK_PACE: f32 = 1.4;
/// How near the player's feet a droid's have to be, in meters, and how far off where the
/// player looks it can be, in radians, for the player to talk to it.
const TALK_REACH: f32 = 2.5;
const TALK_CONE: f32 = 40.0 * std::f32::consts::PI / 180.0;
/// How far ahead of the player, in meters, the droid that stays near them stands: the first of
/// these that is on the floor.
const MEET_AT: [f32; 3] = [2.2, 1.8, 1.4];
/// Where a droid's face is above its feet, in meters, if its model has no head to go by.
const FACE_HEIGHT: f32 = 1.6;
/// Its running pace, in meters per second, if the droid has no run to go by.
const FALLBACK_RUN: f32 = 2.0;
/// How long a droid that has just turned hostile stands before it goes after the player, in
/// seconds: long enough to finish its threat, and for the player to start running.
const WINDUP: f32 = 1.0;
/// How far off a hostile droid can see the player, in meters.
const SIGHT: f32 = 30.0;
/// How far to either side of straight ahead a droid that is not on Alert sees, in radians.
const VIEW_CONE: f32 = 55.0 * std::f32::consts::PI / 180.0;
/// How near the player has to be, in meters, for such a droid to notice them whichever way it
/// faces.
const NOTICE: f32 = 1.5;
/// How much of [`SIGHT`] a droid that is not on Alert sees a player crouched and crawling from.
const CROUCHED_SIGHT: f32 = 0.5;
const CRAWLING_SIGHT: f32 = 0.3;
/// How much of how far it would see, it sees with the lights off and no flashlight on.
const DARK_SIGHT: f32 = 0.35;
/// How long after hearing something a droid pays no heed to another noise, in seconds, but to
/// go on to where that one was.
const HEARING_REST: f32 = 3.0;
/// How long a droid searches for the player once it has lost them, in seconds, and how long it
/// stays wary after that.
pub const EVASION: f32 = 30.0;
pub const CAUTION: f32 = 60.0;
/// How far ahead of where it last saw the player it looks for them first, in meters, the way
/// they were going.
const GUESS: f32 = 4.0;
/// How long it looks about at each spot it searches, in seconds, and how far either way it turns
/// looking, in radians.
const LOOK_ABOUT: f32 = 3.0;
const LOOK_SWEEP: f32 = 70.0 * std::f32::consts::PI / 180.0;
/// How far each spot it searches is from the last, in steps across the grid, from least to most.
const SEARCH_TRIP: (f32, f32) = (10.0, 40.0);
/// How far off, in meters, the pistol pointed at a droid bothers it, and how far to the side of
/// the middle of the view it can be, in meters, for the pistol to count as pointed at it.
const AIM_RANGE: f32 = 20.0;
const AIM_WIDTH: f32 = 0.6;
/// How far above its feet, in meters, the middle of the view is taken to be pointed at a droid.
const CHEST: f32 = 1.2;
/// How long each warning holds, in seconds, on top of how long the droid stands for the pistol
/// in the first place, before it goes on to the next; and how long the pistol has to be off it,
/// in seconds, for it to calm down again.
const WARNING: f32 = 2.5;
const CALM: f32 = 2.5;
/// How far off, in meters, droids that answer an alarm hear it.
const ALARM_RANGE: f32 = 40.0;
/// How often a hostile droid that can see the player works out its way to them again, in
/// seconds.
const REPLAN: f32 = 0.4;
/// How far it looks for a way to the player, in steps across the grid's half-meter cells.
const CHASE_REACH: f32 = 200.0;
/// How near the player's feet, in meters, a hostile droid's have to get to catch them - short of
/// their bodies touching - and how far above or below.
const CATCH: f32 = 0.9;
const CATCH_HEIGHT: f32 = 1.0;
/// How many of the pistol's bolts it takes to stop a hostile droid.
pub const HITS: u32 = 3;
/// How tall a droid that has been stopped is, crouched, in meters.
const DOWN_HEIGHT: f32 = 1.1;

/// How a hostile droid is going about the player: Metal Gear's phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Alert {
    /// It has given up searching for them, and wanders, watching for them.
    Caution,
    /// It has lost them, and is searching.
    Evasion,
    /// It can see them, and is after them.
    Alert,
}

/// How a droid takes having the pistol pointed at it, as it goes from one stage to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Threat {
    /// It stops, faces the player and warns them.
    Warned,
    /// It warns them for the last time.
    WarnedAgain,
    /// It has stood for it long enough: its kind does something about it.
    Provoked,
    /// The pistol has been lowered long enough for it to calm down again.
    Calmed,
}

/// What came of moving everyone along.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct News {
    /// The droid that caught the player, if one did.
    pub caught: Option<usize>,
    /// The droids whose phase changed, as indices, and what to: none for calm again.
    pub alerts: Vec<(usize, Option<Alert>)>,
    /// The droids that heard the player, and are searching where.
    pub heard: Vec<usize>,
    /// The droids that answered an alarm, and are searching where the player was.
    pub alarmed: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Inhabitant {
    body: Handle<Node>,
    collider: Handle<Collider>,
    avatar: Avatar,
    /// Where its feet are.
    feet: Vector3<f32>,
    /// How fast it is going along the ground, in meters per second.
    speed: f32,
    /// Which way it is making for, in radians, left positive from the world's +z.
    heading: f32,
    /// The rest of its route, as points on the floor, the next one last.
    route: Vec<Vector3<f32>>,
    /// How long it has left to stand still, in seconds.
    resting: f32,
    /// How long it has been kept waiting by someone in its way, in seconds.
    waiting: f32,
    /// Where its feet were the last time the graphics effects were told.
    last_seen: Option<Vector3<f32>>,
    /// Which kind of droid it is, as an index into the conversations' characters, and its code.
    character: usize,
    code: u32,
    /// Whether the player is talking to it.
    talking: bool,
    /// Whether it stays where it was put, near where the player starts, rather than wandering.
    stays: bool,
    /// How it is going about the player, once it has turned on them.
    alert: Option<Alert>,
    /// Whether, being hostile, it can see the player, as of this frame.
    sees_player: bool,
    /// Where it last saw the player's feet, and which way they were going then, roughly.
    lost_at: Vector3<f32>,
    lost_going: Vector3<f32>,
    /// How long it has left to search, or to stay wary, in seconds.
    search_left: f32,
    /// How many spots it has set off to search since it lost the player.
    searched: u32,
    /// How long it has left looking about where it is, in seconds, while it is, and which way it
    /// faced as it started.
    looking: Option<f32>,
    look_from: f32,
    /// How long before it heeds another noise, in seconds.
    deaf: f32,
    /// How long it has had the pistol pointed at it since it last warned the player, or since it
    /// first had, in seconds; how many times it has warned them; and how long the pistol has been
    /// off it since, in seconds.
    threat: f32,
    warned: u8,
    unaimed: f32,
    /// How long until it works out its way to the player again, in seconds.
    replan: f32,
    /// How long, having just turned hostile, it stands before going after the player, in seconds.
    windup: f32,
    /// How many of the pistol's bolts have hit it while hostile.
    hits: u32,
    /// Whether it has been stopped, and stays where it went down.
    down: bool,
}

/// Everyone who lives in the maze.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Inhabitants {
    droids: Vec<Inhabitant>,
    /// The phases changed since the last update, as [`News::alerts`] has them, and those that
    /// heard the player.
    alerts: Vec<(usize, Option<Alert>)>,
    heard: Vec<usize>,
    alarmed: Vec<usize>,
    /// Whether they have been put into this round's maze yet.
    populated: bool,
}

/// A number from `range.0` to `range.1`.
fn between(rng: &mut Rng, range: (f32, f32)) -> f32 {
    range.0 + (range.1 - range.0) * rng.below(1001) as f32 / 1000.0
}

/// `vector` along the ground.
fn flat(vector: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(vector.x, 0.0, vector.z)
}

/// Which way `heading` faces, along the ground: left positive from the world's +z.
fn forward(heading: f32) -> Vector3<f32> {
    Vector3::new(heading.sin(), 0.0, heading.cos())
}

/// A droid's right, going `ahead`.
fn right_of(ahead: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-ahead.z, 0.0, ahead.x)
}

/// Which way a droid at `feet` should go to make its way `ahead` round `others` - each where
/// they are, and which way they are walking, if they are - veering to its right, or failing that
/// to its left, but only where `floor_at` says there is floor. And whether someone is right in
/// front of it, so that it has to stop.
fn make_way(
    feet: Vector3<f32>,
    ahead: Vector3<f32>,
    others: impl Iterator<Item = (Vector3<f32>, Option<Vector3<f32>>)>,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> (Vector3<f32>, bool) {
    let right = right_of(ahead);
    let mut veer: f32 = 0.0;
    let mut blocked = false;
    for (there, _) in others {
        let to_them = flat(there - feet);
        let distance = to_them.norm();
        let (along, across) = (to_them.dot(&ahead), to_them.dot(&right));
        if distance > AVOID_RANGE || along <= 0.0 || across.abs() > PASSING {
            continue;
        }
        veer = veer.max(VEER * (1.0 - distance / AVOID_RANGE));
        blocked |= distance < KEEP_CLEAR && along > 0.8 * distance;
    }
    if veer == 0.0 {
        return (ahead, blocked);
    }
    // Right if there is room, then left, and less sharply before not at all.
    for side in [veer, 0.5 * veer, -veer, -0.5 * veer] {
        let way = (ahead + right * side).normalize();
        if floor_at(feet + way * LOOK_AHEAD) {
            // Veering hard, it gets past whoever is in front of it rather than waiting for them.
            return (way, blocked && side.abs() < 0.5 * VEER);
        }
    }
    (ahead, blocked)
}

/// Where a droid standing about at `feet` should step to, to get out of the way of any of
/// `others` walking straight at it: out to the side they are not veering to, or failing that
/// the other side, wherever `floor_at` says there is floor.
fn step_aside(
    feet: Vector3<f32>,
    others: impl Iterator<Item = (Vector3<f32>, Option<Vector3<f32>>)>,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> Option<Vector3<f32>> {
    for (there, walking) in others {
        let Some(going) = walking else {
            continue;
        };
        let to_me = flat(feet - there);
        let distance = to_me.norm();
        let (along, across) = (to_me.dot(&going), to_me.dot(&right_of(going)));
        if distance > YIELD_RANGE || along <= 0.0 || across.abs() > PASSING {
            continue;
        }
        // They veer to their right, so out to their left.
        let left = -right_of(going);
        return [left, -left]
            .into_iter()
            .map(|side| feet + side * STEP_ASIDE)
            .find(|&spot| floor_at(spot) && floor_at(feet + (spot - feet) * 0.5));
    }
    None
}

/// Whether a droid at `feet`, facing `heading`, in `alert`, could see a player at `player`
/// holding themselves in `posture`, if nothing were in the way: in any direction and from as far
/// as it sees at all on Alert; otherwise only in front, and less far the lower the player is -
/// but right next to it, in any direction. `in_the_dark`, with the lights off and no flashlight
/// on, it sees less far either way.
fn could_see(
    alert: Alert,
    feet: Vector3<f32>,
    heading: f32,
    player: Vector3<f32>,
    posture: Posture,
    in_the_dark: bool,
) -> bool {
    let to = player - feet;
    let sight = if in_the_dark { SIGHT * DARK_SIGHT } else { SIGHT };
    if flat(to).norm() < NOTICE {
        return true;
    }
    if to.norm() > sight {
        return false;
    }
    if alert == Alert::Alert {
        return true;
    }
    let reach = sight
        * match posture {
            Posture::Standing => 1.0,
            Posture::Crouching => CROUCHED_SIGHT,
            Posture::Crawling => CRAWLING_SIGHT,
        };
    let across = flat(to).norm();
    across < reach && flat(to).dot(&forward(heading)) >= across * VIEW_CONE.cos()
}

/// Whether the middle of a view from `eye`, looking `ahead` - one meter long - is on `target`:
/// in front, not too far off, and not too far to its side.
fn pointed_at(eye: Vector3<f32>, ahead: Vector3<f32>, target: Vector3<f32>) -> bool {
    let to = target - eye;
    let along = to.dot(&ahead);
    along > 0.0 && along < AIM_RANGE && (to - ahead * along).norm() < AIM_WIDTH
}

/// How long a droid that stands `patience` seconds of the pistol has it pointed at it, in
/// seconds, after it has warned the player `warned` times, before it goes on to the next stage:
/// first its patience, then that and as long again as it takes to warn them.
fn stage_after(warned: u8, patience: f32) -> f32 {
    if warned == 0 {
        patience
    } else {
        patience + WARNING
    }
}

/// The phase a droid in `alert` goes into, seeing the player or not, with `left` seconds left of
/// searching or of being wary: none, once it is calm again.
fn next_alert(alert: Alert, sees: bool, left: f32) -> Option<Alert> {
    Some(match alert {
        _ if sees => Alert::Alert,
        Alert::Alert => Alert::Evasion,
        Alert::Evasion if left <= 0.0 => Alert::Caution,
        Alert::Caution if left <= 0.0 => return None,
        other => other,
    })
}

impl Inhabitant {
    /// Puts it into `alert`, or with none calms it down, and gets it ready to go about it; and
    /// tells of it in `news`, as the `n`th droid.
    fn enter(&mut self, n: usize, alert: Option<Alert>, news: &mut Vec<(usize, Option<Alert>)>) {
        if self.alert == alert {
            return;
        }
        self.alert = alert;
        news.push((n, alert));
        match alert {
            Some(Alert::Alert) => self.replan = 0.0,
            Some(Alert::Evasion) => self.search_afresh(),
            Some(Alert::Caution) | None => {
                self.route.clear();
                self.looking = None;
                self.resting = REST.0;
                self.search_left = CAUTION;
            }
        }
    }

    /// Starts searching from where it last saw the player.
    fn search_afresh(&mut self) {
        self.route.clear();
        self.looking = None;
        self.searched = 0;
        self.search_left = EVASION;
    }
}

/// The middle of a cell of the grid, on its floor.
fn on_floor(grid: &WalkGrid, origin: Vector3<f32>, (x, z): (usize, usize)) -> Vector3<f32> {
    survey::cell_center(origin, x, z) + Vector3::new(0.0, grid.floor(x, z), 0.0)
}

/// The cells of `path` after the first, as points on the floor, the last one first: a route.
fn along(grid: &WalkGrid, origin: Vector3<f32>, path: Vec<(usize, usize)>) -> Vec<Vector3<f32>> {
    path.into_iter()
        .skip(1)
        .rev()
        .map(|cell| on_floor(grid, origin, cell))
        .collect()
}

/// The walkable cell at `point`, or failing that the nearest one.
fn walkable_cell(
    grid: &WalkGrid,
    origin: Vector3<f32>,
    point: Vector3<f32>,
) -> Option<(usize, usize)> {
    survey::cell_at(grid, origin, point)
        .filter(|&(x, z)| grid.is_walkable(x, z))
        .or_else(|| survey::nearest_walkable(grid, origin, point))
}

/// A route from `feet` to `to`, as points on the floor, the first one last, ending at `to`
/// itself. Empty if there is no way there within [`CHASE_REACH`].
fn route_to(
    grid: &WalkGrid,
    origin: Vector3<f32>,
    feet: Vector3<f32>,
    to: Vector3<f32>,
) -> Vec<Vector3<f32>> {
    let (Some(from), Some(goal)) =
        (walkable_cell(grid, origin, feet), walkable_cell(grid, origin, to))
    else {
        return Vec::new();
    };
    let Some(path) = grid.routes_from(from, CHASE_REACH).path_to(goal) else {
        return Vec::new();
    };
    let mut route = along(grid, origin, path);
    let end = Vector3::new(to.x, route.first().map_or(feet.y, |end| end.y), to.z);
    match route.first_mut() {
        Some(last) => *last = end,
        None => route.push(end),
    }
    route
}

/// A route from `feet` to somewhere a trip away, as points on the floor, the first one last. Empty
/// if there is nowhere to go.
fn plan(
    grid: &WalkGrid,
    origin: Vector3<f32>,
    feet: Vector3<f32>,
    trip: (f32, f32),
    rng: &mut Rng,
) -> Vec<Vector3<f32>> {
    let Some(from) =
        survey::cell_at(grid, origin, feet).filter(|&(x, z)| grid.is_walkable(x, z))
    else {
        return Vec::new();
    };
    let routes = grid.routes_from(from, trip.1);
    let reached = |range: (f32, f32)| -> Vec<usize> {
        routes
            .costs
            .iter()
            .enumerate()
            .filter(|(_, cost)| cost.is_some_and(|c| c >= range.0 && c <= range.1))
            .map(|(i, _)| i)
            .collect()
    };
    // Somewhere a trip away; failing that, in a small space, anywhere else at all.
    let mut choices = reached(trip);
    if choices.is_empty() {
        choices = reached((f32::MIN_POSITIVE, f32::INFINITY));
    }
    if choices.is_empty() {
        return Vec::new();
    }
    let goal = choices[rng.below(choices.len())];
    let Some(path) = routes.path_to((goal % grid.width, goal / grid.width)) else {
        return Vec::new();
    };
    along(grid, origin, path)
}

impl Inhabitants {
    /// Whether they have been put into this round's maze yet.
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Takes everyone out of the scene, to be put into the next round afresh.
    pub fn clear(&mut self, graph: &mut Graph) {
        for droid in self.droids.drain(..) {
            if graph.is_valid_handle(droid.body) {
                graph.remove_node(droid.body);
            }
        }
        self.populated = false;
    }

    /// Puts the maze's inhabitants into `scene`, as droids from `model`, on the floor of `grid`
    /// whose corner is at `origin`, well away from the `player`'s feet. They take turns being
    /// each of the `characters` kinds of droid there are to talk to. The first stands just in
    /// front of the player instead, `ahead` of them, facing them, and stays there, so that there
    /// is always someone near to talk to.
    #[allow(clippy::too_many_arguments)]
    pub fn populate(
        &mut self,
        scene: &mut Scene,
        model: &ModelResource,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: Vector3<f32>,
        ahead: Vector3<f32>,
        characters: usize,
        rng: &mut Rng,
    ) {
        self.clear(&mut scene.graph);
        self.populated = true;
        let count = std::env::var("MAZE_INHABITANTS")
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(COUNT);
        let Some(start) = survey::cell_at(grid, origin, player)
            .filter(|&(x, z)| grid.is_walkable(x, z))
            .or_else(|| survey::nearest_walkable(grid, origin, player))
        else {
            return;
        };
        let routes = grid.routes_from(start, f32::INFINITY);
        let mut places: Vec<(usize, usize)> = routes
            .costs
            .iter()
            .enumerate()
            .filter(|(_, cost)| cost.is_some_and(|c| c >= AWAY_FROM_PLAYER))
            .map(|(i, _)| (i % grid.width, i / grid.width))
            .collect();

        // Where the one that stays near the player stands: the furthest of a few steps ahead of
        // them that is on the floor.
        let waiting = MEET_AT.iter().find_map(|&distance| {
            survey::cell_at(grid, origin, player + ahead * distance)
                .filter(|&(x, z)| grid.is_walkable(x, z))
                .map(|cell| on_floor(grid, origin, cell))
        });
        let first = rng.below(characters.max(1));
        for n in 0..count {
            let meeting = n == 0 && waiting.is_some();
            if places.is_empty() && !meeting {
                break;
            }
            let feet = match waiting.filter(|_| meeting) {
                Some(spot) => spot,
                None => on_floor(grid, origin, places.swap_remove(rng.below(places.len()))),
            };
            let collider: Handle<Collider> = ColliderBuilder::new(BaseBuilder::new())
                .with_shape(ColliderShape::capsule_y(MIDDLE - RADIUS, RADIUS))
                .build(&mut scene.graph);
            let body = RigidBodyBuilder::new(
                BaseBuilder::new().with_child(collider).with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(feet + Vector3::new(0.0, MIDDLE, 0.0))
                        .build(),
                ),
            )
            .with_body_type(RigidBodyType::KinematicPositionBased)
            .build(&mut scene.graph)
            .to_base();
            let Some(avatar) = Avatar::spawn(model, scene, body, -MIDDLE, true) else {
                scene.graph.remove_node(body);
                break;
            };
            let to_player = flat(player - feet);
            self.droids.push(Inhabitant {
                body,
                collider,
                avatar,
                feet,
                speed: 0.0,
                heading: if meeting {
                    to_player.x.atan2(to_player.z)
                } else {
                    between(rng, (-std::f32::consts::PI, std::f32::consts::PI))
                },
                route: Vec::new(),
                // Not all setting off at once; and the one that stays never.
                resting: if meeting {
                    f32::INFINITY
                } else {
                    between(rng, REST)
                },
                waiting: 0.0,
                last_seen: None,
                character: (first + n) % characters.max(1),
                code: 10 + rng.below(90) as u32,
                talking: false,
                stays: meeting,
                alert: None,
                sees_player: false,
                lost_at: feet,
                lost_going: Vector3::zeros(),
                search_left: 0.0,
                searched: 0,
                looking: None,
                look_from: 0.0,
                deaf: 0.0,
                threat: 0.0,
                warned: 0,
                unaimed: 0.0,
                replan: 0.0,
                windup: 0.0,
                hits: 0,
                down: false,
            });
        }
        Log::info(format!("Maze: {} inhabitants", self.droids.len()));
        match waiting {
            Some(spot) => Log::info(format!(
                "Maze: one stays {:.1} m in front of the player",
                flat(spot - player).norm()
            )),
            None => Log::warn("Maze: no floor in front of the player for a droid to stay on"),
        }
    }

    /// Moves everyone along for another `dt`, over `grid` whose corner is at `origin`, making
    /// way for each other and - unless they are after them - for the `player`'s feet. Whether
    /// anyone caught the player, and whose phase changed.
    pub fn update(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: Vector3<f32>,
        rng: &mut Rng,
        dt: f32,
    ) -> News {
        let mut caught = None;
        let mut alerts = std::mem::take(&mut self.alerts);
        let heard = std::mem::take(&mut self.heard);
        let alarmed = std::mem::take(&mut self.alarmed);
        let floor_at = |spot: Vector3<f32>| {
            survey::cell_at(grid, origin, spot).is_some_and(|(x, z)| grid.is_walkable(x, z))
        };
        // Where everyone is, and which way those walking are going; the player last.
        let everyone: Vec<(Vector3<f32>, Option<Vector3<f32>>)> = self
            .droids
            .iter()
            .map(|droid| {
                let walking = (droid.speed > 0.1).then(|| forward(droid.heading));
                (droid.feet, walking)
            })
            .chain(std::iter::once((player, None)))
            .collect();
        let the_player = everyone.len() - 1;
        for (me, droid) in self.droids.iter_mut().enumerate() {
            // After the player, it goes straight for them rather than round them.
            let hunting = droid.alert == Some(Alert::Alert);
            let others = everyone
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != me && !(hunting && other == the_player))
                .map(|(_, &other)| other);
            droid.resting = (droid.resting - dt).max(0.0);
            droid.windup = (droid.windup - dt).max(0.0);
            droid.deaf = (droid.deaf - dt).max(0.0);
            // The one that stays may step out of someone's way, but never sets off anywhere.
            if droid.stays {
                droid.resting = f32::INFINITY;
            }
            if droid.sees_player {
                // Where the player is, and roughly which way they are going, from how they have
                // moved lately.
                let moved = flat(player - droid.lost_at);
                droid.lost_going = droid.lost_going * (-4.0 * dt).exp() + moved;
                droid.lost_at = player;
            }
            // Seeing the player or not, a hostile droid goes into the phase that follows - once it
            // is done standing, having just turned hostile.
            if let Some(alert) = droid.alert.filter(|_| droid.windup == 0.0 && !droid.down) {
                if alert != Alert::Alert {
                    droid.search_left -= dt;
                }
                let next = next_alert(alert, droid.sees_player, droid.search_left);
                droid.enter(me, next, &mut alerts);
            }
            // Stopped, it stays where it went down.
            if droid.down {
                droid.route.clear();
            // Talking, or warning them off, it stands and faces the player, and rests a moment
            // once they are done.
            } else if droid.talking || droid.warned > 0 {
                droid.route.clear();
                droid.waiting = 0.0;
                droid.resting = droid.resting.max(REST.0);
                let to_them = flat(player - droid.feet);
                if to_them.norm() > 1.0e-3 {
                    droid.heading = to_them.x.atan2(to_them.z);
                }
            } else if droid.alert.is_some() && droid.windup > 0.0 {
                // Just turned hostile, it stands a moment where it is.
                droid.route.clear();
            } else if droid.alert == Some(Alert::Alert) {
                // After the player, the way to them worked out again every so often as they move.
                droid.replan -= dt;
                if droid.replan <= 0.0 || droid.route.is_empty() {
                    droid.replan = REPLAN;
                    droid.route = route_to(grid, origin, droid.feet, player);
                }
            } else if droid.alert == Some(Alert::Evasion) {
                if droid.route.is_empty() {
                    match droid.looking {
                        // Looking about where it is, one way and then the other.
                        Some(left) if left > 0.0 => {
                            droid.looking = Some(left - dt);
                            let turn = std::f32::consts::TAU * (LOOK_ABOUT - left) / LOOK_ABOUT;
                            droid.heading = droid.look_from + LOOK_SWEEP * turn.sin();
                        }
                        // Done looking: on to another spot nearby.
                        Some(_) => {
                            droid.looking = None;
                            droid.searched += 1;
                            droid.route = plan(grid, origin, droid.feet, SEARCH_TRIP, rng);
                        }
                        // First where the player was going when it last saw them, if there is
                        // floor all the way there, or else where it saw them.
                        None if droid.searched == 0 => {
                            droid.searched = 1;
                            let guess = droid
                                .lost_going
                                .try_normalize(1.0e-3)
                                .map(|going| droid.lost_at + going * GUESS)
                                .filter(|&guess| {
                                    let (from, way) = (droid.lost_at, guess - droid.lost_at);
                                    (1..=4).all(|i| floor_at(from + way * (i as f32 / 4.0)))
                                })
                                .unwrap_or(droid.lost_at);
                            droid.route = route_to(grid, origin, droid.feet, guess);
                        }
                        // Got there, or has nowhere to go: it looks about.
                        None => {
                            droid.looking = Some(LOOK_ABOUT);
                            droid.look_from = droid.heading;
                        }
                    }
                }
            } else if droid.route.is_empty() {
                if let Some(aside) = step_aside(droid.feet, others.clone(), floor_at) {
                    droid.route = vec![aside];
                } else if droid.resting == 0.0 {
                    droid.route = plan(grid, origin, droid.feet, TRIP, rng);
                    if droid.route.is_empty() {
                        droid.resting = REST.0;
                    }
                }
            }
            // Running after the player, and to where it lost them.
            let hurrying = droid.alert == Some(Alert::Alert)
                || (droid.alert == Some(Alert::Evasion) && droid.searched <= 1);
            // Past each point on the way, bar the last, as soon as it is near - or already
            // behind, having been put off course making way for someone.
            while let [.., after, next] = droid.route[..] {
                if flat(next - droid.feet).norm() < REACHED
                    || flat(after - droid.feet).norm() < flat(after - next).norm()
                {
                    droid.route.pop();
                } else {
                    break;
                }
            }

            let mut wanted = 0.0;
            if let Some(&next) = droid.route.last() {
                let to = flat(next - droid.feet);
                if droid.route.len() == 1 && to.norm() < ARRIVED {
                    droid.route.clear();
                    droid.resting = between(rng, REST);
                } else {
                    let (way, blocked) = make_way(droid.feet, to.normalize(), others, floor_at);
                    droid.heading = way.x.atan2(way.z);
                    if blocked {
                        droid.waiting += dt;
                        if droid.waiting > PATIENCE {
                            droid.route.clear();
                            droid.waiting = 0.0;
                            droid.resting = between(rng, (0.5, 2.0));
                        }
                    } else {
                        droid.waiting = 0.0;
                        let pace = match hurrying {
                            true => droid
                                .avatar
                                .pace(Posture::Standing, Gait::Running)
                                .unwrap_or(FALLBACK_RUN),
                            false => droid
                                .avatar
                                .pace(Posture::Standing, Gait::Walking)
                                .unwrap_or(FALLBACK_PACE),
                        };
                        // Slower the further it has yet to turn, and slowing down to stop at the
                        // end of the way.
                        let off = droid.heading - droid.avatar.facing();
                        let left: f32 = droid
                            .route
                            .windows(2)
                            .map(|pair| flat(pair[0] - pair[1]).norm())
                            .sum::<f32>()
                            + to.norm();
                        wanted =
                            (pace * off.cos().max(0.0)).min((2.0 * ACCELERATION * left).sqrt());
                    }
                }
            }
            let step = ACCELERATION * dt;
            droid.speed += (wanted - droid.speed).clamp(-step, step);
            droid.feet += forward(droid.heading) * (droid.speed * dt);
            if let Some((x, z)) = survey::cell_at(grid, origin, droid.feet) {
                if grid.is_walkable(x, z) {
                    let floor = grid.floor(x, z);
                    droid.feet.y += (floor - droid.feet.y) * (1.0 - (-FLOOR_EASING * dt).exp());
                }
            }

            let to_player = player - droid.feet;
            if droid.alert == Some(Alert::Alert)
                && droid.windup == 0.0
                && droid.sees_player
                && flat(to_player).norm() < CATCH
                && to_player.y.abs() < CATCH_HEIGHT
            {
                caught = caught.or(Some(me));
            }

            if let Ok(body) = graph.try_get_mut_of_type::<RigidBody>(droid.body) {
                body.set_next_kinematic_translation(droid.feet + Vector3::new(0.0, MIDDLE, 0.0));
            }
            let going = Going {
                heading: Some(droid.heading),
                speed: droid.speed,
                posture: match droid.down {
                    true => Posture::Crouching,
                    false => Posture::Standing,
                },
                gait: match hurrying {
                    true => Gait::Running,
                    false => Gait::Walking,
                },
                grounded: true,
                jumped: false,
                low: false,
                falling: 0.0,
                cover: false,
                pushing: true,
                strafing: false,
                armed: false,
                trigger: false,
                raised: false,
                look: (0.0, 0.0),
                way: 0.0,
            };
            droid.avatar.animate(graph, going, dt);
        }
        News {
            caught,
            alerts,
            heard,
            alarmed,
        }
    }

    /// Has each hostile droid look for the player, at `player` in `posture`, whom it can see
    /// wherever `in_sight` says nothing is in the way from the player to it, and [`could_see`]
    /// says they are where it is looking - `in_the_dark` or not.
    pub fn look_for_player(
        &mut self,
        player: Vector3<f32>,
        posture: Posture,
        in_the_dark: bool,
        in_sight: impl Fn(Vector3<f32>) -> bool,
    ) {
        for droid in &mut self.droids {
            droid.sees_player = match droid.alert {
                Some(alert) if !droid.down => {
                    could_see(alert, droid.feet, droid.heading, player, posture, in_the_dark)
                        && in_sight(droid.feet)
                }
                _ => false,
            };
        }
    }

    /// Has each droid that is not hostile feel the pistol pointed at it for another `dt`, or
    /// not: by the player looking from `eye` along `ahead` with it out, if they are, as long as
    /// `in_sight` says nothing is in the way from the player to the droid. `patience` says how
    /// long each kind of droid - as an index into the conversations' characters - stands for
    /// it; none, it pays it no heed. Which droids went on to another stage.
    pub fn feel_aimed_at(
        &mut self,
        aim: Option<(Vector3<f32>, Vector3<f32>)>,
        in_sight: impl Fn(Vector3<f32>) -> bool,
        patience: impl Fn(usize) -> Option<f32>,
        dt: f32,
    ) -> Vec<(usize, Threat)> {
        let mut stages = Vec::new();
        for (n, droid) in self.droids.iter_mut().enumerate() {
            let Some(patience) = patience(droid.character) else {
                continue;
            };
            let minding = droid.alert.is_none() && !droid.down && !droid.talking;
            let chest = droid.feet + Vector3::new(0.0, CHEST, 0.0);
            let aimed = minding
                && aim.is_some_and(|(eye, ahead)| pointed_at(eye, ahead, chest))
                && in_sight(droid.feet);
            if !minding {
                droid.threat = 0.0;
                droid.warned = 0;
                continue;
            }
            // Worse only while it is pointed at, and the same while it is not - until it has been
            // off it long enough to calm down.
            if aimed {
                droid.threat += dt;
                droid.unaimed = 0.0;
            } else {
                droid.unaimed += dt;
            }
            if droid.threat >= stage_after(droid.warned, patience) {
                droid.threat = 0.0;
                droid.warned += 1;
                let stage = match droid.warned {
                    1 => Threat::Warned,
                    2 => Threat::WarnedAgain,
                    _ => Threat::Provoked,
                };
                if stage == Threat::Provoked {
                    droid.warned = 0;
                }
                stages.push((n, stage));
            } else if droid.unaimed >= CALM && (droid.warned > 0 || droid.threat > 0.0) {
                droid.threat = 0.0;
                if droid.warned > 0 {
                    droid.warned = 0;
                    stages.push((n, Threat::Calmed));
                }
            }
        }
        stages
    }

    /// Provokes the `n`th droid at once, as if it had stood for the pistol as long as it will:
    /// shot at, say. True if it was one that minds.
    pub fn provoke(&mut self, n: usize) -> bool {
        match self.droids.get_mut(n) {
            Some(droid) if droid.alert.is_none() && !droid.down => {
                droid.threat = 0.0;
                droid.warned = 0;
                true
            }
            _ => false,
        }
    }

    /// The `n`th droid sounds the alarm, with the player at `player`: every droid near it that
    /// `answers`, by its kind, and is not already after them, searches where they are.
    pub fn raise_alarm(&mut self, n: usize, player: Vector3<f32>, answers: impl Fn(usize) -> bool) {
        let Some(from) = self.droids.get(n).map(|droid| droid.feet) else {
            return;
        };
        for (m, droid) in self.droids.iter_mut().enumerate() {
            let answering = m != n
                && answers(droid.character)
                && !droid.down
                && matches!(droid.alert, None | Some(Alert::Caution))
                && (droid.feet - from).norm() < ALARM_RANGE;
            if !answering {
                continue;
            }
            droid.stays = false;
            droid.talking = false;
            droid.warned = 0;
            droid.threat = 0.0;
            droid.lost_at = player;
            droid.lost_going = Vector3::zeros();
            droid.alert = None;
            droid.enter(m, Some(Alert::Evasion), &mut self.alerts);
            self.alarmed.push(m);
        }
    }

    /// The droid whose body `collider` is, if it is one.
    pub fn hit(&self, collider: Handle<Collider>) -> Option<usize> {
        self.droids.iter().position(|droid| droid.collider == collider)
    }

    /// A noise at `at`, that carries `loudness` meters along the corridors of `grid`, whose
    /// corner is at `origin`. Every hostile droid within earshot that is not on Alert - and has
    /// not just heard something else, or been shot - runs to where it was, and searches from
    /// there.
    pub fn hear(
        &mut self,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        at: Vector3<f32>,
        loudness: f32,
    ) {
        let listening = |droid: &Inhabitant| {
            !droid.down && matches!(droid.alert, Some(Alert::Evasion | Alert::Caution))
        };
        if !self.droids.iter().any(listening) {
            return;
        }
        let Some(from) = walkable_cell(grid, origin, at) else {
            return;
        };
        let within = loudness / survey::CELL_SIZE;
        let routes = grid.routes_from(from, within);
        for (n, droid) in self.droids.iter_mut().enumerate() {
            if !listening(droid) {
                continue;
            }
            let heard = survey::cell_at(grid, origin, droid.feet)
                .and_then(|(x, z)| routes.costs[z * grid.width + x])
                .is_some_and(|cost| cost <= within);
            if !heard {
                continue;
            }
            if droid.deaf > 0.0 {
                continue;
            }
            droid.deaf = HEARING_REST;
            droid.lost_at = at;
            droid.lost_going = Vector3::zeros();
            droid.alert = None;
            droid.enter(n, Some(Alert::Evasion), &mut self.alerts);
            self.heard.push(n);
        }
    }

    /// The most urgent phase any droid is in, and the longest any of them in it has left to
    /// search or stay wary, in seconds.
    pub fn alarm(&self) -> Option<(Alert, f32)> {
        let alert = self.droids.iter().filter_map(|droid| droid.alert).max()?;
        let left = self
            .droids
            .iter()
            .filter(|droid| droid.alert == Some(alert))
            .map(|droid| droid.search_left)
            .fold(0.0, f32::max);
        Some((alert, left))
    }

    /// Turns the `n`th droid on the player: after a moment it hunts them, and it cannot be
    /// talked to any more. The one that stays near where the player starts leaves its place to.
    pub fn set_hostile(&mut self, n: usize) {
        if let Some(droid) = self.droids.get_mut(n).filter(|droid| !droid.down) {
            droid.alert = Some(Alert::Alert);
            droid.stays = false;
            droid.windup = WINDUP;
            droid.replan = 0.0;
        }
    }

    /// A bolt from the pistol of the player at `player` has hit `collider`. If it is a hostile
    /// droid's, that is another hit on it, and at [`HITS`] it goes down: crouched, with its eyes
    /// dark, and no taller than it is crouched. Short of that, if it has not seen who fired, it
    /// searches for them where they fired from. The droid that went down, if one did.
    pub fn shot(
        &mut self,
        graph: &mut Graph,
        collider: Handle<Collider>,
        player: Vector3<f32>,
    ) -> Option<usize> {
        let (n, droid) = self
            .droids
            .iter_mut()
            .enumerate()
            .find(|(_, droid)| droid.collider == collider)?;
        if droid.alert.is_none() || droid.down {
            return None;
        }
        droid.hits += 1;
        if droid.hits < HITS {
            if droid.alert != Some(Alert::Alert) {
                // After whoever fired, paying no heed to the bolt's own noise hitting it.
                droid.deaf = HEARING_REST;
                droid.lost_at = player;
                droid.lost_going = Vector3::zeros();
                droid.alert = None;
                droid.enter(n, Some(Alert::Evasion), &mut self.alerts);
            }
            return None;
        }
        droid.down = true;
        droid.alert = None;
        droid.sees_player = false;
        droid.avatar.set_eyes(Some(Color::BLACK));
        if let Ok(shape) = graph.try_get_mut(droid.collider) {
            shape.set_shape(ColliderShape::capsule_y(0.5 * DOWN_HEIGHT - RADIUS, RADIUS));
            shape
                .local_transform_mut()
                .set_position(Vector3::new(0.0, 0.5 * DOWN_HEIGHT - MIDDLE, 0.0));
        }
        Some(n)
    }

    /// The droid the player could talk to, standing at `feet` and looking `ahead` along the
    /// ground, if there is one: the nearest close by and in front of them, as long as `in_sight`
    /// says nothing is in the way from the player to where it is.
    pub fn to_talk_to(
        &self,
        feet: Vector3<f32>,
        ahead: Vector3<f32>,
        in_sight: impl Fn(Vector3<f32>) -> bool,
    ) -> Option<usize> {
        within_talking(self.droids.iter().map(|droid| droid.feet), feet, ahead)
            .into_iter()
            .filter(|&i| self.droids[i].alert.is_none() && !self.droids[i].down)
            .find(|&i| in_sight(self.droids[i].feet))
    }

    /// Which kind of droid the `n`th is, as an index into the conversations' characters, and its
    /// code.
    pub fn who(&self, n: usize) -> Option<(usize, u32)> {
        self.droids.get(n).map(|droid| (droid.character, droid.code))
    }

    /// Where the `n`th droid's feet are.
    pub fn feet(&self, n: usize) -> Option<Vector3<f32>> {
        self.droids.get(n).map(|droid| droid.feet)
    }

    /// Where the middle of the `n`th droid's face is, as of the last frame.
    pub fn face(&self, graph: &Graph, n: usize) -> Option<Vector3<f32>> {
        let droid = self.droids.get(n)?;
        Some(
            droid
                .avatar
                .face_at(graph)
                .unwrap_or(droid.feet + Vector3::new(0.0, FACE_HEIGHT, 0.0)),
        )
    }

    /// Has the player talking to the `n`th droid, or done talking to it; done, it stands a
    /// moment before going on its way, unless it is the one that stays.
    pub fn set_talking(&mut self, n: usize, talking: bool) {
        if let Some(droid) = self.droids.get_mut(n) {
            droid.talking = talking;
            if !talking && !droid.stays {
                droid.resting = REST.0;
            }
        }
    }

    /// Has the `n`th droid's eyes glow `colour`, or their own colour with none.
    pub fn set_eyes(&self, n: usize, colour: Option<Color>) {
        if let Some(droid) = self.droids.get(n) {
            droid.avatar.set_eyes(colour);
        }
    }

    /// Draws only those the player could see from where they are in `level`.
    pub fn show(&self, graph: &mut Graph, level: &Level) {
        for droid in &self.droids {
            droid.avatar.set_visible(graph, level.can_see(droid.feet));
        }
    }

    /// The droid nearest `to` of those that can be seen, for the graphics effects: a capsule
    /// round it, and how far it has moved since the last time this was asked.
    pub fn moving(&mut self, graph: &Graph, to: Vector3<f32>) -> Option<fyrox_gfx::MovingThing> {
        let mut nearest: Option<(f32, fyrox_gfx::MovingThing)> = None;
        for droid in &mut self.droids {
            let moved = droid
                .last_seen
                .map_or(Vector3::zeros(), |last| droid.feet - last);
            droid.last_seen = Some(droid.feet);
            let distance = (droid.feet - to).norm();
            if droid.avatar.is_visible(graph) && nearest.is_none_or(|(d, _)| distance < d) {
                nearest = Some((distance, avatar::capsule(droid.feet, moved)));
            }
        }
        nearest.map(|(_, thing)| thing)
    }
}

/// Those of the droids with their feet at `droids` that a player at `feet`, looking `ahead`
/// along the ground, is close enough to and facing enough to talk to, nearest first.
fn within_talking(
    droids: impl Iterator<Item = Vector3<f32>>,
    feet: Vector3<f32>,
    ahead: Vector3<f32>,
) -> Vec<usize> {
    let mut near: Vec<(f32, usize)> = droids
        .enumerate()
        .filter_map(|(i, there)| {
            let to_them = flat(there - feet);
            let distance = to_them.norm();
            let off = if distance < 1.0e-4 {
                0.0
            } else {
                (to_them.dot(&ahead) / distance).clamp(-1.0, 1.0).acos()
            };
            (distance < TALK_REACH && off < TALK_CONE).then_some((distance, i))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHEAD: Vector3<f32> = Vector3::new(0.0, 0.0, 1.0);
    const EVERYWHERE: fn(Vector3<f32>) -> bool = |_| true;

    fn standing(x: f32, z: f32) -> (Vector3<f32>, Option<Vector3<f32>>) {
        (Vector3::new(x, 0.0, z), None)
    }

    #[test]
    fn only_those_close_by_and_in_front_can_be_talked_to_nearest_first() {
        let droids = [
            Vector3::new(0.0, 0.0, 2.0),  // ahead
            Vector3::new(0.3, 0.0, 1.0),  // nearer, a little to one side
            Vector3::new(0.0, 0.0, -1.0), // behind
            Vector3::new(2.0, 0.0, 0.5),  // well off to the side
            Vector3::new(0.0, 0.0, 4.0),  // too far
        ];
        assert_eq!(within_talking(droids.into_iter(), Vector3::zeros(), AHEAD), [1, 0]);
    }

    #[test]
    fn with_nobody_about_it_goes_straight_on() {
        let (way, blocked) = make_way(Vector3::zeros(), AHEAD, std::iter::empty(), EVERYWHERE);
        assert_eq!((way, blocked), (AHEAD, false));
        // Someone behind, or well off to one side, is not in the way either.
        let others = [standing(0.0, -1.0), standing(2.0, 1.0)];
        let (way, _) = make_way(Vector3::zeros(), AHEAD, others.into_iter(), EVERYWHERE);
        assert_eq!(way, AHEAD);
    }

    #[test]
    fn it_veers_right_round_someone_ahead_and_harder_the_nearer() {
        let right = right_of(AHEAD);
        let (far, _) = make_way(Vector3::zeros(), AHEAD, [standing(0.0, 2.0)].into_iter(), EVERYWHERE);
        let (near, blocked) =
            make_way(Vector3::zeros(), AHEAD, [standing(0.0, 1.0)].into_iter(), EVERYWHERE);
        assert!(far.dot(&right) > 0.0 && near.dot(&right) > far.dot(&right));
        assert!(!blocked);
    }

    #[test]
    fn with_a_wall_on_the_right_it_veers_left() {
        let right = right_of(AHEAD);
        let floor = |spot: Vector3<f32>| spot.dot(&right_of(AHEAD)) < 0.1;
        let (way, _) = make_way(Vector3::zeros(), AHEAD, [standing(0.0, 1.0)].into_iter(), floor);
        assert!(way.dot(&right) < 0.0);
    }

    #[test]
    fn hemmed_in_with_someone_right_in_front_it_stops() {
        let floor = |spot: Vector3<f32>| spot.x.abs() < 0.05;
        let (_, blocked) = make_way(Vector3::zeros(), AHEAD, [standing(0.0, 0.5)].into_iter(), floor);
        assert!(blocked);
    }

    #[test]
    fn out_of_alert_it_sees_only_ahead_and_less_far_the_lower_the_player() {
        // Facing +z.
        let sees = |alert, x: f32, z: f32, posture| {
            could_see(alert, Vector3::zeros(), 0.0, Vector3::new(x, 0.0, z), posture, false)
        };
        let standing = Posture::Standing;
        assert!(sees(Alert::Caution, 0.0, 20.0, standing), "ahead");
        assert!(!sees(Alert::Caution, 0.0, -10.0, standing), "behind");
        assert!(!sees(Alert::Caution, 10.0, 1.0, standing), "off to the side");
        assert!(sees(Alert::Caution, 0.0, -1.0, standing), "right behind it, it notices");
        assert!(!sees(Alert::Caution, 0.0, 20.0, Posture::Crouching), "crouched, far off");
        assert!(sees(Alert::Caution, 0.0, 12.0, Posture::Crouching), "crouched, nearer");
        assert!(!sees(Alert::Evasion, 0.0, 12.0, Posture::Crawling), "crawling");
        // On Alert it keeps track of them wherever they go, as long as they are not too far off.
        assert!(sees(Alert::Alert, 0.0, -20.0, Posture::Crawling));
        assert!(!sees(Alert::Alert, 0.0, SIGHT + 1.0, standing));
    }

    #[test]
    fn in_the_dark_it_sees_less_far() {
        let sees = |alert, z: f32, dark| {
            let player = Vector3::new(0.0, 0.0, z);
            could_see(alert, Vector3::zeros(), 0.0, player, Posture::Standing, dark)
        };
        assert!(sees(Alert::Caution, 20.0, false));
        assert!(!sees(Alert::Caution, 20.0, true));
        assert!(sees(Alert::Caution, 8.0, true));
        assert!(!sees(Alert::Alert, 20.0, true), "even after them");
        assert!(sees(Alert::Caution, 1.0, true), "right next to it");
    }

    #[test]
    fn the_pistol_is_pointed_at_what_is_in_the_middle_of_the_view() {
        let eye = Vector3::zeros();
        let ahead = Vector3::z();
        assert!(pointed_at(eye, ahead, Vector3::new(0.3, 0.0, 10.0)));
        assert!(!pointed_at(eye, ahead, Vector3::new(1.0, 0.0, 10.0)), "off to the side");
        assert!(!pointed_at(eye, ahead, Vector3::new(0.0, 0.0, -5.0)), "behind");
        assert!(!pointed_at(eye, ahead, Vector3::new(0.0, 0.0, AIM_RANGE + 1.0)), "too far");
    }

    #[test]
    fn each_warning_holds_long_enough_to_be_said() {
        assert_eq!(stage_after(0, 1.0), 1.0, "its patience, before it first warns");
        assert!(stage_after(1, 1.0) >= WARNING, "then time to say it");
        assert_eq!(stage_after(1, 1.0), stage_after(2, 1.0));
        assert!(stage_after(0, 1.0) < stage_after(0, 2.0), "a less patient droid is sooner");
    }

    #[test]
    fn it_searches_once_it_loses_them_gives_up_and_calms_down_in_the_end() {
        assert_eq!(next_alert(Alert::Alert, true, 0.0), Some(Alert::Alert));
        assert_eq!(next_alert(Alert::Alert, false, 0.0), Some(Alert::Evasion));
        assert_eq!(next_alert(Alert::Evasion, false, 5.0), Some(Alert::Evasion));
        assert_eq!(next_alert(Alert::Evasion, false, 0.0), Some(Alert::Caution));
        assert_eq!(next_alert(Alert::Evasion, true, 5.0), Some(Alert::Alert));
        assert_eq!(next_alert(Alert::Caution, false, 5.0), Some(Alert::Caution));
        assert_eq!(next_alert(Alert::Caution, true, 5.0), Some(Alert::Alert));
        assert_eq!(next_alert(Alert::Caution, false, 0.0), None, "calm again");
    }

    #[test]
    fn standing_about_it_steps_out_of_the_way_of_someone_coming_at_it() {
        let coming = (Vector3::new(0.0, 0.0, -1.5), Some(AHEAD));
        let spot = step_aside(Vector3::zeros(), [coming].into_iter(), EVERYWHERE).unwrap();
        // Out to their left, since they veer to their right.
        assert!(spot.dot(&right_of(AHEAD)) < -0.5);
        // Nobody walking at it, or someone walking away, leaves it be.
        let going_away = (Vector3::new(0.0, 0.0, -1.5), Some(-AHEAD));
        assert_eq!(step_aside(Vector3::zeros(), [going_away].into_iter(), EVERYWHERE), None);
        let waiting = standing(0.0, -1.5);
        assert_eq!(step_aside(Vector3::zeros(), [waiting].into_iter(), EVERYWHERE), None);
    }
}
