//! The droid the player is seen as in third person: its model, and the cycles it walks, runs,
//! sprints and crouches along with.
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
//! down on the floor. Standing still, the droid settles back into its rest pose; crouched, it
//! holds the crouch where it stopped. Going from one cycle to another, it carries on at the same
//! point in the stride.
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

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Avatar {
    root: Handle<Node>,
    animations: Handle<Node>,
    cycles: Vec<Cycle>,
    /// Every bone, at rest.
    rest: FxHashMap<Handle<Node>, Bone>,
    /// The bones at the top of the rig, which carry the cycles' travel.
    top: Vec<Handle<Node>>,
    anchor: Handle<Node>,
    /// The cycle playing, as an index into `cycles`; none is the rest pose.
    playing: Option<usize>,
    /// Where the bones were when the change to what is playing now began, and how far through it
    /// is, from 0 to 1.
    from: FxHashMap<Handle<Node>, Bone>,
    fade: f32,
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

impl Avatar {
    /// Puts the droid into `scene` from its `model`, standing on `feet` below `body`'s origin,
    /// facing the way the body does. None if the model is not the droid it is expected to be.
    pub(super) fn spawn(
        model: &ModelResource,
        scene: &mut Scene,
        body: Handle<Node>,
        feet: f32,
    ) -> Option<Self> {
        let root = model.instantiate(scene);
        let avatar = Self::build(&mut scene.graph, root, body, feet);
        if avatar.is_none() {
            Log::err(format!(
                "Player: {DROID_MODEL} is missing its rig or its cycles; playing without it"
            ));
            scene.graph.remove_node(root);
        }
        avatar
    }

    fn build(graph: &mut Graph, root: Handle<Node>, body: Handle<Node>, feet: f32) -> Option<Self> {
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
        // Its poses are put on the bones here, blended, rather than by the engine.
        player.set_auto_apply(false);
        let container = player.animations_mut().get_value_mut_silent();
        let mut cycles = Vec::new();
        for (name, gait) in CYCLES {
            let Some((handle, animation)) = container.find_by_name_mut(name) else {
                Log::warn(format!("Player: the droid has no {name}"));
                continue;
            };
            let length = animation.length();
            let Some(distance) = travel(animation, anchor).filter(|d| *d > 1.0e-3) else {
                Log::warn(format!("Player: the droid's {name} goes nowhere"));
                continue;
            };
            let speed = distance * SCALE / length;
            Log::info(format!("Player: the droid's {name} goes {speed:.2} m/s"));
            cycles.push(Cycle {
                animation: handle,
                gait,
                speed,
            });
        }
        if cycles.is_empty() {
            return None;
        }

        Some(Self {
            root,
            animations,
            cycles,
            rest,
            top,
            anchor,
            playing: None,
            from: Default::default(),
            fade: 1.0,
        })
    }

    fn gaits(&self) -> Vec<Option<Gait>> {
        self.cycles.iter().map(|c| c.gait).collect()
    }

    /// How fast the droid goes at `gait` in `posture`, in meters per second, with its feet
    /// keeping to the floor. None without a cycle to go by.
    pub(super) fn pace(&self, posture: Posture, gait: Gait) -> Option<f32> {
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

    pub(super) fn is_visible(&self, graph: &Graph) -> bool {
        graph[self.root].global_visibility()
    }

    pub(super) fn set_visible(&self, graph: &mut Graph, visible: bool) {
        if graph[self.root].visibility() != visible {
            graph[self.root].set_visibility(visible);
        }
    }

    /// Poses the droid for a body going `speed` meters per second along the ground at `gait` in
    /// `posture`, with its feet on the ground or not.
    pub(super) fn animate(
        &mut self,
        graph: &mut Graph,
        speed: f32,
        posture: Posture,
        gait: Gait,
        grounded: bool,
        dt: f32,
    ) {
        let crouched = posture != Posture::Standing;
        let wanted = choose(&self.gaits(), gait, crouched, speed >= STILL);

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
            self.playing = wanted;
            self.from = self
                .rest
                .keys()
                .map(|&bone| (bone, Bone::of(&graph[bone])))
                .collect();
            self.fade = 0.0;
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
            let rate = if grounded { speed / cycle.speed } else { 0.0 };
            animation.set_speed(rate);
            animation.tick(dt);
            for (&bone, pose) in target.iter_mut() {
                let (position, rotation) = posed(animation, bone);
                pose.position = position.unwrap_or(pose.position);
                pose.rotation = rotation.unwrap_or(pose.rotation);
            }
            // The travel taken back out: the anchor stays where it rests, going forward, and
            // the rest of the top of the rig goes with it - the bones the cycle moves, that is.
            // The skinned mesh hangs off the rig too, and is left where it is.
            let drift = target[&self.anchor].position.z - self.rest[&self.anchor].position.z;
            for bone in &self.top {
                if posed(animation, *bone).0.is_some() {
                    if let Some(pose) = target.get_mut(bone) {
                        pose.position.z -= drift;
                    }
                }
            }
        }

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
