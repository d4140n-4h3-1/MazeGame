//! A droid gone limp: its body falls as the physics has it, one rigid body for each part of it.
//!
//! [`MOTION`]'s `ragdoll` has the parts, made along with the droid's animations: eighteen bodies -
//! pelvis, abdomen, chest, head, and each side's collarbone, upper arm, forearm, hand, thigh, shin
//! and foot - each driving one bone, with capsules and boxes fitted to the droid's skin, joined by
//! ball joints limited to how far a body bends, and every pose in the animations. Each body sits
//! exactly where its bone does, so a body's pose is its bone's; the bones in between, the fingers
//! and toes ride along as they were.
//!
//! Going limp takes two steps. The joints' frames are fixed by where the engine finds the joint
//! and the two bodies the first time it sees them, so the bodies are first laid out as the droid
//! stands in its rest pose - out of the way, touching nothing, not moving - long enough for the
//! engine to join them ([`BIND_FRAMES`]). Then each is put where its bone is in whatever the droid
//! was doing, carrying on as fast as the droid was going, the bolt that stopped it knocking it
//! back, and let go. From then on the bones follow the bodies.
//!
//! The bodies bump into the maze and into each other, but not into anyone's capsule: the player
//! and the droids walk through someone lying on the floor rather than climb over them. Bolts hit
//! them, and shove them.

use crate::player::avatar::{MOTION, SCALE};
use fyrox::{
    core::{
        algebra::{Matrix3, Matrix4, UnitQuaternion, Vector3},
        log::Log,
        math::Matrix4Ext,
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        collider::{BitMask, Collider, ColliderBuilder, ColliderShape, InteractionGroups},
        graph::Graph,
        joint::{BallJoint, Joint, JointBuilder, JointParams},
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
    },
};
use serde::Deserialize;

/// The collision group everyone's capsule is in - the player's and the droids' - and no body of
/// a limp droid ever is, nor bumps into.
pub const CHARACTERS: u32 = 1 << 31;
/// How many frames the bodies are laid out in the rest pose for the engine to join them.
const BIND_FRAMES: u32 = 3;
/// How hard the bolt that stops a droid knocks it, and one that hits it lying there shoves it,
/// in newton seconds.
pub const STOPPING_BLOW: f32 = 15.0;
pub const SHOVE: f32 = 6.0;
/// How far wide of every body a bolt can go and still be counted as striking the nearest, in
/// meters.
const NEAR_MISS: f32 = 0.05;

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Motion {
    #[serde(default)]
    ragdoll: Option<Spec>,
}

/// The ragdoll as [`MOTION`] has it. Lengths are in the model's own meters, as it comes, before
/// the game scales it by [`SCALE`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Spec {
    physics: Physics,
    bodies: Vec<BodySpec>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Physics {
    friction: f32,
    restitution: f32,
    linear_damping: f32,
    angular_damping: f32,
    joint_damping: f32,
    joint_max_torque: f32,
    /// The bodies small and quick enough to go through the floor in a frame, without care.
    ccd: Vec<String>,
    solver_iterations: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct BodySpec {
    name: String,
    bone: String,
    /// The body it hangs off, before it in the list; none for the pelvis.
    parent: Option<String>,
    density_kg_m3: f32,
    collision_bit: u32,
    /// The bodies it does not bump into: those it is joined to, and those that overlap it in
    /// some animation.
    ignores: Vec<String>,
    colliders: Vec<Shape>,
    joint: Option<JointSpec>,
}

/// A collider, in its body's (its bone's) own terms.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
enum Shape {
    Capsule {
        begin: [f32; 3],
        end: [f32; 3],
        radius: f32,
    },
    Cuboid {
        position: [f32; 3],
        rotation: [f32; 4],
        half_extents: [f32; 3],
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct JointSpec {
    /// Where the joint is and which way it faces on the parent's body, and on its own.
    frame_in_parent: Frame,
    frame_in_body: Frame,
    /// How far it bends about each of its axes either way from the rest pose, in degrees.
    limits_deg: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct Frame {
    position: [f32; 3],
    /// A quaternion, as x, y, z, w.
    rotation: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct Limits {
    x: [f32; 2],
    y: [f32; 2],
    z: [f32; 2],
}

fn vector(v: [f32; 3]) -> Vector3<f32> {
    Vector3::new(v[0], v[1], v[2])
}

/// How far along the segment from `a` to `b`, from 0 to 1, it comes closest to the one from `c` to
/// `d`, and how close.
fn closest(a: Vector3<f32>, b: Vector3<f32>, c: Vector3<f32>, d: Vector3<f32>) -> (f32, f32) {
    let (u, v, w) = (b - a, d - c, a - c);
    let (uu, uv, vv, uw, vw) = (u.dot(&u), u.dot(&v), v.dot(&v), u.dot(&w), v.dot(&w));
    let denominator = uu * vv - uv * uv;
    let mut s = if denominator > 1.0e-9 { ((uv * vw - vv * uw) / denominator).clamp(0.0, 1.0) } else { 0.0 };
    let mut t = if vv > 1.0e-9 { ((uv * s + vw) / vv).clamp(0.0, 1.0) } else { 0.0 };
    if uu > 1.0e-9 {
        s = ((uv * t - uw) / uu).clamp(0.0, 1.0);
        t = if vv > 1.0e-9 { ((uv * s + vw) / vv).clamp(0.0, 1.0) } else { 0.0 };
    }
    (s, ((a + u * s) - (c + v * t)).norm())
}

fn quaternion(q: [f32; 4]) -> UnitQuaternion<f32> {
    UnitQuaternion::from_quaternion(fyrox::core::algebra::Quaternion::new(q[3], q[0], q[1], q[2]))
}

/// A pose without scale: where, and which way round.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    position: Vector3<f32>,
    rotation: UnitQuaternion<f32>,
}

impl Pose {
    /// `frame`, scaled to the droid's size in the game, as a pose.
    fn of(frame: &Frame) -> Self {
        Self {
            position: vector(frame.position) * SCALE,
            rotation: quaternion(frame.rotation),
        }
    }

    /// The pose of whatever sits `other` along from this pose.
    fn then(self, other: Self) -> Self {
        Self {
            position: self.position + self.rotation * other.position,
            rotation: self.rotation * other.rotation,
        }
    }

    fn inverse(self) -> Self {
        let rotation = self.rotation.inverse();
        Self {
            position: -(rotation * self.position),
            rotation,
        }
    }

    /// Where a node is and which way it faces, from its transform across the world, leaving any
    /// scale out.
    fn of_transform(transform: &Matrix4<f32>) -> Self {
        let basis = transform.basis();
        let unscaled = Matrix3::from_columns(&[
            basis.column(0).normalize(),
            basis.column(1).normalize(),
            basis.column(2).normalize(),
        ]);
        Self {
            position: transform.position(),
            rotation: UnitQuaternion::from_matrix_eps(
                &unscaled,
                f32::EPSILON,
                16,
                UnitQuaternion::identity(),
            ),
        }
    }

    /// The transform of a bone of the droid posed like this: scaled as the model is.
    fn transform(self) -> Matrix4<f32> {
        Matrix4::new_translation(&self.position)
            * self.rotation.to_homogeneous()
            * Matrix4::new_scaling(SCALE)
    }
}

/// What [`MOTION`] has for the ragdoll, read once for every droid; none if it cannot be read or
/// has none.
fn spec() -> Option<&'static Spec> {
    static SPEC: std::sync::OnceLock<Option<Spec>> = std::sync::OnceLock::new();
    SPEC.get_or_init(|| {
        let read = crate::platform::read_to_string(MOTION);
        match read.and_then(|text| serde_json::from_str::<Motion>(&text).map_err(|e| e.to_string())) {
            Ok(Motion { ragdoll: Some(spec) }) => Some(spec),
            Ok(_) => {
                Log::warn(format!("Ragdoll: {MOTION} has none; stopped droids just crouch"));
                None
            }
            Err(error) => {
                Log::err(format!("Ragdoll: could not read {MOTION}: {error}"));
                None
            }
        }
    })
    .as_ref()
}

/// Has the physics solve joints as carefully as the ragdoll needs: with fewer passes, a limp
/// droid's joints stretch as it hits the floor, and its shoulders come out of their sockets.
pub fn prepare(graph: &mut Graph) {
    if let Some(spec) = spec() {
        let parameters = &mut graph.physics.integration_parameters;
        if parameters.num_solver_iterations < spec.physics.solver_iterations {
            parameters.num_solver_iterations = spec.physics.solver_iterations;
        }
    }
}

/// The collision groups of everyone's capsule: see [`CHARACTERS`].
pub fn character_groups() -> InteractionGroups {
    InteractionGroups::new(BitMask(CHARACTERS), BitMask(u32::MAX))
}

/// One of the bodies: its name, the bone it drives, its colliders, and the joint that holds it to
/// the body it hangs off, unless it has none or has been let loose.
#[derive(Debug, Clone, PartialEq)]
struct Limb {
    name: &'static str,
    bone: Handle<Node>,
    body: Handle<RigidBody>,
    colliders: Vec<Handle<Collider>>,
    joint: Option<Handle<Joint>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// Laid out in the rest pose while the engine joins the bodies, for this many more frames;
    /// then let go at the speed the droid was going, knocked by a blow at a point, if one.
    Joining {
        frames: u32,
        velocity: Vector3<f32>,
        blow: Option<(Vector3<f32>, Vector3<f32>)>,
    },
    /// Falling, or lying there.
    Limp,
}

/// A droid gone limp, or about to.
#[derive(Debug, Clone, PartialEq)]
pub struct Ragdoll {
    /// The model's root, which every bone hangs off.
    root: Handle<Node>,
    /// Its bodies, each after the one it hangs off; the pelvis first.
    limbs: Vec<Limb>,
    state: State,
    /// Frames since it was let go; and whether to log how it falls (MAZE_KNOCKDOWN).
    frames: u32,
    report: bool,
}

impl Ragdoll {
    /// Starts the droid whose model's root is `root` going limp, going at `velocity` across the
    /// world, and knocked by `blow` - an impulse and where it lands - if one. None if the droid
    /// has no ragdoll, or its model not the bones it needs.
    pub fn start(
        graph: &mut Graph,
        root: Handle<Node>,
        velocity: Vector3<f32>,
        blow: Option<(Vector3<f32>, Vector3<f32>)>,
    ) -> Option<Self> {
        let spec = spec()?;
        let bones = spec
            .bodies
            .iter()
            .map(|body| graph.find_by_name(root, &body.bone).map(|(bone, _)| bone))
            .collect::<Option<Vec<_>>>();
        let Some(bones) = bones else {
            Log::err("Ragdoll: the droid's model is missing some of its bones");
            return None;
        };
        let index = |name: &str| spec.bodies.iter().position(|body| body.name == name);
        let bit = |name: &str| index(name).map_or(0, |i| 1u32 << spec.bodies[i].collision_bit);
        let physics = &spec.physics;

        // Laid out in the rest pose, from where the pelvis is now.
        let mut poses: Vec<Pose> = Vec::with_capacity(bones.len());
        let mut limbs: Vec<Limb> = Vec::with_capacity(bones.len());
        for (i, body) in spec.bodies.iter().enumerate() {
            let parent = body.parent.as_deref().and_then(index);
            let pose = match (parent, &body.joint) {
                (Some(parent), Some(joint)) => poses[parent]
                    .then(Pose::of(&joint.frame_in_parent))
                    .then(Pose::of(&joint.frame_in_body).inverse()),
                _ => Pose::of_transform(&graph[bones[i]].global_transform()),
            };
            poses.push(pose);

            let filter = body.ignores.iter().fold(!CHARACTERS, |filter, other| filter & !bit(other));
            let groups = InteractionGroups::new(BitMask(1 << body.collision_bit), BitMask(filter));
            let colliders: Vec<Handle<Collider>> = body
                .colliders
                .iter()
                .map(|shape| {
                    let (shape, place) = match *shape {
                        Shape::Capsule { begin, end, radius } => (
                            ColliderShape::capsule(vector(begin) * SCALE, vector(end) * SCALE, radius * SCALE),
                            TransformBuilder::new().build(),
                        ),
                        Shape::Cuboid { position, rotation, half_extents } => {
                            let h = vector(half_extents) * SCALE;
                            (
                                ColliderShape::cuboid(h.x, h.y, h.z),
                                TransformBuilder::new()
                                    .with_local_position(vector(position) * SCALE)
                                    .with_local_rotation(quaternion(rotation))
                                    .build(),
                            )
                        }
                    };
                    ColliderBuilder::new(BaseBuilder::new().with_local_transform(place))
                        .with_shape(shape)
                        .with_density(Some(body.density_kg_m3))
                        .with_friction(physics.friction)
                        .with_restitution(physics.restitution)
                        .with_collision_groups(groups)
                        // Touching nothing until it is let go.
                        .with_sensor(true)
                        .build(graph)
                })
                .collect();
            let rigid_body = RigidBodyBuilder::new(
                colliders
                    .iter()
                    .fold(BaseBuilder::new(), |base, &collider| base.with_child(collider))
                    .with_name(format!("ragdoll {}", body.name))
                    .with_local_transform(
                        TransformBuilder::new()
                            .with_local_position(pose.position)
                            .with_local_rotation(pose.rotation)
                            .build(),
                    ),
            )
            .with_body_type(RigidBodyType::KinematicPositionBased)
            // Its mass, and how hard it is to turn, come from its colliders.
            .with_mass(0.0)
            .with_lin_damping(physics.linear_damping)
            .with_ang_damping(physics.angular_damping)
            .with_ccd_enabled(physics.ccd.contains(&body.name))
            .build(graph);

            let mut held = None;
            if let (Some(parent), Some(joint)) = (parent, &body.joint) {
                let at = pose.then(Pose::of(&joint.frame_in_body));
                let range = |[low, high]: [f32; 2]| low.to_radians()..high.to_radians();
                let limits = joint.limits_deg;
                let handle = JointBuilder::new(
                    BaseBuilder::new().with_local_transform(
                        TransformBuilder::new()
                            .with_local_position(at.position)
                            .with_local_rotation(at.rotation)
                            .build(),
                    ),
                )
                .with_params(JointParams::BallJoint(BallJoint {
                    x_limits_enabled: true,
                    x_limits_angles: range(limits.x),
                    y_limits_enabled: true,
                    y_limits_angles: range(limits.y),
                    z_limits_enabled: true,
                    z_limits_angles: range(limits.z),
                }))
                .with_body1(limbs[parent].body)
                .with_body2(rigid_body)
                .with_contacts_enabled(false)
                .with_auto_rebinding_enabled(false)
                .build(graph);
                if let Ok(joint) = graph.try_get_mut_of_type::<Joint>(handle.to_base()) {
                    // Some give in every joint, so it does not flail.
                    let _ = joint.set_motor_resistive_torque_as_ball(
                        0.0,
                        physics.joint_max_torque,
                        physics.joint_damping,
                    );
                }
                held = Some(handle);
            }
            limbs.push(Limb {
                name: body.name.as_str(),
                bone: bones[i],
                body: rigid_body,
                colliders,
                joint: held,
            });
        }
        Some(Self {
            root,
            limbs,
            frames: 0,
            report: crate::platform::var("MAZE_KNOCKDOWN").is_some(),
            state: State::Joining {
                frames: BIND_FRAMES,
                velocity,
                blow,
            },
        })
    }

    /// Whether it has been let go, and has the droid's bones.
    pub fn is_limp(&self) -> bool {
        self.state == State::Limp
    }

    /// Where the pelvis is, across the world.
    pub fn pelvis(&self, graph: &Graph) -> Option<Vector3<f32>> {
        let limb = self.limbs.first()?;
        Some(
            graph
                .try_get_of_type::<RigidBody>(limb.body.to_base())
                .ok()?
                .global_position(),
        )
    }

    /// Whether `collider` is one of its bodies'.
    pub fn owns(&self, collider: Handle<Collider>) -> bool {
        self.limbs.iter().any(|limb| limb.colliders.contains(&collider))
    }

    /// The name of the body `collider` belongs to, if one of its bodies'.
    pub fn body_of(&self, collider: Handle<Collider>) -> Option<&'static str> {
        self.limbs.iter().find(|limb| limb.colliders.contains(&collider)).map(|limb| limb.name)
    }

    /// The body a bolt going `way` that struck the droid at `at` went into: the first it goes
    /// through, within a meter on from where it struck - it may have struck the droid's capsule,
    /// round the outside of it - or failing that the one it passes nearest, if within
    /// [`NEAR_MISS`]. The bodies are measured where the bones are, whether it has been let go yet
    /// or not.
    pub fn body_struck(&self, graph: &Graph, at: Vector3<f32>, way: Vector3<f32>) -> Option<&'static str> {
        let spec = spec()?;
        let (from, to) = (at - way * 0.3, at + way * 1.0);
        let mut through: Option<(f32, &'static str)> = None;
        let mut nearest: Option<(f32, &'static str)> = None;
        for (limb, body) in self.limbs.iter().zip(&spec.bodies) {
            let pose = Pose::of_transform(&graph[limb.bone].global_transform());
            let place = |v: [f32; 3]| pose.position + pose.rotation * (vector(v) * SCALE);
            for shape in &body.colliders {
                let (begin, end, radius) = match *shape {
                    Shape::Capsule { begin, end, radius } => (place(begin), place(end), radius * SCALE),
                    Shape::Cuboid { position, half_extents, .. } => {
                        let middle = place(position);
                        let [x, y, z] = half_extents;
                        (middle, middle, (x + y + z) / 3.0 * SCALE)
                    }
                };
                let (along, gap) = closest(from, to, begin, end);
                let gap = gap - radius;
                if gap <= 0.0 && through.is_none_or(|(t, _)| along < t) {
                    through = Some((along, limb.name));
                }
                if nearest.is_none_or(|(g, _)| gap < g) {
                    nearest = Some((gap, limb.name));
                }
            }
        }
        through
            .or(nearest.filter(|&(gap, _)| gap <= NEAR_MISS))
            .map(|(_, name)| name)
    }

    /// Lets the body called `name` loose of the one it hangs off, to fall on its own. Whether it
    /// was held.
    pub fn let_loose(&mut self, graph: &mut Graph, name: &str) -> bool {
        let Some(joint) = self.limbs.iter_mut().find(|limb| limb.name == name).and_then(|limb| limb.joint.take())
        else {
            return false;
        };
        if graph.is_valid_handle(joint) {
            graph.remove_node(joint);
        }
        true
    }

    /// Shoves the body that `collider` belongs to with `impulse` at `point`.
    pub fn shove(&self, graph: &mut Graph, collider: Handle<Collider>, impulse: Vector3<f32>, point: Vector3<f32>) {
        let Some(limb) = self.limbs.iter().find(|limb| limb.colliders.contains(&collider)) else {
            return;
        };
        if let Ok(body) = graph.try_get_mut_of_type::<RigidBody>(limb.body.to_base()) {
            body.apply_impulse_at_point(impulse, point);
            body.wake_up();
        }
    }

    /// Carries on for another frame: waits for the engine to join the bodies and then lets them
    /// go; or, gone limp, puts the droid's bones where its bodies are.
    pub fn update(&mut self, graph: &mut Graph) {
        match self.state {
            State::Joining { frames, velocity, blow } if frames > 0 => {
                self.state = State::Joining {
                    frames: frames - 1,
                    velocity,
                    blow,
                };
            }
            State::Joining { velocity, blow, .. } => {
                self.let_go(graph, velocity, blow);
                self.state = State::Limp;
            }
            State::Limp => self.follow(graph),
        }
    }

    /// Puts every body where its bone is now, going at `velocity`, knocks the one nearest the
    /// `blow` with it, and lets them all fall.
    fn let_go(
        &self,
        graph: &mut Graph,
        velocity: Vector3<f32>,
        blow: Option<(Vector3<f32>, Vector3<f32>)>,
    ) {
        let poses: Vec<Pose> = self
            .limbs
            .iter()
            .map(|limb| Pose::of_transform(&graph[limb.bone].global_transform()))
            .collect();
        let struck = blow.and_then(|(_, at)| {
            (0..poses.len()).min_by(|&a, &b| {
                let distance = |i: usize| (poses[i].position - at).norm();
                distance(a).total_cmp(&distance(b))
            })
        });
        for (i, (limb, pose)) in self.limbs.iter().zip(poses).enumerate() {
            for &collider in &limb.colliders {
                if let Ok(collider) = graph.try_get_mut(collider) {
                    collider.set_is_sensor(false);
                }
            }
            if let Ok(body) = graph.try_get_mut_of_type::<RigidBody>(limb.body.to_base()) {
                body.local_transform_mut()
                    .set_position(pose.position)
                    .set_rotation(pose.rotation);
                body.set_body_type(RigidBodyType::Dynamic);
                body.set_lin_vel(velocity);
                body.set_ang_vel(Vector3::zeros());
                if let (Some((impulse, at)), true) = (blow, struck == Some(i)) {
                    body.apply_impulse_at_point(impulse, at);
                }
                body.wake_up();
            }
        }
    }

    /// Puts each bone where its body is. Every bone's parent is worked out along the way from the
    /// model's root, as the bones above it have just been put, since the engine only works out
    /// where everything is after the frame.
    fn follow(&mut self, graph: &mut Graph) {
        let mut placed: Vec<(Handle<Node>, Matrix4<f32>)> = Vec::with_capacity(self.limbs.len());
        for limb in &self.limbs {
            let Ok(body) = graph.try_get_of_type::<RigidBody>(limb.body.to_base()) else {
                continue;
            };
            let target = Pose::of_transform(&body.global_transform()).transform();
            let parent = graph[limb.bone].parent();
            let parent_transform = self.global(graph, parent, &placed);
            let local = parent_transform.try_inverse().unwrap_or_else(Matrix4::identity) * target;
            let local_pose = Pose::of_transform(&local);
            graph[limb.bone]
                .local_transform_mut()
                .set_position(local_pose.position)
                .set_rotation(local_pose.rotation);
            placed.push((limb.bone, target));
        }
        self.frames += 1;
        if self.report && self.frames % 30 == 1 {
            self.log(graph);
        }
    }

    /// Logs how the fall is going, for MAZE_KNOCKDOWN: how high the pelvis is, and how far
    /// apart the widest-stretched joint has come.
    fn log(&self, graph: &Graph) {
        let Some(spec) = spec() else { return };
        let pose = |i: usize| {
            graph
                .try_get_of_type::<RigidBody>(self.limbs[i].body.to_base())
                .map(|body| Pose::of_transform(&body.global_transform()))
                .ok()
        };
        let mut widest = (0.0f32, "");
        for (i, body) in spec.bodies.iter().enumerate() {
            let (Some(parent), Some(joint)) = (body.parent.as_deref(), &body.joint) else { continue };
            let Some(p) = spec.bodies.iter().position(|b| b.name == parent) else { continue };
            let (Some(a), Some(b)) = (pose(p), pose(i)) else { continue };
            let gap = (a.then(Pose::of(&joint.frame_in_parent)).position
                - b.then(Pose::of(&joint.frame_in_body)).position)
                .norm();
            if gap > widest.0 {
                widest = (gap, &body.name);
            }
        }
        let height = pose(0).map_or(f32::NAN, |p| p.position.y);
        Log::info(format!(
            "Ragdoll: {:.1} s, pelvis {:.2} m up, widest joint {:.0} mm ({})",
            self.frames as f32 / 60.0,
            height,
            widest.0 * 1000.0,
            widest.1
        ));
    }

    /// Where `node` is across the world, going by the bones `placed` this frame, and otherwise
    /// by each bone's own transform on top of its parent's, up to the model's root.
    fn global(&self, graph: &Graph, node: Handle<Node>, placed: &[(Handle<Node>, Matrix4<f32>)]) -> Matrix4<f32> {
        if let Some((_, transform)) = placed.iter().find(|(bone, _)| *bone == node) {
            return *transform;
        }
        if node == self.root || node.is_none() {
            return graph.try_get(node).map_or_else(|_| Matrix4::identity(), |n| n.global_transform());
        }
        let parent = graph[node].parent();
        self.global(graph, parent, placed) * graph[node].local_transform().matrix()
    }

    /// Takes its bodies and joints out of the scene.
    pub fn remove(&self, graph: &mut Graph) {
        for joint in self.limbs.iter().filter_map(|limb| limb.joint) {
            if graph.is_valid_handle(joint) {
                graph.remove_node(joint);
            }
        }
        for limb in &self.limbs {
            if graph.is_valid_handle(limb.body) {
                graph.remove_node(limb.body);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_come_closest_where_they_cross_or_at_their_ends() {
        let (along, gap) = closest(
            Vector3::new(-1.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.5, -1.0, 0.2),
            Vector3::new(0.5, 1.0, 0.2),
        );
        assert!((along - 0.75).abs() < 1.0e-5 && (gap - 0.2).abs() < 1.0e-5, "{along} {gap}");
        let (along, gap) = closest(
            Vector3::zeros(),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(3.0, 1.0, 0.0),
            Vector3::new(3.0, 1.0, 0.0),
        );
        assert!((along - 1.0).abs() < 1.0e-5 && (gap - 5.0f32.sqrt()).abs() < 1.0e-5);
    }

    #[test]
    fn a_pose_undone_is_where_it_started() {
        let pose = Pose {
            position: Vector3::new(1.0, 2.0, 3.0),
            rotation: UnitQuaternion::from_euler_angles(0.3, -0.2, 0.9),
        };
        let back = pose.then(pose.inverse());
        assert!(back.position.norm() < 1.0e-5);
        assert!(back.rotation.angle() < 1.0e-5);
    }

    #[test]
    fn a_bones_transform_gives_its_pose_back_without_the_scale() {
        let pose = Pose {
            position: Vector3::new(-0.4, 1.1, 0.2),
            rotation: UnitQuaternion::from_euler_angles(1.2, 0.4, -0.7),
        };
        let back = Pose::of_transform(&pose.transform());
        assert!((back.position - pose.position).norm() < 1.0e-5);
        assert!(back.rotation.angle_to(&pose.rotation) < 1.0e-4);
    }

    #[test]
    fn the_models_ragdoll_is_read_and_each_body_follows_the_one_it_hangs_off() {
        let spec = spec().expect("the ragdoll in droid_motion.json");
        assert_eq!(spec.bodies.len(), 18);
        for (i, body) in spec.bodies.iter().enumerate() {
            if let Some(parent) = &body.parent {
                let p = spec.bodies.iter().position(|b| &b.name == parent).expect("its parent");
                assert!(p < i, "{} comes before {}", parent, body.name);
                assert!(body.joint.is_some());
            }
            assert!(body.collision_bit < 31, "clear of the capsules' group");
        }
    }
}
