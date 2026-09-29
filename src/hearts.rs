//! Hearts: health items floating in the maze's corridors, each slowly spinning and bobbing up and
//! down. For now they only float there: there is no health for them to give back yet.
//!
//! The model, [`HEART_MODEL`], is made in Blender from `health.blend` and exported as it is; it
//! is scaled here to [`SIZE`] however big it was made. The engine takes a glTF model's surfaces
//! for solid whatever their alpha, so a see-through one - the heart's outer layer - is made glass
//! here, of its own colour ([`Hearts::glaze`]), as the ceiling's panes are.
//!
//! A few are scattered afresh each maze ([`count`]), each on open floor away from the start and
//! from the others ([`spots`]) - the first a little way from the start, so there is one to come
//! across early.

use crate::{
    fixtures::{property, DIFFUSE_COLOR},
    layout::{Rng, WalkGrid},
    survey,
};
use fyrox_gfx::{replace_materials, GlassMaterial};
use fyrox::{
    core::{
        algebra::{Matrix4, Point3, UnitQuaternion, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{Material, MaterialProperty, MaterialResource},
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        graph::Graph,
        mesh::{
            buffer::{VertexAttributeUsage, VertexReadTrait},
            Mesh,
        },
        node::Node,
        Scene,
    },
};

/// The heart's model.
pub const HEART_MODEL: &str = "data/health.glb";
/// How big a heart is across its biggest side, in meters.
const SIZE: f32 = 0.45;
/// How high the middle of a heart floats above the floor, in meters; how far it bobs up and
/// down from there, and how long a bob takes, in seconds; and how long it takes to turn round
/// once, in seconds.
const HOVER: f32 = 1.1;
const BOB: f32 = 0.05;
const BOB_TIME: f32 = 2.0;
const TURN_TIME: f32 = 4.0;
/// How many hearts a maze gets: one for so many cells of floor, between the fewest and the most.
const FLOOR_PER_HEART: usize = 5000;
const FEWEST: usize = 3;
const MOST: usize = 12;
/// How many cells of walking from the start a heart has to be at least, and the first at most;
/// how many cells apart two hearts are at first asked to be; and how many cells of floor all
/// round a heart's cell has to have, so that it floats in a corridor rather than against a wall.
const AWAY_FROM_START: u32 = 16;
const NEAR_START: u32 = 40;
const APART: f32 = 40.0;
const ROOM: i64 = 2;

/// How many hearts a maze with `floor` cells of floor gets.
pub fn count(floor: usize) -> usize {
    (floor / FLOOR_PER_HEART).clamp(FEWEST, MOST)
}

/// Where `count` hearts go in `grid`, whose player starts at `start`: cells of open floor far
/// enough from the start - the first no further than [`NEAR_START`], where there is room - as far
/// apart as there are cells for, picked by `rng`. Fewer if the maze has no room for them all.
pub fn spots(grid: &WalkGrid, start: (usize, usize), count: usize, rng: &mut Rng) -> Vec<(usize, usize)> {
    let open = |x: usize, z: usize| {
        (-ROOM..=ROOM).all(|dx| {
            (-ROOM..=ROOM).all(|dz| {
                let (x, z) = (x as i64 + dx, z as i64 + dz);
                x >= 0
                    && z >= 0
                    && (x as usize) < grid.width
                    && (z as usize) < grid.depth
                    && grid.is_walkable(x as usize, z as usize)
            })
        })
    };
    let distances = grid.distances_from(start);
    let fits: Vec<(usize, usize)> = distances
        .iter()
        .enumerate()
        .filter(|(_, d)| d.is_some_and(|d| d >= AWAY_FROM_START))
        .map(|(i, _)| (i % grid.width, i / grid.width))
        .filter(|&(x, z)| open(x, z))
        .collect();
    let mut spots: Vec<(usize, usize)> = Vec::new();
    let near: Vec<(usize, usize)> = fits
        .iter()
        .copied()
        .filter(|&(x, z)| distances[z * grid.width + x].is_some_and(|d| d <= NEAR_START))
        .collect();
    if count > 0 && !near.is_empty() {
        spots.push(near[rng.below(near.len())]);
    }
    let mut apart = APART;
    while spots.len() < count {
        let far: Vec<(usize, usize)> = fits
            .iter()
            .copied()
            .filter(|&(x, z)| {
                spots.iter().all(|&(sx, sz)| {
                    (x as f32 - sx as f32).hypot(z as f32 - sz as f32) >= apart
                })
            })
            .collect();
        if far.is_empty() {
            // No room left that far apart: nearer, then, or no more at all.
            if apart < 1.0 {
                break;
            }
            apart /= 2.0;
            continue;
        }
        spots.push(far[rng.below(far.len())]);
    }
    spots
}

/// The hearts in the scene.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Hearts {
    /// Each heart, where it floats - above the middle of its cell - and how far into its bob and
    /// its turn it started, as a part of each.
    hearts: Vec<(Handle<Node>, Vector3<f32>, f32)>,
    /// How big the model is made, to be [`SIZE`] across.
    scale: f32,
    /// The glass its see-through surfaces are made of, shared by every heart.
    glass: Option<MaterialResource>,
    /// How long they have been floating, in seconds.
    time: f32,
}

impl Hearts {
    /// Takes the hearts of the last maze out of `scene`, and puts one from `model` at each of
    /// `spots` in the maze whose `grid` has its corner at `origin`.
    pub fn place(
        &mut self,
        model: &ModelResource,
        scene: &mut Scene,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        spots: &[(usize, usize)],
    ) {
        self.clear(&mut scene.graph);
        for (n, &(x, z)) in spots.iter().enumerate() {
            let root = model.instantiate(scene);
            if n == 0 {
                self.scale = scale_for(&scene.graph, root);
            }
            self.glaze(&mut scene.graph, root);
            let at = survey::cell_center(origin, x, z);
            let at = Vector3::new(at.x, grid.floor(x, z) + HOVER, at.z);
            // Each out of step with the others.
            let phase = n as f32 * 0.37 % 1.0;
            self.hearts.push((root, at, phase));
        }
        Log::info(format!("Hearts: {} in the maze", self.hearts.len()));
        self.update(&mut scene.graph, 0.0);
    }

    /// Makes the see-through surfaces of the heart under `root` glass, of their own colour.
    fn glaze(&mut self, graph: &mut Graph, root: Handle<Node>) {
        let see_through = |material: &Material| match property(material, DIFFUSE_COLOR) {
            Some(MaterialProperty::Color(colour)) if colour.a < 255 => Some(colour),
            _ => None,
        };
        if self.glass.is_none() {
            // The first see-through colour there is, which is the one the heart has.
            let mut colour = None;
            for node in graph.traverse_handle_iter(root) {
                if let Some(mesh) = graph[node].cast::<Mesh>() {
                    for surface in mesh.surfaces() {
                        let state = surface.material().state();
                        colour = colour.or_else(|| state.data_ref().and_then(see_through));
                    }
                }
            }
            let Some(colour) = colour else {
                return;
            };
            self.glass = Some(
                GlassMaterial {
                    tint: Color::opaque(colour.r, colour.g, colour.b),
                    // As much as the surface hides of what is behind it.
                    tint_strength: colour.a as f32 / 255.0,
                    ..Default::default()
                }
                .build_resource(),
            );
        }
        if let Some(glass) = &self.glass {
            replace_materials(graph, root, |material| see_through(material).is_some(), glass);
        }
    }

    /// Takes them all out of `graph`.
    pub fn clear(&mut self, graph: &mut Graph) {
        for (root, ..) in self.hearts.drain(..) {
            if graph.is_valid_handle(root) {
                graph.remove_node(root);
            }
        }
    }

    /// Bobs and turns each another `dt` seconds on.
    pub fn update(&mut self, graph: &mut Graph, dt: f32) {
        self.time += dt;
        for &(root, at, phase) in &self.hearts {
            let Ok(node) = graph.try_get_mut(root) else {
                continue;
            };
            let bob = BOB * ((self.time / BOB_TIME + phase) * std::f32::consts::TAU).sin();
            let turn = (self.time / TURN_TIME + phase) * std::f32::consts::TAU;
            node.local_transform_mut()
                .set_position(at + Vector3::new(0.0, bob, 0.0))
                .set_rotation(UnitQuaternion::from_axis_angle(&Vector3::y_axis(), turn))
                .set_scale(Vector3::repeat(self.scale));
        }
    }

    /// Shows only the hearts `can_see` says could be seen from where the player is.
    pub fn cull(&self, graph: &mut Graph, can_see: impl Fn(Vector3<f32>) -> bool) {
        for &(root, at, _) in &self.hearts {
            if let Ok(node) = graph.try_get_mut(root) {
                node.set_visibility(can_see(at));
            }
        }
    }
}

/// How much the model under `root` has to be scaled to be [`SIZE`] across its biggest side, as
/// its meshes are placed under it.
fn scale_for(graph: &Graph, root: Handle<Node>) -> f32 {
    let (mut low, mut high) = (Vector3::repeat(f32::MAX), Vector3::repeat(f32::MIN));
    for node in graph.traverse_handle_iter(root) {
        let Some(mesh) = graph[node].cast::<Mesh>() else {
            continue;
        };
        // Its place under the root, from its own transform and each parent's up to the root.
        let mut transform: Matrix4<f32> = graph[node].local_transform().matrix();
        let mut above = graph[node].parent();
        while above.is_some() && above != root {
            transform = graph[above].local_transform().matrix() * transform;
            above = graph[above].parent();
        }
        for surface in mesh.surfaces() {
            let data = surface.data();
            let data = data.data_ref();
            for vertex in data.vertex_buffer.iter() {
                if let Ok(position) = vertex.read_3_f32(VertexAttributeUsage::Position) {
                    let point = transform.transform_point(&Point3::from(position)).coords;
                    low = low.inf(&point);
                    high = high.sup(&point);
                }
            }
        }
    }
    let biggest = (high - low).max();
    if biggest.is_finite() && biggest > 0.0 {
        SIZE / biggest
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_hearts_for_any_maze() {
        assert_eq!(count(0), FEWEST);
        assert_eq!(count(5_000), FEWEST);
        assert_eq!(count(20_000), 4);
        assert_eq!(count(1_000_000), MOST);
    }

    #[test]
    fn hearts_go_on_open_floor_away_from_the_start_and_from_each_other() {
        // A room 60 cells wide and deep, walled all round.
        let mut grid = WalkGrid::new(62, 62);
        for x in 1..61 {
            for z in 1..61 {
                grid.set(x, z, true);
            }
        }
        let start = (5, 5);
        let spots = spots(&grid, start, 4, &mut Rng::new(3));
        assert_eq!(spots.len(), 4);
        let distances = grid.distances_from(start);
        let (x, z) = spots[0];
        assert!(distances[z * grid.width + x].unwrap() <= NEAR_START, "the first near the start");
        for (n, &(x, z)) in spots.iter().enumerate() {
            assert!(distances[z * grid.width + x].unwrap() >= AWAY_FROM_START);
            assert!(x >= 3 && z >= 3 && x <= 58 && z <= 58, "not against a wall: {x}, {z}");
            for &(ox, oz) in &spots[n + 1..] {
                assert!((x, z) != (ox, oz), "two in one place");
            }
        }
    }

    #[test]
    fn a_maze_with_no_room_gets_none() {
        let grid = WalkGrid::new(10, 10);
        assert!(spots(&grid, (1, 1), 3, &mut Rng::new(1)).is_empty());
    }
}
