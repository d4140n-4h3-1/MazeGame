//! The droid the player is seen as in third person, and the maze's inhabitants are too: its
//! model, and the cycles it walks, runs, sprints and crouches along with.
//!
//! The droid's feet set the pace. However a cycle was made - walking on the spot, or forward
//! through the scene - its hips go past whichever foot is planted on the floor as fast as the
//! droid goes when that foot stays put, and that is what each gait's speed is (see
//! [`Avatar::pace`]). At any speed along the way, speeding up or slowing down, a cycle is played
//! exactly as fast as the floor goes by under it. Any travel built into it is taken back out,
//! since the body does the moving.
//!
//! Each gait standing has a cycle of its own: walking, running with Caps Lock, sprinting with
//! Shift. Crouching and crawling share the crouch, played faster for the quicker gaits and slower
//! down on the floor. Standing still, the droid settles into its idle, or its rest pose if it has
//! none; crouched, it holds the crouch where it stopped. Going from one cycle to another, it
//! carries on at the same point in the stride, with the same foot down - whichever foot each
//! cycle starts on.
//!
//! The droid turns to face the way the keys held send it - ahead, to either side, back, or along
//! any of the diagonals between - turning the short way round; let go, it turns back to face
//! ahead. The camera and the body stay facing straight ahead.
//!
//! Strafing, with the right mouse button held, it keeps facing ahead instead, and steps whichever
//! way the body goes: forward, or sideways, back and along each diagonal in its strafes, walking,
//! running, sprinting or crouched, crawling too. Whichever of those is nearest the way it is
//! going, it turns only by what is left over, a few degrees. Without a strafe back it plays its
//! cycle backwards, and without the others it turns as far as it has to, up to side on.
//!
//! Running or sprinting flat out, the droid skids. Turning round - from going forward to going
//! back, say - it digs its feet in, slides, swings round whichever way is shorter and runs out
//! the other way, out of a sprint in a skid of its own. Turning a quarter of the way round, it
//! cuts across. Letting go of the keys, it slides to a standstill and idles. Which it does goes
//! by how fast it is going rather than the keys, so letting go of Shift with the rest still
//! stops out of a sprint.
//!
//! The skids are made on the spot; [`MOTION`] has where each takes the droid and how far round,
//! frame by frame. The skid carries the body along that path, as far as it goes for how fast
//! the droid went in, and swings the droid round with it. A turn hands back to the cycles as it
//! runs out, and a stop to the idle, which it ends on. Leaving the ground, crouching, strafing or
//! taking cover cuts a skid short, as does setting off again in a stop, and strafing or in cover
//! the droid never skids.
//!
//! Jumping, the droid pushes off, flies with its legs tucked and takes the landing in its knees,
//! standing or on the move as it is going at the time: from standing still it springs straight
//! up and lands on the spot, and on the move it leaps in its stride and lands running. Every jump
//! starts as a high one, and turns into a low one if the jump is cut short. The push off starts
//! from its lowest point, since the body leaves the ground the moment the key goes down. Falling
//! off an edge it flies the same way, once it has been in the air long enough to be more than a
//! step down. It lands hard or lightly as it was falling fast or not.
//!
//! In cover against a wall, it plays its cover idle and its cover walk, edging along the wall,
//! in place of the usual ones - once it has them. Until then it idles and walks as ever.
//!
//! Its meshes cast no shadows. The traced shadows are gathered once, when meshes are added or
//! removed, so a droid in them would leave its shadow behind where it was first put down.

use super::posture::{Gait, Posture};
use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        log::Log,
        pool::Handle,
    },
    fxhash::FxHashMap,
    generic_animation::value::{TrackValue, ValueBinding},
    graph::SceneGraph,
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        animation::{Animation, AnimationContainer, AnimationPlayer},
        graph::Graph,
        mesh::Mesh,
        node::Node,
        Scene,
    },
};

/// The droid's model.
pub const DROID_MODEL: &str = "data/droid_full_deform.glb";
/// How much the model is scaled. It stands 2 m tall; the body is 1.7 m.
const SCALE: f32 = 0.85;
/// The droid's cycles by name, with the gait each is for. The crouch, for no gait, is for
/// crouching and crawling at any of them.
const CYCLES: [(&str, Option<Gait>); 4] = [
    ("droid_walk_cycle", Some(Gait::Walking)),
    ("droid_run_cycle", Some(Gait::Running)),
    ("droid_sprint_cycle", Some(Gait::Sprinting)),
    ("droid_crouch_cycle", None),
];
/// The droid's strafes, walking, running and crouched, each to its left and to its right,
/// forward to its left and to its right, back, and back to its left and to its right.
const STRAFES: [[&str; 7]; 3] = [
    [
        "droid_strafe_walk_L",
        "droid_strafe_walk_R",
        "droid_strafe_walk_FL",
        "droid_strafe_walk_FR",
        "droid_strafe_walk_B",
        "droid_strafe_walk_BL",
        "droid_strafe_walk_BR",
    ],
    [
        "droid_strafe_run_L",
        "droid_strafe_run_R",
        "droid_strafe_run_FL",
        "droid_strafe_run_FR",
        "droid_strafe_run_B",
        "droid_strafe_run_BL",
        "droid_strafe_run_BR",
    ],
    [
        "droid_strafe_crouch_L",
        "droid_strafe_crouch_R",
        "droid_strafe_crouch_FL",
        "droid_strafe_crouch_FR",
        "droid_strafe_crouch_B",
        "droid_strafe_crouch_BL",
        "droid_strafe_crouch_BR",
    ],
];
/// The gait each row of [`STRAFES`] is for, like [`CYCLES`].
const STRAFE_GAITS: [Option<Gait>; 3] = [Some(Gait::Walking), Some(Gait::Running), None];
/// How much nearer the way it is going another step has to be, in radians, for the droid to
/// change to it strafing, so that going just about halfway between two it does not keep changing.
const STRAFE_MARGIN: f32 = 10.0 * std::f32::consts::PI / 180.0;
/// Where the droid's animations take it and how far round, frame by frame, as they were made:
/// the skids, made on the spot, go by it.
pub const MOTION: &str = "data/droid_motion.json";
/// The droid's skids round, out of a run and out of a sprint, each to its left and to its right.
const TURNS: [[&str; 2]; 2] = [
    ["droid_skid_turn_L", "droid_skid_turn_R"],
    ["droid_skid_sprint_turn_L", "droid_skid_sprint_turn_R"],
];
/// Its quarter turns cut across, to its left and to its right.
const CUTS: [&str; 2] = ["droid_skid_turn90_L", "droid_skid_turn90_R"];
/// Its slides to a standstill, out of a run and out of a sprint.
const STOPS: [&str; 2] = ["droid_skid_stop", "droid_skid_sprint_stop"];
/// How far round the keys have to swing the droid, in radians, for it to skid round: from ahead
/// to straight back, or to either of the diagonals behind.
const SKID_ANGLE: f32 = 0.75 * std::f32::consts::PI - 1.0e-3;
/// How far round they have to swing it to cut across: a quarter of the way.
const CUT_ANGLE: f32 = 0.5 * std::f32::consts::PI - 1.0e-3;
/// How near a gait's full pace the droid has to be going to skid out of it, as a share of it.
const SKID_SPEED: f32 = 0.75;
/// The droid's jumps, standing still and on the move, each low and high: pushing off, in the air,
/// and landing.
const LEAPS: [[[&str; 3]; 2]; 2] = [
    [
        [
            "droid_jump_stand_short_start",
            "droid_jump_stand_short_loop",
            "droid_jump_stand_short_land",
        ],
        [
            "droid_jump_stand_high_start",
            "droid_jump_stand_high_loop",
            "droid_jump_stand_high_land",
        ],
    ],
    [
        [
            "droid_jump_run_short_start",
            "droid_jump_run_short_loop",
            "droid_jump_run_short_land",
        ],
        [
            "droid_jump_run_high_start",
            "droid_jump_run_high_loop",
            "droid_jump_run_high_land",
        ],
    ],
];
/// How fast the droid has to be falling as it lands, in meters per second, to land hard: from a
/// high jump, or a drop of more than about half a meter.
const HARD_LANDING: f32 = 3.2;
/// Above this speed along the ground, in meters per second, the droid jumps and lands on the
/// move rather than standing still.
const LEAP_MOVING: f32 = 0.5;
/// How fast a jump's animation can carry the droid along the ground, in meters per second, and
/// still be one that stays where it is: landing on the spot shuffles the feet a little.
const IN_PLACE: f32 = 0.3;
/// How long the droid is in the air, in seconds, before going off an edge counts as a fall rather
/// than a step down.
const FALLING_AFTER: f32 = 0.2;
/// The droid's idle, played standing still. It stays where it is, so has no travel to take out.
const IDLE: &str = "droid_idle_cycle";
/// Its idle and its walk in cover, up against a wall, if it has them.
const COVER_IDLE: &str = "droid_cover_idle";
const COVER_WALK: &str = "droid_cover_walk";
/// How fast the crouch is played for each gait - walking, running, sprinting - crouched, and
/// down on the floor crawling, as a multiple of how it was made. Each is slower than the one
/// above it, as every gait is slower the lower the posture.
const CROUCHING_RATES: [f32; 3] = [1.0, 1.3, 1.6];
const CRAWLING_RATES: [f32; 3] = [0.5, 0.65, 0.8];
/// The droid's hips, which its speed is measured by, and which way it faces goes by.
const HIPS: &str = "DEF-spine";
/// Its feet, one of which is planted on the floor at a time, walking.
const FEET: [&str; 2] = ["DEF-foot.L", "DEF-foot.R"];
/// How near the floor, in the model's own meters, a foot has to be to be planted on it: as low as
/// either foot gets in the animation, give or take.
const FOOT_DOWN: f32 = 0.03;
/// How often an animation is sampled to measure it, in seconds.
const SAMPLE: f32 = 1.0 / 120.0;
/// Below this speed along the ground, in meters per second, the droid is standing still.
const STILL: f32 = 0.05;
/// How long going from one cycle to another, or to and from rest, takes, in seconds.
const FADE_TIME: f32 = 0.2;
/// How quickly the droid turns to face the way it is going, like a rate: after 1/TURN_RATE
/// seconds, about two thirds of the turn is done.
const TURN_RATE: f32 = 12.0;

/// The capsule round the droid for the graphics effects, in meters: how tall, and how far out
/// from its middle. Wide enough for its arms, held out a little from its sides.
const CAPSULE_HEIGHT: f32 = 1.75;
const CAPSULE_RADIUS: f32 = 0.55;

/// A droid standing with its feet at `feet`, having moved by `moved` since the last frame, for
/// the graphics effects.
pub(crate) fn capsule(feet: Vector3<f32>, moved: Vector3<f32>) -> fyrox_gfx::MovingThing {
    fyrox_gfx::MovingThing {
        bottom: feet + Vector3::new(0.0, CAPSULE_RADIUS, 0.0),
        top: feet + Vector3::new(0.0, CAPSULE_HEIGHT - CAPSULE_RADIUS, 0.0),
        radius: CAPSULE_RADIUS,
        moved,
    }
}

/// A bone's pose, relative to its parent.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bone {
    position: Vector3<f32>,
    rotation: UnitQuaternion<f32>,
}

impl Bone {
    /// No move and no turn.
    fn identity() -> Self {
        Self {
            position: Vector3::zeros(),
            rotation: UnitQuaternion::identity(),
        }
    }

    fn of(node: &Node) -> Self {
        let transform = node.local_transform();
        Self {
            position: **transform.position(),
            rotation: **transform.rotation(),
        }
    }

    /// `t` of the way from this pose to `other`.
    fn towards(self, other: Self, t: f32) -> Self {
        Self {
            position: self.position.lerp(&other.position, t),
            rotation: self
                .rotation
                .try_slerp(&other.rotation, t, 1.0e-6)
                .unwrap_or(other.rotation),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Cycle {
    animation: Handle<Animation>,
    /// The gait it is for; none is the crouch.
    gait: Option<Gait>,
    /// How fast the droid goes over the ground as the cycle was made, with its feet keeping to
    /// the floor, in meters per second at the droid's size in the game.
    speed: f32,
    /// Which way it carries the droid, in radians from ahead, left positive: ahead but for the
    /// strafes.
    way: f32,
    /// How far through it, from 0 to 1, the left foot is down, like [`left_step`].
    phase: f32,
}

/// Which way the droid steps, strafing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Ahead,
    /// The cycle played backwards.
    Back,
    /// A strafe, any way round, as an index into `cycles`.
    Strafe(usize),
}

/// Which of `steps`, each with the way it carries the droid, is nearest `way`, in radians: the
/// first of the nearest, give or take a rounding, but keeping to `current` unless another is
/// nearer by `STRAFE_MARGIN`.
fn nearest<T: Copy + PartialEq>(way: f32, steps: &[(f32, T)], current: Option<T>) -> Option<T> {
    let off = |step: f32| wrap(way - step).abs();
    let (best, best_off) = steps
        .iter()
        .map(|&(step, t)| (t, off(step)))
        .fold(None, |best: Option<(T, f32)>, (t, o)| match best {
            Some((_, b)) if b <= o + 1.0e-3 => best,
            _ => Some((t, o)),
        })?;
    let kept = current.and_then(|c| steps.iter().find(|&&(_, t)| t == c));
    Some(match kept {
        Some(&(step, t)) if off(step) <= best_off + STRAFE_MARGIN => t,
        _ => best,
    })
}

/// Where the hips and the feet are, in the model's own terms.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stance {
    hips: Bone,
    feet: [Vector3<f32>; 2],
}

/// How far the hips went past the planted foot, along the ground, from `before` to `after`, in
/// the model's own terms: how far the droid went, with that foot staying put. None unless the
/// same foot is down, as low as `ground`, both times.
fn stride(before: &Stance, after: &Stance, ground: f32) -> Option<Vector3<f32>> {
    let lower = usize::from(before.feet[1].y < before.feet[0].y);
    let down = |stance: &Stance| stance.feet[lower].y - ground < FOOT_DOWN;
    (down(before) && down(after)).then(|| {
        let hips = after.hips.position - before.hips.position;
        let went = hips - (after.feet[lower] - before.feet[lower]);
        Vector3::new(went.x, 0.0, went.z)
    })
}

/// How fast the droid goes along the ground over `stances`, taken `SAMPLE` apart, in the model's
/// meters per second and which way, going by how far the hips go past the planted foot while
/// one is. None if neither ever is.
fn pace_of(stances: &[Stance], ground: f32) -> Option<Vector3<f32>> {
    let (went, planted) = stances
        .windows(2)
        .filter_map(|pair| stride(&pair[0], &pair[1], ground))
        .fold((Vector3::zeros(), 0), |(went, planted), stride| (went + stride, planted + 1));
    (planted > 0).then(|| went / (planted as f32 * SAMPLE))
}

/// How low the feet get in `stances`: the floor they are planted on.
fn ground(stances: &[Stance]) -> f32 {
    stances
        .iter()
        .flat_map(|stance| stance.feet.map(|foot| foot.y))
        .fold(f32::INFINITY, f32::min)
}

/// The bones from the droid's top down to its hips and to each foot, to find where those are in
/// the model's own terms whatever the rig puts above them.
#[derive(Debug, Clone, PartialEq)]
struct Skeleton {
    hips: Vec<Handle<Node>>,
    feet: [Vec<Handle<Node>>; 2],
}

/// The bones from just under `top` down to `node`, top first, going up by `parent_of`.
fn chain(
    parent_of: impl Fn(Handle<Node>) -> Handle<Node>,
    top: Handle<Node>,
    node: Handle<Node>,
) -> Vec<Handle<Node>> {
    let mut chain = Vec::new();
    let mut at = node;
    while at != top && at.is_some() {
        chain.push(at);
        at = parent_of(at);
    }
    chain.reverse();
    chain
}

/// Where the end of `chain` is and how it is turned, in the droid's own terms, with each bone
/// along it posed by `pose`.
fn place(chain: &[Handle<Node>], pose: impl Fn(Handle<Node>) -> Bone) -> Bone {
    chain.iter().fold(Bone::identity(), |above, &bone| {
        let bone = pose(bone);
        Bone {
            position: above.position + above.rotation * bone.position,
            rotation: above.rotation * bone.rotation,
        }
    })
}

impl Skeleton {
    /// Where the hips and feet are with every bone posed by `pose`.
    fn stance(&self, pose: impl Fn(Handle<Node>) -> Bone + Copy) -> Stance {
        Stance {
            hips: place(&self.hips, pose),
            feet: self.feet.each_ref().map(|foot| place(foot, pose).position),
        }
    }

    /// Where the hips and feet are in `animation` as it stands, with whatever it does not move at
    /// `rest`.
    fn stance_in(&self, animation: &Animation, rest: &FxHashMap<Handle<Node>, Bone>) -> Stance {
        self.stance(|bone| {
            let rest = rest.get(&bone).copied().unwrap_or_else(Bone::identity);
            let (position, rotation) = posed(animation, bone);
            Bone {
                position: position.unwrap_or(rest.position),
                rotation: rotation.unwrap_or(rest.rotation),
            }
        })
    }

    /// Where the hips and feet are all the way through `animation`, `SAMPLE` apart, with the time
    /// each is at.
    fn stances(
        &self,
        animation: &mut Animation,
        rest: &FxHashMap<Handle<Node>, Bone>,
    ) -> Vec<(f32, Stance)> {
        let slice = animation.time_slice();
        let looped = animation.is_loop();
        // Not looping for now, so that the end is the end rather than wrapped round to the start.
        animation.set_loop(false);
        let stances = (0..)
            .map(|step| slice.start + step as f32 * SAMPLE)
            .take_while(|&time| time <= slice.end)
            .map(|time| {
                animation.set_time_position(time);
                animation.tick(0.0);
                (time, self.stance_in(animation, rest))
            })
            .collect();
        animation.set_loop(looped);
        animation.rewind();
        stances
    }
}

/// An animation's motion as [`MOTION`] has it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct Clip {
    fps: f32,
    /// How far forward, how far to the left and how far round to the left, in the model's own
    /// meters and in degrees, each frame is from the first, in the way the droid faced then.
    forward_m: Vec<f32>,
    left_m: Vec<f32>,
    turn_left_deg: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct Motions {
    clips: std::collections::HashMap<String, Clip>,
}

/// What [`MOTION`] has, read once for every droid; none if it cannot be read.
fn motions() -> Option<&'static Motions> {
    static MOTIONS: std::sync::OnceLock<Option<Motions>> = std::sync::OnceLock::new();
    MOTIONS
        .get_or_init(|| {
            let read = std::fs::read_to_string(MOTION).map_err(|e| e.to_string());
            match read.and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string())) {
                Ok(motions) => Some(motions),
                Err(error) => {
                    Log::err(format!("Droid: could not read {MOTION}: {error}"));
                    None
                }
            }
        })
        .as_ref()
}

/// A skid: an animation played once through, and where it takes the droid.
#[derive(Debug, Clone, PartialEq)]
struct Skid {
    animation: Handle<Animation>,
    /// Where each frame takes the droid, from the first: how far forward and to the left, in the
    /// model's own meters, and how far round to the left, in radians - all in the way it faced as
    /// it went in.
    path: Vec<Vector3<f32>>,
    /// Frames a second.
    fps: f32,
    /// How fast it goes in, in the model's own meters per second.
    entry: f32,
}

impl Skid {
    /// The skid in `animation`, going by its `clip`. None if it does not go in on the move.
    fn new(animation: Handle<Animation>, clip: &Clip) -> Option<Self> {
        let path: Vec<Vector3<f32>> = clip
            .forward_m
            .iter()
            .zip(&clip.left_m)
            .zip(&clip.turn_left_deg)
            .map(|((&forward, &left), &turn)| Vector3::new(forward, left, turn.to_radians()))
            .collect();
        let entry = match path.as_slice() {
            [first, second, ..] => (second.xy() - first.xy()).norm() * clip.fps,
            _ => 0.0,
        };
        (entry > 1.0e-3 && clip.fps > 0.0).then_some(Self {
            animation,
            path,
            fps: clip.fps,
            entry,
        })
    }

    /// Where the skid has taken the droid `time` seconds in, like `path`: between frames, part
    /// of the way from one to the next; after the last, where that leaves it.
    fn at(&self, time: f32) -> Vector3<f32> {
        let frame = (time * self.fps).max(0.0);
        let last = self.path.len().saturating_sub(1);
        let before = (frame.floor() as usize).min(last);
        let after = (before + 1).min(last);
        self.path[before].lerp(&self.path[after], (frame - before as f32).min(1.0))
    }
}

/// Which way a skid goes: round, cut across, or to a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkidKind {
    Round,
    Cut,
    Stop,
}

/// The skids, by name, the droid could go into flat out at `gait`, from facing `from` towards
/// `to`, in radians, left positive, with the keys sending it anywhere or not - the gait's own
/// first, and failing that the other's. None for going on as it is.
fn skid_for(gait: Gait, from: f32, to: f32, pushing: bool) -> Option<(SkidKind, [&'static str; 2])> {
    let sprinting = usize::from(gait == Gait::Sprinting);
    let ours_first = |pair: [&'static str; 2]| [pair[sprinting], pair[1 - sprinting]];
    let left = wrap(to - from);
    let side = usize::from(left < 0.0);
    if !pushing {
        Some((SkidKind::Stop, ours_first(STOPS)))
    } else if left.abs() >= SKID_ANGLE {
        Some((SkidKind::Round, ours_first([TURNS[0][side], TURNS[1][side]])))
    } else if left.abs() >= CUT_ANGLE {
        Some((SkidKind::Cut, [CUTS[side]; 2]))
    } else {
        None
    }
}

/// One way through the air: an animation for each part of a jump.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Leap {
    start: Move,
    flight: Move,
    land: Move,
    /// The time in `start` at which the droid pushes off: its lowest.
    push_off: f32,
}

/// An animation played once through, or over and over in the air.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Move {
    animation: Handle<Animation>,
    /// How fast the droid goes along the ground in it, with its feet keeping to the floor, in
    /// meters per second at the droid's size in the game; none if it stays where it is.
    speed: Option<f32>,
}

/// Which part of a jump the droid is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    PushingOff,
    Flying,
    Landing,
}

/// A jump, or a fall, under way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Leaping {
    /// Which of `leaps`: standing still or on the move, and low or high.
    leap: (usize, usize),
    phase: Phase,
    /// Whether the feet have left the ground yet: they have not as the key goes down.
    left_ground: bool,
}

/// A skid under way.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Skidding {
    /// Which skid, by name.
    skid: &'static str,
    kind: SkidKind,
    /// Which way the droid faced as it began, like [`Avatar::heading`].
    heading: f32,
    /// Where one of the model's meters forward and to the left along the skid's path takes the
    /// body, across the world: the way the droid faced as it began, and as far as makes the
    /// skid go in as fast as the body did.
    forward: Vector3<f32>,
    left: Vector3<f32>,
    /// Where along the path it was last frame, like [`Skid::path`].
    last: Vector3<f32>,
}

/// What the body is doing, for the droid to go along with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Going {
    /// Which way the keys send it, like [`heading`]; none while the player cannot move.
    pub(crate) heading: Option<f32>,
    /// How fast it is going along the ground, in meters per second.
    pub(crate) speed: f32,
    pub(crate) posture: Posture,
    pub(crate) gait: Gait,
    /// Whether its feet are on the ground.
    pub(crate) grounded: bool,
    /// Whether it pushed off the ground to jump this frame.
    pub(crate) jumped: bool,
    /// Whether the jump it is in was cut short this frame, into a low one.
    pub(crate) low: bool,
    /// How fast it is falling, in meters per second.
    pub(crate) falling: f32,
    /// Whether it is in cover, up against a wall.
    pub(crate) cover: bool,
    /// Whether the keys send it anywhere.
    pub(crate) pushing: bool,
    /// Whether it is strafing: keeping facing ahead whichever way it goes.
    pub(crate) strafing: bool,
    /// Which way it is actually going along the ground, in radians from the way it faces, left
    /// positive. Only strafing goes by it.
    pub(crate) way: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Avatar {
    root: Handle<Node>,
    /// How the model's root is turned as it comes, before the droid turns it any further.
    upright: UnitQuaternion<f32>,
    animations: Handle<Node>,
    cycles: Vec<Cycle>,
    /// How many of `cycles`, from the first, are the ones for the gaits; the rest are played
    /// only in cover or strafing.
    gaited: usize,
    /// Played standing still, when the droid has one.
    idle: Option<Handle<Animation>>,
    /// Played in cover in place of the idle, and of the walk, as an index into `cycles`, when
    /// the droid has them.
    cover_idle: Option<Handle<Animation>>,
    cover_walk: Option<usize>,
    /// Its strafes walking, running and crouched, each way, as indexes into `cycles`, as far as
    /// it has them: like [`STRAFES`].
    strafes: [[Option<usize>; 7]; 3],
    /// Which way it is stepping, strafing on the move.
    stepping: Option<Step>,
    /// Whether it was in cover as of the last frame.
    covered: bool,
    /// Its skids by name, as far as it has them.
    skids: FxHashMap<&'static str, Skid>,
    skidding: Option<Skidding>,
    /// Its jumps standing still and on the move, each low and high, as far as it has them.
    leaps: [[Option<Leap>; 2]; 2],
    leaping: Option<Leaping>,
    /// How long it has been in the air, in seconds.
    airborne: f32,
    /// The fastest it has fallen since it left the ground, in meters per second.
    fall: f32,
    /// How fast the skid under way carries the body, in meters per second across the world, as
    /// of the last frame.
    travel: Option<Vector3<f32>>,
    /// Every bone, at rest.
    rest: FxHashMap<Handle<Node>, Bone>,
    skeleton: Skeleton,
    /// The hips at rest, in the model's own terms.
    rest_hips: Bone,
    /// The highest bones the animations move, each with where the bone it hangs off is in the
    /// droid's own terms. They carry any travel built into an animation, and everything else
    /// with them.
    tops: Vec<(Handle<Node>, Bone)>,
    /// The cycle playing, as an index into `cycles`; none is the idle, or the rest pose.
    playing: Option<usize>,
    /// Where the bones were when the change to what is playing now began, and how far through it
    /// is, from 0 to 1.
    from: FxHashMap<Handle<Node>, Bone>,
    fade: f32,
    /// How far the droid is turned from the way the body faces, in radians, left positive.
    heading: f32,
}

/// Which way the droid faces relative to the body, in radians from -PI to PI, left positive, for
/// the keys held: the way they send it, and straight ahead when they send it nowhere.
pub(super) fn heading(forward: bool, back: bool, left: bool, right: bool) -> f32 {
    let along = f32::from(u8::from(forward)) - f32::from(u8::from(back));
    let across = f32::from(u8::from(left)) - f32::from(u8::from(right));
    if along == 0.0 && across == 0.0 {
        0.0
    } else {
        across.atan2(along)
    }
}

/// `angle`, in radians, the short way round: from -PI to PI.
pub(super) fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Which of the cycles, for the gaits in `cycles`, to play: the crouch when `crouched`, and
/// otherwise the gait's own when `moving`. None is the rest pose.
fn choose(cycles: &[Option<Gait>], gait: Gait, crouched: bool, moving: bool) -> Option<usize> {
    if crouched {
        cycles.iter().position(Option::is_none)
    } else if !moving {
        None
    } else {
        standing_cycle(cycles, gait)
    }
}

/// The cycle for `gait` standing up: its own, or failing that the walk, or failing that any.
fn standing_cycle(cycles: &[Option<Gait>], gait: Gait) -> Option<usize> {
    let find = |wanted: Gait| cycles.iter().position(|&c| c == Some(wanted));
    find(gait)
        .or_else(|| find(Gait::Walking))
        .or_else(|| cycles.iter().position(Option::is_some))
}

/// How fast the crouch is played for `gait` in `posture`, as a multiple of how it was made.
fn crouch_rate(posture: Posture, gait: Gait) -> f32 {
    let rates = match posture {
        Posture::Crawling => CRAWLING_RATES,
        _ => CROUCHING_RATES,
    };
    match gait {
        Gait::Walking => rates[0],
        Gait::Running => rates[1],
        Gait::Sprinting => rates[2],
    }
}

/// Where `animation`'s pose puts `node`, as far as the pose says.
fn posed(
    animation: &Animation,
    node: Handle<Node>,
) -> (Option<Vector3<f32>>, Option<UnitQuaternion<f32>>) {
    let (mut position, mut rotation) = (None, None);
    if let Some(pose) = animation.pose().poses().get(&node) {
        for value in &pose.values.values {
            match (&value.binding, &value.value) {
                (ValueBinding::Position, TrackValue::Vector3(v)) => position = Some(*v),
                (ValueBinding::Rotation, TrackValue::UnitQuaternion(q)) => rotation = Some(*q),
                _ => (),
            }
        }
    }
    (position, rotation)
}

/// Puts `animation`'s pose on the bones in `target`, as far as the pose says.
fn take_pose(animation: &Animation, target: &mut FxHashMap<Handle<Node>, Bone>) {
    for (&bone, pose) in target.iter_mut() {
        let (position, rotation) = posed(animation, bone);
        pose.position = position.unwrap_or(pose.position);
        pose.rotation = rotation.unwrap_or(pose.rotation);
    }
}

/// Starts `animation` at its first key rather than at none: a model's frames count from one, and
/// the time before the first is only its pose held still - a hitch every time round a cycle, and
/// a stretch of going nowhere to anything measuring it.
fn trim(animation: &mut Animation) {
    let first = {
        let state = animation.tracks_data().state();
        let Some(data) = state.data_ref() else {
            return;
        };
        data.tracks
            .iter()
            .flat_map(|track| track.data_container().curves_ref())
            .filter_map(|curve| curve.keys().first().map(|key| key.location))
            .fold(f32::INFINITY, f32::min)
    };
    let slice = animation.time_slice();
    if first > slice.start && first < slice.end {
        animation.set_time_slice(first..slice.end);
    }
}

/// How far through the cycle made of `stances`, from 0 to 1, its left foot is down: the middle,
/// going round, of the time it is lower than the right. None if it never is.
fn left_step(stances: &[(f32, Stance)]) -> Option<f32> {
    let (start, end) = (stances.first()?.0, stances.last()?.0);
    if end <= start {
        return None;
    }
    let (sin, cos) = stances
        .iter()
        .filter(|(_, stance)| stance.feet[0].y < stance.feet[1].y)
        .map(|(time, _)| (time - start) / (end - start) * std::f32::consts::TAU)
        .fold((0.0, 0.0), |(sin, cos), angle| (sin + angle.sin(), cos + angle.cos()));
    (sin != 0.0 || cos != 0.0)
        .then(|| sin.atan2(cos).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU)
}

/// How far through a cycle whose left foot is down `to` of the way through, from 0 to 1, has
/// the same foot down as `through` of the way through one whose left foot is down `from`.
fn in_step(through: f32, from: f32, to: f32) -> f32 {
    (through - from + to).rem_euclid(1.0)
}

/// The time in `stances` at which the hips are at their lowest.
fn lowest(stances: &[(f32, Stance)]) -> Option<f32> {
    stances
        .iter()
        .min_by(|a, b| a.1.hips.position.y.total_cmp(&b.1.hips.position.y))
        .map(|&(time, _)| time)
}

impl Avatar {
    /// Puts the droid into `scene` from its `model`, standing on `feet` below `body`'s origin,
    /// facing the way the body does. None if the model is not the droid it is expected to be.
    /// `quiet` keeps what it finds in the model out of the log, for every droid after the first.
    pub(crate) fn spawn(
        model: &ModelResource,
        scene: &mut Scene,
        body: Handle<Node>,
        feet: f32,
        quiet: bool,
    ) -> Option<Self> {
        let root = model.instantiate(scene);
        let avatar = Self::build(&mut scene.graph, root, body, feet, quiet);
        if avatar.is_none() {
            Log::err(format!("Droid: {DROID_MODEL} is missing its rig or its cycles"));
            scene.graph.remove_node(root);
        }
        avatar
    }

    fn build(
        graph: &mut Graph,
        root: Handle<Node>,
        body: Handle<Node>,
        feet: f32,
        quiet: bool,
    ) -> Option<Self> {
        let warn = |text: String| {
            if !quiet {
                Log::warn(text);
            }
        };
        let info = |text: String| {
            if !quiet {
                Log::info(text);
            }
        };
        graph.link_nodes(root, body);
        let transform = graph[root].local_transform_mut();
        transform.set_position(Vector3::new(0.0, feet, 0.0));
        transform.set_scale(Vector3::repeat(SCALE));
        let nodes: Vec<Handle<Node>> = graph.traverse_handle_iter(root).collect();
        for &node in &nodes {
            if graph[node].cast::<Mesh>().is_some() {
                graph[node].set_cast_shadows(false);
            }
        }

        // Everything is measured in the model's own terms: those of its root, which the droid
        // turns and scales as a whole, whatever the rig puts between that and the bones.
        let (hips, _) = graph.find_by_name(root, HIPS)?;
        let [left, right] = FEET.map(|name| graph.find_by_name(root, name).map(|(foot, _)| foot));
        let parent_of = |node: Handle<Node>| graph[node].parent();
        let skeleton = Skeleton {
            hips: chain(parent_of, root, hips),
            feet: [chain(parent_of, root, left?), chain(parent_of, root, right?)],
        };
        let rest = graph
            .traverse_handle_iter(root)
            .filter(|&bone| bone != root)
            .map(|bone| (bone, Bone::of(&graph[bone])))
            .collect::<FxHashMap<_, _>>();
        let rest_hips = place(&skeleton.hips, |bone| rest[&bone]);
        let parents: FxHashMap<Handle<Node>, Handle<Node>> =
            rest.keys().map(|&bone| (bone, graph[bone].parent())).collect();

        let animations = nodes
            .iter()
            .copied()
            .find(|&node| graph[node].cast::<AnimationPlayer>().is_some())?;
        let player = graph
            .try_get_mut_of_type::<AnimationPlayer>(animations)
            .ok()?;
        // Its poses are put on the bones here, blended, rather than by the engine, and its
        // animations are ticked here too: the engine ticks every enabled one each frame, which on
        // top of the ticking here would play them twice as fast.
        player.set_auto_apply(false);
        let container = player.animations_mut().get_value_mut_silent();
        for animation in container.iter_mut() {
            animation.set_enabled(false);
            trim(animation);
        }
        // How fast an animation carries the droid along the ground, and which way, in meters per
        // second at its size in the game; and how far through it the left foot is down.
        let measure = |animation: &mut Animation| {
            let timed = skeleton.stances(animation, &rest);
            let stances: Vec<Stance> = timed.iter().map(|&(_, stance)| stance).collect();
            let pace = pace_of(&stances, ground(&stances)).map(|pace| pace * SCALE);
            (pace, left_step(&timed).unwrap_or(0.0))
        };
        // How fast it carries the droid forward.
        let speed_of = |animation: &mut Animation| measure(animation).0.map(|pace| pace.z);
        let mut cycles = Vec::new();
        for (name, gait) in CYCLES {
            let Some((handle, animation)) = container.find_by_name_mut(name) else {
                warn(format!("Droid: it has no {name}"));
                continue;
            };
            let (pace, phase) = measure(animation);
            let Some(speed) = pace.map(|pace| pace.z).filter(|s| *s > STILL) else {
                warn(format!("Droid: its {name} goes nowhere"));
                continue;
            };
            info(format!("Droid: its {name} goes {speed:.2} m/s"));
            cycles.push(Cycle {
                animation: handle,
                gait,
                speed,
                way: 0.0,
                phase,
            });
        }
        if cycles.is_empty() {
            return None;
        }
        // The rest are only ever played by name.
        let gaited = cycles.len();
        // The highest bones the cycles move: the walk's, or whichever cycle came first.
        let tops: Vec<(Handle<Node>, Bone)> = {
            let animation = &mut container[cycles[0].animation];
            animation.tick(0.0);
            let moves = |bone: Handle<Node>| {
                let (position, rotation) = posed(animation, bone);
                position.is_some() || rotation.is_some()
            };
            rest.keys()
                .copied()
                .filter(|&bone| moves(bone) && !parents.get(&bone).is_some_and(|&p| moves(p)))
                .map(|bone| {
                    let above = chain(|node| parents[&node], root, parents[&bone]);
                    (bone, place(&above, |bone| rest[&bone]))
                })
                .collect()
        };
        // In cover: made or not yet, so there is nothing to warn about without them.
        let cover_walk = container.find_by_name_mut(COVER_WALK).and_then(|(handle, animation)| {
            let (pace, phase) = measure(animation);
            let speed = pace.map(|pace| pace.z).filter(|s| *s > STILL)?;
            info(format!("Droid: its {COVER_WALK} goes {speed:.2} m/s"));
            cycles.push(Cycle {
                animation: handle,
                gait: Some(Gait::Walking),
                speed,
                way: 0.0,
                phase,
            });
            Some(cycles.len() - 1)
        });
        let strafes: [[Option<usize>; 7]; 3] = std::array::from_fn(|row| STRAFES[row].map(|name| {
            let Some((handle, animation)) = container.find_by_name_mut(name) else {
                warn(format!("Droid: it has no {name}"));
                return None;
            };
            let (pace, phase) = measure(animation);
            let Some(pace) = pace.filter(|p| p.norm() > STILL) else {
                warn(format!("Droid: its {name} goes nowhere"));
                return None;
            };
            let way = pace.x.atan2(pace.z);
            info(format!(
                "Droid: its {name} goes {:.2} m/s, {:.0} degrees left of ahead",
                pace.norm(),
                way.to_degrees()
            ));
            cycles.push(Cycle {
                animation: handle,
                gait: STRAFE_GAITS[row],
                speed: pace.norm(),
                way,
                phase,
            });
            Some(cycles.len() - 1)
        }));
        let cover_idle = container.find_by_name_mut(COVER_IDLE).map(|(handle, animation)| {
            animation.set_loop(true);
            handle
        });
        if cover_walk.is_none() || cover_idle.is_none() {
            info(format!(
                "Droid: in cover it walks and idles as usual, without {COVER_WALK} and {COVER_IDLE}"
            ));
        }
        let idle = match container.find_by_name_mut(IDLE) {
            Some((handle, animation)) => {
                animation.set_loop(true);
                Some(handle)
            }
            None => {
                warn(format!("Droid: it has no {IDLE}"));
                None
            }
        };
        let leaps = LEAPS.map(|heights| heights.map(|names| {
            let moves = names.map(|name| {
                let Some((handle, animation)) = container.find_by_name_mut(name) else {
                    warn(format!("Droid: it has no {name}"));
                    return None;
                };
                let speed = speed_of(animation).filter(|s| *s > IN_PLACE);
                Some(Move {
                    animation: handle,
                    speed,
                })
            });
            let [Some(start), Some(flight), Some(land)] = moves else {
                return None;
            };
            // Pushing off and landing are played once through; flying, for as long as it lasts.
            container[start.animation].set_loop(false);
            container[land.animation].set_loop(false);
            let push_off = lowest(&skeleton.stances(&mut container[start.animation], &rest))?;
            Some(Leap {
                start,
                flight,
                land,
                push_off,
            })
        }));
        let motions = motions();
        let mut skids = FxHashMap::default();
        for &name in TURNS.iter().flatten().chain(&CUTS).chain(&STOPS) {
            let Some((handle, animation)) = container.find_by_name_mut(name) else {
                warn(format!("Droid: it has no {name}"));
                continue;
            };
            animation.set_loop(false);
            let Some(clip) = motions.and_then(|motions| motions.clips.get(name)) else {
                warn(format!("Droid: {MOTION} has nothing for its {name}"));
                continue;
            };
            let Some(skid) = Skid::new(handle, clip) else {
                warn(format!("Droid: its {name} does not go in on the move"));
                continue;
            };
            let turn = skid.path.last().map_or(0.0, |at| at.z.to_degrees());
            info(format!(
                "Droid: its {name} goes in at {:.2} m/s and turns {turn:.0} degrees",
                skid.entry * SCALE
            ));
            skids.insert(name, skid);
        }

        Some(Self {
            upright: **graph[root].local_transform().rotation(),
            root,
            animations,
            cycles,
            gaited,
            idle,
            cover_idle,
            cover_walk,
            strafes,
            stepping: None,
            covered: false,
            skids,
            skidding: None,
            leaps,
            leaping: None,
            airborne: 0.0,
            fall: 0.0,
            travel: None,
            rest,
            skeleton,
            rest_hips,
            tops,
            playing: None,
            from: Default::default(),
            fade: 1.0,
            heading: 0.0,
        })
    }

    fn gaits(&self) -> Vec<Option<Gait>> {
        self.cycles[..self.gaited].iter().map(|c| c.gait).collect()
    }

    /// How fast the droid goes at `gait` in `posture`, in meters per second, with its feet
    /// keeping to the floor. None without a cycle to go by.
    pub(crate) fn pace(&self, posture: Posture, gait: Gait) -> Option<f32> {
        let gaits = self.gaits();
        let (index, rate) = match posture {
            Posture::Standing => (standing_cycle(&gaits, gait)?, 1.0),
            _ => (
                gaits.iter().position(Option::is_none)?,
                crouch_rate(posture, gait),
            ),
        };
        Some(self.cycles[index].speed * rate)
    }

    fn container<'a>(&self, graph: &'a mut Graph) -> Option<&'a mut AnimationContainer> {
        let player = graph
            .try_get_mut_of_type::<AnimationPlayer>(self.animations)
            .ok()?;
        Some(player.animations_mut().get_value_mut_silent())
    }

    /// Which way the droid faces from the way the body faces, in radians, left positive.
    pub(crate) fn facing(&self) -> f32 {
        self.heading
    }

    pub(crate) fn is_visible(&self, graph: &Graph) -> bool {
        graph[self.root].global_visibility()
    }

    pub(crate) fn set_visible(&self, graph: &mut Graph, visible: bool) {
        if graph[self.root].visibility() != visible {
            graph[self.root].set_visibility(visible);
        }
    }

    /// How fast the skid under way carries the body, in meters per second across the world, if
    /// one is.
    pub(super) fn travel(&self) -> Option<Vector3<f32>> {
        self.travel
    }

    /// Turns the model's root to face the droid's heading.
    fn face(&self, graph: &mut Graph) {
        graph[self.root].local_transform_mut().set_rotation(
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.heading) * self.upright,
        );
    }

    /// Turns the droid towards facing `heading` from the way the body faces, in radians, left
    /// positive.
    fn turn(&mut self, graph: &mut Graph, heading: f32, dt: f32) {
        let left = wrap(heading - self.heading);
        self.heading = if left.abs() < 1.0e-4 {
            heading
        } else {
            wrap(self.heading + left * (1.0 - (-TURN_RATE * dt).exp()))
        };
        self.face(graph);
    }

    /// Starts a change to something else to play, from wherever the bones are now.
    fn fade_from_here(&mut self, graph: &Graph) {
        self.from = self
            .rest
            .keys()
            .map(|&bone| (bone, Bone::of(&graph[bone])))
            .collect();
        self.fade = 0.0;
    }

    /// Takes any travel built into `animation` back out of `target`: the hips stay where they
    /// rest along `way`, the way it carries the droid like [`Cycle::way`], and everything the
    /// highest bones it moves carry goes with them.
    fn hold_in_place(
        &self,
        animation: &Animation,
        target: &mut FxHashMap<Handle<Node>, Bone>,
        way: f32,
    ) {
        let hips = place(&self.skeleton.hips, |bone| target[&bone]);
        let along = Vector3::new(way.sin(), 0.0, way.cos());
        let drift = along * (hips.position - self.rest_hips.position).dot(&along);
        for (bone, above) in &self.tops {
            if posed(animation, *bone).0.is_some() {
                if let Some(pose) = target.get_mut(bone) {
                    pose.position -= above.rotation.inverse() * drift;
                }
            }
        }
    }

    /// Puts `target` on the bones, as far through the change to it as the fade has got after
    /// another `dt`.
    fn put(&mut self, graph: &mut Graph, target: FxHashMap<Handle<Node>, Bone>, dt: f32) {
        self.fade = (self.fade + dt / FADE_TIME).min(1.0);
        let t = self.fade * self.fade * (3.0 - 2.0 * self.fade);
        for (bone, pose) in target {
            let pose = match self.from.get(&bone) {
                Some(from) if t < 1.0 => from.towards(pose, t),
                _ => pose,
            };
            let transform = graph[bone].local_transform_mut();
            transform.set_position(pose.position);
            transform.set_rotation(pose.rotation);
        }
    }

    /// Poses the droid for what the body is doing, turning it to face the way it is going.
    pub(crate) fn animate(&mut self, graph: &mut Graph, going: Going, dt: f32) {
        self.travel = None;
        if self.skidding.is_some() || self.start_skid(graph, going) {
            if self.skid(graph, going, dt) {
                return;
            }
            self.end_skid(graph);
        }
        let facing = self.facing_for(going);
        self.turn(graph, facing, dt);
        if going.grounded {
            self.airborne = 0.0;
        } else {
            self.airborne += dt;
            self.fall = self.fall.max(going.falling);
        }
        if self.leap(graph, going, dt) {
            return;
        }

        let crouched = going.posture != Posture::Standing;
        let mut wanted = choose(&self.gaits(), going.gait, crouched, going.speed >= STILL);
        // In cover, edging along the wall in place of walking.
        let walking = |i: usize| self.cycles[i].gait == Some(Gait::Walking);
        if going.cover && !crouched && wanted.is_some_and(walking) {
            wanted = self.cover_walk.or(wanted);
        }
        // Strafing, sideways in a strafe, or back in the cycle played backwards.
        let mut backwards = false;
        if going.strafing && wanted.is_some() {
            match self.stepping {
                Some(Step::Strafe(strafe)) => wanted = Some(strafe),
                Some(Step::Back) => backwards = true,
                _ => (),
            }
        }
        // Standing still, going into cover or out of it changes one idle for the other.
        if wanted.is_none() && self.playing.is_none() && going.cover != self.covered {
            self.covered = going.cover;
            self.rewind_idle(graph);
            self.fade_from_here(graph);
        }
        self.covered = going.cover;
        if wanted != self.playing {
            // The new cycle picks up at the same point in the stride, with the same foot down, so
            // the feet carry on.
            if let (Some(old), Some(new)) = (self.playing, wanted) {
                let (old, new) = (&self.cycles[old], &self.cycles[new]);
                let (from, to) = (old.phase, new.phase);
                let (old, new) = (old.animation, new.animation);
                if let Some(container) = self.container(graph) {
                    let old = &container[old];
                    let through = (old.time_position() - old.time_slice().start) / old.length();
                    let new = &mut container[new];
                    let through = in_step(through, from, to);
                    new.set_time_position(new.time_slice().start + through * new.length());
                }
            }
            if wanted.is_none() {
                self.rewind_idle(graph);
            }
            self.playing = wanted;
            self.fade_from_here(graph);
        }

        let mut target = self.rest.clone();
        if let Some(index) = self.playing {
            let cycle = &self.cycles[index];
            let Some(container) = self.container(graph) else {
                return;
            };
            let (animation, way) = (&mut container[cycle.animation], cycle.way);
            // As fast as the floor goes by, so the feet stay on it. In the air, or crouched and
            // still, it is held where it is.
            let rate = if going.grounded {
                going.speed / cycle.speed
            } else {
                0.0
            };
            animation.set_speed(if backwards { -rate } else { rate });
            animation.tick(dt);
            take_pose(animation, &mut target);
            self.hold_in_place(animation, &mut target, way);
        } else if let Some(idle) = self.idle_now() {
            let Some(container) = self.container(graph) else {
                return;
            };
            let animation = &mut container[idle];
            animation.set_speed(1.0);
            animation.tick(dt);
            take_pose(animation, &mut target);
        }
        self.put(graph, target, dt);
    }

    /// Which way the droid is to face from the way the body faces, in radians, left positive: the
    /// way the keys send it, or strafing on the move, ahead but for what is left over by the
    /// step nearest the way it is going. It faces the way the keys send it in the air, where
    /// the jumps leap forward.
    fn facing_for(&mut self, going: Going) -> f32 {
        let heading = going.heading.unwrap_or(0.0);
        if !going.strafing || going.heading.is_none() || self.leaping.is_some() {
            self.stepping = None;
            return heading;
        }
        if going.speed < STILL {
            self.stepping = None;
            return 0.0;
        }
        let steps = self.steps(going);
        self.stepping = nearest(going.way, &steps, self.stepping);
        let way = steps
            .iter()
            .find(|&&(_, step)| Some(step) == self.stepping)
            .map_or(0.0, |&(way, _)| way);
        wrap(going.way - way)
    }

    /// The ways the droid can step strafing, in what it is doing: ahead, every way it has a
    /// strafe for its gait standing up, or crouched, and back in the cycle played backwards if
    /// it has no strafe for that.
    fn steps(&self, going: Going) -> Vec<(f32, Step)> {
        let mut steps = vec![(0.0, Step::Ahead)];
        // Crawling in the crouched ones, and sprinting in the run's; the walk's and the run's
        // each stand in for the other.
        let rows: &[usize] = match (going.posture, going.gait) {
            (Posture::Standing, Gait::Walking) => &[0, 1],
            (Posture::Standing, _) => &[1, 0],
            _ => &[2],
        };
        for side in 0..STRAFES[0].len() {
            if let Some(strafe) = rows.iter().find_map(|&row| self.strafes[row][side]) {
                steps.push((self.cycles[strafe].way, Step::Strafe(strafe)));
            }
        }
        let back = std::f32::consts::PI;
        if !steps.iter().any(|&(way, _)| wrap(way - back).abs() < STRAFE_MARGIN) {
            steps.push((back, Step::Back));
        }
        steps
    }

    /// The idle to play standing still: the one for cover, in cover, if the droid has it.
    fn idle_now(&self) -> Option<Handle<Animation>> {
        if self.covered {
            self.cover_idle.or(self.idle)
        } else {
            self.idle
        }
    }

    /// The idle starts from the top each time the droid comes to a stop.
    fn rewind_idle(&self, graph: &mut Graph) {
        if let Some(idle) = self.idle_now() {
            if let Some(container) = self.container(graph) {
                container[idle].rewind();
            }
        }
    }

    /// The fastest of its gaits the droid is going flat out at, going `speed` along the ground:
    /// sprinting or running. None if it is going slower than either.
    fn flat_out(&self, speed: f32) -> Option<Gait> {
        [Gait::Sprinting, Gait::Running].into_iter().find(|&gait| {
            self.cycles[..self.gaited]
                .iter()
                .find(|cycle| cycle.gait == Some(gait))
                .is_some_and(|cycle| speed >= SKID_SPEED * cycle.speed)
        })
    }

    /// Starts a skid if the droid is going flat out and the keys turn it sharply, or send it
    /// nowhere. Whether it did.
    fn start_skid(&mut self, graph: &mut Graph, going: Going) -> bool {
        let Some(heading) = going.heading else {
            return false;
        };
        if going.posture != Posture::Standing
            || !going.grounded
            || going.strafing
            || going.cover
            || self.leaping.is_some()
        {
            return false;
        }
        let Some(gait) = self.flat_out(going.speed) else {
            return false;
        };
        let Some((kind, names)) = skid_for(gait, self.heading, heading, going.pushing) else {
            return false;
        };
        let Some((name, animation, entry)) = names.into_iter().find_map(|name| {
            let skid = self.skids.get(name)?;
            Some((name, skid.animation, skid.entry))
        }) else {
            return false;
        };
        let Some(container) = self.container(graph) else {
            return false;
        };
        let animation = &mut container[animation];
        animation.set_speed(1.0);
        animation.rewind();
        // Along the way the droid faces, and as far as makes it go in as fast as the body does.
        let frame = graph[self.root].global_transform();
        let scale = going.speed / entry;
        let along = |axis: Vector3<f32>| {
            let way = frame.transform_vector(&axis);
            Vector3::new(way.x, 0.0, way.z).try_normalize(1.0e-6).unwrap_or_default() * scale
        };
        self.skidding = Some(Skidding {
            skid: name,
            kind,
            heading: self.heading,
            forward: along(Vector3::z()),
            left: along(Vector3::x()),
            last: Vector3::zeros(),
        });
        self.stepping = None;
        self.fade_from_here(graph);
        true
    }

    /// Plays the skid under way for another `dt`, carrying the body along its path and swinging
    /// the droid round with it. False once it is over, or cut short.
    fn skid(&mut self, graph: &mut Graph, going: Going, dt: f32) -> bool {
        let Some(mut skidding) = self.skidding else {
            return false;
        };
        if going.heading.is_none()
            || !going.grounded
            || going.jumped
            || going.strafing
            || going.cover
            || going.posture != Posture::Standing
            || (skidding.kind == SkidKind::Stop && going.pushing)
        {
            return false;
        }
        let Some(skid) = self.skids.get(skidding.skid) else {
            return false;
        };
        let Some(container) = self.container(graph) else {
            return false;
        };
        let animation = &mut container[skid.animation];
        if animation.has_ended() {
            return false;
        }
        animation.tick(dt);
        // Made on the spot, so there is no travel to take out.
        let mut target = self.rest.clone();
        take_pose(animation, &mut target);
        let at = skid.at(animation.time_position() - animation.time_slice().start);
        self.heading = wrap(skidding.heading + at.z);
        self.face(graph);
        if dt > 0.0 {
            let moved = at - skidding.last;
            self.travel = Some((skidding.forward * moved.x + skidding.left * moved.y) / dt);
        }
        skidding.last = at;
        self.skidding = Some(skidding);
        self.put(graph, target, dt);
        true
    }

    /// The jump the droid has for going `moving` or not, `high` or low, as `leaps` indexes it:
    /// that one, or failing that the other height, or failing that any at all.
    fn leap_for(&self, moving: usize, high: usize) -> Option<(usize, usize)> {
        [
            (moving, high),
            (moving, 1 - high),
            (1 - moving, high),
            (1 - moving, 1 - high),
        ]
        .into_iter()
        .find(|&(m, h)| self.leaps[m][h].is_some())
    }

    /// The animation for `phase` of `leap`, and for pushing off, the time it pushes off at.
    fn part(leap: Leap, phase: Phase) -> (Move, Option<f32>) {
        match phase {
            Phase::PushingOff => (leap.start, Some(leap.push_off)),
            Phase::Flying => (leap.flight, None),
            Phase::Landing => (leap.land, None),
        }
    }

    /// Starts on `phase` of the jump under way, `through` of the way from its top - or for
    /// pushing off, from the push - to its end.
    fn enter(&mut self, graph: &mut Graph, leaping: Leaping, phase: Phase, through: f32) {
        let Some(leap) = self.leaps[leaping.leap.0][leaping.leap.1] else {
            return;
        };
        let (part, from) = Self::part(leap, phase);
        if let Some(container) = self.container(graph) {
            let animation = &mut container[part.animation];
            animation.set_speed(1.0);
            animation.rewind();
            let slice = animation.time_slice();
            let from = from.unwrap_or(slice.start);
            animation.set_time_position(from + through * (slice.end - from));
        }
        self.leaping = Some(Leaping { phase, ..leaping });
        self.fade_from_here(graph);
    }

    /// Plays whatever part of a jump the droid is in for another `dt`, starting one as it pushes
    /// off or falls, and going from one part to the next. False while it is in none.
    fn leap(&mut self, graph: &mut Graph, going: Going, dt: f32) -> bool {
        let moving = usize::from(going.speed >= LEAP_MOVING);
        if going.jumped {
            // High until it turns out to be low.
            if let Some(leap) = self.leap_for(moving, 1) {
                self.fall = 0.0;
                let leaping = Leaping {
                    leap,
                    phase: Phase::PushingOff,
                    left_ground: false,
                };
                self.enter(graph, leaping, Phase::PushingOff, 0.0);
            }
        } else if self.leaping.is_none_or(|l| l.phase == Phase::Landing)
            && self.airborne > FALLING_AFTER
        {
            if let Some(leap) = self.leap_for(moving, 0) {
                self.fall = going.falling;
                let leaping = Leaping {
                    leap,
                    phase: Phase::Flying,
                    left_ground: true,
                };
                self.enter(graph, leaping, Phase::Flying, 0.0);
            }
        }
        let Some(mut leaping) = self.leaping else {
            return false;
        };
        leaping.left_ground |= !going.grounded;
        self.leaping = Some(leaping);

        // Cut short: the low jump from here, just as far through.
        if going.low && leaping.phase != Phase::Landing && leaping.leap.1 == 1 {
            let low = (leaping.leap.0, 0);
            if self.leaps[low.0][low.1].is_some() {
                let through = self.leaps[leaping.leap.0][1]
                    .and_then(|high| {
                        let (part, from) = Self::part(high, leaping.phase);
                        let container = self.container(graph)?;
                        let animation = &container[part.animation];
                        let slice = animation.time_slice();
                        let from = from.unwrap_or(slice.start);
                        Some((animation.time_position() - from) / (slice.end - from).max(1.0e-3))
                    })
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let low = Leaping { leap: low, ..leaping };
                self.enter(graph, low, leaping.phase, through);
            }
        }

        let leaping = self.leaping.unwrap_or(leaping);
        if leaping.phase != Phase::Landing && leaping.left_ground && going.grounded {
            // Down again: landing standing or on the move as it is going now, and hard or
            // lightly as it came down.
            let high = usize::from(self.fall >= HARD_LANDING);
            self.fall = 0.0;
            let leaping = Leaping {
                leap: self.leap_for(moving, high).unwrap_or(leaping.leap),
                ..leaping
            };
            self.enter(graph, leaping, Phase::Landing, 0.0);
        }
        let Some(leaping) = self.leaping else {
            return false;
        };
        let Some(leap) = self.leaps[leaping.leap.0][leaping.leap.1] else {
            self.leaping = None;
            return false;
        };
        let (part, _) = Self::part(leap, leaping.phase);
        // Landing on the spot and then setting off, or on the move and then stopping, or
        // crouching, the landing is cut short.
        if leaping.phase == Phase::Landing
            && (going.posture != Posture::Standing || moving != leaping.leap.0)
        {
            self.end_leap(graph);
            return false;
        }

        let Some(container) = self.container(graph) else {
            return false;
        };
        let animation = &mut container[part.animation];
        if animation.has_ended() {
            // Pushed off, it flies - unless something overhead kept it on the ground.
            if leaping.phase == Phase::PushingOff && leaping.left_ground {
                self.enter(graph, leaping, Phase::Flying, 0.0);
                return true;
            }
            self.end_leap(graph);
            return false;
        }
        // On the ground, as fast as the floor goes by; in the air, as it was made.
        let rate = match part.speed {
            Some(speed) if going.grounded => (going.speed / speed).max(0.5),
            _ => 1.0,
        };
        animation.set_speed(rate);
        animation.tick(dt);
        let mut target = self.rest.clone();
        take_pose(animation, &mut target);
        self.hold_in_place(animation, &mut target, 0.0);
        self.put(graph, target, dt);
        true
    }

    /// Hands back from the jump under way, if any, to the cycles.
    fn end_leap(&mut self, graph: &mut Graph) {
        if self.leaping.take().is_some() {
            self.playing = None;
            self.rewind_idle(graph);
            self.fade_from_here(graph);
        }
    }

    /// Hands back from the skid under way, if any: from a stop to the idle, which it ends on, and
    /// from a turn to the cycles, as it runs out - whichever goes with how fast the body is
    /// going now.
    fn end_skid(&mut self, graph: &mut Graph) {
        if self.skidding.take().is_none() {
            return;
        }
        self.playing = None;
        self.rewind_idle(graph);
        self.fade_from_here(graph);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Option<Gait>; 4] = [
        Some(Gait::Walking),
        Some(Gait::Running),
        Some(Gait::Sprinting),
        None,
    ];

    #[test]
    fn each_gait_plays_its_own_cycle() {
        assert_eq!(choose(&ALL, Gait::Walking, false, true), Some(0), "walking walks");
        assert_eq!(choose(&ALL, Gait::Running, false, true), Some(1));
        assert_eq!(choose(&ALL, Gait::Sprinting, false, true), Some(2));
    }

    #[test]
    fn crouched_it_crouches_and_still_it_rests() {
        assert_eq!(choose(&ALL, Gait::Sprinting, true, true), Some(3));
        assert_eq!(choose(&ALL, Gait::Walking, true, false), Some(3), "held crouched");
        assert_eq!(choose(&ALL, Gait::Running, false, false), None, "standing at rest");
    }

    #[test]
    fn it_faces_the_way_the_keys_send_it() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        let cases = [
            ((true, false, false, false), 0.0, "W is straight ahead"),
            ((true, false, true, false), FRAC_PI_4, "W+A"),
            ((false, false, true, false), FRAC_PI_2, "A"),
            ((false, true, true, false), 3.0 * FRAC_PI_4, "S+A"),
            ((false, true, false, false), PI, "S"),
            ((false, true, false, true), -3.0 * FRAC_PI_4, "S+D"),
            ((false, false, false, true), -FRAC_PI_2, "D"),
            ((true, false, false, true), -FRAC_PI_4, "W+D"),
            ((false, false, false, false), 0.0, "nothing held"),
            ((true, true, true, true), 0.0, "everything cancels"),
            ((true, true, true, false), FRAC_PI_2, "W and S cancel"),
        ];
        for ((forward, back, left, right), expected, keys) in cases {
            let got = heading(forward, back, left, right);
            assert!((got - expected).abs() < 1e-6, "{keys}: {got}");
        }
    }

    #[test]
    fn it_skids_round_cuts_across_or_stops_and_the_short_way() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        let skid = |gait: Gait, from: f32, to: f32, pushing: bool| {
            skid_for(gait, from, to, pushing).map(|(kind, names)| (kind, names[0]))
        };
        let running = |from: f32, to: f32| skid(Gait::Running, from, to, true);
        let round = |name| Some((SkidKind::Round, name));
        assert_eq!(running(0.0, FRAC_PI_4), None, "an eighth of the way round is only a turn");
        assert_eq!(running(0.0, FRAC_PI_2), Some((SkidKind::Cut, CUTS[0])), "W to A");
        assert_eq!(running(FRAC_PI_4, -FRAC_PI_4), Some((SkidKind::Cut, CUTS[1])), "W+A to W+D");
        assert_eq!(running(0.0, 3.0 * FRAC_PI_4), round(TURNS[0][0]), "W to S+A");
        assert_eq!(running(0.0, -3.0 * FRAC_PI_4), round(TURNS[0][1]), "W to S+D");
        assert!(running(0.0, PI).is_some_and(|(kind, _)| kind == SkidKind::Round), "W to S");
        // Facing back-left, going forward-left is round to the right.
        assert_eq!(running(3.0 * FRAC_PI_4, -FRAC_PI_4 + 0.1), round(TURNS[0][1]));
    }

    #[test]
    fn sprinting_it_skids_its_own_way_and_falls_back_on_the_run() {
        use std::f32::consts::PI;
        let (kind, names) = skid_for(Gait::Sprinting, 0.0, 0.9 * PI, true).unwrap();
        assert_eq!((kind, names), (SkidKind::Round, [TURNS[1][0], TURNS[0][0]]));
        let (_, names) = skid_for(Gait::Running, 0.0, 0.9 * PI, true).unwrap();
        assert_eq!(names, [TURNS[0][0], TURNS[1][0]], "running, the run's first");
    }

    #[test]
    fn letting_go_at_full_speed_stops_whichever_way_it_faced() {
        for to in [0.0, 1.0, 3.0] {
            assert_eq!(
                skid_for(Gait::Sprinting, 0.0, to, false),
                Some((SkidKind::Stop, [STOPS[1], STOPS[0]]))
            );
        }
        assert_eq!(skid_for(Gait::Running, 0.0, 0.0, false).unwrap().1[0], STOPS[0]);
    }

    #[test]
    fn a_skid_goes_along_its_path_between_frames_and_stays_at_the_end() {
        let clip = Clip {
            fps: 10.0,
            forward_m: vec![0.0, 0.2, 0.3],
            left_m: vec![0.0, 0.0, 0.1],
            turn_left_deg: vec![0.0, 45.0, 90.0],
        };
        let skid = Skid::new(Handle::NONE, &clip).unwrap();
        assert!((skid.entry - 2.0).abs() < 1e-5, "0.2 m in the first tenth of a second");
        let halfway = skid.at(0.15);
        assert!((halfway - Vector3::new(0.25, 0.05, 67.5f32.to_radians())).norm() < 1e-5);
        assert_eq!(skid.at(5.0), skid.at(0.2), "stays where the last frame leaves it");
        let on_the_spot = Clip {
            forward_m: vec![0.0, 0.0],
            left_m: vec![0.0, 0.0],
            turn_left_deg: vec![0.0, 0.0],
            ..clip
        };
        assert!(Skid::new(Handle::NONE, &on_the_spot).is_none(), "not going in on the move");
    }

    /// Standing with its hips at `hips` along the way, its left foot at `left` along the way and
    /// `left_up` off the floor, and its right foot lifted.
    fn stance(hips: f32, left: f32, left_up: f32) -> Stance {
        Stance {
            hips: Bone {
                position: Vector3::new(0.0, 1.0, hips),
                rotation: UnitQuaternion::identity(),
            },
            feet: [Vector3::new(0.1, left_up, left), Vector3::new(-0.1, 0.3, 0.0)],
        }
    }

    #[test]
    fn the_hips_going_past_a_planted_foot_is_going_along_the_ground() {
        // Walking on the spot: the foot goes back under still hips.
        let on_the_spot = stride(&stance(0.0, 0.2, 0.0), &stance(0.0, 0.1, 0.0), 0.0);
        assert_eq!(on_the_spot, Some(Vector3::new(0.0, 0.0, 0.1)));
        // Walking through the scene: the hips go on over a foot that stays put.
        let through = stride(&stance(0.0, 0.2, 0.0), &stance(0.1, 0.2, 0.0), 0.0);
        assert_eq!(through, Some(Vector3::new(0.0, 0.0, 0.1)));
        // With the foot off the floor, it says nothing about the ground.
        assert_eq!(stride(&stance(0.0, 0.2, 0.2), &stance(0.0, 0.1, 0.2), 0.0), None);
    }

    #[test]
    fn the_pace_counts_only_the_time_a_foot_is_down() {
        // Half the time on the floor, the foot going back 1 cm a sample; half in the air.
        let stances: Vec<Stance> = (0..20)
            .map(|i| {
                let up = if i < 10 { 0.0 } else { 0.3 };
                stance(0.0, -0.01 * i as f32, up)
            })
            .collect();
        let pace = pace_of(&stances, ground(&stances)).unwrap();
        assert!((pace.z - 0.01 / SAMPLE).abs() < 1e-3, "{pace:?}");
    }

    #[test]
    fn it_turns_the_short_way_round() {
        use std::f32::consts::PI;
        // From back-right to back-left is a quarter turn through straight back, not three
        // quarters through straight ahead.
        let (from, to) = (-3.0 * PI / 4.0, 3.0 * PI / 4.0);
        assert!((wrap(to - from) + PI / 2.0).abs() < 1e-5);
        assert!((wrap(PI + 0.1) - (-PI + 0.1)).abs() < 1e-5);
    }

    #[test]
    fn strafing_it_steps_the_nearest_way_and_keeps_to_it_near_halfway() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        let steps = [(0.0, "ahead"), (PI, "back"), (FRAC_PI_2, "left"), (-FRAC_PI_2, "right")];
        let step = |way: f32, current: Option<&'static str>| nearest(way, &steps, current);
        assert_eq!(step(0.1, None), Some("ahead"));
        assert_eq!(step(-PI + 0.1, None), Some("back"), "round the back");
        assert_eq!(step(FRAC_PI_2 + 0.3, None), Some("left"));
        assert_eq!(step(-1.4, None), Some("right"));
        // Exactly halfway, going ahead or back wins.
        assert_eq!(step(FRAC_PI_4, None), Some("ahead"), "W+A");
        assert_eq!(step(3.0 * FRAC_PI_4, None), Some("back"), "S+A");
        // Just past halfway, it keeps to the step it is on; well past, it changes.
        assert_eq!(step(FRAC_PI_4 + 0.05, Some("ahead")), Some("ahead"));
        assert_eq!(step(FRAC_PI_4 + 0.15, Some("ahead")), Some("left"));
        // Halfway give or take a rounding, still ahead.
        assert_eq!(step(FRAC_PI_4 + 1.0e-5, None), Some("ahead"));
        assert_eq!(nearest::<&str>(0.0, &[], None), None, "nothing to step");
    }

    #[test]
    fn strafing_diagonally_forward_it_steps_the_diagonal() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        // As the model has them: a little further round than they are meant to be.
        let steps = [
            (0.0, "ahead"),
            (PI, "back"),
            (1.62, "left"),
            (-1.62, "right"),
            (0.81, "forward left"),
            (-0.81, "forward right"),
        ];
        let step = |way: f32| nearest(way, &steps, None);
        assert_eq!(step(FRAC_PI_4), Some("forward left"), "W+A");
        assert_eq!(step(-FRAC_PI_4), Some("forward right"), "W+D");
        assert_eq!(step(FRAC_PI_2), Some("left"), "A");
        assert_eq!(step(0.2), Some("ahead"), "nearly straight on");
    }

    #[test]
    fn a_cycle_starting_on_the_other_foot_is_picked_up_half_a_stride_on() {
        // The left foot down, and the right lifted, for the first half; then the other way.
        let stances = |left_first: bool| -> Vec<(f32, Stance)> {
            (0..=40)
                .map(|i| {
                    let first = i < 20;
                    let (left, right) = if first == left_first { (0.0, 0.2) } else { (0.2, 0.0) };
                    let mut stance = stance(0.0, 0.0, left);
                    stance.feet[1].y = right;
                    (i as f32 * 0.05, stance)
                })
                .collect()
        };
        let left = left_step(&stances(true)).unwrap();
        let right = left_step(&stances(false)).unwrap();
        assert!((left - 0.25).abs() < 0.02, "{left}");
        assert!((right - 0.75).abs() < 0.02, "{right}");
        // A tenth of the way into a stride on the left foot is six tenths into one on the right.
        assert!((in_step(0.1, left, right) - 0.6).abs() < 0.02);
        assert!((in_step(0.9, right, right) - 0.9).abs() < 1e-6, "same foot, same place");
        assert!((in_step(0.9, left, right) - 0.4).abs() < 0.02, "round past the end");
    }

    #[test]
    fn a_missing_cycle_falls_back_to_the_walk() {
        let no_sprint = [Some(Gait::Walking), Some(Gait::Running)];
        assert_eq!(choose(&no_sprint, Gait::Sprinting, false, true), Some(0));
        assert_eq!(choose(&no_sprint, Gait::Walking, true, true), None, "no crouch: rest");
    }

    #[test]
    fn the_crouch_goes_faster_for_quicker_gaits_and_slower_on_the_floor() {
        let gaits = [Gait::Walking, Gait::Running, Gait::Sprinting];
        for pair in gaits.windows(2) {
            for posture in [Posture::Crouching, Posture::Crawling] {
                assert!(crouch_rate(posture, pair[1]) > crouch_rate(posture, pair[0]));
            }
        }
        for gait in gaits {
            assert!(crouch_rate(Posture::Crawling, gait) < crouch_rate(Posture::Crouching, gait));
        }
    }
}
