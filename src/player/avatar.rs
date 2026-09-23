//! The droid the player is seen as in third person, and the maze's inhabitants are too: its
//! model, and the cycles it walks, runs, sprints and crouches along with.
//!
//! The droid's feet set the pace. Its cycles were made walking forward through the scene - the
//! bones at the top of the rig travel a stride each time round - and how far they travel over how
//! long a cycle lasts is how fast the droid goes when its feet stay put on the floor. That is what
//! each gait's speed is (see [`Avatar::pace`]), and at any speed along the way, speeding up or
//! slowing down, a cycle is played exactly as fast as the floor goes by under it. The travel
//! itself is taken back out, since the body does the moving.
//!
//! Each gait standing has a cycle of its own: walking, running with Caps Lock, sprinting with
//! Shift. Crouching and crawling share the crouch, played faster for the quicker gaits and slower
//! down on the floor. Standing still, the droid settles into its idle, or its rest pose if it has
//! none; crouched, it holds the crouch where it stopped. Going from one cycle to another, it carries on at the same
//! point in the stride.
//!
//! The droid turns to face the way the keys held send it - ahead, to either side, back, or along
//! any of the diagonals between - turning the short way round; let go, it turns back to face
//! ahead. The camera and the body stay facing straight ahead.
//!
//! Running or sprinting flat out, turning round - from going forward to going back, say - skids:
//! the droid digs its feet in, slides to a stop and swings round, whichever way round is shorter,
//! and sets off the other way. The skid carries the body while it lasts, as far and as fast as
//! its feet slide, and hands back to the cycles once the droid has swung round. Leaving the
//! ground or crouching cuts it short.
//!
//! Jumping, the droid pushes off, flies with its legs tucked and takes the landing in its knees,
//! standing or on the move as it is going at the time: from standing still it springs straight
//! up and lands on the spot, and on the move it leaps in its stride and lands running. Every jump
//! starts as a high one, and turns into a low one if the jump is cut short. The push off starts
//! from its lowest point, since the body leaves the ground the moment the key goes down. Falling
//! off an edge it flies the same way, once it has been in the air long enough to be more than a
//! step down. It lands hard or lightly as it was falling fast or not.
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
/// The droid's skids round, to its left and to its right.
const SKIDS: [&str; 2] = ["droid_skid_turn_L", "droid_skid_turn_R"];
/// How far round the keys have to swing the droid, in radians, for it to skid: from ahead to
/// straight back, or to either of the diagonals behind.
const SKID_ANGLE: f32 = 0.75 * std::f32::consts::PI - 1.0e-3;
/// How near its gait's full pace the droid has to be going to skid, as a share of it.
const SKID_SPEED: f32 = 0.75;
/// How far round a skid swings the droid, in radians, before it hands back to the cycles. What
/// is left of it is setting off the other way, which the cycles do anyway.
const SKID_TURNED: f32 = 150.0 * std::f32::consts::PI / 180.0;
/// How long at the start of a skid, in seconds, its speed going in is measured over.
const SKID_ENTRY: f32 = 0.1;
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
/// How far a jump's animation can shift the droid, in the rig's own meters, and still be one
/// that stays where it is: landing on the spot shuffles the feet a little.
const IN_PLACE: f32 = 0.05;
/// How long the droid is in the air, in seconds, before going off an edge counts as a fall rather
/// than a step down.
const FALLING_AFTER: f32 = 0.2;
/// The droid's idle, played standing still. It stays where it is, so has no travel to take out.
const IDLE: &str = "droid_idle_cycle";
/// How fast the crouch is played for each gait - walking, running, sprinting - crouched, and
/// down on the floor crawling, as a multiple of how it was made. Each is slower than the one
/// above it, as every gait is slower the lower the posture.
const CROUCHING_RATES: [f32; 3] = [1.0, 1.3, 1.6];
const CRAWLING_RATES: [f32; 3] = [0.5, 0.65, 0.8];
/// The bone whose travel over a cycle is the cycle's stride. The bones at the top of the rig all
/// travel together, so any of them would do; the spine is the one the rest hang off.
const ANCHOR: &str = "DEF-spine";
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Skid {
    animation: Handle<Animation>,
    /// How fast the droid is going as the skid begins, in meters per second at the droid's size
    /// in the game.
    speed: f32,
    /// The time in it at which it has swung the droid round.
    turned: f32,
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
    /// Which of the skids, as an index into `skids`.
    skid: usize,
    /// Which way the droid faced as it began, like [`Avatar::heading`].
    heading: f32,
    /// How fast it is played, as a multiple of how it was made.
    rate: f32,
    /// Where it had the anchor, going forward, last frame.
    last: f32,
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
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Avatar {
    root: Handle<Node>,
    /// How the model's root is turned as it comes, before the droid turns it any further.
    upright: UnitQuaternion<f32>,
    animations: Handle<Node>,
    cycles: Vec<Cycle>,
    /// Played standing still, when the droid has one.
    idle: Option<Handle<Animation>>,
    /// Its skids round to the left and to the right, as far as it has them.
    skids: [Option<Skid>; 2],
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
    /// The bones at the top of the rig, which carry the cycles' travel.
    top: Vec<Handle<Node>>,
    /// The node they hang off.
    rig: Handle<Node>,
    anchor: Handle<Node>,
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
fn wrap(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Which way `rotation` turns forward, in radians, left positive.
fn yaw(rotation: UnitQuaternion<f32>) -> f32 {
    let forward = rotation * Vector3::z();
    forward.x.atan2(forward.z)
}

/// Which of the skids, as an index into [`SKIDS`], turns the droid from facing `from` to facing
/// `to`, in radians, left positive: none unless it is turning round.
fn skid_for(from: f32, to: f32) -> Option<usize> {
    let left = wrap(to - from);
    if left.abs() < SKID_ANGLE {
        None
    } else if left > 0.0 {
        Some(0)
    } else {
        Some(1)
    }
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

/// How far `node` travels forward over one time round `animation`, in the rig's own meters.
fn travel(animation: &mut Animation, node: Handle<Node>) -> Option<f32> {
    let slice = animation.time_slice();
    // Not looping for now, so that the end is the end rather than wrapped round to the start.
    animation.set_loop(false);
    let mut z_at = |time: f32| {
        animation.set_time_position(time);
        animation.tick(0.0);
        posed(animation, node).0.map(|p| p.z)
    };
    let distance = z_at(slice.end)
        .zip(z_at(slice.start))
        .map(|(end, start)| end - start);
    animation.set_loop(true);
    animation.rewind();
    distance
}

/// When through `animation` `anchor` is at its lowest.
fn lowest(animation: &mut Animation, anchor: Handle<Node>) -> Option<f32> {
    let slice = animation.time_slice();
    let mut lowest: Option<(f32, f32)> = None;
    let mut time = slice.start;
    while time <= slice.end {
        animation.set_time_position(time);
        animation.tick(0.0);
        if let Some(y) = posed(animation, anchor).0.map(|p| p.y) {
            if lowest.is_none_or(|(_, low)| y < low) {
                lowest = Some((time, y));
            }
        }
        time += 1.0 / 60.0;
    }
    animation.rewind();
    lowest.map(|(time, _)| time)
}

/// How fast `animation`, a skid, has `anchor` going forward as it begins, in the rig's own meters
/// per second, and the time in it at which it has turned the anchor round from its `rest`.
fn measure_skid(
    animation: &mut Animation,
    anchor: Handle<Node>,
    rest: UnitQuaternion<f32>,
) -> Option<(f32, f32)> {
    let slice = animation.time_slice();
    animation.set_loop(false);
    animation.set_speed(1.0);
    let mut at = |time: f32| {
        animation.set_time_position(time);
        animation.tick(0.0);
        posed(animation, anchor)
    };
    let speed = at(slice.start + SKID_ENTRY)
        .0
        .zip(at(slice.start).0)
        .map(|(early, start)| (early.z - start.z) / SKID_ENTRY);
    let turned = (0..)
        .map(|frame| slice.start + frame as f32 / 60.0)
        .take_while(|&time| time <= slice.end)
        .find(|&time| at(time).1.is_some_and(|r| yaw(r * rest.inverse()).abs() >= SKID_TURNED));
    animation.rewind();
    speed.filter(|s| *s > 1.0e-3).zip(turned)
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

        let (anchor, _) = graph.find_by_name(root, ANCHOR)?;
        let rig = graph[anchor].parent();
        let top = graph[rig].children().to_vec();
        let rest = graph
            .traverse_handle_iter(rig)
            .filter(|&bone| bone != rig)
            .map(|bone| (bone, Bone::of(&graph[bone])))
            .collect::<FxHashMap<_, _>>();

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
        }
        let mut cycles = Vec::new();
        for (name, gait) in CYCLES {
            let Some((handle, animation)) = container.find_by_name_mut(name) else {
                warn(format!("Droid: it has no {name}"));
                continue;
            };
            let length = animation.length();
            let Some(distance) = travel(animation, anchor).filter(|d| *d > 1.0e-3) else {
                warn(format!("Droid: its {name} goes nowhere"));
                continue;
            };
            let speed = distance * SCALE / length;
            info(format!("Droid: its {name} goes {speed:.2} m/s"));
            cycles.push(Cycle {
                animation: handle,
                gait,
                speed,
            });
        }
        if cycles.is_empty() {
            return None;
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
                let length = animation.length();
                let speed = travel(animation, anchor)
                    .filter(|d| *d > IN_PLACE)
                    .map(|distance| distance * SCALE / length);
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
            let push_off = lowest(&mut container[start.animation], anchor)?;
            Some(Leap {
                start,
                flight,
                land,
                push_off,
            })
        }));
        let skids = SKIDS.map(|name| {
            let Some((handle, animation)) = container.find_by_name_mut(name) else {
                warn(format!("Droid: it has no {name}"));
                return None;
            };
            let Some((speed, turned)) = measure_skid(animation, anchor, rest[&anchor].rotation)
            else {
                warn(format!("Droid: its {name} never skids round"));
                return None;
            };
            let speed = speed * SCALE;
            info(format!("Droid: its {name} goes into a skid at {speed:.2} m/s"));
            Some(Skid {
                animation: handle,
                speed,
                turned,
            })
        });

        Some(Self {
            upright: **graph[root].local_transform().rotation(),
            root,
            animations,
            cycles,
            idle,
            skids,
            skidding: None,
            leaps,
            leaping: None,
            airborne: 0.0,
            fall: 0.0,
            travel: None,
            rest,
            top,
            rig,
            anchor,
            playing: None,
            from: Default::default(),
            fade: 1.0,
            heading: 0.0,
        })
    }

    fn gaits(&self) -> Vec<Option<Gait>> {
        self.cycles.iter().map(|c| c.gait).collect()
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

    /// Takes `animation`'s travel back out of `target`: the anchor stays where it rests, going
    /// forward, and the rest of the top of the rig goes with it - the bones `animation` moves,
    /// that is. The skinned mesh hangs off the rig too, and is left where it is.
    fn hold_in_place(&self, animation: &Animation, target: &mut FxHashMap<Handle<Node>, Bone>) {
        let drift = target[&self.anchor].position.z - self.rest[&self.anchor].position.z;
        for bone in &self.top {
            if posed(animation, *bone).0.is_some() {
                if let Some(pose) = target.get_mut(bone) {
                    pose.position.z -= drift;
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
        self.turn(graph, going.heading.unwrap_or(0.0), dt);
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
        let wanted = choose(&self.gaits(), going.gait, crouched, going.speed >= STILL);
        if wanted != self.playing {
            // The new cycle picks up at the same point in the stride, so the feet carry on.
            if let (Some(old), Some(new)) = (self.playing, wanted) {
                let (old, new) = (self.cycles[old].animation, self.cycles[new].animation);
                if let Some(container) = self.container(graph) {
                    let old = &container[old];
                    let through = (old.time_position() - old.time_slice().start) / old.length();
                    let new = &mut container[new];
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
            let animation = &mut container[cycle.animation];
            // As fast as the floor goes by, so the feet stay on it. In the air, or crouched and
            // still, it is held where it is.
            let rate = if going.grounded {
                going.speed / cycle.speed
            } else {
                0.0
            };
            animation.set_speed(rate);
            animation.tick(dt);
            take_pose(animation, &mut target);
            self.hold_in_place(animation, &mut target);
        } else if let Some(idle) = self.idle {
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

    /// The idle starts from the top each time the droid comes to a stop.
    fn rewind_idle(&self, graph: &mut Graph) {
        if let Some(idle) = self.idle {
            if let Some(container) = self.container(graph) {
                container[idle].rewind();
            }
        }
    }

    /// Starts a skid if the droid is going flat out and the keys turn it round. Whether it did.
    fn start_skid(&mut self, graph: &mut Graph, going: Going) -> bool {
        let Some(heading) = going.heading else {
            return false;
        };
        if going.posture != Posture::Standing || !going.grounded || going.gait == Gait::Walking {
            return false;
        }
        let pace = self.pace(Posture::Standing, going.gait).unwrap_or(f32::INFINITY);
        if going.speed < SKID_SPEED * pace {
            return false;
        }
        let Some(index) = skid_for(self.heading, heading) else {
            return false;
        };
        let Some(skid) = self.skids[index] else {
            return false;
        };
        let Some(container) = self.container(graph) else {
            return false;
        };
        let animation = &mut container[skid.animation];
        animation.set_speed(1.0);
        animation.rewind();
        animation.tick(0.0);
        let Some(last) = posed(animation, self.anchor).0.map(|p| p.z) else {
            return false;
        };
        self.skidding = Some(Skidding {
            skid: index,
            heading: self.heading,
            // Going into it as fast as the body is going, so the feet carry on.
            rate: going.speed / skid.speed,
            last,
        });
        self.fade_from_here(graph);
        true
    }

    /// Plays the skid under way for another `dt`, facing the way the droid faced as it began,
    /// since the skid turns it round itself. False once it is over.
    fn skid(&mut self, graph: &mut Graph, going: Going, dt: f32) -> bool {
        let Some(mut skidding) = self.skidding else {
            return false;
        };
        if going.heading.is_none()
            || !going.grounded
            || going.jumped
            || going.posture != Posture::Standing
        {
            return false;
        }
        let Some(skid) = self.skids[skidding.skid] else {
            return false;
        };
        let Some(container) = self.container(graph) else {
            return false;
        };
        let animation = &mut container[skid.animation];
        if animation.time_position() >= skid.turned {
            return false;
        }
        animation.set_speed(skidding.rate);
        animation.tick(dt);
        let mut target = self.rest.clone();
        take_pose(animation, &mut target);
        let along = target[&self.anchor].position.z;
        self.hold_in_place(animation, &mut target);
        if dt > 0.0 {
            // The rig's forward, in the world, is as long as one of its meters there.
            let forward = graph[self.rig]
                .global_transform()
                .transform_vector(&Vector3::z());
            self.travel = Some(forward * ((along - skidding.last) / dt));
        }
        skidding.last = along;
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
        self.hold_in_place(animation, &mut target);
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

    /// Hands back from the skid under way, if any, to the cycles. The skid has turned the bones
    /// round, not the droid: the droid is turned as far instead, and the bones back again by as
    /// much, so that it stands just as it did.
    fn end_skid(&mut self, graph: &mut Graph) {
        let Some(skidding) = self.skidding.take() else {
            return;
        };
        let turned = yaw(
            **graph[self.anchor].local_transform().rotation()
                * self.rest[&self.anchor].rotation.inverse(),
        );
        let back = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), -turned);
        let animation = self.skids[skidding.skid].map(|skid| skid.animation);
        let moved: Vec<Handle<Node>> = match (animation, self.container(graph)) {
            (Some(animation), Some(container)) => self
                .top
                .iter()
                .copied()
                .filter(|&bone| posed(&container[animation], bone).1.is_some())
                .collect(),
            _ => Vec::new(),
        };
        for bone in moved {
            let pose = Bone::of(&graph[bone]);
            let transform = graph[bone].local_transform_mut();
            transform.set_position(back * pose.position);
            transform.set_rotation(back * pose.rotation);
        }
        self.heading = wrap(skidding.heading + turned);
        self.face(graph);
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
    fn it_skids_only_to_turn_round_and_the_short_way() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        let skid = |from: f32, to: f32| skid_for(from, to).map(|i| SKIDS[i]);
        let (left, right) = (Some(SKIDS[0]), Some(SKIDS[1]));
        assert_eq!(skid(0.0, FRAC_PI_2), None, "a quarter turn is only a turn");
        assert_eq!(skid(0.0, 3.0 * FRAC_PI_4), left, "W to S+A");
        assert_eq!(skid(0.0, -3.0 * FRAC_PI_4), right, "W to S+D");
        assert!(skid(0.0, PI).is_some(), "W to S, one way or the other");
        assert!(skid(FRAC_PI_4, -3.0 * FRAC_PI_4).is_some(), "W+A to S+D is straight back too");
        // Facing back-left, going forward-left is round to the right.
        assert_eq!(skid(3.0 * FRAC_PI_4, FRAC_PI_4 - 0.2), None);
        assert_eq!(skid(3.0 * FRAC_PI_4, -FRAC_PI_4 + 0.1), right);
    }

    #[test]
    fn yaw_is_left_positive() {
        let turn = |angle: f32| UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle);
        assert!((yaw(turn(0.5)) - 0.5).abs() < 1e-5);
        assert!((yaw(turn(-2.5)) + 2.5).abs() < 1e-5);
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
