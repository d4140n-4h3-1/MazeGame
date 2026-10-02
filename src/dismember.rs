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
//! As it breaks, loose voxels spill out of both ends - little glowing tetrahedra, like the voxels
//! in the ends and as big, as many as [`MOTION`] says - tumbling out either way along the bone and
//! bouncing off the floor, until they are swept up with the droid. They are not bodies for the
//! physics: a hundred small fast bodies kept from passing through the floor cost it a frame's time
//! many times over. Each is moved here instead, and feels its way along with a ray, which is as
//! sure of the floor and costs next to nothing (see [`Dismember::fly`]).

use crate::fixtures::glow_strength;
use crate::player::avatar::{MOTION, SCALE};
use crate::ragdoll::CHARACTERS;
use fyrox::{
    core::{
        algebra::{Matrix3, Matrix4, Point3, UnitQuaternion, Vector2, Vector3},
        log::Log,
        math::Matrix4Ext,
        pool::Handle,
    },
    fxhash::{FxHashMap, FxHashSet},
    graph::SceneGraph,
    material::MaterialResource,
    scene::{
        base::BaseBuilder,
        collider::{BitMask, InteractionGroups},
        graph::{physics::RayCastOptions, Graph},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            vertex::StaticVertex,
            Mesh, MeshBuilder,
        },
        node::Node,
        pivot::PivotBuilder,
        transform::TransformBuilder,
    },
    utils::raw_mesh::RawMeshBuilder,
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
        match read.and_then(|text| serde_json::from_str::<Motion>(&text).map_err(|e| e.to_string()))
        {
            Ok(Motion {
                dismember: Some(spec),
            }) => Some(spec),
            Ok(_) => {
                Log::warn(format!(
                    "Dismember: {MOTION} has none; droids stay in one piece"
                ));
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
    spec.breaks
        .iter()
        .find(|b| b.body == body)
        .map(|b| b.body.as_str())
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
    /// Whether the part that came off has been blown apart, and gone.
    blown: bool,
}

/// How fast the loose voxels come out, in meters a second, slowest and fastest; and how fast they
/// tumble at most, in radians a second.
const SPILL_SPEED: (f32, f32) = (0.6, 2.2);
const SPILL_SPIN: f32 = 12.0;
/// A loose voxel striking something: how much of its speed into it it bounces back with, how much
/// of its speed along it it keeps, and how much of its tumbling; and slower than this, in meters a
/// second, on something it can lie on - facing this much up - it comes to rest.
const BOUNCE: f32 = 0.3;
const GLANCE: f32 = 0.6;
const BOUNCE_SPIN: f32 = 0.5;
const REST_SPEED: f32 = 0.4;
const LIE_ON: f32 = 0.7;
const GRAVITY: f32 = 9.81;
/// A loose voxel that has fallen this far below where it came out - out of the maze - is let be.
const LOST: f32 = 50.0;
/// A part blown apart bursts into this many voxels, up to this many times as big across as the
/// break's, flying out from its middle this fast, in meters a second, slowest and fastest, and
/// carried on along the bolt by up to this much of its speed.
const BURST: usize = 90;
const BURST_GROWTH: f32 = 2.2;
const BURST_SPEED: (f32, f32) = (1.5, 5.5);
const BURST_CARRY: f32 = 0.6;

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
    spilt: Vec<Voxel>,
    dice: u64,
}

/// A loose voxel: its mesh, how fast it is going and tumbling, how far its corners reach from its
/// middle, the height it came out at, and whether it has come to rest.
#[derive(Debug, Clone, PartialEq)]
struct Voxel {
    node: Handle<Node>,
    velocity: Vector3<f32>,
    spin: Vector3<f32>,
    reach: f32,
    from: f32,
    resting: bool,
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
                    blown: false,
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
            Log::err(
                "Dismember: the droid's model is missing some of its parts; it stays in one piece",
            );
            return None;
        };
        // The ends' glowing materials, each once.
        let mut glows: Vec<MaterialResource> = Vec::new();
        for part in parts.iter().filter(|part| part.end) {
            let Some(mesh) = graph[part.node].cast::<Mesh>() else {
                continue;
            };
            for surface in mesh.surfaces() {
                let material = surface.material();
                let glowing = material
                    .state()
                    .data_ref()
                    .is_some_and(|m| glow_strength(m).is_some());
                if glowing && !glows.iter().any(|g| g.key() == material.key()) {
                    glows.push(material.clone());
                }
            }
        }
        let dice = 0x9e37_79b9_7f4a_7c15 ^ u64::from(root.index());
        Some(Self {
            intact,
            parts,
            breaks,
            apart: false,
            glows,
            spilt: Vec::new(),
            dice,
        })
    }

    /// Breaks off the part the ragdoll body `body` carries, if it has a break and it has not come
    /// off already: shows the broken ends, and ties each side's skin to its own half. Whether it
    /// came off.
    pub fn break_off(&mut self, graph: &mut Graph, body: &str) -> bool {
        let Some(at) = self.breaks.iter().position(|b| b.body == body && !b.broken) else {
            return false;
        };
        self.breaks[at].broken = true;
        let Break {
            bone, stump, end, ..
        } = self.breaks[at].clone();
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
            let surfaces: Vec<Vec<Handle<Node>>> = mesh
                .surfaces()
                .iter()
                .map(|surface| surface.bones().to_vec())
                .collect();
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

    /// Blows the part the ragdoll body `body` carries apart, struck by a bolt going `way`: it
    /// breaks off if it has not already, then is gone - its pieces, its broken end and whatever
    /// rides its bones, the eyes in a head - and bursts into glowing voxels flying out from
    /// `middle`, all through `radius` of it. Whether it was there to blow apart.
    pub fn blow_up(
        &mut self,
        graph: &mut Graph,
        body: &str,
        middle: Vector3<f32>,
        radius: f32,
        way: Vector3<f32>,
    ) -> bool {
        let Some(at) = self.breaks.iter().position(|b| b.body == body && !b.blown) else {
            return false;
        };
        if !self.breaks[at].broken {
            self.break_off(graph, body);
        }
        self.breaks[at].blown = true;
        let Break { bone, end, .. } = self.breaks[at].clone();
        let off: FxHashSet<Handle<Node>> = graph.traverse_handle_iter(bone).collect();
        for part in self.parts.iter().filter(|part| off.contains(&part.bone)) {
            graph[part.node].set_visibility(false);
        }
        graph[end].set_visibility(false);
        // And everything that rides its bones, the eyes in a head.
        for node in off {
            graph[node].set_visibility(false);
        }
        self.burst(graph, at, middle, radius, way);
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
            let v =
                Vector3::new(self.roll(), self.roll(), self.roll()) * 2.0 - Vector3::repeat(1.0);
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
        let Break {
            bone, voxel, spill, ..
        } = self.breaks[at].clone();
        let transform = graph[bone].global_transform();
        let from = transform.position();
        let along = transform
            .basis()
            .column(1)
            .try_normalize(1.0e-6)
            .unwrap_or_else(Vector3::y);
        let size = voxel * SCALE;
        for k in 0..spill {
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let way = (along * side + self.any_way() * 0.7)
                .try_normalize(1.0e-6)
                .unwrap_or(along);
            let speed = SPILL_SPEED.0 + (SPILL_SPEED.1 - SPILL_SPEED.0) * self.roll();
            let place = from + way * (size * 1.5) + self.any_way() * (size * self.roll());
            self.voxel(graph, k, place, size, way * speed);
        }
    }

    /// Bursts the part of the `at`th break into [`BURST`] voxels, all through `radius` of
    /// `middle`, flying out from it and on along the bolt's `way`.
    fn burst(
        &mut self,
        graph: &mut Graph,
        at: usize,
        middle: Vector3<f32>,
        radius: f32,
        way: Vector3<f32>,
    ) {
        if self.glows.is_empty() {
            return;
        }
        let voxel = self.breaks[at].voxel * SCALE;
        for k in 0..BURST {
            let out = self.any_way();
            // Evenly through the ball, not bunched at its middle.
            let place = middle + out * (radius * self.roll().cbrt());
            let speed = BURST_SPEED.0 + (BURST_SPEED.1 - BURST_SPEED.0) * self.roll();
            let carry = way * (speed * BURST_CARRY * self.roll());
            let size = voxel * (1.0 + (BURST_GROWTH - 1.0) * self.roll());
            self.voxel(graph, k, place, size, out * speed + carry);
        }
    }

    /// A loose glowing voxel `size` across at `place`, the `k`th material of the ends' glows,
    /// flying off at `velocity` and tumbling, swept up with the droid.
    fn voxel(
        &mut self,
        graph: &mut Graph,
        k: usize,
        place: Vector3<f32>,
        size: f32,
        velocity: Vector3<f32>,
    ) {
        let spin = self.any_way() * SPILL_SPIN * self.roll();
        let turned =
            UnitQuaternion::from_scaled_axis(self.any_way() * std::f32::consts::PI * self.roll());
        let material = self.glows[k % self.glows.len()].clone();
        self.spilt.push(Voxel {
            node: spilt_voxel(graph, material, size, place, turned),
            velocity,
            spin,
            reach: 0.5 * 3.0f32.sqrt() * size,
            from: place.y,
            resting: false,
        });
    }

    /// Moves the loose voxels along for another `dt`: falling, tumbling, and bouncing off
    /// whatever their way runs into - the maze and the droids lying in it, but no one's capsule -
    /// until they come to rest on something. One ray each, from its middle along the way it goes
    /// this frame and as far again as its corners reach, so it never passes through a floor or a
    /// wall however fast it goes. Those resting feel below them for what they lie on, and fall
    /// again if it has gone.
    pub fn fly(&mut self, graph: &mut Graph, dt: f32) {
        let groups = InteractionGroups::new(BitMask(u32::MAX), BitMask(!CHARACTERS));
        let mut hits = Vec::new();
        for voxel in &mut self.spilt {
            let Ok(node) = graph.try_get(voxel.node) else {
                continue;
            };
            // Its own place: it hangs off nothing.
            let at = **node.local_transform().position();
            if at.y < voxel.from - LOST {
                continue;
            }
            let ray = |graph: &Graph, way: Vector3<f32>, length: f32, hits: &mut Vec<_>| {
                hits.clear();
                graph.physics.cast_ray(
                    RayCastOptions {
                        ray_origin: Point3::from(at),
                        ray_direction: way,
                        max_len: length,
                        groups,
                        sort_results: true,
                    },
                    hits,
                );
                // Not what it is inside - it comes out of a droid's body - only what it meets.
                hits.iter().find(|hit| hit.toi > 0.0).cloned()
            };
            if voxel.resting {
                if ray(graph, -Vector3::y(), 1.5 * voxel.reach, &mut hits).is_some() {
                    continue;
                }
                voxel.resting = false;
            }
            voxel.velocity.y -= GRAVITY * dt;
            let step = voxel.velocity * dt;
            let length = step.norm();
            let mut next = at + step;
            if length > 1.0e-6 {
                if let Some(hit) = ray(graph, step / length, length + voxel.reach, &mut hits) {
                    let normal = hit.normal.try_normalize(1.0e-6).unwrap_or_else(Vector3::y);
                    next = hit.position.coords + normal * voxel.reach;
                    let into = voxel.velocity.dot(&normal);
                    if into < 0.0 {
                        let along = voxel.velocity - normal * into;
                        voxel.velocity = along * GLANCE - normal * (into * BOUNCE);
                        voxel.spin *= BOUNCE_SPIN;
                    }
                    if normal.y > LIE_ON && voxel.velocity.norm() < REST_SPEED {
                        voxel.velocity = Vector3::zeros();
                        voxel.spin = Vector3::zeros();
                        voxel.resting = true;
                    }
                }
            }
            let turned = UnitQuaternion::from_scaled_axis(voxel.spin * dt)
                * **node.local_transform().rotation();
            graph[voxel.node]
                .local_transform_mut()
                .set_position(next)
                .set_rotation(turned);
        }
    }

    /// Takes every loose voxel spilt so far out of the scene.
    pub fn sweep_up(&mut self, graph: &mut Graph) {
        for voxel in self.spilt.drain(..) {
            if graph.is_valid_handle(voxel.node) {
                graph.remove_node(voxel.node);
            }
        }
    }
}

/// A loose glowing tetrahedron `size` across of `material` at `place`, turned `turned`.
fn spilt_voxel(
    graph: &mut Graph,
    material: MaterialResource,
    size: f32,
    place: Vector3<f32>,
    turned: UnitQuaternion<f32>,
) -> Handle<Node> {
    MeshBuilder::new(
        BaseBuilder::new()
            .with_name("spilt voxel")
            .with_cast_shadows(false)
            .with_local_transform(
                TransformBuilder::new()
                    .with_local_position(place)
                    .with_local_rotation(turned)
                    .build(),
            ),
    )
    .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(
        tetrahedron(size),
    ))
    .with_material(material)
    .build()])
    .build(graph)
    .to_base()
}

/// A tetrahedron with its corners on four of the corners of a cube `size` across, flat-shaded:
/// four triangles, where the cube took twelve.
fn tetrahedron(size: f32) -> SurfaceData {
    let h = 0.5 * size;
    let corners = [
        Vector3::new(h, h, h),
        Vector3::new(h, -h, -h),
        Vector3::new(-h, h, -h),
        Vector3::new(-h, -h, h),
    ];
    let mut builder = RawMeshBuilder::<StaticVertex>::new(12, 12);
    for skip in 0..4 {
        let [a, b, c]: [Vector3<f32>; 3] = std::array::from_fn(|k| corners[(skip + 1 + k) % 4]);
        // Wound to face away from the corner left out.
        let normal = (b - a).cross(&(c - a)).normalize();
        let (b, c, normal) = if normal.dot(&(a - corners[skip])) < 0.0 {
            (c, b, -normal)
        } else {
            (b, c, normal)
        };
        for (at, uv) in [
            (a, Vector2::new(0.0, 0.0)),
            (b, Vector2::new(1.0, 0.0)),
            (c, Vector2::new(0.5, 1.0)),
        ] {
            builder.insert(StaticVertex::from_pos_uv_normal(at, uv, normal));
        }
    }
    let mut data = SurfaceData::from_raw_mesh(builder.build());
    data.calculate_tangents().ok();
    data
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
    let local = graph[anchor]
        .global_transform()
        .try_inverse()
        .unwrap_or_else(Matrix4::identity)
        * graph[bone].global_transform();
    let basis = local.basis();
    let scale = Vector3::new(
        basis.column(0).norm(),
        basis.column(1).norm(),
        basis.column(2).norm(),
    );
    let unscaled = Matrix3::from_columns(&[
        basis.column(0) / scale.x,
        basis.column(1) / scale.y,
        basis.column(2) / scale.z,
    ]);
    let rotation =
        UnitQuaternion::from_matrix_eps(&unscaled, f32::EPSILON, 16, UnitQuaternion::identity());
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

    #[test]
    fn a_spilt_voxel_is_four_triangles_each_facing_out() {
        use fyrox::scene::mesh::buffer::{VertexAttributeUsage, VertexReadTrait};
        let data = tetrahedron(2.0);
        let triangles: Vec<_> = data.geometry_buffer.iter().collect();
        assert_eq!(triangles.len(), 4);
        let at = |i: u32, usage| {
            let v = data.vertex_buffer.get(i as usize).unwrap();
            Vector3::from(v.read_3_f32(usage).unwrap())
        };
        for t in triangles {
            let [a, b, c] = t.0.map(|i| at(i, VertexAttributeUsage::Position));
            let wound = (b - a).cross(&(c - a));
            // The middle is at the origin, so out is away from it.
            assert!(wound.dot(&(a + b + c)) > 0.0);
            let normal = at(t.0[0], VertexAttributeUsage::Normal);
            assert!((wound.normalize() - normal).norm() < 1.0e-5);
            assert!([a, b, c].iter().all(|p| p.iter().all(|x| x.abs() == 1.0)));
        }
    }

    use fyrox::scene::{
        collider::{ColliderBuilder, ColliderShape, GeometrySource},
        graph::GraphUpdateSwitches,
        rigidbody::{RigidBodyBuilder, RigidBodyType},
    };

    /// A slab of the maze, `scale` big with its middle at `at`, for loose voxels to strike: a
    /// triangle mesh, as the maze is to the physics, and thin to them.
    fn slab(graph: &mut Graph, at: Vector3<f32>, scale: Vector3<f32>) {
        let mesh = MeshBuilder::new(
            BaseBuilder::new()
                .with_local_transform(TransformBuilder::new().with_local_position(at).build()),
        )
        .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(
            SurfaceData::make_cube(Matrix4::new_nonuniform_scaling(&scale)),
        ))
        .build()])
        .build(graph);
        graph.update_hierarchical_data();
        let collider = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(ColliderShape::trimesh(vec![GeometrySource(mesh.to_base())]))
            .build(graph);
        RigidBodyBuilder::new(BaseBuilder::new().with_child(collider))
            .with_body_type(RigidBodyType::Static)
            .build(graph);
    }

    /// No droid's parts, only voxels let loose, `size` across, from `from` at each of `velocities`.
    fn let_loose(
        graph: &mut Graph,
        size: f32,
        from: Vector3<f32>,
        velocities: &[Vector3<f32>],
    ) -> Dismember {
        let mut parts = Dismember {
            intact: Handle::NONE,
            parts: Vec::new(),
            breaks: Vec::new(),
            apart: false,
            glows: vec![MaterialResource::default()],
            spilt: Vec::new(),
            dice: 0x9e37_79b9_7f4a_7c15,
        };
        for (k, &velocity) in velocities.iter().enumerate() {
            parts.voxel(
                graph,
                k,
                from + Vector3::new(0.0, 0.0, 0.05 * k as f32),
                size,
                velocity,
            );
        }
        parts
    }

    fn run(graph: &mut Graph, parts: &mut Dismember, seconds: f32) {
        let dt = 1.0 / 60.0;
        for _ in 0..(seconds / dt) as usize {
            parts.fly(graph, dt);
            graph.update(
                Vector2::new(800.0, 600.0),
                dt,
                GraphUpdateSwitches::default(),
            );
        }
    }

    #[test]
    fn a_spilt_voxel_lands_on_the_floor_and_comes_to_rest() {
        let mut graph = Graph::new();
        slab(
            &mut graph,
            Vector3::new(0.0, -0.1, 0.0),
            Vector3::new(10.0, 0.2, 10.0),
        );
        // As big as a thigh's voxels, dropped from half a meter, going sideways and tumbling.
        let size = 0.013 * SCALE;
        let mut parts = let_loose(
            &mut graph,
            size,
            Vector3::new(0.0, 0.5, 0.0),
            &[Vector3::new(0.5, 0.0, 0.0)],
        );
        run(&mut graph, &mut parts, 4.0);
        let voxel = &parts.spilt[0];
        let at = **graph[voxel.node].local_transform().position();
        assert!(voxel.resting, "at rest: {voxel:?}");
        assert!(
            (at.y - voxel.reach).abs() < 1.0e-3,
            "on the floor: {at:?} ({size})"
        );
        assert!(at.x > 0.0 && at.x < 0.5, "having slid a little: {at:?}");
    }

    #[test]
    fn loose_voxels_never_pass_through_a_floor_or_a_wall() {
        let mut graph = Graph::new();
        // A floor 0.2 m thick with its top at 0, and a wall 0.2 m thick across x = 1, too tall and
        // wide to go over or round.
        slab(
            &mut graph,
            Vector3::new(0.0, -0.1, 0.0),
            Vector3::new(100.0, 0.2, 100.0),
        );
        slab(
            &mut graph,
            Vector3::new(1.0, 10.0, 0.0),
            Vector3::new(0.2, 20.0, 100.0),
        );
        // As fast as a head blown apart sends them, and faster, at its smallest and biggest.
        let velocities: Vec<Vector3<f32>> = (0..40)
            .map(|k| {
                let t = k as f32 / 40.0 * std::f32::consts::TAU;
                Vector3::new(8.0 * t.cos().abs(), 8.0 * t.sin(), 3.0 * (2.0 * t).sin())
            })
            .collect();
        for size in [0.007 * SCALE, 0.013 * SCALE * BURST_GROWTH] {
            let mut parts = let_loose(&mut graph, size, Vector3::new(0.0, 1.6, -1.0), &velocities);
            run(&mut graph, &mut parts, 4.0);
            for voxel in &parts.spilt {
                let at = **graph[voxel.node].local_transform().position();
                assert!(at.y > 0.0, "above the floor: {at:?}");
                assert!(at.x < 0.9, "this side of the wall: {at:?}");
            }
            parts.sweep_up(&mut graph);
        }
    }
}
