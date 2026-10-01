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
//! Only those the player could see are drawn, but all of them cast their shadows, even round a
//! corner out of sight.
//!
//! Each is one of the kinds of droid in the conversations (see [`crate::dialogue`]), with a code
//! of its own, and can be talked to by a player close by and facing it. While it is, it stands
//! still and turns to face them, and afterwards stands a moment before going on its way. They
//! all start out of the player's sight, and wander - all but one, which stands a little way in
//! front of where the player starts, facing them, for a while before it sets off too.
//!
//! A conversation can turn a droid hostile (see [`Inhabitants::set_hostile`]), and a hostile
//! droid hunts the player the way Metal Gear's guards do, through its [`Alert`] phases:
//!
//! - **Alert**: it can see the player, and after a first moment runs at them, breaking into a
//!   sprint now and then (see [`SPRINT_EVERY`]), on breath spent and got back as the player's
//!   is: out of it, it walks until enough is back. A bar over its head shows how much it has
//!   left, as the player's does (see [`Inhabitants::breath`]). If it gets close enough to touch them, it
//!   has caught them.
//! - **Evasion**: it has lost them. It runs to where they were going when it last saw them, and
//!   looks about; then walks to one spot after another nearby, looking about at each, until
//!   [`EVASION`] seconds are up.
//! - **Caution**: it has given up, and wanders as before, but watching for the player still,
//!   for [`CAUTION`] seconds; then it is calm again, and can be talked to once more.
//!
//! A droid sees only what is in front of it, in every phase, and even right next to it: it can be
//! crept up on from behind, or slipped round (see [`Sight`]). Out of Alert it does not see as far
//! as it could: a player crouched is seen from half as far, and one crawling from less than a
//! third. With the lights off it sees a good deal less far in any phase - unless the player's
//! flashlight is on. Seeing the player again, it is back on
//! Alert, and a bolt from the pistol has it search where the shot came from.
//!
//! Out of Alert, it also listens (see [`Inhabitants::hear`]): a noise that carries as far as it
//! is, along the corridors, has it run to where the noise was and search from there.
//!
//! A sentry that sees another sentry after the player goes after them too (see
//! [`Inhabitants::join_chases`]), calm or not: the one it sees shows it where the player is. It
//! sees the other as it would see the player standing, and a sentry that sees one that has joined
//! in joins in as well, so a chase draws in every sentry that catches sight of it.
//!
//! Hostile, it cannot be talked to any more, but after [`HITS`] bolts it goes down, its eyes
//! dark: it goes limp and falls, knocked back by the last bolt, and lies where it falls (see
//! [`crate::ragdoll`]) - or, without a ragdoll, crouches. Lying there, a bolt still shoves it.
//! It stays there, for the rest of the round; the others step over it rather than round it.
//!
//! A droid that is not hostile minds having the player's pistol pointed at it (see
//! [`Inhabitants::feel_aimed_at`]): for as long as its kind stands for it, and then it stops,
//! faces the player and warns them. Each warning then holds for [`WARNING`] seconds more, time
//! to say it and for the player to take it in, before the next: the last warning, and then
//! being provoked. What it does then is up to its kind: it turns hostile, or sounds the alarm
//! (see [`Inhabitants::raise_alarm`]) for those near it that would. Looking away only holds the
//! next stage off; with the pistol off it for [`CALM`] seconds, it calms down again.

pub use hydroxus_ai::alert::{Alert, CAUTION, EVASION};
pub(crate) use hydroxus_ai::route::{between, plan, route_to};
use hydroxus_ai::{
    flat, forward,
    hearing::Heard,
    search::SearchMap,
    sight::{Sight, Stance},
    steer::{make_way, step_aside},
};
use crate::{
    ctf::{self, Side},
    dismember,
    layout::{Rng, WalkGrid},
    level::Level,
    player::{
        avatar::{self, Avatar, Going},
        breath,
        posture::{Gait, Posture},
        Strike,
    },
    ragdoll::{self, Ragdoll},
    survey,
};
use fyrox::{
    core::{
        algebra::{Point3, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
    },
    graph::SceneGraph,
    material::MaterialResource,
    resource::model::ModelResource,
    scene::{
        base::BaseBuilder,
        collider::{Collider, ColliderBuilder, ColliderShape},
        graph::{physics::RayCastOptions, Graph},
        mesh::Mesh,
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        Scene,
    },
};

/// The droid in the colours any kind of droid takes on while it is after the player.
pub const HOSTILE_MODEL: &str = "data/droid_hostile.glb";
/// How many droids live in a maze, unless MAZE_INHABITANTS says otherwise.
const COUNT: usize = 6;
/// Their bodies: a capsule this far out from its middle line, with its middle this far above the
/// feet - 1.7 m tall, like the player's.
const RADIUS: f32 = 0.3;
const MIDDLE: f32 = 0.85;
/// How far from the player they are put down, in steps across the grid's half-meter cells
/// (see [`WalkGrid::routes_from`]): out of sight to begin with.
const AWAY_FROM_PLAYER: f32 = 24.0;
/// How far from the player the one put down near them is, from least to most, in the same steps;
/// how far to either side of the way they face, in radians, if there is room there; and how long
/// it stands there, in seconds, before it sets off.
const NEAR_PLAYER: (f32, f32) = (8.0, 14.0);
const NEAR_CONE: f32 = 30.0 * std::f32::consts::PI / 180.0;
const NEAR_REST: f32 = 20.0;
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
/// Where a droid's face is above its feet, in meters, if its model has no head to go by.
const FACE_HEIGHT: f32 = 1.6;
/// Its jogging, running and sprinting paces, in meters per second, if the droid has no jog, run
/// or sprint to go by.
const FALLBACK_RUN: f32 = 2.0;
const FALLBACK_RUNNING: f32 = 2.8;
const FALLBACK_SPRINT: f32 = 3.5;
/// How long a droid that has just turned hostile stands before it goes after the player, in
/// seconds: long enough to finish its threat, and for the player to start running.
const WINDUP: f32 = 1.0;
/// How long after hearing something a droid pays no heed to another noise, in seconds, but to
/// go on to where that one was.
const HEARING_REST: f32 = 3.0;
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
/// In capture the flag: how far from its post a droid wanders, and is put down, in meters; how long it keeps after one of the other side it has lost sight of, and
/// how long it keeps its pistol out once there is no one to shoot at, in seconds.
const POST_REACH: f32 = 3.0;
const SPAWN_REACH: f32 = 1.5;
const FOE_MEMORY: f32 = 6.0;
const ARMED_FOR: f32 = 5.0;
/// In capture the flag, a droid shoots at whoever it sees of the other side - the player too,
/// for the enemy's - from as far as [`FIRE_RANGE`], in meters, coming on to [`ENGAGE`] and standing
/// there; once every so many seconds, as [`FIRE_EVERY`] says; wide by up to [`SPREAD`] meters
/// every ten meters away; and only facing within [`FIRE_CONE`] radians of them.
const FIRE_RANGE: f32 = 24.0;
const ENGAGE: f32 = 11.0;
const FIRE_EVERY: (f32, f32) = (0.7, 1.3);
const SPREAD: f32 = 0.55;
const FIRE_CONE: f32 = 0.3;
/// How high over the player's feet a droid aims.
const PLAYER_CHEST: f32 = 1.0;
/// How far out from the middle of the player's body it goes, standing, in meters (see
/// `crate::player::posture`).
const PLAYER_RADIUS: f32 = 0.35;
/// How near the player's feet, in meters, a hostile droid's have to get to hit them - their
/// bodies touching, give or take a few centimeters - and how far above or below; and how long,
/// in seconds, it stands after a hit before it comes on again.
const HIT_PAUSE: f32 = 1.2;
const CATCH: f32 = RADIUS + PLAYER_RADIUS + 0.05;
const CATCH_HEIGHT: f32 = 1.0;
/// How long, in seconds, a droid on Alert runs before it breaks into a sprint, from least to
/// most - picked afresh each time, so the player cannot count on when - and how long each sprint
/// lasts. It never sprints the moment it turns on them.
const SPRINT_EVERY: (f32, f32) = (4.0, 10.0);
const SPRINT_FOR: (f32, f32) = (1.5, 3.0);
/// How much breath a sentry has, to the player's 1: it is trained; and how fast it gets it back,
/// to the player's 1: it is heavier.
const SENTRY_STAMINA: f32 = 1.5;
const SENTRY_RECOVERY: f32 = 2.0 / 3.0;
/// How many of the pistol's bolts it takes to stop a hostile droid.
pub const HITS: u32 = 3;
/// The ragdoll body a good headshot has to go through, and how near the middle of it, as a share
/// of how far round it is: through the middle of the skull, not just clipping it.
const HEAD: &str = "head";
const SQUARELY: f32 = 0.6;
/// How tall a droid that has been stopped is, crouched, in meters.
const DOWN_HEIGHT: f32 = 1.1;

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
    /// The droid that hit the player, if one did.
    pub hit: Option<usize>,
    /// The droids whose phase changed, as indices, and what to: none for calm again.
    pub alerts: Vec<(usize, Option<Alert>)>,
    /// The droids that heard the player, and are searching where.
    pub heard: Vec<usize>,
    /// The droids that answered an alarm, and are searching where the player was.
    pub alarmed: Vec<usize>,
    /// In capture the flag, the droids that have just gone after one of the other side; and the
    /// shots fired: from where, which way, one meter long, and by which side.
    pub engaged: Vec<usize>,
    pub shots: Vec<(Vector3<f32>, Vector3<f32>, Side)>,
}

/// How much breath a droid has left, for its stamina bar: where its feet and face are, how much
/// of its most it has left, from 0 to 1, and whether it has run out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Breath {
    pub feet: Vector3<f32>,
    pub face: Vector3<f32>,
    pub left: f32,
    pub winded: bool,
    /// How much of its health it has left, from 0 to 1: bolts take it.
    pub health: f32,
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
    /// Whether it is in the hostile droid's colours.
    looks_hostile: bool,
    /// Whether the player is talking to it.
    talking: bool,
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
    /// On Alert, how long until it sprints, in seconds - below 0 before it has been picked - and
    /// how long it has left sprinting.
    sprint_in: f32,
    sprinting: f32,
    /// Its breath, from 1 - or a sentry's [`SENTRY_STAMINA`] - down to 0, spent and got back as
    /// the player's is, but a sentry's back more slowly; and whether it has run out of it, and
    /// can only walk until enough is back.
    stamina: f32,
    winded: bool,
    /// How much breath it has at most: 1, or a sentry's more.
    most: f32,
    /// Whether it is a sentry.
    sentry: bool,
    /// How many of the pistol's bolts have hit it while hostile.
    hits: u32,
    /// Whether it has been stopped, and stays where it went down.
    down: bool,
    /// Its body gone limp, once it has been stopped, if it has a ragdoll.
    ragdoll: Option<Ragdoll>,
    /// In capture the flag, its side, and where it keeps to (see [`crate::ctf`]); the droid of
    /// the other side it is after, if any, how long since it last saw it, in seconds, and how
    /// long before it can strike again.
    side: Option<Side>,
    post: Option<Vector3<f32>>,
    foe: Option<usize>,
    foe_unseen: f32,
    /// How long it has left speaking to the player, its head turned to them, in seconds.
    speaking: f32,
    /// How long before it can fire again, and before it puts its pistol away, in seconds; and
    /// what it last aimed at.
    reload: f32,
    armed_left: f32,
    aimed_at: Option<Vector3<f32>>,
}

impl Inhabitant {
    /// Whether it has anything to do with the player's being found: not one of the player's
    /// own side in capture the flag.
    fn hunts_player(&self) -> bool {
        self.side.is_none_or(|side| !side.is_players())
    }
}

impl Inhabitant {
    /// Breaks off the part of it lying there that a bolt struck the ragdoll body `body` of, if
    /// a hit there breaks anything off: its head, an arm at the shoulder or the elbow, a leg at
    /// the hip or the knee - a hand at the elbow, a foot at the knee.
    fn break_off(&mut self, graph: &mut Graph, body: &str) {
        let (Some(ragdoll), Some(part)) = (self.ragdoll.as_mut(), dismember::break_for(body))
        else {
            return;
        };
        if self.avatar.break_off(graph, part) {
            ragdoll.let_loose(graph, part);
        }
    }

    /// Where its head is and how far round, if a bolt going `way` that struck it at `at` went
    /// squarely through it: a good headshot.
    fn headshot(
        &self,
        graph: &Graph,
        at: Vector3<f32>,
        way: Vector3<f32>,
    ) -> Option<(Vector3<f32>, f32)> {
        ragdoll::struck_squarely(graph, self.avatar.root(), HEAD, at, way, SQUARELY)
    }

    /// Blows its head apart into voxels, wherever it is - on it or lying broken off - after a
    /// good headshot going `way`, its head `middle` and `radius` round. Whether it was there to.
    fn blow_head_up(
        &mut self,
        graph: &mut Graph,
        (middle, radius): (Vector3<f32>, f32),
        way: Vector3<f32>,
    ) -> bool {
        if !self.avatar.blow_up(graph, HEAD, middle, radius, way) {
            return false;
        }
        if let Some(ragdoll) = self.ragdoll.as_mut() {
            ragdoll.let_loose(graph, HEAD);
        }
        Log::info(format!("Headshot: head blown apart at {middle:?}"));
        true
    }
}

/// What a kind of droid looks like: the model it is made from, and which of that model's
/// materials the hostile droid's model has others in place of, with those, for while it is after
/// the player.
#[derive(Debug, Clone, PartialEq)]
pub struct Livery {
    model: ModelResource,
    hostile: Vec<(MaterialResource, MaterialResource)>,
}

impl Livery {
    /// A kind of droid made from `model`, loaded, that looks like the droid made from `hostile`
    /// while it is after the player - the same droid in other colours - or, without, like itself.
    pub fn new(model: ModelResource, hostile: Option<&ModelResource>) -> Self {
        let hostile = hostile.map_or_else(Vec::new, |hostile| changed_materials(&model, hostile));
        Self { model, hostile }
    }
}

/// Each of `from`'s materials that `to` has a different one in place of, mesh by mesh by name and
/// surface by surface, with the one in its place.
fn changed_materials(
    from: &ModelResource,
    to: &ModelResource,
) -> Vec<(MaterialResource, MaterialResource)> {
    let (from, to) = (from.data_ref(), to.data_ref());
    let (from, to) = (&from.get_scene().graph, &to.get_scene().graph);
    let mut changed: Vec<(MaterialResource, MaterialResource)> = Vec::new();
    for node in from.linear_iter() {
        let Some(mesh) = node.cast::<Mesh>() else {
            continue;
        };
        let Some((_, other)) = to.find_by_name_from_root(node.name()) else {
            continue;
        };
        let Some(other) = other.cast::<Mesh>() else {
            continue;
        };
        for (mine, theirs) in mesh.surfaces().iter().zip(other.surfaces()) {
            let (mine, theirs) = (mine.material(), theirs.material());
            if changed.iter().any(|(had, _)| had.key() == mine.key()) {
                continue;
            }
            let (mine_state, theirs_state) = (mine.state(), theirs.state());
            let differs = match (mine_state.data_ref(), theirs_state.data_ref()) {
                (Some(a), Some(b)) => a != b,
                _ => false,
            };
            if differs {
                changed.push((mine.clone(), theirs.clone()));
            }
        }
    }
    changed
}

/// Puts the droid whose model is under `root` in `livery`'s hostile colours, or back in its own.
fn recolour(graph: &mut Graph, root: Handle<Node>, livery: &Livery, hostile: bool) {
    let nodes: Vec<Handle<Node>> = graph.traverse_handle_iter(root).collect();
    for node in nodes {
        let Some(mesh) = graph[node].cast_mut::<Mesh>() else {
            continue;
        };
        for surface in mesh.surfaces_mut() {
            let key = surface.material().key();
            let swap = livery
                .hostile
                .iter()
                .find_map(|(own, theirs)| match hostile {
                    true => (own.key() == key).then_some(theirs),
                    false => (theirs.key() == key).then_some(own),
                });
            if let Some(material) = swap {
                surface.set_material(material.clone());
            }
        }
    }
}

/// Everyone who lives in the maze.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Inhabitants {
    droids: Vec<Inhabitant>,
    /// What each kind of droid looks like, by its index into the conversations' characters.
    liveries: Vec<Livery>,
    /// The phases changed since the last update, as [`News::alerts`] has them, and those that
    /// heard the player.
    alerts: Vec<(usize, Option<Alert>)>,
    heard: Vec<usize>,
    alarmed: Vec<usize>,
    /// Whether they have been put into this round's maze yet.
    populated: bool,
    /// Where the player could be, since they were last seen, shared by everyone searching for
    /// them (see [`hydroxus_ai::search`]); and where and which way they were going when last seen,
    /// while a droid after them can see them.
    search: Option<SearchMap>,
    last_sight: Option<(Vector3<f32>, Vector3<f32>)>,
    /// Where a noise or an alarm has sent droids to search, to search from.
    search_from: Option<Vector3<f32>>,
    /// How long till the searchers next look over the map, in seconds.
    look_in: f32,
}

/// How fast the player is taken to get away, in meters per second, for where they could be.
const PLAYER_SPEED: f32 = 4.0;
/// How far a searcher is taken to see, in meters, for clearing the map - no further than it sees
/// at all - and how often it looks it over, in seconds.
const SEARCH_SIGHT: f32 = 16.0;
const LOOK_EVERY: f32 = 0.25;
/// How far apart, in meters, the searchers keep where they are going.
const SEARCH_APART: f32 = 4.0;
/// How far ahead of the player, at most, a droid after them that is not the nearest makes for, to
/// cut them off, in meters; and how near it has to be to go straight at them instead.
const CUT_OFF: f32 = 10.0;
const CLOSE_IN: f32 = 4.0;

/// Whether a droid at `feet`, facing `heading`, in `alert` - or calm, with none - could see
/// another droid at `other`, if nothing were in the way: as it would see the player standing
/// there.
fn could_see_droid(
    alert: Option<Alert>,
    feet: Vector3<f32>,
    heading: f32,
    other: Vector3<f32>,
    in_the_dark: bool,
) -> bool {
    Sight::default().could_see(alert, feet, heading, other, Stance::Standing, in_the_dark)
}

/// What deciding who joins a chase takes from a droid.
struct Watch {
    /// Whether its kind is a sentry.
    sentry: bool,
    alert: Option<Alert>,
    sees_player: bool,
    /// Down, or talking to the player.
    busy: bool,
    windup: f32,
    feet: Vector3<f32>,
    heading: f32,
}

/// Which of `droids` join a chase: each sentry, not busy nor after the player already, that
/// could see a sentry after them - done standing, having just turned hostile - by
/// [`could_see_droid`], `in_the_dark` or not, where `clear` says nothing is in the way from the
/// one, as an index, to the other.
fn joiners(
    droids: &[Watch],
    in_the_dark: bool,
    clear: impl Fn(usize, usize) -> bool,
) -> Vec<usize> {
    let after_player =
        |droid: &Watch| droid.alert == Some(Alert::Alert) && droid.sees_player && !droid.busy;
    let chasing: Vec<usize> = (0..droids.len())
        .filter(|&n| droids[n].sentry && after_player(&droids[n]) && droids[n].windup == 0.0)
        .collect();
    (0..droids.len())
        .filter(|&m| {
            let droid = &droids[m];
            droid.sentry
                && !droid.busy
                && !after_player(droid)
                && chasing.iter().any(|&n| {
                    n != m
                        && could_see_droid(
                            droid.alert,
                            droid.feet,
                            droid.heading,
                            droids[n].feet,
                            in_the_dark,
                        )
                        && clear(m, n)
                })
        })
        .collect()
}

/// Whether nothing is in the way of `watcher` seeing `seen`: a ray from its face to the other's
/// chest meets the other before anything else - aside from its own body, which the ray starts in.
/// Somewhere on the floor of `grid`, whose corner is at `origin`, within `reach` of `at` as the
/// crow flies, picked by `rng` - or, with nowhere there, the floor nearest `at`.
pub(crate) fn spot_near(
    (grid, origin): (&WalkGrid, Vector3<f32>),
    at: Vector3<f32>,
    reach: f32,
    rng: &mut Rng,
) -> Option<Vector3<f32>> {
    for _ in 0..24 {
        let angle = between(rng, (-std::f32::consts::PI, std::f32::consts::PI));
        let away = between(rng, (0.0, reach));
        let spot = at + Vector3::new(angle.sin(), 0.0, angle.cos()) * away;
        if let Some(cell) = survey::cell_at(grid, origin, spot).filter(|&(x, z)| grid.is_walkable(x, z)) {
            return Some(grid.on_floor(origin, cell));
        }
    }
    survey::nearest_walkable(grid, origin, at).map(|cell| grid.on_floor(origin, cell))
}

fn in_sight_of(graph: &Graph, watcher: &Inhabitant, seen: &Inhabitant) -> bool {
    let from = watcher.feet + Vector3::new(0.0, FACE_HEIGHT, 0.0);
    let way = seen.feet + Vector3::new(0.0, CHEST, 0.0) - from;
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
        .find(|hit| hit.collider != watcher.collider)
        .is_none_or(|hit| hit.collider == seen.collider)
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

/// How long until a droid sprints, `sprint_in` - below 0 before it has been picked - and how
/// long it has left `sprinting`, after another `dt` of `chasing` the player or not: a sprint
/// comes some while into a chase, never at its start, and again some while after each ends.
fn sprints(sprint_in: f32, sprinting: f32, chasing: bool, rng: &mut Rng, dt: f32) -> (f32, f32) {
    if !chasing {
        return (-1.0, 0.0);
    }
    if sprinting > 0.0 {
        return (sprint_in, sprinting - dt);
    }
    if sprint_in < 0.0 {
        return (between(rng, SPRINT_EVERY), 0.0);
    }
    match sprint_in - dt {
        due if due <= 0.0 => (between(rng, SPRINT_EVERY), between(rng, SPRINT_FOR)),
        due => (due, 0.0),
    }
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

impl Inhabitants {
    /// Whether they have been put into this round's maze yet.
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Takes everyone out of the scene, to be put into the next round afresh.
    pub fn clear(&mut self, graph: &mut Graph) {
        for mut droid in self.droids.drain(..) {
            droid.avatar.sweep_up(graph);
            if graph.is_valid_handle(droid.body) {
                graph.remove_node(droid.body);
            }
            if let Some(ragdoll) = &droid.ragdoll {
                ragdoll.remove(graph);
            }
        }
        self.populated = false;
        self.search = None;
        self.last_sight = None;
        self.search_from = None;
    }

    /// Puts the maze's inhabitants into `scene` on the floor of `grid` whose corner is at
    /// `origin`, well away from the `player`'s feet - all but one, a little way in front of them
    /// as they face `facing`. They take turns being each of the kinds of droid there are to talk
    /// to, each looking as its one of `liveries` has it.
    pub fn populate(
        &mut self,
        scene: &mut Scene,
        liveries: Vec<Livery>,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: Vector3<f32>,
        facing: f32,
        rng: &mut Rng,
    ) {
        self.clear(&mut scene.graph);
        self.populated = true;
        self.liveries = liveries;
        let characters = self.liveries.len();
        if characters == 0 {
            return;
        }
        ragdoll::prepare(&mut scene.graph);
        let count = crate::platform::var("MAZE_INHABITANTS")
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
        let near: Vec<(usize, usize)> = routes
            .costs
            .iter()
            .enumerate()
            .filter(|(_, cost)| cost.is_some_and(|c| (NEAR_PLAYER.0..=NEAR_PLAYER.1).contains(&c)))
            .map(|(i, _)| (i % grid.width, i / grid.width))
            .collect();
        // In front of them if there is room, where they will see it.
        let ahead: Vec<(usize, usize)> = near
            .iter()
            .copied()
            .filter(|&cell| {
                let to = flat(grid.on_floor(origin, cell) - player).try_normalize(1.0e-3);
                to.is_some_and(|to| to.dot(&forward(facing)) >= NEAR_CONE.cos())
            })
            .collect();
        let near = if ahead.is_empty() { near } else { ahead };
        let first = rng.below(characters);
        for n in 0..count {
            // The first near the player, if there is room; the rest away.
            let close = n == 0 && !near.is_empty();
            let place = match close {
                true => near[rng.below(near.len())],
                false if places.is_empty() => break,
                false => places.swap_remove(rng.below(places.len())),
            };
            let feet = grid.on_floor(origin, place);
            let heading = match close {
                true => {
                    let to_player = flat(player - feet);
                    to_player.x.atan2(to_player.z)
                }
                false => between(rng, (-std::f32::consts::PI, std::f32::consts::PI)),
            };
            // Not all setting off at once, and the one near the player not for a while.
            let resting = match close {
                true => NEAR_REST,
                false => between(rng, REST),
            };
            let character = (first + n) % characters;
            if !self.spawn_one(scene, feet, heading, resting, character, rng) {
                break;
            }
        }
        Log::info(format!("Maze: {} inhabitants", self.droids.len()));
    }

    /// Puts one droid, of kind `character`, into `scene` with its feet at `feet`, facing
    /// `heading`, to stand `resting` seconds before it sets off. Whether it could be.
    fn spawn_one(
        &mut self,
        scene: &mut Scene,
        feet: Vector3<f32>,
        heading: f32,
        resting: f32,
        character: usize,
        rng: &mut Rng,
    ) -> bool {
        let collider: Handle<Collider> = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(ColliderShape::capsule_y(MIDDLE - RADIUS, RADIUS))
            .with_collision_groups(ragdoll::character_groups())
            .build(&mut scene.graph);
        let body = RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_child(collider)
                .with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(feet + Vector3::new(0.0, MIDDLE, 0.0))
                        .build(),
                ),
        )
        .with_body_type(RigidBodyType::KinematicPositionBased)
        .build(&mut scene.graph)
        .to_base();
        let model = &self.liveries[character].model;
        let Some(avatar) = Avatar::spawn(model, scene, body, -MIDDLE, true) else {
            scene.graph.remove_node(body);
            return false;
        };
        self.droids.push(Inhabitant {
            body,
            collider,
            avatar,
            feet,
            speed: 0.0,
            heading,
            route: Vec::new(),
            resting,
            waiting: 0.0,
            last_seen: None,
            character,
            code: 10 + rng.below(90) as u32,
            looks_hostile: false,
            talking: false,
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
            sprint_in: -1.0,
            sprinting: 0.0,
            stamina: SENTRY_STAMINA,
            winded: false,
            most: SENTRY_STAMINA,
            sentry: false,
            hits: 0,
            down: false,
            ragdoll: None,
            side: None,
            post: None,
            foe: None,
            foe_unseen: 0.0,
            speaking: 0.0,
            reload: 0.0,
            armed_left: 0.0,
            aimed_at: None,
        });
        true
    }

    /// Puts each side's droids into `scene` for capture the flag, on the floor of `grid` whose
    /// corner is at `origin`, each at its post in `bases`, to keep to (see [`ctf::Bases::post`]).
    /// Each looks
    /// as its side's kind of droid does, as `kind` says, of `liveries`. The enemy's watch for
    /// the player from the start.
    pub fn populate_sides(
        &mut self,
        scene: &mut Scene,
        liveries: Vec<Livery>,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        bases: &ctf::Bases,
        kind: impl Fn(Side) -> usize,
        rng: &mut Rng,
    ) {
        self.clear(&mut scene.graph);
        self.populated = true;
        self.liveries = liveries;
        if self.liveries.is_empty() {
            return;
        }
        ragdoll::prepare(&mut scene.graph);
        for side in Side::BOTH {
            let facing = flat(bases.flag(side.other()) - bases.flag(side));
            for n in 0..ctf::DROIDS {
                let Some(feet) = spot_near((grid, origin), bases.post(side, n), SPAWN_REACH, rng) else {
                    continue;
                };
                let character = kind(side).min(self.liveries.len() - 1);
                let heading = facing.x.atan2(facing.z);
                if !self.spawn_one(scene, feet, heading, between(rng, (0.5, 2.0)), character, rng) {
                    break;
                }
                let m = self.droids.len() - 1;
                let droid = &mut self.droids[m];
                droid.side = Some(side);
                droid.post = Some(bases.post(side, n));
                if !side.is_players() {
                    droid.alert = Some(Alert::Caution);
                    droid.search_left = CAUTION;
                }
            }
        }
        Log::info(format!("Capture the flag: {} droids", self.droids.len()));
    }

    /// Has the `n`th droid speak to the player for `seconds`: it turns its head to them, as far
    /// as a head turns, without turning round.
    pub fn speak_to_player(&mut self, n: usize, seconds: f32) {
        if let Some(droid) = self.droids.get_mut(n) {
            droid.speaking = seconds;
        }
    }

    /// The side of the `n`th droid, in capture the flag.
    pub fn side(&self, n: usize) -> Option<Side> {
        self.droids.get(n).and_then(|droid| droid.side)
    }

    /// Each droid standing, as a target for a drone: which it is, its side, its feet, the middle
    /// of its body, and its collider.
    pub fn standing(&self) -> Vec<(usize, Option<Side>, Vector3<f32>, Vector3<f32>, Handle<Collider>)> {
        self.droids
            .iter()
            .enumerate()
            .filter(|(_, droid)| !droid.down)
            .map(|(n, droid)| (n, droid.side, droid.feet, droid.feet + Vector3::new(0.0, CHEST, 0.0), droid.collider))
            .collect()
    }

    /// Moves everyone along for another `dt`, over `grid` whose corner is at `origin`, making
    /// way for each other and - unless they are after them - for the `player`'s feet, sentries
    /// as `sentry` says of their kind with more breath than the rest. Whether anyone caught the
    /// player, and whose phase changed.
    pub fn update(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: Vector3<f32>,
        rng: &mut Rng,
        dt: f32,
        sentry: impl Fn(usize) -> bool,
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
        // Those lying where they went down are in no one's way.
        let down: Vec<bool> = self.droids.iter().map(|droid| droid.down).collect();
        // In capture the flag, which of the other side each is after, and whether it sees it:
        // the nearest it sees, or else the one it was after, for a while after losing sight of it.
        let foes: Vec<(Option<usize>, bool)> = (0..self.droids.len())
            .map(|m| {
                let droid = &self.droids[m];
                let Some(side) = droid.side.filter(|_| !droid.down) else {
                    return (None, false);
                };
                let enemy = |n: usize| !self.droids[n].down && self.droids[n].side == Some(side.other());
                let away = |n: usize| flat(self.droids[n].feet - droid.feet).norm();
                let seen = (0..self.droids.len())
                    .filter(|&n| enemy(n))
                    .filter(|&n| {
                        could_see_droid(Some(Alert::Alert), droid.feet, droid.heading, self.droids[n].feet, false)
                            && in_sight_of(graph, droid, &self.droids[n])
                    })
                    .min_by(|&a, &b| away(a).total_cmp(&away(b)));
                match (seen, droid.foe.filter(|&n| enemy(n))) {
                    (Some(n), _) => (Some(n), true),
                    (None, Some(n)) if droid.foe_unseen < FOE_MEMORY => (Some(n), false),
                    _ => (None, false),
                }
            })
            .collect();
        let mut engaged = Vec::new();
        let mut shots = Vec::new();

        // The search: lost from sight, the player could be anywhere they could have got to since,
        // and wherever the searchers look and do not see them, they are not.
        let seeing = self
            .droids
            .iter()
            .find(|droid| droid.alert == Some(Alert::Alert) && droid.sees_player && !droid.down);
        let cell_of = |at: Vector3<f32>| survey::cell_at(grid, origin, at).filter(|&(x, z)| grid.is_walkable(x, z));
        let mut search = self.search.take().unwrap_or_else(|| SearchMap::new(grid));
        if let Some(seeing) = seeing {
            self.last_sight = Some((seeing.lost_at, seeing.lost_going));
            self.search_from = None;
        } else if let Some((at, going)) = self.last_sight.take() {
            if let Some(cell) = cell_of(at) {
                let going = (going.norm() > 1.0e-3).then_some((going.x, going.z));
                search.lose(grid, cell, going);
            }
        } else if let Some(at) = self.search_from.take() {
            // Something new about where they are: search from there.
            if let Some(cell) = cell_of(at) {
                search.lose(grid, cell, None);
            }
        }
        let searching = self.droids.iter().any(|d| d.alert == Some(Alert::Evasion) && !d.down);
        if searching && !search.is_empty() {
            search.spread(grid, PLAYER_SPEED / grid.cell_size * dt);
            self.look_in -= dt;
            if self.look_in <= 0.0 {
                self.look_in = LOOK_EVERY;
                let sight = Sight::default();
                for droid in self.droids.iter().filter(|d| d.alert == Some(Alert::Evasion) && !d.down) {
                    if let Some(cell) = cell_of(droid.feet) {
                        let reach = SEARCH_SIGHT.min(sight.range) / grid.cell_size;
                        search.look(grid, cell, droid.heading, (reach, sight.cone, 2.0));
                    }
                }
            }
        }
        // Where each searcher is making for, to keep the others clear of it.
        let goals: Vec<Option<(usize, usize)>> = self
            .droids
            .iter()
            .map(|d| (d.alert == Some(Alert::Evasion)).then(|| d.route.first().and_then(|&g| cell_of(g))).flatten())
            .collect();
        // The droid after the player that is nearest them goes straight at them; the others make
        // for somewhere ahead of them, to cut them off.
        let nearest_chaser = self
            .droids
            .iter()
            .enumerate()
            .filter(|(_, d)| d.alert == Some(Alert::Alert) && !d.down && d.windup == 0.0)
            .min_by(|(_, a), (_, b)| flat(a.feet - player).norm().total_cmp(&flat(b.feet - player).norm()))
            .map(|(n, _)| n);
        let player_going = seeing.map(|d| d.lost_going);
        for (me, droid) in self.droids.iter_mut().enumerate() {
            // In the hostile droid's colours while it is after the player; it goes down in
            // whichever it had on.
            let hostile = droid.alert.is_some();
            if !droid.down && droid.side.is_none() && hostile != droid.looks_hostile {
                if let Some(livery) = self.liveries.get(droid.character) {
                    recolour(graph, droid.avatar.root(), livery, hostile);
                }
                droid.looks_hostile = hostile;
            }
            // Limp, the physics has it: its bones follow its bodies, its capsule is gone, and it
            // is wherever its pelvis has got to.
            if let Some(ragdoll) = droid.ragdoll.as_mut() {
                ragdoll.update(graph);
                if ragdoll.is_limp() {
                    if graph.is_valid_handle(droid.collider) {
                        graph.remove_node(droid.collider);
                    }
                    if let Some(pelvis) = ragdoll.pelvis(graph) {
                        droid.feet.x = pelvis.x;
                        droid.feet.z = pelvis.z;
                    }
                    droid.speed = 0.0;
                    droid.route.clear();
                    continue;
                }
            }
            let (foe, sees_foe) = foes[me];
            if foe.is_some() && droid.foe.is_none() {
                engaged.push(me);
            }
            droid.foe = foe;
            droid.foe_unseen = if sees_foe { 0.0 } else { droid.foe_unseen + dt };
            droid.reload = (droid.reload - dt).max(0.0);
            droid.armed_left = (droid.armed_left - dt).max(0.0);
            // After the player, it goes straight for them rather than round them; after one of
            // the other side, straight for it - the player coming first.
            let hunting = droid.alert == Some(Alert::Alert);
            let fighting = droid.foe.filter(|_| !hunting);
            let others = everyone
                .iter()
                .enumerate()
                .filter(|&(other, _)| {
                    other != me
                        && !down.get(other).copied().unwrap_or(false)
                        && !(hunting && other == the_player)
                        && Some(other) != fighting
                })
                .map(|(_, &other)| other);
            // In capture the flag, whoever of the other side it sees to shoot at: the player
            // first, for the enemy's; and near enough, it stands to shoot.
            let target = match droid.side {
                Some(_) if hunting && droid.sees_player => Some(player + Vector3::new(0.0, PLAYER_CHEST, 0.0)),
                Some(_) if sees_foe => fighting.map(|n| everyone[n].0 + Vector3::new(0.0, CHEST, 0.0)),
                _ => None,
            }
            .filter(|&at| flat(at - droid.feet).norm() < FIRE_RANGE);
            let standing_to_shoot = target.is_some_and(|at| flat(at - droid.feet).norm() < ENGAGE);
            if target.is_some() {
                droid.armed_left = ARMED_FOR;
                droid.aimed_at = target;
            }
            droid.resting = (droid.resting - dt).max(0.0);
            droid.windup = (droid.windup - dt).max(0.0);
            droid.deaf = (droid.deaf - dt).max(0.0);
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
                let next = alert.next(droid.sees_player, droid.search_left);
                // One guarding its side's end never calms down.
                let next = match next {
                    None if droid.side.is_some() => {
                        droid.search_left = CAUTION;
                        Some(Alert::Caution)
                    }
                    next => next,
                };
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
                    let away = flat(player - droid.feet).norm();
                    let cut_off = player_going
                        .filter(|_| Some(me) != nearest_chaser && away > CLOSE_IN)
                        .and_then(|going| going.try_normalize(1.0e-3))
                        .map(|going| player + going * (away * 0.6).min(CUT_OFF))
                        .filter(|&ahead| (1..=4).all(|i| floor_at(player + (ahead - player) * (i as f32 / 4.0))));
                    droid.route = route_to((grid, origin), droid.feet, cut_off.unwrap_or(player), CHASE_REACH);
                }
            } else if let Some(foe) = fighting {
                // After one of the other side, the way to it worked out again every so often.
                droid.replan -= dt;
                if droid.replan <= 0.0 || droid.route.is_empty() {
                    droid.replan = REPLAN;
                    droid.route = route_to((grid, origin), droid.feet, everyone[foe].0, CHASE_REACH);
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
                        // Done looking: on to where the player is likeliest to be, of where it
                        // can get to, clear of where the others are going - or, with nowhere
                        // likelier than anywhere else, another spot nearby.
                        Some(_) => {
                            droid.looking = None;
                            droid.searched += 1;
                            let taken: Vec<(usize, usize)> = goals
                                .iter()
                                .enumerate()
                                .filter(|&(n, _)| n != me)
                                .filter_map(|(_, g)| *g)
                                .collect();
                            let next = cell_of(droid.feet).filter(|_| !search.is_empty()).and_then(|from| {
                                search.best(grid, from, &taken, SEARCH_APART / grid.cell_size, CHASE_REACH)
                            });
                            droid.route = match next {
                                Some(cell) => route_to((grid, origin), droid.feet, grid.on_floor(origin, cell), CHASE_REACH),
                                None => Vec::new(),
                            };
                            if droid.route.is_empty() {
                                droid.route = plan((grid, origin), droid.feet, SEARCH_TRIP, rng);
                            }
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
                            droid.route = route_to((grid, origin), droid.feet, guess, CHASE_REACH);
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
                    droid.route = match droid.post {
                        // In capture the flag, about where it keeps to.
                        Some(post) => spot_near((grid, origin), post, POST_REACH, rng)
                            .map(|to| route_to((grid, origin), droid.feet, to, CHASE_REACH))
                            .unwrap_or_default(),
                        None => plan((grid, origin), droid.feet, TRIP, rng),
                    };
                    if droid.route.is_empty() {
                        droid.resting = REST.0;
                    }
                }
            }
            // Near enough to shoot, it stands and faces them.
            if standing_to_shoot && !droid.down {
                droid.route.clear();
                if let Some(to) = target.map(|at| flat(at - droid.feet)).filter(|to| to.norm() > 1.0e-3) {
                    droid.heading = to.x.atan2(to.z);
                }
            }
            // Running after the player, and jogging to where it lost them.
            let after = droid.alert == Some(Alert::Alert) || fighting.is_some();
            let hurrying = after || (droid.alert == Some(Alert::Evasion) && droid.searched <= 1);
            // After the player, it sprints now and then, when it is not to be told.
            let chasing = droid.alert == Some(Alert::Alert) && droid.windup == 0.0 && !droid.down;
            (droid.sprint_in, droid.sprinting) =
                sprints(droid.sprint_in, droid.sprinting, chasing, rng, dt);
            // Out of breath, it walks until enough is back, and a sprint it was due waits.
            let gait = match (hurrying, droid.sprinting > 0.0) {
                _ if droid.winded => Gait::Walking,
                (true, true) => Gait::Sprinting,
                (true, false) if after => Gait::Running,
                (true, false) => Gait::Jogging,
                (false, _) => Gait::Walking,
            };
            let moving = droid.speed > 0.1 && !droid.down;
            let breathing = if moving { gait } else { Gait::Walking };
            droid.sentry = sentry(droid.character);
            let (most, recovery) = match droid.sentry {
                true => (SENTRY_STAMINA, SENTRY_RECOVERY),
                false => (1.0, 1.0),
            };
            droid.most = most;
            droid.stamina = droid.stamina.min(most);
            (droid.stamina, droid.winded) =
                breath::breathe(droid.stamina, droid.winded, breathing, dt, most, recovery);
            if droid.winded {
                droid.sprinting = 0.0;
            }
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
                        let fallback = match gait {
                            Gait::Sprinting => FALLBACK_SPRINT,
                            Gait::Running => FALLBACK_RUNNING,
                            Gait::Walking => FALLBACK_PACE,
                            Gait::Jogging => FALLBACK_RUN,
                        };
                        let pace = droid
                            .avatar
                            .pace(Posture::Standing, gait)
                            .unwrap_or(fallback);
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
                && droid.side.is_none()
                && droid.windup == 0.0
                && droid.sees_player
                && flat(to_player).norm() < CATCH
                && to_player.y.abs() < CATCH_HEIGHT
            {
                caught = caught.or(Some(me));
                // Having hit them, it stands a moment before coming on again.
                droid.windup = HIT_PAUSE;
                droid.speed = 0.0;
            }

            if let Ok(body) = graph.try_get_mut_of_type::<RigidBody>(droid.body) {
                body.set_next_kinematic_translation(droid.feet + Vector3::new(0.0, MIDDLE, 0.0));
            }
            // Speaking to the player, its head turned to them.
            droid.speaking = (droid.speaking - dt).max(0.0);
            let to_player = flat(player - droid.feet);
            let head = (droid.speaking > 0.0 && !droid.down && to_player.norm() > 1.0e-3)
                .then(|| to_player.x.atan2(to_player.z) - droid.heading);
            droid.avatar.turn_head(head);
            // Firing when it can, facing them near enough; looking up or down at them.
            let facing_them = target.and_then(|at| {
                let to = flat(at - droid.feet);
                let off = (to.x.atan2(to.z) - droid.heading + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                (off.abs() < FIRE_CONE).then_some(at)
            });
            let trigger = facing_them.is_some() && droid.reload == 0.0 && !droid.down;
            if trigger {
                droid.reload = between(rng, FIRE_EVERY);
            }
            let look = target.map_or((0.0, 0.0), |at| {
                let from = droid.feet + Vector3::new(0.0, FACE_HEIGHT, 0.0);
                let to = at - from;
                (to.y.atan2(flat(to).norm()), 0.0)
            });
            let going = Going {
                heading: Some(droid.heading),
                speed: droid.speed,
                posture: match droid.down {
                    true => Posture::Crouching,
                    false => Posture::Standing,
                },
                gait,
                grounded: true,
                jumped: false,
                low: false,
                falling: 0.0,
                cover: None,
                corner: false,
                peeking: false,
                pushing: true,
                strafing: false,
                armed: droid.armed_left > 0.0 && !droid.down,
                trigger,
                raised: target.is_some(),
                look,
                way: 0.0,
            };
            droid.avatar.animate(graph, going, dt);
            // A shot leaves the muzzle as the pistol fires, at what it aimed at, give or take.
            if let (Some((from, _)), Some(at), Some(side)) = (droid.avatar.shot(), droid.aimed_at, droid.side) {
                let wide = SPREAD * (at - from).norm() / 10.0;
                let miss = Vector3::new(
                    between(rng, (-wide, wide)),
                    between(rng, (-0.6 * wide, 0.6 * wide)),
                    between(rng, (-wide, wide)),
                );
                if let Some(way) = (at + miss - from).try_normalize(1.0e-3) {
                    shots.push((from, way, side));
                }
            }
        }
        self.search = Some(search);
        News {
            hit: caught,
            alerts,
            heard,
            alarmed,
            engaged,
            shots,
        }
    }

    /// Has each hostile droid look for the player, at `player` in `posture`, whom it can see
    /// wherever `in_sight` says nothing is in the way from the player to it, and [`Sight`] says
    /// they are in front of it and near enough - `in_the_dark` or not.
    pub fn look_for_player(
        &mut self,
        player: Vector3<f32>,
        posture: Posture,
        in_the_dark: bool,
        in_sight: impl Fn(Vector3<f32>) -> bool,
    ) {
        for droid in &mut self.droids {
            droid.sees_player = match droid.alert {
                Some(alert) if !droid.down && droid.hunts_player() => {
                    Sight::default().could_see(
                        Some(alert),
                        droid.feet,
                        droid.heading,
                        player,
                        posture.into(),
                        in_the_dark,
                    ) && in_sight(droid.feet)
                }
                _ => false,
            };
        }
    }

    /// Has each sentry - each droid whose kind, as an index into the conversations' characters,
    /// `sentry` says is one - that sees another sentry after the player go after them too, as if
    /// it saw them itself. It sees the other as [`could_see_droid`] has it, `in_the_dark` or not,
    /// and as long as nothing is in the way. Calm, it stands a moment first, as one does that has
    /// just turned hostile. Call it after [`Self::look_for_player`]: those after the player are the
    /// ones that can see them, or see another that can.
    pub fn join_chases(
        &mut self,
        graph: &Graph,
        in_the_dark: bool,
        sentry: impl Fn(usize) -> bool,
    ) {
        let watches: Vec<Watch> = self
            .droids
            .iter()
            .map(|droid| Watch {
                sentry: sentry(droid.character),
                alert: droid.alert,
                sees_player: droid.sees_player,
                busy: droid.down || droid.talking || !droid.hunts_player(),
                windup: droid.windup,
                feet: droid.feet,
                heading: droid.heading,
            })
            .collect();
        let droids = &self.droids;
        let joining = joiners(&watches, in_the_dark, |m, n| {
            in_sight_of(graph, &droids[m], &droids[n])
        });
        for m in joining {
            let droid = &mut self.droids[m];
            if droid.alert.is_none() {
                droid.windup = WINDUP;
                droid.threat = 0.0;
                droid.warned = 0;
            }
            droid.sees_player = true;
            droid.enter(m, Some(Alert::Alert), &mut self.alerts);
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
            let Some(patience) = patience(droid.character).filter(|_| droid.hunts_player()) else {
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
                && droid.hunts_player()
                && answers(droid.character)
                && !droid.down
                && matches!(droid.alert, None | Some(Alert::Caution))
                && (droid.feet - from).norm() < ALARM_RANGE;
            if !answering {
                continue;
            }
            droid.talking = false;
            droid.warned = 0;
            droid.threat = 0.0;
            droid.lost_at = player;
            droid.lost_going = Vector3::zeros();
            droid.alert = None;
            droid.enter(m, Some(Alert::Evasion), &mut self.alerts);
            self.alarmed.push(m);
            // Where the player was, as the alarm had it, is where the search starts from.
            self.search_from = Some(player);
        }
    }

    /// The droid whose body `collider` is, if it is one.
    pub fn hit(&self, collider: Handle<Collider>) -> Option<usize> {
        self.droids
            .iter()
            .position(|droid| droid.collider == collider)
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
            !droid.down && droid.hunts_player() && matches!(droid.alert, Some(Alert::Evasion | Alert::Caution))
        };
        if !self.droids.iter().any(listening) {
            return;
        }
        let Some(noise) = Heard::at((grid, origin), at, loudness) else {
            return;
        };
        for (n, droid) in self.droids.iter_mut().enumerate() {
            if !listening(droid) {
                continue;
            }
            if !noise.by((grid, origin), droid.feet) {
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
            // Where the noise was is where the search starts from.
            self.search_from = Some(at);
        }
    }

    /// The most urgent phase any droid is in, and the longest any of them in it has left to
    /// search or stay wary, in seconds.
    pub fn alarm(&self) -> Option<(Alert, f32)> {
        // A guard's standing watch in capture the flag is no alarm.
        let raised = |droid: &&Inhabitant| !(droid.side.is_some() && droid.alert == Some(Alert::Caution));
        let alert = self.droids.iter().filter(raised).filter_map(|droid| droid.alert).max()?;
        let left = self
            .droids
            .iter()
            .filter(raised)
            .filter(|droid| droid.alert == Some(alert))
            .map(|droid| droid.search_left)
            .fold(0.0, f32::max);
        Some((alert, left))
    }

    /// How much breath each sentry that is hostile, or has spent some, has left.
    pub fn breath(&self, graph: &Graph) -> Vec<Breath> {
        self.droids
            .iter()
            .enumerate()
            .filter(|(_, droid)| droid.sentry && !droid.down)
            .filter(|(_, droid)| droid.alert.is_some() || droid.stamina < droid.most || droid.hits > 0)
            .filter_map(|(n, droid)| {
                Some(Breath {
                    feet: droid.feet,
                    face: self.face(graph, n)?,
                    left: droid.stamina / droid.most,
                    winded: droid.winded,
                    health: 1.0 - droid.hits as f32 / HITS as f32,
                })
            })
            .collect()
    }

    /// Turns the `n`th droid on the player: after a moment it hunts them, and it cannot be
    /// talked to any more.
    pub fn set_hostile(&mut self, n: usize) {
        if let Some(droid) = self.droids.get_mut(n).filter(|droid| !droid.down && droid.hunts_player()) {
            droid.alert = Some(Alert::Alert);
            droid.windup = WINDUP;
            droid.replan = 0.0;
        }
    }

    /// Shoots down the droid nearest the `player`'s feet, as bolts from them would, [`HITS`]
    /// times over, for MAZE_KNOCKDOWN, or once through the head with MAZE_HEADSHOT; and with MAZE_DISMEMBER=<bodies>, a comma-separated list
    /// of ragdoll bodies such as head,forearm.L,shin.R, breaks those off it too. Which one, if any
    /// is standing.
    pub fn knock_down(&mut self, graph: &mut Graph, player: Vector3<f32>) -> Option<usize> {
        let n = (0..self.droids.len())
            .filter(|&n| !self.droids[n].down)
            .min_by(|&a, &b| {
                let distance = |n: usize| (self.droids[n].feet - player).norm();
                distance(a).total_cmp(&distance(b))
            })?;
        self.set_hostile(n);
        let droid = &self.droids[n];
        let mut at = droid.feet + Vector3::new(0.0, 1.2, 0.0);
        let mut from = player + Vector3::new(0.0, 1.5, 0.0);
        // With MAZE_HEADSHOT, one bolt level through the middle of its skull.
        let headshot = crate::platform::var("MAZE_HEADSHOT").is_some();
        if headshot {
            if let Some((head, _)) = graph.find_by_name(droid.avatar.root(), "DEF-spine.004") {
                at = graph[head].global_position() + Vector3::new(0.0, 0.19, 0.0);
                from = Vector3::new(player.x, at.y, player.z);
            }
        }
        let strike = Strike {
            collider: droid.collider,
            at,
            way: (at - from).try_normalize(1.0e-6).unwrap_or_else(Vector3::z),
        };
        let n = (0..HITS).find_map(|_| self.shot(graph, strike, player))?;
        if let Some(bodies) = crate::platform::var("MAZE_DISMEMBER") {
            for body in bodies
                .split(',')
                .map(str::trim)
                .filter(|body| !body.is_empty())
            {
                self.droids[n].break_off(graph, body);
                Log::info(format!("MAZE_DISMEMBER: {body} broken off droid {n}"));
            }
        }
        Some(n)
    }

    /// A bolt from the pistol of the player at `player` has struck something. If it is a hostile
    /// droid, that is another hit on it, and at [`HITS`] it goes down, its eyes dark: limp,
    /// knocked back by the bolt, or without a ragdoll crouched, and no taller than it is
    /// crouched. Short of that, if it has not seen who fired, it searches for them where they
    /// fired from. A droid already lying there is just shoved. The droid that went down, if one
    /// did.
    pub fn shot(
        &mut self,
        graph: &mut Graph,
        strike: Strike,
        player: Vector3<f32>,
    ) -> Option<usize> {
        let collider = strike.collider;
        if let Some(droid) = self.droids.iter_mut().find(|droid| {
            droid
                .ragdoll
                .as_ref()
                .is_some_and(|ragdoll| ragdoll.owns(collider))
        }) {
            let ragdoll = droid.ragdoll.as_ref()?;
            ragdoll.shove(graph, collider, strike.way * ragdoll::SHOVE, strike.at);
            let body = ragdoll.body_of(collider);
            let headshot = body
                .filter(|&body| body == HEAD)
                .and_then(|_| droid.headshot(graph, strike.at, strike.way));
            if !headshot.is_some_and(|head| droid.blow_head_up(graph, head, strike.way)) {
                if let Some(body) = body {
                    droid.break_off(graph, body);
                }
            }
            return None;
        }
        let (n, droid) = self
            .droids
            .iter_mut()
            .enumerate()
            .find(|(_, droid)| droid.collider == collider)?;
        if (droid.alert.is_none() && droid.side.is_none()) || droid.down {
            return None;
        }
        // A good headshot stops it there and then.
        let headshot = droid.headshot(graph, strike.at, strike.way);
        droid.hits += 1;
        if droid.hits < HITS && headshot.is_none() {
            if droid.side.is_some() {
                // In capture the flag, it turns to whoever it was, to see them.
                let to = flat(player - droid.feet);
                if to.norm() > 1.0e-3 && droid.alert != Some(Alert::Alert) {
                    droid.heading = to.x.atan2(to.z);
                }
            } else if droid.alert != Some(Alert::Alert) {
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
        droid.ragdoll = Ragdoll::start(
            graph,
            droid.avatar.root(),
            forward(droid.heading) * droid.speed,
            Some((strike.way * ragdoll::STOPPING_BLOW, strike.at)),
        );
        if let Some(head) = headshot {
            droid.blow_head_up(graph, head, strike.way);
        }
        if let Some(ragdoll) = &droid.ragdoll {
            if headshot.is_none() {
                if let Some(body) = ragdoll.body_struck(graph, strike.at, strike.way) {
                    droid.break_off(graph, body);
                }
            }
            return Some(n);
        }
        if let Ok(shape) = graph.try_get_mut(droid.collider) {
            shape.set_shape(ColliderShape::capsule_y(0.5 * DOWN_HEIGHT - RADIUS, RADIUS));
            shape.local_transform_mut().set_position(Vector3::new(
                0.0,
                0.5 * DOWN_HEIGHT - MIDDLE,
                0.0,
            ));
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
        self.droids
            .get(n)
            .map(|droid| (droid.character, droid.code))
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
    /// moment before going on its way.
    pub fn set_talking(&mut self, n: usize, talking: bool) {
        if let Some(droid) = self.droids.get_mut(n) {
            droid.talking = talking;
            if !talking {
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
            droid
                .avatar
                .set_visible(graph, level.can_see(droid.feet));
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
    #[test]
    fn only_those_close_by_and_in_front_can_be_talked_to_nearest_first() {
        let droids = [
            Vector3::new(0.0, 0.0, 2.0),  // ahead
            Vector3::new(0.3, 0.0, 1.0),  // nearer, a little to one side
            Vector3::new(0.0, 0.0, -1.0), // behind
            Vector3::new(2.0, 0.0, 0.5),  // well off to the side
            Vector3::new(0.0, 0.0, 4.0),  // too far
        ];
        assert_eq!(
            within_talking(droids.into_iter(), Vector3::zeros(), AHEAD),
            [1, 0]
        );
    }

    #[test]
    fn it_sprints_now_and_then_but_never_as_the_chase_starts() {
        let dt = 0.05;
        let mut rng = Rng::new(3);
        let (mut sprint_in, mut sprinting) = (-1.0, 0.0);
        let mut first = None;
        let mut sprints_begun = 0;
        for frame in 0..(60.0 / dt) as u32 {
            let was = sprinting > 0.0;
            (sprint_in, sprinting) = sprints(sprint_in, sprinting, true, &mut rng, dt);
            if !was && sprinting > 0.0 {
                sprints_begun += 1;
                first.get_or_insert(frame as f32 * dt);
            }
        }
        let first = first.expect("it sprints");
        assert!(first >= SPRINT_EVERY.0 - dt, "not straight away: {first}");
        assert!(sprints_begun >= 3, "again and again: {sprints_begun}");
        // No longer chasing, it forgets, and starts over when it chases again.
        assert_eq!(sprints(2.0, 1.0, false, &mut rng, dt), (-1.0, 0.0));
    }

    #[test]
    fn a_sentry_sees_another_as_it_would_the_player_standing() {
        let sees = |alert, x: f32, z: f32, dark| {
            could_see_droid(alert, Vector3::zeros(), 0.0, Vector3::new(x, 0.0, z), dark)
        };
        // Calm, it looks about it as it does wary: ahead, and not behind or off to the side.
        assert!(sees(None, 0.0, 20.0, false), "ahead");
        assert!(!sees(None, 0.0, -10.0, false), "behind");
        assert!(!sees(None, 10.0, 1.0, false), "off to the side");
        assert!(!sees(None, 0.0, 20.0, true), "far off, in the dark");
        // Searching or after the player, the same: only in front.
        assert!(!sees(Some(Alert::Evasion), 0.0, -10.0, false));
        assert!(!sees(Some(Alert::Alert), 0.0, -10.0, false));
        assert!(sees(Some(Alert::Alert), 0.0, 10.0, false));
        assert!(!sees(Some(Alert::Alert), 0.0, Sight::default().range + 1.0, false));
    }

    #[test]
    fn a_sentry_that_sees_another_after_the_player_joins_in() {
        let droid = |sentry, alert, sees_player, z: f32| Watch {
            sentry,
            alert,
            sees_player,
            busy: false,
            windup: 0.0,
            feet: Vector3::new(0.0, 0.0, z),
            heading: 0.0,
        };
        let chaser = || droid(true, Some(Alert::Alert), true, 10.0);
        let open = |_, _| true;
        // Calm and searching sentries facing the chase join in; the chaser does not.
        let calm = droid(true, None, false, 0.0);
        let searching = droid(true, Some(Alert::Evasion), false, 0.0);
        assert_eq!(joiners(&[chaser(), calm], false, open), [1]);
        assert_eq!(joiners(&[chaser(), searching], false, open), [1]);
        // Not a sentry, facing away, with a wall between, or talking: it does not.
        assert!(joiners(&[chaser(), droid(false, None, false, 0.0)], false, open).is_empty());
        let facing_away = droid(true, None, false, 20.0);
        assert!(joiners(&[chaser(), facing_away], false, open).is_empty());
        let walled_off = joiners(&[chaser(), droid(true, None, false, 0.0)], false, |_, _| {
            false
        });
        assert!(walled_off.is_empty());
        let talking = Watch {
            busy: true,
            ..droid(true, None, false, 0.0)
        };
        assert!(joiners(&[chaser(), talking], false, open).is_empty());
        // One still standing, having just turned hostile, is not yet after them; nor is one on
        // Alert that has lost sight of them.
        let winding_up = Watch {
            windup: 0.5,
            ..chaser()
        };
        assert!(joiners(&[winding_up, droid(true, None, false, 0.0)], false, open).is_empty());
        let lost = droid(true, Some(Alert::Alert), false, 10.0);
        assert!(joiners(&[lost, droid(true, None, false, 0.0)], false, open).is_empty());
    }

    #[test]
    fn the_pistol_is_pointed_at_what_is_in_the_middle_of_the_view() {
        let eye = Vector3::zeros();
        let ahead = Vector3::z();
        assert!(pointed_at(eye, ahead, Vector3::new(0.3, 0.0, 10.0)));
        assert!(
            !pointed_at(eye, ahead, Vector3::new(1.0, 0.0, 10.0)),
            "off to the side"
        );
        assert!(
            !pointed_at(eye, ahead, Vector3::new(0.0, 0.0, -5.0)),
            "behind"
        );
        assert!(
            !pointed_at(eye, ahead, Vector3::new(0.0, 0.0, AIM_RANGE + 1.0)),
            "too far"
        );
    }

    #[test]
    fn each_warning_holds_long_enough_to_be_said() {
        assert_eq!(
            stage_after(0, 1.0),
            1.0,
            "its patience, before it first warns"
        );
        assert!(stage_after(1, 1.0) >= WARNING, "then time to say it");
        assert_eq!(stage_after(1, 1.0), stage_after(2, 1.0));
        assert!(
            stage_after(0, 1.0) < stage_after(0, 2.0),
            "a less patient droid is sooner"
        );
    }

}
