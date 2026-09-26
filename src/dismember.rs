//! A droid coming apart: its head, an arm or a leg broken off where a bolt hits it, as it goes
//! down or lying there - no blood, just the broken shell and the glowing voxels inside it.
//!
//! [`MOTION`]'s `dismember` has the model's parts, made along with it. Besides the whole body in
//! one piece, the model carries the body again cut into pieces - the torso, the head, and each
//! side's upper arm, forearm, thigh and shin - and at each of the nine breaks between them two
//! broken ends, one for the body's side and one for the part that comes off. All of those are out
//! of sight until something breaks: then the whole body gives way to the pieces, which put
//! together look just as it did, and the break's two ends show.
//!
//! The skin across a break is bent by the bones either side of it, so a part flying off would
//! drag the skin of the body after it, and the body's the part's. Breaking, each side's skin is
//! tied to its own half: on the body's side the bones of the part that comes off are swapped for
//! stand-ins fixed where those bones were at that moment to the bone the part hung off, and on the
//! part the bones of the body for stand-ins fixed to the part. The ragdoll lets the part's body go
//! for the physics to carry off (see [`crate::ragdoll::Ragdoll::let_loose`]).
//!
//! As it breaks, loose voxels spill out of both ends - little glowing cubes, as big as the voxels
//! in the ends, as many as [`MOTION`] says - tumbling out either way along the bone and bouncing
//! off the floor, until they are swept up with the droid.

use crate::fixtures::glow_strength;
use crate::player::avatar::{MOTION, SCALE};
use crate::ragdoll::CHARACTERS;
use fyrox::{
    core::{
        algebra::{Matrix3, Matrix4, UnitQuaternion, Vector3},
        log::Log,
        math::Matrix4Ext,
        pool::Handle,
    },
    fxhash::{FxHashMap, FxHashSet},
    graph::SceneGraph,
    material::MaterialResource,
    scene::{
        base::BaseBuilder,
        collider::{BitMask, ColliderBuilder, ColliderShape, InteractionGroups},
        graph::Graph,
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            Mesh, MeshBuilder,
        },
        node::Node,
        pivot::PivotBuilder,
        rigidbody::RigidBodyBuilder,
        transform::TransformBuilder,
    },
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Motion {
    #[serde(default)]
    dismember: Option<Spec>,
}

/// The parts as [`MOTION`] has them: nodes and bones by name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Spec {
    /// The whole body in one piece.
    intact: String,
    pieces: HashMap<String, PartSpec>,
    ends: HashMap<String, PartSpec>,
    breaks: Vec<BreakSpec>,
    /// The break a hit on a ragdoll body that has none of its own goes to: a hand to the elbow,
    /// say.
    hit_body: HashMap<String, String>,
}

/// A mesh, and the bone it goes with: which side of a break it is on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct PartSpec {
    node: String,
    bone: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct BreakSpec {
    /// The ragdoll body that comes off, and the bone at the top of it, that everything that comes
    /// off hangs off.
    body: String,
    bone: String,
    /// The broken ends on the body's side and on the part's.
    stump: String,
    end: String,
    /// How big the voxels are, in the model's own meters, and how many spill out.
    voxel: f32,
    spill: usize,
}

/// What [`MOTION`] has for coming apart, read once for every droid; none if it has none.
fn spec() -> Option<&'static Spec> {
    static SPEC: std::sync::OnceLock<Option<Spec>> = std::sync::OnceLock::new();
    SPEC.get_or_init(|| {
        let read = crate::platform::read_to_string(MOTION);
        match read.and_then(|text| serde_json::from_str::<Motion>(&text).map_err(|e| e.to_string())) {
            Ok(Motion { dismember: Some(spec) }) => Some(spec),
            Ok(_) => {
                Log::warn(format!("Dismember: {MOTION} has none; droids stay in one piece"));
                None
            }
            Err(error) => {
                Log::err(format!("Dismember: could not read {MOTION}: {error}"));
                None
            }
        }
    })
    .as_ref()
}

/// The ragdoll body whose break a hit on `body` breaks, if a hit there breaks anything.
pub fn break_for(body: &str) -> Option<&'static str> {
    let spec = spec()?;
    let body = spec.hit_body.get(body).map_or(body, String::as_str);
    spec.breaks.iter().find(|b| b.body == body).map(|b| b.body.as_str())
}

/// A piece of the body, or a broken end: its mesh, and the bone it goes with.
#[derive(Debug, Clone, PartialEq)]
struct Part {
    node: Handle<Node>,
    bone: Handle<Node>,
    end: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Break {
    body: &'static str,
    bone: Handle<Node>,
    stump: Handle<Node>,
    end: Handle<Node>,
    voxel: f32,
    spill: usize,
    broken: bool,
}

/// The collision group of the loose voxels, which bump into the maze and the droids lying in it,
/// but not into each other, nor anyone's capsule.
pub const SPILL: u32 = 1 << 30;
/// How fast the loose voxels come out, in meters a second, slowest and fastest; how fast they
/// tumble at most, in radians a second; and how heavy they are, in kilograms a cubic meter.
const SPILL_SPEED: (f32, f32) = (0.6, 2.2);
const SPILL_SPIN: f32 = 12.0;
const SPILL_DENSITY: f32 = 1500.0;

/// One droid's parts, whole or come apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Dismember {
    intact: Handle<Node>,
    parts: Vec<Part>,
    breaks: Vec<Break>,
    /// Whether the pieces show in place of the whole body.
    apart: bool,
    /// The ends' glowing materials, which the loose voxels are made of; the loose voxels spilt
    /// so far; and where the voxels' dice are, which fall differently for each droid.
    glows: Vec<MaterialResource>,
    spilt: Vec<Handle<Node>>,
    dice: u64,
}

impl Dismember {
    /// Finds the parts of the droid whose model's root is `root`, and puts them out of sight.
    /// None if the model has none, or not all of them.
    pub fn new(graph: &mut Graph, root: Handle<Node>) -> Option<Self> {
        let spec = spec()?;
        let find = |name: &str| graph.find_by_name(root, name).map(|(node, _)| node);
        let part = |spec: &PartSpec, end: bool| {
            Some(Part {
                node: find(&spec.node)?,
                bone: find(&spec.bone)?,
                end,
            })
        };
        let parts: Option<Vec<Part>> = spec
            .pieces
            .values()
            .map(|p| part(p, false))
            .chain(spec.ends.values().map(|p| part(p, true)))
            .collect();
        let breaks: Option<Vec<Break>> = spec
            .breaks
            .iter()
            .map(|b| {
                Some(Break {
                    body: b.body.as_str(),
                    bone: find(&b.bone)?,
                    stump: find(&b.stump)?,
                    end: find(&b.end)?,
                    voxel: b.voxel,
                    spill: b.spill,
                    broken: false,
                })
            })
            .collect();
        let intact = find(&spec.intact);
        // Whatever of them there is, out of sight, so that the whole body is not doubled.
        let every: Vec<Handle<Node>> = spec
            .pieces
            .values()
            .chain(spec.ends.values())
            .filter_map(|p| find(&p.node))
            .collect();
        for node in every {
            graph[node].set_visibility(false);
        }
        let (Some(intact), Some(parts), Some(breaks)) = (intact, parts, breaks) else {
            Log::err("Dismember: the droid's model is missing some of its parts; it stays in one piece");
            return None;
        };
        // The ends' glowing materials, each once.
        let mut glows: Vec<MaterialResource> = Vec::new();
        for part in parts.iter().filter(|part| part.end) {
            let Some(mesh) = graph[part.node].cast::<Mesh>() else { continue };
            for surface in mesh.surfaces() {
                let material = surface.material();
                let glowing = material.state().data_ref().is_some_and(|m| glow_strength(m).is_some());
                if glowing && !glows.iter().any(|g| g.key() == material.key()) {
                    glows.push(material.clone());
                }
            }
        }
        let dice = 0x9e37_79b9_7f4a_7c15 ^ u64::from(root.index());
        Some(Self { intact, parts, breaks, apart: false, glows, spilt: Vec::new(), dice })
    }

    /// Breaks off the part the ragdoll body `body` carries, if it has a break and it has not come
    /// off already: shows the broken ends, and ties each side's skin to its own half. Whether it
    /// came off.
    pub fn break_off(&mut self, graph: &mut Graph, body: &str) -> bool {
        let Some(at) = self.breaks.iter().position(|b| b.body == body && !b.broken) else {
            return false;
        };
        self.breaks[at].broken = true;
        let Break { bone, stump, end, .. } = self.breaks[at].clone();
        if !self.apart {
            self.apart = true;
            graph[self.intact].set_visibility(false);
            for part in self.parts.iter().filter(|part| !part.end) {
                graph[part.node].set_visibility(true);
            }
        }
        graph[stump].set_visibility(true);
        graph[end].set_visibility(true);
        self.spill(graph, at);

        let off: FxHashSet<Handle<Node>> = graph.traverse_handle_iter(bone).collect();
        let above = graph[bone].parent();
        let mut stand_ins = FxHashMap::default();
        for part in &self.parts {
            let coming_off = off.contains(&part.bone);
            // On the part coming off, the body's bones go; on the body, the part's.
            let anchor = if coming_off { bone } else { above };
            let Some(mesh) = graph[part.node].cast::<Mesh>() else {
                continue;
            };
            let surfaces: Vec<Vec<Handle<Node>>> =
                mesh.surfaces().iter().map(|surface| surface.bones().to_vec()).collect();
            let tied: Vec<Vec<Handle<Node>>> = surfaces
                .into_iter()
                .map(|bones| {
                    bones
                        .into_iter()
                        .map(|b| {
                            if off.contains(&b) == coming_off {
                                b
                            } else {
                                stand_in(graph, &mut stand_ins, b, anchor)
                            }
                        })
                        .collect()
                })
                .collect();
            if let Some(mesh) = graph[part.node].cast_mut::<Mesh>() {
                for (surface, bones) in mesh.surfaces_mut().iter_mut().zip(tied) {
                    surface.bones.set_value_and_mark_modified(bones);
                }
            }
        }
        true
    }
}

impl Dismember {
    /// A number from 0 to 1, from the voxels' dice.
    fn roll(&mut self) -> f32 {
        // xorshift64*
        self.dice ^= self.dice >> 12;
        self.dice ^= self.dice << 25;
        self.dice ^= self.dice >> 27;
        (self.dice.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A way at random, one meter long.
    fn any_way(&mut self) -> Vector3<f32> {
        loop {
            let v = Vector3::new(self.roll(), self.roll(), self.roll()) * 2.0 - Vector3::repeat(1.0);
            let length = v.norm();
            if length > 0.05 && length <= 1.0 {
                return v / length;
            }
        }
    }

    /// Spills loose voxels out of both ends of the `at`th break, half each way along its bone.
    fn spill(&mut self, graph: &mut Graph, at: usize) {
        if self.glows.is_empty() {
            return;
        }
        let Break { bone, voxel, spill, .. } = self.breaks[at].clone();
        let transform = graph[bone].global_transform();
        let from = transform.position();
        let along = transform.basis().column(1).try_normalize(1.0e-6).unwrap_or_else(Vector3::y);
        let size = voxel * SCALE;
        let groups = InteractionGroups::new(BitMask(SPILL), BitMask(!(CHARACTERS | SPILL)));
        for k in 0..spill {
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let way = (along * side + self.any_way() * 0.7).try_normalize(1.0e-6).unwrap_or(along);
            let speed = SPILL_SPEED.0 + (SPILL_SPEED.1 - SPILL_SPEED.0) * self.roll();
            let spin = self.any_way() * SPILL_SPIN * self.roll();
            let place = from + way * (size * 1.5) + self.any_way() * (size * self.roll());
            let turned = UnitQuaternion::from_scaled_axis(self.any_way() * std::f32::consts::PI * self.roll());
            let material = self.glows[k % self.glows.len()].clone();
            let cube = MeshBuilder::new(BaseBuilder::new().with_cast_shadows(false))
                .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(
                    SurfaceData::make_cube(Matrix4::new_scaling(size)),
                ))
                .with_material(material)
                .build()])
                .build(graph);
            let half = 0.5 * size;
            let collider = ColliderBuilder::new(BaseBuilder::new())
                .with_shape(ColliderShape::cuboid(half, half, half))
                .with_density(Some(SPILL_DENSITY))
                .with_friction(0.6)
                .with_restitution(0.3)
                .with_collision_groups(groups)
                .build(graph);
            let body = RigidBodyBuilder::new(
                BaseBuilder::new()
                    .with_name("spilt voxel")
                    .with_child(cube)
                    .with_child(collider)
                    .with_local_transform(
                        TransformBuilder::new()
                            .with_local_position(place)
                            .with_local_rotation(turned)
                            .build(),
                    ),
            )
            // Its mass, and how hard it is to turn, come from its collider.
            .with_mass(0.0)
            .with_lin_vel(way * speed)
            .with_ang_vel(spin)
            .with_ccd_enabled(true)
            .build(graph);
            self.spilt.push(body.to_base());
        }
    }

    /// Takes every loose voxel spilt so far out of the scene.
    pub fn sweep_up(&mut self, graph: &mut Graph) {
        for voxel in self.spilt.drain(..) {
            if graph.is_valid_handle(voxel) {
                graph.remove_node(voxel);
            }
        }
    }
}

/// A stand-in for `bone`, fixed to `anchor` where the bone is now: it bends the skin as the bone
/// did at this moment, and from then on moves only with the anchor. One for each bone and anchor.
fn stand_in(
    graph: &mut Graph,
    stand_ins: &mut FxHashMap<(Handle<Node>, Handle<Node>), Handle<Node>>,
    bone: Handle<Node>,
    anchor: Handle<Node>,
) -> Handle<Node> {
    if let Some(&node) = stand_ins.get(&(bone, anchor)) {
        return node;
    }
    let local = graph[anchor].global_transform().try_inverse().unwrap_or_else(Matrix4::identity)
        * graph[bone].global_transform();
    let basis = local.basis();
    let scale = Vector3::new(basis.column(0).norm(), basis.column(1).norm(), basis.column(2).norm());
    let unscaled = Matrix3::from_columns(&[
        basis.column(0) / scale.x,
        basis.column(1) / scale.y,
        basis.column(2) / scale.z,
    ]);
    let rotation = UnitQuaternion::from_matrix_eps(&unscaled, f32::EPSILON, 16, UnitQuaternion::identity());
    let name = format!("{} stand-in", graph[bone].name());
    let node = PivotBuilder::new(
        BaseBuilder::new()
            .with_name(name)
            .with_inv_bind_pose_transform(graph[bone].inv_bind_pose_transform())
            .with_local_transform(
                TransformBuilder::new()
                    .with_local_position(local.position())
                    .with_local_rotation(rotation)
                    .with_local_scale(scale)
                    .build(),
            ),
    )
    .build(graph)
    .to_base();
    graph.link_nodes(node, anchor);
    stand_ins.insert((bone, anchor), node);
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_breaks_are_read_and_a_hand_breaks_at_the_elbow() {
        let spec = spec().expect("the dismemberment in droid_motion.json");
        assert_eq!(spec.breaks.len(), 9);
        assert_eq!(spec.pieces.len(), 10);
        assert_eq!(spec.ends.len(), 18);
        assert_eq!(break_for("head"), Some("head"));
        assert_eq!(break_for("hand.L"), Some("forearm.L"));
        assert_eq!(break_for("foot.R"), Some("shin.R"));
        assert_eq!(break_for("chest"), None);
        assert_eq!(break_for("clavicle.L"), None);
    }
}
