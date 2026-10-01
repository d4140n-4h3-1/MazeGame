//! Finding the walkable ground of a level that is already in the scene.
//!
//! This is the scene side of [`layout`](crate::layout): it samples the level's colliders on a
//! grid of cells, and turns cells back into places in the world.
//!
//! A cell's floor is the highest in it with headroom over it, under a ceiling, and room around:
//! the ground, or a stair, or a floor up above. A cell holds one floor, so ground under a floor
//! up above is left out. Of those, the walkable cells are the ground and whatever can be climbed
//! to from it a stair's step at a time ([`hydroxus_ai::grid::MAX_CLIMB`]) - not the tops of
//! crates or walls, which no stairs lead up to.

use crate::layout::WalkGrid;
use hydroxus_ai::grid::MAX_CLIMB;
use std::collections::VecDeque;
use fyrox::{
    core::{
        algebra::{Point3, Vector3},
        log::Log,
        pool::Handle,
    },
    scene::{
        collider::Collider,
        graph::{physics::RayCastOptions, Graph},
        node::Node,
    },
};

/// Spacing of the walkability samples, in meters.
pub(crate) const CELL_SIZE: f32 = 0.5;

/// How much room a floor needs over it to be stood on.
const HEADROOM: f32 = 2.0;
/// How high a floor can be and still be the ground, which the climbing starts from.
const GROUND: f32 = 0.5;

/// Samples the maze for walkable ground, returning the grid and the world position of its corner.
/// `maze` is the level's collider, which must exist already, and `ignore` a mesh that is not part
/// of the maze.
pub fn survey(
    graph: &Graph,
    maze: Handle<Collider>,
    ignore: Handle<Node>,
) -> Option<(WalkGrid, Vector3<f32>)> {
    // The footprint of every mesh in the scene is the footprint of the maze.
    let mut min = Vector3::repeat(f32::MAX);
    let mut max = Vector3::repeat(f32::MIN);
    for node in graph.linear_iter() {
        if node.is_mesh() && node.handle() != ignore {
            let aabb = node.world_bounding_box();
            min = min.inf(&aabb.min);
            max = max.sup(&aabb.max);
        }
    }
    if min.x > max.x {
        return None;
    }
    let width = ((max.x - min.x) / CELL_SIZE).ceil() as usize;
    let depth = ((max.z - min.z) / CELL_SIZE).ceil() as usize;
    let mut grid = WalkGrid::new(width, depth, CELL_SIZE);
    let origin = Vector3::new(min.x, 0.0, min.z);
    // Each cell's floor, if it has one.
    let mut floors = vec![None; width * depth];
    for z in 0..depth {
        for x in 0..width {
            floors[z * width + x] = top_floor(graph, maze, cell_center(origin, x, z), max.y + 1.0);
        }
    }
    // Out from the ground, onto every floor a step from one already walkable.
    let mut queue = VecDeque::new();
    for (i, floor) in floors.iter().enumerate() {
        if floor.is_some_and(|floor| floor <= GROUND) {
            queue.push_back((i % width, i / width));
        }
    }
    for &(x, z) in &queue {
        grid.set(x, z, true);
        grid.set_floor(x, z, floors[z * width + x].unwrap_or_default());
    }
    let mut walkable = queue.len();
    let ground = walkable;
    while let Some((x, z)) = queue.pop_front() {
        let neighbours = [(x.wrapping_sub(1), z), (x + 1, z), (x, z.wrapping_sub(1)), (x, z + 1)];
        for (nx, nz) in neighbours {
            if nx >= width || nz >= depth || grid.is_walkable(nx, nz) {
                continue;
            }
            let Some(floor) = floors[nz * width + nx] else {
                continue;
            };
            if (floor - grid.floor(x, z)).abs() <= MAX_CLIMB {
                grid.set(nx, nz, true);
                grid.set_floor(nx, nz, floor);
                walkable += 1;
                queue.push_back((nx, nz));
            }
        }
    }
    Log::info(format!(
        "Maze: {width}x{depth} cells over {:.1}x{:.1} m, {walkable} walkable, {} of them up off the ground",
        max.x - min.x,
        max.z - min.z,
        walkable - ground
    ));
    Some((grid, origin))
}

/// The middle of cell `(x, z)` of the survey's grid, whose corner is at `origin`.
pub fn cell_center(origin: Vector3<f32>, x: usize, z: usize) -> Vector3<f32> {
    hydroxus_ai::grid::cell_center(origin, CELL_SIZE, x, z)
}

/// The cell of `grid` that `position` is in, if it is on the grid at all.
pub fn cell_at(
    grid: &WalkGrid,
    origin: Vector3<f32>,
    position: Vector3<f32>,
) -> Option<(usize, usize)> {
    grid.cell_at(origin, position)
}

/// The walkable cell nearest to a point, which is where the player can stand to reach it.
pub fn nearest_walkable(
    grid: &WalkGrid,
    origin: Vector3<f32>,
    point: Vector3<f32>,
) -> Option<(usize, usize)> {
    grid.nearest_walkable(origin, point)
}

/// The direction from `cell` towards the walkable floor around it, reached by walking.
pub fn open_direction(grid: &WalkGrid, origin: Vector3<f32>, cell: (usize, usize)) -> Vector3<f32> {
    const REACH: u32 = 8;
    let center = cell_center(origin, cell.0, cell.1);
    let distances = grid.distances_from(cell);
    let mut sum = Vector3::zeros();
    for (i, distance) in distances.iter().enumerate() {
        if distance.is_some_and(|d| d > 0 && d <= REACH) {
            sum += cell_center(origin, i % grid.width, i / grid.width) - center;
        }
    }
    if sum.norm_squared() > 0.0 {
        sum
    } else {
        Vector3::z()
    }
}

/// The walkable map as text, with the start and the exit marked: the quickest way to see what the
/// survey made of a new model. Every other row is left out, so that the map is about as tall as
/// it is wide in a terminal.
pub fn draw_map(grid: &WalkGrid, start: (usize, usize), exit: (usize, usize)) -> String {
    let mut text = String::new();
    for z in (0..grid.depth).step_by(2) {
        for x in 0..grid.width {
            let c = if (x, z) == start || (x, z + 1) == start {
                'S'
            } else if (x, z) == exit || (x, z + 1) == exit {
                'E'
            } else if grid.is_walkable(x, z) {
                '.'
            } else {
                '#'
            };
            text.push(c);
        }
        text.push('\n');
    }
    text
}

/// How high the highest floor at `spot` is that has headroom over it, under a ceiling, with room
/// around: a ray is dropped from `top`, over everything, through all of the maze, surface by
/// surface - a cast meets only the first surface of a collider - and the floor is the first it
/// meets with [`HEADROOM`] or more of open air above it.
fn top_floor(graph: &Graph, maze: Handle<Collider>, spot: Vector3<f32>, top: f32) -> Option<f32> {
    /// Below a surface, to cast on from past it; and the most surfaces a cell is looked through.
    const PAST: f32 = 0.01;
    const MOST: usize = 32;
    let mut heights = Vec::new();
    let mut from = top;
    while heights.len() < MOST {
        let below = Vector3::new(spot.x, from, spot.z);
        let Some((hit, collider)) = first_hit(graph, below, -Vector3::y(), from + 1.0) else {
            break;
        };
        if collider == maze {
            heights.push(hit.y);
        }
        from = hit.y - PAST;
    }
    // The first surface down is the top of the roof, with open sky over it.
    let floor = heights
        .windows(2)
        .find(|pair| pair[0] - pair[1] >= HEADROOM)
        .map(|pair| pair[1])?;
    let chest = Vector3::new(spot.x, floor + 1.0, spot.z);
    [Vector3::x(), -Vector3::x(), Vector3::z(), -Vector3::z()]
        .iter()
        .all(|dir| first_hit(graph, chest, *dir, 0.45).is_none())
        .then_some(floor)
}

fn first_hit(
    graph: &Graph,
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    length: f32,
) -> Option<(Vector3<f32>, Handle<Collider>)> {
    let mut hits = Vec::new();
    graph.physics.cast_ray(
        RayCastOptions {
            ray_origin: Point3::from(origin),
            ray_direction: direction,
            max_len: length,
            groups: Default::default(),
            sort_results: true,
        },
        &mut hits,
    );
    hits.first().map(|hit| (hit.position.coords, hit.collider))
}
