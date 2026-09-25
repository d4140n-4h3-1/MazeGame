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
//! Strafing, with the right mouse button held or the pistol drawn, it keeps facing ahead instead,
//! its face and shoulders square to straight ahead however its hips turn, and steps whichever
//! way the body goes: forward, or sideways, back and along each diagonal in its strafes, walking,
//! running - the player's sprint slows to a run strafing - or crouched, crawling too. Whichever of those is nearest the way it is
//! going, it turns only by what is left over, a few degrees. Without a strafe back it plays its
//! cycle backwards, and without the others it turns as far as it has to, up to side on.
//!
//! Sprinting flat out, and only then, the droid skids; running, it just turns and slows down.
//! Turning round - from going forward to going back, say - it digs its feet in, slides, swings
//! round whichever way is shorter and runs out the other way through the run. Turning a quarter
//! of the way round, it cuts across. Letting go of the keys, it slides to a standstill and idles.
//! Whether it is sprinting goes by how fast it is going rather than the keys, so letting go of
//! Shift with the rest still stops out of the sprint.
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
//! It draws a pistol, holds it at the ready, raises it to aim and fires it, lowers it again a
//! moment after the last shot unless it is held raised, and holsters it, with its upper body - from the
//! middle of its back up - while its legs go on walking, running, strafing or standing as ever.
//! The pistol shows partway through the draw and goes again partway through the holster, as
//! [`MOTION`] says, and until then is out of sight in its right hand. Out, it follows the
//! camera: its upper body leans the pistol up, down and round towards wherever the camera looks,
//! as far as the model's aims go, by the aims [`MOTION`] lays out, blended and added on top of
//! whatever the pistol is doing. A shot leaves the muzzle
//! on the frame the model says, the way the barrel pointed as the trigger was pulled - before
//! the recoil kicks it up - for the body to send on its way: see [`Avatar::shot`]. The screen
//! on top of the pistol, see-through green glass, flashes with each pull of the trigger. The
//! ball at the muzzle the shots come from - a glowing core in a see-through green shell - shows
//! all the while the pistol is out, the core and the shell tumbling every way round, each the
//! opposite way to the other.
//!
//! Its meshes cast no shadows. The traced shadows are gathered once, when meshes are added or
//! removed, so a droid in them would leave its shadow behind where it was first put down.

use super::posture::{Gait, Posture};
use crate::fixtures::{glow_strength, property, DIFFUSE_COLOR, EMISSION_STRENGTH};
use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
    },
    fxhash::FxHashMap,
    generic_animation::value::{TrackValue, ValueBinding},
    graph::SceneGraph,
    material::{MaterialProperty, MaterialResource},
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        animation::{Animation, AnimationContainer, AnimationPlayer},
        graph::Graph,
        mesh::Mesh,
        node::Node,
        Scene,
    },
};
use fyrox_gfx::{replace_materials, GlassMaterial};

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
/// The droid's skids round out of a sprint, to its left and to its right.
const TURNS: [&str; 2] = ["droid_skid_sprint_turn_L", "droid_skid_sprint_turn_R"];
/// Its quarter turns cut across out of a sprint, to its left and to its right.
const CUTS: [&str; 2] = ["droid_skid_sprint_turn90_L", "droid_skid_sprint_turn90_R"];
/// Its slide to a standstill out of a sprint.
const STOP: &str = "droid_skid_sprint_stop";
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
/// The pistol in its right hand, which everything on it hangs off, as long as [`MOTION`] does not
/// say otherwise.
const PISTOL: &str = "pistol";
/// The bone its upper body hangs off, which the pistol has from the moment it is drawn: the
/// middle of its back, with the chest, the arms and the head above it.
const UPPER_BODY: &str = "DEF-spine.002";
/// Its head, and the tops of its arms, which its face and its shoulders go by.
const HEAD: &str = "DEF-spine.006";
const SHOULDERS: [&str; 2] = ["DEF-upper_arm.L", "DEF-upper_arm.R"];
/// How far above the head bone, in the model's own units up the bone, the middle of the face is:
/// the bone sits at the bottom of the head, 0.22 below the top of the model.
const FACE_ABOVE_HEAD: f32 = 0.11;
/// How long the face and shoulders take to square up to straight ahead as the droid starts
/// strafing, or to let go as it stops, in seconds.
const SQUARE_FADE: f32 = 0.2;
/// The screen on top of the pistol, crosshair and all, and the shell round the glowing core of
/// the ball at its muzzle that the shots come out of.
const PISTOL_SCREEN: &str = "gun_hud";
const PISTOL_SHELL: &str = "projectile_source";
const PISTOL_CORE: &str = "projectile_source.001";
/// How fast the shell tumbles about each of its own axes - across, up and ahead - in radians a
/// second. The core tumbles just as fast the opposite way round: turned back each moment by as
/// much as the shell is turned on.
const SPIN: Vector3<f32> = Vector3::new(1.3, 2.1, 0.8);
/// How quickly a flash of the screen dies down after the trigger is pulled, like a rate: after
/// 1/`FLASH_FADE` seconds it is down to about a third; and how far down, from 1 to 0, it goes
/// dark again.
const FLASH_FADE: f32 = 8.0;
const FLASH_DARK: f32 = 0.3;
/// The glass of the screen and of the shell: green, and see-through, glowing a little by itself.
const GLASS_GREEN: Color = Color::opaque(40, 255, 60);
const SCREEN_TINT: f32 = 0.6;
const SCREEN_GLOW: f32 = 1.5;
const SHELL_TINT: f32 = 0.4;
const SHELL_GLOW: f32 = 0.5;
/// The droid's eyes, which glow the colour of its mood while it talks, and their own otherwise.
const EYES: &str = "eyes";

/// The glowing surfaces of the droid's eyes: a material the droid has to itself, since the
/// model's is shared with every other droid, with the colour and the glow it came with.
#[derive(Debug, Clone, PartialEq)]
struct Eyes {
    material: MaterialResource,
    colour: MaterialProperty,
    glow: MaterialProperty,
}

/// Gives the droid its own copy of the glowing material of the eyes under `eyes`, if they glow.
fn claim_eyes(graph: &mut Graph, eyes: Handle<Node>) -> Option<Eyes> {
    let mesh = graph[eyes].cast_mut::<Mesh>()?;
    let (key, eyes) = mesh.surfaces().iter().find_map(|surface| {
        let original = surface.material();
        let state = original.state();
        let material = state.data_ref()?;
        let glow = glow_strength(material)?;
        let colour = property(material, DIFFUSE_COLOR)?;
        let copy = MaterialResource::new_embedded(material.clone());
        Some((original.key(), Eyes { material: copy, colour, glow }))
    })?;
    for surface in mesh.surfaces_mut() {
        if surface.material().key() == key {
            surface.set_material(eyes.material.clone());
        }
    }
    Some(eyes)
}

/// How far round something tumbling at [`SPIN`] has gone after `time` seconds.
fn tumbled(time: f32) -> UnitQuaternion<f32> {
    let angle = SPIN * time;
    UnitQuaternion::from_euler_angles(angle.x, angle.y, angle.z)
}

/// See-through green glass, tinting what is behind it `tint` of the way to green and glowing
/// `glow` brightly. Nothing seen through it is bent: a screen and a shell, not lenses.
fn green_glass(tint: f32, glow: f32) -> fyrox::material::MaterialResource {
    GlassMaterial {
        tint: GLASS_GREEN,
        tint_strength: tint,
        index_of_refraction: 1.0,
        distortion: 0.0,
        waviness: 0.0,
        emission: GLASS_GREEN,
        emission_strength: glow,
        ..Default::default()
    }
    .build_resource()
}
/// How quickly the turn that brings the barrel round to the camera catches up with how far it
/// has to, like a rate: after 1/`AIM_FIX_RATE` seconds, about two thirds of the way.
const AIM_FIX_RATE: f32 = 15.0;
/// How long after the last shot the pistol is lowered to the ready again, unless it is held
/// raised, in seconds; and how long going from one of its animations to the next takes.
const READY_AFTER: f32 = 1.0;
const ARMS_BLEND: f32 = 0.12;
/// How long the upper body takes to go over to the pistol, or back, in seconds.
const ARMS_FADE: f32 = 0.15;
/// The rate the droid's animations were made at, in frames a second.
const FRAME_RATE: f32 = 24.0;
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
    #[serde(default)]
    pistol: Option<PistolMotion>,
    #[serde(default)]
    aim: Option<AimMotion>,
}

/// The pistol's aims as [`MOTION`] has them: a held upper-body pose for each pitch and yaw on a
/// grid, in degrees - up, and to the droid's left, positive - each pointing the barrel exactly
/// that way; the pose they are all offsets from, the first frame of `reference_clip`; and the
/// bones they move.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct AimMotion {
    reference_clip: String,
    pitches: Vec<f32>,
    yaws: Vec<f32>,
    /// By `"pitch,yaw"`.
    clips: std::collections::HashMap<String, String>,
    bones: Vec<String>,
}

/// The pistol's aims, ready to blend.
#[derive(Debug, Clone, PartialEq)]
struct Aims {
    /// The grid's pitches and yaws, in radians, each from the lowest up.
    pitches: Vec<f32>,
    yaws: Vec<f32>,
    /// How far each aim moves each of its bones from the reference pose, in the bone's parent's
    /// terms, by pitch and then yaw: how far on it is, and how far round.
    offsets: Vec<Vec<FxHashMap<Handle<Node>, Bone>>>,
}

/// Where `value` falls among `grid`, from the lowest up: the two either side of it, and how far
/// from the first to the second it is, from 0 to 1. Beyond either end, the end.
fn between(grid: &[f32], value: f32) -> (usize, usize, f32) {
    let last = grid.len().saturating_sub(1);
    let above = grid.iter().position(|&g| g >= value).unwrap_or(last);
    let below = above.saturating_sub(1);
    let span = grid[above] - grid[below];
    let t = if span > 0.0 {
        ((value - grid[below]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (below, above, t)
}

impl Aims {
    /// How far the aims move each bone for aiming `pitch` up and `yaw` to the left, in radians:
    /// the four aims round it blended, pitch and yaw each held to the grid.
    fn offset(&self, pitch: f32, yaw: f32) -> FxHashMap<Handle<Node>, Bone> {
        let (p0, p1, tp) = between(&self.pitches, pitch);
        let (y0, y1, ty) = between(&self.yaws, yaw);
        let at = |p: usize, y: usize| &self.offsets[p][y];
        at(p0, y0)
            .iter()
            .map(|(&bone, &low_right)| {
                let blend = |p: usize, y0: usize, y1: usize, first: Bone| {
                    let second = at(p, y1).get(&bone).copied().unwrap_or(first);
                    let row_first = at(p, y0).get(&bone).copied().unwrap_or(first);
                    row_first.towards(second, ty)
                };
                let low = blend(p0, y0, y1, low_right);
                let high = blend(p1, y0, y1, low_right);
                (bone, low.towards(high, tp))
            })
            .collect()
    }
}

/// The droid's pistol as [`MOTION`] has it: its nodes, its animations, and the frames in them,
/// counted from one, at which it shows, goes, and fires.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct PistolMotion {
    node: String,
    muzzle_node: String,
    clips: PistolClips,
    show_frame: u32,
    hide_after_frame: u32,
    shot_frame: u32,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct PistolClips {
    draw: String,
    aim: String,
    fire: String,
    holster: String,
    /// Holding it lower at the ready, and going from there to aiming and back; without them, it
    /// aims all the while it is out.
    #[serde(default)]
    ready: Option<String>,
    #[serde(default)]
    ready_to_aim: Option<String>,
    #[serde(default)]
    aim_to_ready: Option<String>,
}

/// How far into an animation frame `frame` is, counted from one, in seconds.
fn frame_time(frame: u32) -> f32 {
    frame.saturating_sub(1) as f32 / FRAME_RATE
}

/// The droid's pistol.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pistol {
    node: Handle<Node>,
    muzzle: Handle<Node>,
    draw: Handle<Animation>,
    aim: Handle<Animation>,
    fire: Handle<Animation>,
    holster: Handle<Animation>,
    /// Held at the ready, raised from there to aim, and lowered back, if it has them.
    ready: Option<Ready>,
    /// How far into the draw it shows, into the holster it goes, and into firing the shot
    /// leaves, in seconds.
    show_at: f32,
    hide_after: f32,
    shot_at: f32,
}

/// The pistol's animations for holding it at the ready.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ready {
    ready: Handle<Animation>,
    raise: Handle<Animation>,
    lower: Handle<Animation>,
}

/// What the droid is doing with its pistol.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Arms {
    #[default]
    Holstered,
    Drawing,
    /// Held lower, at the ready.
    Ready,
    /// From the ready up to aiming, and back down.
    Raising,
    Lowering,
    Aiming,
    /// Whether the shot has left yet.
    Firing(bool),
    Holstering,
}

/// What the player wants of the pistol, and how things stand with it, for [`next_arms`].
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Wants {
    /// The pistol out.
    out: bool,
    /// It raised to aim: the right mouse button held.
    raised: bool,
    /// A shot: the trigger pulled, now or while it was being raised.
    trigger: bool,
    /// Whether what it is playing has ended.
    ended: bool,
    /// Whether it has been long enough since the last shot to lower it.
    rested: bool,
    /// Whether it has a ready to lower it to.
    can_ready: bool,
}

/// The pistol's screen, which flashes as the trigger is pulled.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flash {
    screen: Handle<Node>,
    /// How long since the trigger was last pulled, in seconds.
    since: f32,
}

/// How bright a flash is `since` seconds after it went off, from 1 down to 0.
fn flash_glow(since: f32) -> f32 {
    (-FLASH_FADE * since.max(0.0)).exp()
}

/// The bones the droid squares its face and shoulders by, strafing, each from the top down.
#[derive(Debug, Clone, PartialEq)]
struct Square {
    /// Down to the bone the upper body hangs off, which is turned to square the shoulders.
    upper: Vec<Handle<Node>>,
    /// Down to the head, which is turned to square the face.
    head: Vec<Handle<Node>>,
    /// Down to the tops of the arms, left and right.
    shoulders: [Vec<Handle<Node>>; 2],
    /// Which way the head faces, in its own terms: ahead, at rest.
    face: Vector3<f32>,
}

/// Which way shoulders at `left` and `right` face, in radians from ahead, left positive: straight
/// ahead when the line from one to the other runs straight across.
fn shoulders_yaw(left: Vector3<f32>, right: Vector3<f32>) -> f32 {
    let across = left - right;
    (-across.z).atan2(across.x)
}

/// Which way `way` points along the ground, in radians from ahead, left positive.
fn yaw_of(way: Vector3<f32>) -> f32 {
    way.x.atan2(way.z)
}

/// Turns the last bone of `chain` in `target` by `angle` about the droid's up, in its own terms,
/// left positive, and with it everything that hangs off it.
fn turn_bone(target: &mut FxHashMap<Handle<Node>, Bone>, chain: &[Handle<Node>], angle: f32) {
    rotate_bone(target, chain, UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle));
}

/// Turns the last bone of `chain` in `target` by `turn`, in the droid's own terms, and with it
/// everything that hangs off it.
fn rotate_bone(
    target: &mut FxHashMap<Handle<Node>, Bone>,
    chain: &[Handle<Node>],
    turn: UnitQuaternion<f32>,
) {
    let Some((&bone, above)) = chain.split_last() else {
        return;
    };
    let parent = place(above, |bone| target[&bone]).rotation;
    let own = place(chain, |bone| target[&bone]).rotation;
    if let Some(pose) = target.get_mut(&bone) {
        pose.rotation = parent.inverse() * turn * own;
    }
}

/// The way `pitch` up and `yaw` to the left, in radians, points: one meter long, in the droid's
/// own terms.
fn pointing(pitch: f32, yaw: f32) -> Vector3<f32> {
    Vector3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos())
}

/// What the droid does with its pistol next, doing `arms` with things as `wants` has them. None
/// to go on. Drawn, it is held at the ready unless it is raised to aim or fired; the draw, the
/// shots and the holster all go through aiming.
fn next_arms(arms: Arms, wants: Wants) -> Option<Arms> {
    let rise = wants.raised || wants.trigger;
    match arms {
        Arms::Holstered => wants.out.then_some(Arms::Drawing),
        Arms::Holstering if wants.out => Some(Arms::Drawing),
        Arms::Holstering => wants.ended.then_some(Arms::Holstered),
        _ if !wants.out => Some(Arms::Holstering),
        Arms::Drawing if wants.ended && (wants.raised || !wants.can_ready) => Some(Arms::Aiming),
        Arms::Drawing => wants.ended.then_some(Arms::Lowering),
        Arms::Ready | Arms::Lowering if rise => Some(Arms::Raising),
        Arms::Ready => None,
        Arms::Lowering => wants.ended.then_some(Arms::Ready),
        Arms::Raising => wants.ended.then_some(Arms::Aiming),
        // Once the shot has left, the trigger can go again.
        Arms::Aiming | Arms::Firing(true) if wants.trigger => Some(Arms::Firing(false)),
        Arms::Firing(_) => wants.ended.then_some(Arms::Aiming),
        Arms::Aiming if !wants.raised && wants.rested && wants.can_ready => Some(Arms::Lowering),
        Arms::Aiming => None,
    }
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

/// The skid, by name, the droid goes into flat out at `gait`, from facing `from` towards `to`, in
/// radians, left positive, with the keys sending it anywhere or not. None for going on as it is,
/// as it always does short of a sprint.
fn skid_for(gait: Gait, from: f32, to: f32, pushing: bool) -> Option<(SkidKind, &'static str)> {
    if gait != Gait::Sprinting {
        return None;
    }
    let left = wrap(to - from);
    let side = usize::from(left < 0.0);
    if !pushing {
        Some((SkidKind::Stop, STOP))
    } else if left.abs() >= SKID_ANGLE {
        Some((SkidKind::Round, TURNS[side]))
    } else if left.abs() >= CUT_ANGLE {
        Some((SkidKind::Cut, CUTS[side]))
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
    /// Whether it wants its pistol out.
    pub(crate) armed: bool,
    /// Whether the trigger was pulled this frame.
    pub(crate) trigger: bool,
    /// Whether it wants the pistol raised to aim, rather than held at the ready.
    pub(crate) raised: bool,
    /// Which way the camera looks, in radians from the body's ahead: up, and to the left.
    pub(crate) look: (f32, f32),
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
    /// Its pistol, if it has one it can draw.
    pistol: Option<Pistol>,
    arms: Arms,
    /// How far over to the pistol the upper body is, from 0 to 1.
    arms_weight: f32,
    /// Where the pistol had the upper body as it went from one of its animations to the next,
    /// and how far through going over it is, from 0 to 1.
    arms_from: FxHashMap<Handle<Node>, Bone>,
    arms_blend: f32,
    /// Whether a shot is waiting for the pistol to be raised; and how long since the last one,
    /// in seconds.
    queued: bool,
    since_shot: f32,
    /// Where the pistol has every bone of the upper body, as of this frame.
    arms_pose: FxHashMap<Handle<Node>, Bone>,
    /// Which way the barrel pointed, across the world, as the trigger was last pulled.
    barrel: Vector3<f32>,
    /// Where the muzzle was as a shot left it this frame, and which way it went, if one did.
    shot: Option<(Vector3<f32>, Vector3<f32>)>,
    /// Its pistol's screen, which flashes as the trigger is pulled, if it has a pistol.
    flash: Option<Flash>,
    /// Its eyes, if they glow.
    eyes: Option<Eyes>,
    /// The shell and the core of the ball at the muzzle, each with whether it tumbles the
    /// opposite way round to how [`SPIN`] has it; and how long they have been tumbling, in
    /// seconds.
    spinning: Vec<(Handle<Node>, bool)>,
    spun: f32,
    /// The pistol's aims, if the droid has them, and where it is aiming them as of this frame, like
    /// [`Going::look`].
    aims: Option<Aims>,
    look: (f32, f32),
    /// The bones its face and shoulders go by, if it has them.
    square: Option<Square>,
    /// Down to the pistol's muzzle, along whose +X the barrel runs.
    barrel_chain: Vec<Handle<Node>>,
    /// How far the upper body is turned on top of the aims to bring the barrel round to where
    /// the camera looks, in the droid's own terms: measured while aiming, and held while
    /// drawing, firing or holstering, so that those move the barrel as they were made to.
    aim_fix: UnitQuaternion<f32>,
    /// The pose the bones were last given before the face, the shoulders, the aim and the
    /// ball's tumble were added: what a change to something else to play fades from, so that
    /// none of those is added twice over while it does.
    blended: FxHashMap<Handle<Node>, Bone>,
    /// Whether it is strafing, and so squaring its face and shoulders to straight ahead, as of
    /// this frame; and how far it has, from 0 to 1.
    squaring: bool,
    square_weight: f32,
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
        // Out of sight until it is drawn.
        let motions = motions();
        let pistol_motion = motions.and_then(|motions| motions.pistol.as_ref());
        let pistol_node = graph
            .find_by_name(root, pistol_motion.map_or(PISTOL, |motion| motion.node.as_str()))
            .map(|(pistol, _)| pistol);
        if let Some(pistol) = pistol_node {
            graph[pistol].set_visibility(false);
        }
        let muzzle = pistol_motion
            .and_then(|motion| graph.find_by_name(root, &motion.muzzle_node))
            .map(|(muzzle, _)| muzzle);
        let upper: Vec<Handle<Node>> = graph
            .find_by_name(root, UPPER_BODY)
            .map(|(upper, _)| graph.traverse_handle_iter(upper).collect())
            .unwrap_or_default();

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
        let find = |name: &str| graph.find_by_name(root, name).map(|(node, _)| node);
        let square = (|| {
            let [left, right] = SHOULDERS.map(find);
            let head = chain(parent_of, root, find(HEAD)?);
            let face = place(&head, |bone| rest[&bone]).rotation.inverse() * Vector3::z();
            Some(Square {
                upper: chain(parent_of, root, find(UPPER_BODY)?),
                head,
                shoulders: [chain(parent_of, root, left?), chain(parent_of, root, right?)],
                face,
            })
        })();
        let barrel_chain = muzzle.map_or_else(Vec::new, |muzzle| chain(parent_of, root, muzzle));
        let parents: FxHashMap<Handle<Node>, Handle<Node>> =
            rest.keys().map(|&bone| (bone, graph[bone].parent())).collect();
        let eyes = find(EYES).and_then(|eyes| claim_eyes(graph, eyes));
        // The screen and the shell as see-through green glass - the model's own see-through
        // shell comes in solid, and would hide the core - and the screen dark until the trigger
        // is pulled.
        let spinning: Vec<(Handle<Node>, bool)> = pistol_node.map_or_else(Vec::new, |pistol| {
            [(PISTOL_SHELL, false), (PISTOL_CORE, true)]
                .into_iter()
                .filter_map(|(name, reversed)| Some((graph.find_by_name(pistol, name)?.0, reversed)))
                .collect()
        });
        let flash = pistol_node.and_then(|pistol| {
            let find = |name: &str| graph.find_by_name(pistol, name).map(|(node, _)| node);
            let (screen, shell) = (find(PISTOL_SCREEN), find(PISTOL_SHELL));
            if let Some(shell) = shell {
                replace_materials(graph, shell, |_| true, &green_glass(SHELL_TINT, SHELL_GLOW));
            }
            let screen = screen?;
            replace_materials(graph, screen, |_| true, &green_glass(SCREEN_TINT, SCREEN_GLOW));
            graph[screen].set_visibility(false);
            Some(Flash {
                screen,
                since: f32::INFINITY,
            })
        });

        let named: FxHashMap<String, Handle<Node>> =
            nodes.iter().map(|&node| (graph[node].name().to_owned(), node)).collect();
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
        let mut skids = FxHashMap::default();
        for &name in TURNS.iter().chain(&CUTS).chain([&STOP]) {
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
        let pistol = match (pistol_motion, pistol_node, muzzle) {
            (Some(motion), Some(node), Some(muzzle)) if !upper.is_empty() => {
                let clips = &motion.clips;
                let mut find = |name: &str, looped: bool| {
                    let found = container.find_by_name_mut(name).map(|(handle, animation)| {
                        animation.set_loop(looped);
                        handle
                    });
                    if found.is_none() {
                        warn(format!("Droid: it has no {name}"));
                    }
                    found
                };
                let (draw, aim) = (find(&clips.draw, false), find(&clips.aim, true));
                let (fire, holster) = (find(&clips.fire, false), find(&clips.holster, false));
                let mut find_some = |name: &Option<String>, looped: bool| {
                    name.as_deref().and_then(|name| find(name, looped))
                };
                let ready = match (
                    find_some(&clips.ready, true),
                    find_some(&clips.ready_to_aim, false),
                    find_some(&clips.aim_to_ready, false),
                ) {
                    (Some(ready), Some(raise), Some(lower)) => Some(Ready { ready, raise, lower }),
                    _ => {
                        info("Droid: without a ready, its pistol aims all the while it is out".into());
                        None
                    }
                };
                match (draw, aim, fire, holster) {
                    (Some(draw), Some(aim), Some(fire), Some(holster)) => Some(Pistol {
                        node,
                        muzzle,
                        draw,
                        aim,
                        fire,
                        holster,
                        ready,
                        show_at: frame_time(motion.show_frame),
                        hide_after: frame_time(motion.hide_after_frame),
                        shot_at: frame_time(motion.shot_frame),
                    }),
                    _ => None,
                }
            }
            _ => {
                warn(format!("Droid: without a pistol in {MOTION} and the model, it has none"));
                None
            }
        };
        let aims = motions.and_then(|motions| motions.aim.as_ref()).and_then(|motion| {
            let bones: Vec<Handle<Node>> =
                motion.bones.iter().filter_map(|name| named.get(name).copied()).collect();
            // A held pose: the bones as the first frame of the animation called `name` has them.
            let mut pose_of = |name: &str| {
                let Some((_, animation)) = container.find_by_name_mut(name) else {
                    warn(format!("Droid: it has no {name}"));
                    return None;
                };
                animation.rewind();
                animation.tick(0.0);
                let pose: FxHashMap<Handle<Node>, Bone> = bones
                    .iter()
                    .map(|&bone| {
                        let rest = rest.get(&bone).copied().unwrap_or_else(Bone::identity);
                        let (position, rotation) = posed(animation, bone);
                        let posed = Bone {
                            position: position.unwrap_or(rest.position),
                            rotation: rotation.unwrap_or(rest.rotation),
                        };
                        (bone, posed)
                    })
                    .collect();
                Some(pose)
            };
            let reference = pose_of(&motion.reference_clip)?;
            let mut offsets = Vec::new();
            for &pitch in &motion.pitches {
                let mut row = Vec::new();
                for &yaw in &motion.yaws {
                    let name = motion.clips.get(&format!("{pitch},{yaw}"))?;
                    let pose = pose_of(name)?;
                    // Off the reference pose, in each bone's parent's terms.
                    row.push(
                        pose.iter()
                            .map(|(bone, pose)| {
                                let reference = reference[bone];
                                let offset = Bone {
                                    position: pose.position - reference.position,
                                    rotation: reference.rotation.inverse() * pose.rotation,
                                };
                                (*bone, offset)
                            })
                            .collect(),
                    );
                }
                offsets.push(row);
            }
            info(format!(
                "Droid: its pistol aims {} to {} degrees up and {} to {} to the left",
                motion.pitches.first()?,
                motion.pitches.last()?,
                motion.yaws.first()?,
                motion.yaws.last()?
            ));
            Some(Aims {
                pitches: motion.pitches.iter().map(|p| p.to_radians()).collect(),
                yaws: motion.yaws.iter().map(|y| y.to_radians()).collect(),
                offsets,
            })
        });
        if aims.is_none() {
            warn(format!("Droid: without aims in {MOTION}, its pistol only points ahead"));
        }
        let arms_pose = upper
            .iter()
            .filter_map(|bone| Some((*bone, *rest.get(bone)?)))
            .collect();

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
            pistol,
            arms: Arms::Holstered,
            arms_weight: 0.0,
            arms_from: Default::default(),
            arms_blend: 1.0,
            queued: false,
            since_shot: READY_AFTER,
            arms_pose,
            barrel: Vector3::z(),
            shot: None,
            flash,
            eyes,
            spinning,
            spun: 0.0,
            aims,
            look: (0.0, 0.0),
            square,
            barrel_chain,
            aim_fix: UnitQuaternion::identity(),
            blended: Default::default(),
            squaring: false,
            square_weight: 0.0,
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

    /// Where the middle of the droid's face is, across the world, as of the last frame: a little
    /// above its head bone, which sits at the bottom of the head. None without the bone.
    pub(crate) fn face_at(&self, graph: &Graph) -> Option<Vector3<f32>> {
        let &head = self.square.as_ref()?.head.last()?;
        let point = graph[head]
            .global_transform()
            .transform_point(&Point3::new(0.0, FACE_ABOVE_HEAD, 0.0));
        Some(point.coords)
    }

    /// Has the droid's eyes glow `colour`, as brightly as they were made to, or as they were made
    /// to with none.
    pub(crate) fn set_eyes(&self, colour: Option<Color>) {
        let Some(eyes) = &self.eyes else {
            return;
        };
        let (colour, glow) = match colour {
            Some(colour) => {
                let brightest = match eyes.glow {
                    MaterialProperty::Vector3(glow) => glow.max(),
                    MaterialProperty::Float(glow) => glow,
                    _ => 1.0,
                };
                let glow = Vector3::new(colour.r, colour.g, colour.b).cast::<f32>() / 255.0;
                (colour.into(), MaterialProperty::Vector3(glow * brightest))
            }
            None => (eyes.colour.clone(), eyes.glow.clone()),
        };
        let mut material = eyes.material.data_ref();
        material.set_property(DIFFUSE_COLOR, colour);
        material.set_property(EMISSION_STRENGTH, glow);
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

    /// Where the pistol's muzzle was, across the world, as a shot left it this frame, and which way
    /// the shot went, one meter long: the way the barrel pointed as the trigger was pulled. None
    /// if no shot left.
    pub(super) fn shot(&self) -> Option<(Vector3<f32>, Vector3<f32>)> {
        self.shot
    }

    /// Draws, aims, fires or holsters the pistol for another `dt`, as `going` wants it out or not
    /// and pulls the trigger: shows or hides it, lets any shot go, and puts where it has the
    /// upper body in `arms_pose`.
    fn arm(&mut self, graph: &mut Graph, going: Going, dt: f32) {
        self.shot = None;
        let Some(pistol) = self.pistol else {
            return;
        };
        let clip = |arms: Arms| match arms {
            Arms::Holstered => None,
            Arms::Drawing => Some(pistol.draw),
            Arms::Ready => pistol.ready.map(|ready| ready.ready),
            Arms::Raising => pistol.ready.map(|ready| ready.raise),
            Arms::Lowering => pistol.ready.map(|ready| ready.lower),
            Arms::Aiming => Some(pistol.aim),
            Arms::Firing(_) => Some(pistol.fire),
            Arms::Holstering => Some(pistol.holster),
        };
        let shown = graph[pistol.node].visibility();
        // The barrel runs along the muzzle's +X.
        let barrel = graph[pistol.muzzle]
            .global_transform()
            .transform_vector(&Vector3::x())
            .try_normalize(1.0e-6);
        let Some(container) = self.container(graph) else {
            return;
        };
        let ended = clip(self.arms).is_some_and(|clip| container[clip].has_ended());
        self.since_shot += dt;
        // Pulled at the ready, or on the way up or down, the shot waits for the pistol to be up.
        let waiting = matches!(self.arms, Arms::Ready | Arms::Raising | Arms::Lowering);
        self.queued |= going.trigger && waiting;
        let wants = Wants {
            out: going.armed,
            raised: going.raised,
            trigger: going.trigger || self.queued,
            ended,
            rested: self.since_shot >= READY_AFTER,
            can_ready: pistol.ready.is_some(),
        };
        if let Some(mut next) = next_arms(self.arms, wants) {
            // Put away before it was even in hand, there is nothing to holster.
            if next == Arms::Holstering && !shown {
                next = Arms::Holstered;
            }
            if next == Arms::Holstering {
                self.queued = false;
            }
            // From one of its animations to the next, the upper body goes over rather than jumps.
            if clip(self.arms).is_some() {
                self.arms_from.clone_from(&self.arms_pose);
                self.arms_blend = 0.0;
            }
            if let Some(clip) = clip(next) {
                let animation = &mut container[clip];
                animation.set_speed(1.0);
                animation.rewind();
                // Drawn again while still in hand, from the point the draw has it there.
                if next == Arms::Drawing && shown {
                    let start = animation.time_slice().start;
                    animation.set_time_position(start + pistol.show_at);
                }
            }
            if next == Arms::Firing(false) {
                self.queued = false;
                self.since_shot = 0.0;
                self.barrel = barrel.unwrap_or(self.barrel);
                if let Some(flash) = self.flash.as_mut() {
                    flash.since = 0.0;
                }
            }
            self.arms = next;
        }
        let mut into = 0.0;
        if let Some(clip) = clip(self.arms) {
            let animation = &mut container[clip];
            animation.tick(dt);
            into = animation.time_position() - animation.time_slice().start;
            take_pose(animation, &mut self.arms_pose);
            if self.arms_blend < 1.0 {
                self.arms_blend = (self.arms_blend + dt / ARMS_BLEND).min(1.0);
                let t = self.arms_blend * self.arms_blend * (3.0 - 2.0 * self.arms_blend);
                for (bone, pose) in self.arms_pose.iter_mut() {
                    if let Some(from) = self.arms_from.get(bone) {
                        *pose = from.towards(*pose, t);
                    }
                }
            }
        }
        let show = match self.arms {
            Arms::Holstered => false,
            Arms::Drawing => into >= pistol.show_at,
            Arms::Ready | Arms::Raising | Arms::Lowering | Arms::Aiming | Arms::Firing(_) => true,
            Arms::Holstering => into <= pistol.hide_after,
        };
        if show != shown {
            graph[pistol.node].set_visibility(show);
        }
        if self.arms == Arms::Firing(false) && into >= pistol.shot_at {
            self.arms = Arms::Firing(true);
            self.shot = Some((graph[pistol.muzzle].global_position(), self.barrel));
        }
        self.flash(graph, show, dt);
        let out = if self.arms == Arms::Holstered { 0.0 } else { 1.0 };
        let step = dt / ARMS_FADE;
        self.arms_weight += (out - self.arms_weight).clamp(-step, step);
    }

    /// Lets the flash of the pistol's screen, from the last pull of the trigger, die down for
    /// another `dt`, while it is `out`.
    fn flash(&mut self, graph: &mut Graph, out: bool, dt: f32) {
        let Some(mut flash) = self.flash else {
            return;
        };
        flash.since += dt;
        let lit = out && flash_glow(flash.since) > FLASH_DARK;
        if graph[flash.screen].visibility() != lit {
            graph[flash.screen].set_visibility(lit);
        }
        self.flash = Some(flash);
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
        self.from = if self.blended.is_empty() {
            self.rest
                .keys()
                .map(|&bone| (bone, Bone::of(&graph[bone])))
                .collect()
        } else {
            self.blended.clone()
        };
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

    /// Puts `target` on the bones, with the upper body gone over to the pistol as far as it has,
    /// as far through the change to it as the fade has got after another `dt` - and strafing,
    /// with the face and shoulders squared to straight ahead.
    fn put(&mut self, graph: &mut Graph, mut target: FxHashMap<Handle<Node>, Bone>, dt: f32) {
        if self.arms_weight > 0.0 {
            let w = self.arms_weight * self.arms_weight * (3.0 - 2.0 * self.arms_weight);
            for (bone, pose) in &self.arms_pose {
                if let Some(target) = target.get_mut(bone) {
                    *target = target.towards(*pose, w);
                }
            }
        }
        self.fade = (self.fade + dt / FADE_TIME).min(1.0);
        let t = self.fade * self.fade * (3.0 - 2.0 * self.fade);
        for (bone, pose) in target.iter_mut() {
            if let Some(from) = self.from.get(bone).filter(|_| t < 1.0) {
                *pose = from.towards(*pose, t);
            }
        }
        self.blended.clone_from(&target);
        // The ball at the muzzle tumbles, the core and the shell against each other.
        self.spun += dt;
        let tumble = tumbled(self.spun);
        for &(node, reversed) in &self.spinning {
            if let (Some(pose), Some(rest)) = (target.get_mut(&node), self.rest.get(&node)) {
                let tumble = if reversed { tumble.inverse() } else { tumble };
                pose.rotation = rest.rotation * tumble;
            }
        }
        // Strafing, the face and the shoulders square up to straight ahead - the body's ahead,
        // however far the droid is turned from it - whatever the hips below are doing. Last of
        // all, so that nothing still fading in from before turns them away again.
        let step = dt / SQUARE_FADE;
        let squared = if self.squaring { 1.0 } else { 0.0 };
        self.square_weight += (squared - self.square_weight).clamp(-step, step);
        if let Some(square) = self.square.as_ref().filter(|_| self.square_weight > 0.0) {
            let w = self.square_weight * self.square_weight * (3.0 - 2.0 * self.square_weight);
            let ahead = -self.heading;
            let [left, right] = square
                .shoulders
                .each_ref()
                .map(|chain| place(chain, |bone| target[&bone]).position);
            let off = wrap(ahead - shoulders_yaw(left, right));
            turn_bone(&mut target, &square.upper, w * off);
            let face = place(&square.head, |bone| target[&bone]).rotation * square.face;
            turn_bone(&mut target, &square.head, w * wrap(ahead - yaw_of(face)));
        }
        // The pistol follows the camera, as far over to the pistol as the upper body has gone:
        // the aims' offsets laid on top, from where the droid itself faces.
        if let Some(aims) = self.aims.as_ref().filter(|_| self.arms_weight > 0.0) {
            let w = self.arms_weight * self.arms_weight * (3.0 - 2.0 * self.arms_weight);
            // Squared up, the upper body already faces the body's ahead; otherwise the droid's.
            let facing = self.heading * (1.0 - self.square_weight);
            let (pitch, yaw) = (self.look.0, wrap(self.look.1 - facing));
            for (bone, offset) in aims.offset(pitch, yaw) {
                if let Some(pose) = target.get_mut(&bone) {
                    let offset = Bone::identity().towards(offset, w);
                    pose.position += offset.position;
                    pose.rotation *= offset.rotation;
                }
            }
            // Whatever the legs below lean the upper body by - forward in a walk, say - the
            // barrel is turned the rest of the way to where the camera looks, as far as the aims
            // reach, the upper body and all.
            if let (Some(square), Some(&top), Some(&bottom)) =
                (&self.square, aims.pitches.last(), aims.pitches.first())
            {
                let (left, right) = (aims.yaws.last().copied(), aims.yaws.first().copied());
                let yaw = yaw.clamp(right.unwrap_or(yaw), left.unwrap_or(yaw));
                let wanted = pointing(pitch.clamp(bottom, top), wrap(yaw + facing - self.heading));
                if matches!(self.arms, Arms::Aiming | Arms::Ready) {
                    let barrel =
                        place(&self.barrel_chain, |bone| target[&bone]).rotation * Vector3::x();
                    if let Some(needed) = UnitQuaternion::rotation_between(&barrel, &wanted) {
                        let catch_up = 1.0 - (-AIM_FIX_RATE * dt).exp();
                        self.aim_fix = self.aim_fix.slerp(&needed, catch_up);
                    }
                }
                let turn = UnitQuaternion::identity().slerp(&self.aim_fix, w);
                rotate_bone(&mut target, &square.upper, turn);
            }
        }
        for (bone, pose) in target {
            let transform = graph[bone].local_transform_mut();
            transform.set_position(pose.position);
            transform.set_rotation(pose.rotation);
        }
    }

    /// Poses the droid for what the body is doing, turning it to face the way it is going.
    pub(crate) fn animate(&mut self, graph: &mut Graph, going: Going, dt: f32) {
        self.travel = None;
        self.arm(graph, going, dt);
        self.squaring = going.strafing;
        self.look = going.look;
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
        let Some((kind, name)) = skid_for(gait, self.heading, heading, going.pushing) else {
            return false;
        };
        let Some(&Skid { animation, entry, .. }) = self.skids.get(name) else {
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
    fn sprinting_it_skids_round_cuts_across_or_stops_and_the_short_way() {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
        let sprinting = |from: f32, to: f32| skid_for(Gait::Sprinting, from, to, true);
        let round = |name| Some((SkidKind::Round, name));
        assert_eq!(sprinting(0.0, FRAC_PI_4), None, "an eighth of the way round is only a turn");
        assert_eq!(sprinting(0.0, FRAC_PI_2), Some((SkidKind::Cut, CUTS[0])), "W to A");
        assert_eq!(sprinting(FRAC_PI_4, -FRAC_PI_4), Some((SkidKind::Cut, CUTS[1])), "W+A to W+D");
        assert_eq!(sprinting(0.0, 3.0 * FRAC_PI_4), round(TURNS[0]), "W to S+A");
        assert_eq!(sprinting(0.0, -3.0 * FRAC_PI_4), round(TURNS[1]), "W to S+D");
        assert!(sprinting(0.0, PI).is_some_and(|(kind, _)| kind == SkidKind::Round), "W to S");
        // Facing back-left, going forward-left is round to the right.
        assert_eq!(sprinting(3.0 * FRAC_PI_4, -FRAC_PI_4 + 0.1), round(TURNS[1]));
    }

    #[test]
    fn short_of_a_sprint_it_never_skids() {
        use std::f32::consts::{FRAC_PI_2, PI};
        for gait in [Gait::Walking, Gait::Running] {
            assert_eq!(skid_for(gait, 0.0, PI, true), None, "{gait:?} round");
            assert_eq!(skid_for(gait, 0.0, FRAC_PI_2, true), None, "{gait:?} across");
            assert_eq!(skid_for(gait, 0.0, 0.0, false), None, "{gait:?} letting go");
        }
    }

    #[test]
    fn letting_go_sprinting_stops_whichever_way_it_faced() {
        for to in [0.0, 1.0, 3.0] {
            assert_eq!(skid_for(Gait::Sprinting, 0.0, to, false), Some((SkidKind::Stop, STOP)));
        }
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
    fn the_pistol_is_drawn_to_the_ready_raised_fired_and_holstered_in_turn() {
        use Arms::*;
        let wants = Wants { out: true, rested: true, can_ready: true, ..Default::default() };
        let ended = Wants { ended: true, ..wants };
        let raised = Wants { raised: true, ..wants };
        let trigger = Wants { trigger: true, ..wants };
        // Drawn, it comes down to the ready - or stays up, held raised.
        assert_eq!(next_arms(Holstered, wants), Some(Drawing));
        assert_eq!(next_arms(Drawing, wants), None, "still drawing");
        assert_eq!(next_arms(Drawing, ended), Some(Lowering));
        assert_eq!(next_arms(Drawing, Wants { raised: true, ..ended }), Some(Aiming));
        assert_eq!(next_arms(Lowering, ended), Some(Ready));
        // Raised to aim, or to fire; and lowered again, rested and let go.
        assert_eq!(next_arms(Ready, wants), None, "at the ready");
        assert_eq!(next_arms(Ready, raised), Some(Raising));
        assert_eq!(next_arms(Ready, trigger), Some(Raising), "up to fire");
        assert_eq!(next_arms(Lowering, trigger), Some(Raising), "back up halfway down");
        assert_eq!(next_arms(Raising, Wants { ended: true, ..raised }), Some(Aiming));
        assert_eq!(next_arms(Aiming, raised), None, "held up");
        assert_eq!(next_arms(Aiming, Wants { rested: false, ..wants }), None, "just fired");
        assert_eq!(next_arms(Aiming, wants), Some(Lowering), "let go, and rested");
        // The trigger fires only once it is up, and again only once the shot has left.
        assert_eq!(next_arms(Drawing, trigger), None, "not drawn yet");
        assert_eq!(next_arms(Aiming, trigger), Some(Firing(false)));
        assert_eq!(next_arms(Firing(false), trigger), None, "the shot is yet to leave");
        assert_eq!(next_arms(Firing(true), trigger), Some(Firing(false)));
        assert_eq!(next_arms(Firing(true), ended), Some(Aiming));
        // Put away from anything, and drawn again halfway through putting it away.
        let away = Wants { out: false, ..wants };
        for arms in [Drawing, Ready, Raising, Lowering, Aiming, Firing(false), Firing(true)] {
            assert_eq!(next_arms(arms, away), Some(Holstering), "{arms:?}");
        }
        assert_eq!(next_arms(Holstering, Wants { ended: true, ..away }), Some(Holstered));
        assert_eq!(next_arms(Holstering, wants), Some(Drawing));
        assert_eq!(next_arms(Holstered, Wants { trigger: true, ..away }), None);
        // Without a ready, it aims all the while it is out.
        let no_ready = Wants { can_ready: false, ..ended };
        assert_eq!(next_arms(Drawing, no_ready), Some(Aiming));
        assert_eq!(next_arms(Aiming, Wants { can_ready: false, ..wants }), None);
    }

    #[test]
    fn shoulders_and_faces_turn_left_positive() {
        let turn = |angle: f32| UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle);
        let (left, right) = (Vector3::new(0.2, 1.5, 0.0), Vector3::new(-0.2, 1.5, 0.0));
        assert!(shoulders_yaw(left, right).abs() < 1e-6, "square");
        let turned = shoulders_yaw(turn(0.4) * left, turn(0.4) * right);
        assert!((turned - 0.4).abs() < 1e-5, "{turned}");
        assert!((yaw_of(turn(-0.7) * Vector3::z()) + 0.7).abs() < 1e-5);
    }

    #[test]
    fn a_bone_turned_takes_what_hangs_off_it_along_and_keeps_its_parent_as_it_was() {
        let (hips, chest, arm) = (Handle::new(1, 1), Handle::new(2, 1), Handle::new(3, 1));
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 0.3);
        let mut target: FxHashMap<Handle<Node>, Bone> = [
            (hips, Bone { position: Vector3::new(0.0, 1.0, 0.0), rotation: tilt }),
            (chest, Bone { position: Vector3::new(0.0, 0.2, 0.0), rotation: UnitQuaternion::identity() }),
            (arm, Bone { position: Vector3::new(0.2, 0.3, 0.0), rotation: UnitQuaternion::identity() }),
        ]
        .into_iter()
        .collect();
        let before = place(&[hips, chest, arm], |bone| target[&bone]).position;
        turn_bone(&mut target, &[hips, chest], 0.5);
        assert_eq!(target[&hips].rotation, tilt, "the hips stay as they were");
        let chest_at = place(&[hips, chest], |bone| target[&bone]).position;
        let after = place(&[hips, chest, arm], |bone| target[&bone]).position;
        // The arm swings round the chest about the droid's up, keeping its height and reach.
        let (was, now) = (before - chest_at, after - chest_at);
        assert!((was.y - now.y).abs() < 1e-5 && (was.norm() - now.norm()).abs() < 1e-5);
        let turned = yaw_of(now) - yaw_of(was);
        assert!((wrap(turned) - 0.5).abs() < 1e-4, "{turned}");
    }

    #[test]
    fn the_flash_dies_away() {
        assert_eq!(flash_glow(0.0), 1.0);
        assert!(flash_glow(0.1) < 0.5 && flash_glow(0.1) > flash_glow(0.2));
        assert!(flash_glow(f32::INFINITY) == 0.0, "dark until the first pull");
    }

    #[test]
    fn it_tumbles_every_way_round_and_keeps_going() {
        assert_eq!(tumbled(0.0), UnitQuaternion::identity(), "at rest to start with");
        // About every axis at once: not a turn about any one of them.
        let axis = tumbled(0.01).axis().unwrap();
        assert!(axis.x.abs() > 0.1 && axis.y.abs() > 0.1 && axis.z.abs() > 0.1, "{axis:?}");
        // And not settling on one: the way it turns wanders as it goes.
        let later = (tumbled(2.0) * tumbled(1.9).inverse()).axis().unwrap();
        let sooner = (tumbled(0.6) * tumbled(0.5).inverse()).axis().unwrap();
        assert!(later.dot(&sooner) < 0.99, "{later:?} {sooner:?}");
    }

    #[test]
    fn an_aim_falls_between_the_two_either_side_of_it() {
        let grid = [-60.0, -30.0, 0.0, 30.0, 60.0];
        assert_eq!(between(&grid, 0.0), (1, 2, 1.0), "on one, all the way to it");
        assert_eq!(between(&grid, 15.0), (2, 3, 0.5));
        assert_eq!(between(&grid, -45.0), (0, 1, 0.5));
        assert_eq!(between(&grid, 90.0), (3, 4, 1.0), "held to the top");
        assert_eq!(between(&grid, -90.0), (0, 0, 0.0), "held to the bottom");
    }

    #[test]
    fn aims_blend_the_four_round_where_it_looks() {
        let bone = Handle::new(1, 1);
        let turned = |angle: f32| {
            let pose = Bone {
                position: Vector3::new(angle, 0.0, 0.0),
                rotation: UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle),
            };
            [(bone, pose)].into_iter().collect::<FxHashMap<_, _>>()
        };
        // Offsets that turn and move the bone by pitch + 2 * yaw.
        let (pitches, yaws) = (vec![-0.5, 0.0, 0.5], vec![-0.5, 0.0, 0.5]);
        let offsets = pitches
            .iter()
            .map(|p| yaws.iter().map(|y| turned(p + 2.0 * y)).collect())
            .collect();
        let aims = Aims { pitches, yaws, offsets };
        let at = |pitch: f32, yaw: f32| aims.offset(pitch, yaw)[&bone].position.x;
        assert!((at(0.0, 0.0) - 0.0).abs() < 1e-5);
        assert!((at(0.25, 0.25) - 0.75).abs() < 1e-5, "halfway between four");
        assert!((at(2.0, -2.0) - (0.5 - 1.0)).abs() < 1e-5, "held to the corner");
    }

    #[test]
    fn a_way_to_point_is_up_and_to_the_left() {
        assert!((pointing(0.0, 0.0) - Vector3::z()).norm() < 1e-6, "ahead");
        assert!((pointing(0.0, std::f32::consts::FRAC_PI_2) - Vector3::x()).norm() < 1e-6, "left");
        let up = pointing(0.5, 0.3);
        assert!((up.y.asin() - 0.5).abs() < 1e-5 && (up.x.atan2(up.z) - 0.3).abs() < 1e-5);
    }

    #[test]
    fn a_bone_rotated_keeps_its_parent_and_turns_in_the_droids_own_terms() {
        let (hips, chest) = (Handle::new(1, 1), Handle::new(2, 1));
        let tilt = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 0.3);
        let mut target: FxHashMap<Handle<Node>, Bone> = [
            (hips, Bone { position: Vector3::zeros(), rotation: tilt }),
            (chest, Bone { position: Vector3::y(), rotation: UnitQuaternion::identity() }),
        ]
        .into_iter()
        .collect();
        let turn = UnitQuaternion::from_axis_angle(&Vector3::z_axis(), 0.4);
        let before = place(&[hips, chest], |bone| target[&bone]).rotation;
        rotate_bone(&mut target, &[hips, chest], turn);
        let after = place(&[hips, chest], |bone| target[&bone]).rotation;
        assert_eq!(target[&hips].rotation, tilt);
        assert!(after.angle_to(&(turn * before)) < 1e-5);
    }

    #[test]
    fn frames_are_counted_from_one() {
        assert_eq!(frame_time(1), 0.0);
        assert!((frame_time(6) - 5.0 / 24.0).abs() < 1e-6);
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
