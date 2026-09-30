//! Firewalls round the flags, for capture the flag: each a glowing orange shell on a grey plinth,
//! `data/ctf/firewall.glb`, with its side's flag standing inside it, `data/ctf/flag_red.glb` or
//! `data/ctf/flag_blue.glb`.
//!
//! A maze model puts them where it has an empty named `flag_red` or `flag_blue`, turned as the
//! empty is, and can say where the computer that opens each one goes with another,
//! `computer_red` or `computer_blue`, facing the way its screen does. Every firewall answers to
//! one of the maze's computers (see [`crate::computer`]): while it is locked the shell stands in
//! the way, and once it is hacked the shell goes, leaving the plinth and the flag on it to take.
//!
//! The shell is orange glass, the flag seen through it, and its glow and its light flicker.

use crate::{
    computer::Computer,
    fixtures::EMISSION_STRENGTH,
    layout::WalkGrid,
    level::Marker,
};
use fyrox::{
    asset::manager::ResourceManager,
    core::{
        algebra::{UnitQuaternion, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{MaterialProperty, MaterialResource},
    resource::model::{Model, ModelResource, ModelResourceExtension},
    scene::{
        base::BaseBuilder,
        collider::{ColliderBuilder, ColliderShape},
        graph::Graph,
        light::{
            point::{PointLight, PointLightBuilder},
            BaseLightBuilder,
        },
        mesh::Mesh,
        node::Node,
        rigidbody::{RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        Scene,
    },
};

pub const FIREWALL_MODEL: &str = "data/ctf/firewall.glb";
/// Each side, and its flag.
const SIDES: [(&str, &str); 2] = [("red", "data/ctf/flag_red.glb"), ("blue", "data/ctf/flag_blue.glb")];
/// The shell's part of the model, which goes when the firewall is opened.
const BARRIER: &str = "firewall_barrier";

/// The shell: a round wall this wide, from the plinth's top to its own; and the plinth, this far
/// across from its middle to each side, and this high.
const BARRIER_RADIUS: f32 = 1.0;
const BARRIER_TOP: f32 = 3.5;
const PLINTH_HALF: f32 = 1.22;
const PLINTH_HEIGHT: f32 = 0.5;
/// How far round the middle the floor is taken out of the walk grid, so the droids walk round it.
const TAKEN: f32 = 1.8;
/// How close to the middle the player has to come to take the flag, off the plinth's side.
pub const REACH: f32 = 2.0;
/// The shell's colour: how strongly it colours what is seen through it, and how brightly it glows
/// at most; and the light it gives off, how far, and how brightly at most.
const COLOUR: Color = Color::opaque(255, 46, 0);
const TINT: f32 = 0.4;
const GLOW: f32 = 0.3;
const LIGHT_RADIUS: f32 = 6.0;
const LIGHT: f32 = 1.0;

/// The firewalls in the maze, and their models, as they load.
#[derive(Debug, Default, PartialEq)]
pub struct Firewalls {
    firewall: Option<ModelResource>,
    flags: [Option<ModelResource>; 2],
    walls: Vec<Firewall>,
}

#[derive(Debug, PartialEq)]
pub struct Firewall {
    /// Whose flag it holds: "red" or "blue".
    pub side: &'static str,
    /// Where it stands, on the floor, in the middle.
    pub position: Vector3<f32>,
    /// The computer that opens it, in the maze's list of computers, once one is given it.
    pub computer: Option<usize>,
    /// Everything it put into the scene.
    nodes: Vec<Handle<Node>>,
    /// The shell's meshes and their glass, its light, and the static body that stands in the
    /// way while it is up.
    barrier: Vec<Handle<Node>>,
    glass: MaterialResource,
    light: Handle<Node>,
    wall: Handle<Node>,
    open: bool,
}

impl Firewalls {
    /// Asks for the models: the firewall, and each side's flag.
    pub fn request(resources: &ResourceManager) -> Self {
        Self {
            firewall: Some(resources.request::<Model>(FIREWALL_MODEL)),
            flags: SIDES.map(|(_, path)| Some(resources.request::<Model>(path))),
            walls: Vec::new(),
        }
    }

    /// The models, by path, to wait for before the level is put together.
    pub fn models(&self) -> Vec<(String, ModelResource)> {
        let flags = SIDES.iter().zip(&self.flags).map(|((_, path), m)| (*path, m));
        [(FIREWALL_MODEL, &self.firewall)]
            .into_iter()
            .chain(flags)
            .filter_map(|(path, model)| Some((path.to_string(), model.clone()?)))
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Firewall> {
        self.walls.iter()
    }

    pub fn get_mut(&mut self, n: usize) -> Option<&mut Firewall> {
        self.walls.get_mut(n)
    }

    /// Puts a firewall, with its side's flag in it, at each of the level's `markers` for one,
    /// in place of any from before, and takes the floor round each out of `grid`.
    pub fn place(
        &mut self,
        scene: &mut Scene,
        markers: &[Marker],
        grid: &mut WalkGrid,
        origin: Vector3<f32>,
    ) {
        self.clear(&mut scene.graph);
        let Some(firewall) = self.firewall.clone().filter(|m| m.is_ok()) else {
            return;
        };
        for marker in markers {
            let Some(side) = SIDES.iter().position(|(side, _)| marker.name == format!("flag_{side}")) else {
                continue;
            };
            let Some(flag) = self.flags[side].clone().filter(|m| m.is_ok()) else {
                continue;
            };
            let wall = Firewall::spawn(scene, &firewall, &flag, SIDES[side].0, marker);
            take_floor(grid, origin, marker.position);
            Log::info(format!(
                "Firewall: {}'s flag at {:.1} {:.1} {:.1}",
                wall.side, wall.position.x, wall.position.y, wall.position.z
            ));
            self.walls.push(wall);
        }
    }

    /// Takes them all out of the scene.
    pub fn clear(&mut self, graph: &mut Graph) {
        for wall in self.walls.drain(..) {
            for node in wall.nodes {
                if graph.is_valid_handle(node) {
                    graph.remove_node(node);
                }
            }
        }
    }

    /// Opens each firewall whose computer has been hacked, and closes it again once the computer
    /// is locked - as it is for each new round. One with no computer to answer to stays open,
    /// since there would be no opening it. Those up flicker, `time` seconds in.
    pub fn update(&mut self, graph: &mut Graph, computers: &[Computer], time: f32) {
        for (n, wall) in self.walls.iter_mut().enumerate() {
            if !wall.open {
                wall.flicker(graph, time + n as f32 * 7.3);
            }
            let open = wall
                .computer
                .and_then(|n| computers.get(n))
                .is_none_or(Computer::cleared);
            if open != wall.open {
                wall.set_open(graph, open);
                Log::info(format!(
                    "Firewall: {}'s {}",
                    wall.side,
                    if open { "is down" } else { "is up" }
                ));
            }
        }
    }

    /// The firewall standing at `point`, give or take a little, if there is one.
    pub fn at(&self, point: Vector3<f32>) -> Option<&Firewall> {
        self.walls.iter().find(|wall| {
            let flat = Vector3::new(point.x - wall.position.x, 0.0, point.z - wall.position.z);
            flat.norm() < 0.5
        })
    }
}

impl Firewall {
    fn spawn(
        scene: &mut Scene,
        firewall: &ModelResource,
        flag: &ModelResource,
        side: &'static str,
        marker: &Marker,
    ) -> Self {
        // The models are made with +x the way the flag's cloth faces; the marker's yaw is the
        // way its own +x does.
        let rotation = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), marker.yaw);
        let at = marker.position;
        let shell = firewall.instantiate(scene);
        let banner = flag.instantiate(scene);
        let graph = &mut scene.graph;
        for root in [shell, banner] {
            graph[root]
                .local_transform_mut()
                .set_position(at)
                .set_rotation(rotation);
        }

        // The shell glows orange - its own copy of the model's material, as the computer's frame
        // has - and casts no shadow: its light is inside it.
        let barrier: Vec<_> = graph
            .traverse_handle_iter(shell)
            .filter(|&node| graph[node].name().starts_with(BARRIER) && graph[node].is_mesh())
            .collect();
        if barrier.is_empty() {
            Log::err(format!("Firewall: {FIREWALL_MODEL} has no {BARRIER}"));
        }
        let glass = fyrox_gfx::GlassMaterial {
            tint: COLOUR,
            tint_strength: TINT,
            emission: COLOUR,
            emission_strength: GLOW,
            // It does not bend what is seen through it, as glass does: the bend is so many meters
            // behind the surface, which close up is most of the screen, and smears what is behind
            // into one colour. Nor does it reflect.
            index_of_refraction: 1.0,
            reflectivity: 0.0,
            ..Default::default()
        }
        .build_resource();
        for &node in &barrier {
            graph[node].set_cast_shadows(false);
            if let Some(mesh) = graph[node].cast_mut::<Mesh>() {
                for surface in mesh.surfaces_mut() {
                    surface.set_material(glass.clone());
                }
            }
        }
        let light = PointLightBuilder::new(
            BaseLightBuilder::new(BaseBuilder::new().with_local_transform(
                TransformBuilder::new()
                    .with_local_position(at + Vector3::y() * 2.0)
                    .build(),
            ))
            .with_color(COLOUR),
        )
        .with_radius(LIGHT_RADIUS)
        .build(graph)
        .to_base();

        // The plinth is always in the way; the shell only while it is up.
        let static_body = |graph: &mut Graph, shape: ColliderShape, height: f32| {
            let collider = ColliderBuilder::new(BaseBuilder::new().with_local_transform(
                TransformBuilder::new()
                    .with_local_position(Vector3::y() * height)
                    .build(),
            ))
            .with_shape(shape)
            .build(graph);
            RigidBodyBuilder::new(
                BaseBuilder::new()
                    .with_child(collider)
                    .with_local_transform(
                        TransformBuilder::new()
                            .with_local_position(at)
                            .with_local_rotation(rotation)
                            .build(),
                    ),
            )
            .with_body_type(RigidBodyType::Static)
            .build(graph)
            .to_base()
        };
        let plinth = static_body(
            graph,
            ColliderShape::cuboid(PLINTH_HALF, PLINTH_HEIGHT / 2.0, PLINTH_HALF),
            PLINTH_HEIGHT / 2.0,
        );
        let half = (BARRIER_TOP - PLINTH_HEIGHT) / 2.0;
        let wall = static_body(
            graph,
            ColliderShape::cylinder(half, BARRIER_RADIUS),
            PLINTH_HEIGHT + half,
        );

        Self {
            side,
            position: at,
            computer: None,
            nodes: vec![shell, banner, light, plinth, wall],
            barrier,
            glass,
            light,
            wall,
            open: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Its glow and its light, `time` seconds in: a quick unsteady waver, and now and then a
    /// moment when it nearly goes out.
    fn flicker(&self, graph: &mut Graph, time: f32) {
        let level = flicker(time);
        self.glass
            .data_ref()
            .set_property(EMISSION_STRENGTH, MaterialProperty::Float(GLOW * level));
        if let Some(light) = graph[self.light].cast_mut::<PointLight>() {
            light.base_light_mut().set_intensity(LIGHT * level);
        }
    }

    fn set_open(&mut self, graph: &mut Graph, open: bool) {
        self.open = open;
        for &node in self.barrier.iter().chain([&self.light]) {
            graph[node].set_visibility(!open);
        }
        // Out of the way, far below, rather than gone: it comes back up for the next round.
        let y = if open { -1000.0 } else { 0.0 };
        graph[self.wall]
            .local_transform_mut()
            .set_position(Vector3::new(self.position.x, self.position.y + y, self.position.z));
    }
}

/// Takes the floor within [`TAKEN`] of `at` out of `grid`, whose corner is at `origin`.
fn take_floor(grid: &mut WalkGrid, origin: Vector3<f32>, at: Vector3<f32>) {
    let reach = Vector3::new(TAKEN, 0.0, TAKEN);
    let (Some(low), Some(high)) = (grid.cell_at(origin, at - reach), grid.cell_at(origin, at + reach)) else {
        return;
    };
    for z in low.1..=high.1 {
        for x in low.0..=high.0 {
            let middle = crate::survey::cell_center(origin, x, z);
            if Vector3::new(middle.x - at.x, 0.0, middle.z - at.z).norm() < TAKEN {
                grid.set(x, z, false);
            }
        }
    }
}

/// How brightly a firewall glows, `time` seconds in, from 0 to 1: two quick waves over each
/// other, and a dip to a fraction of that in some twelfths of a second, picked by a hash of which
/// twelfth it is.
fn flicker(time: f32) -> f32 {
    let waver = 0.78 + 0.12 * (time * 23.0).sin() + 0.1 * (time * 57.3 + 1.7).sin();
    let tick = (time * 12.0).floor();
    let hash = ((tick * 12.9898).sin() * 43_758.547).rem_euclid(1.0);
    if hash > 0.92 { waver * 0.3 } else { waver }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_firewall_flickers_between_dim_and_full() {
        let levels: Vec<f32> = (0..2400).map(|i| flicker(i as f32 / 240.0)).collect();
        assert!(levels.iter().all(|l| (0.0..=1.0).contains(l)));
        assert!(levels.iter().any(|&l| l < 0.3), "it nearly goes out now and then");
        assert!(levels.iter().filter(|&&l| l > 0.5).count() > levels.len() * 3 / 4, "mostly lit");
    }
}
