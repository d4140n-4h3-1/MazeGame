//! Finding the walkable ground of a level that is already in the scene.
//!
//! This is the scene side of [`layout`](crate::layout): it samples the level's colliders on a
//! grid of cells, and turns cells back into places in the world.

use crate::layout::WalkGrid;
use fyrox::{
    core::{
        algebra::{Point3, Vector2, Vector3},
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
const CELL_SIZE: f32 = 0.5;

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
    let mut grid = WalkGrid::new(width, depth);
    let origin = Vector3::new(min.x, 0.0, min.z);
    let mut walkable = 0;
    for z in 0..depth {
        for x in 0..width {
            let spot = cell_center(origin, x, z);
            if is_open_floor(graph, maze, spot) {
                grid.set(x, z, true);
                walkable += 1;
            }
        }
    }
    Log::info(format!(
        "Maze: {width}x{depth} cells over {:.1}x{:.1} m, {walkable} walkable",
        max.x - min.x,
        max.z - min.z
    ));
    Some((grid, origin))
}

pub fn cell_center(origin: Vector3<f32>, x: usize, z: usize) -> Vector3<f32> {
    origin
        + Vector3::new(
            (x as f32 + 0.5) * CELL_SIZE,
            0.0,
            (z as f32 + 0.5) * CELL_SIZE,
        )
}

/// The walkable cell nearest to a point, which is where the player can stand to reach it.
pub fn nearest_walkable(
    grid: &WalkGrid,
    origin: Vector3<f32>,
    point: Vector3<f32>,
) -> Option<(usize, usize)> {
    grid.walkable_cells().min_by(|&a, &b| {
        let distance = |cell: (usize, usize)| {
            let center = cell_center(origin, cell.0, cell.1);
            Vector2::new(center.x - point.x, center.z - point.z).norm_squared()
        };
        distance(a).total_cmp(&distance(b))
    })
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

/// Whether a person could stand at `spot`, which is inside the maze's tubes: there is maze floor
/// just below, maze ceiling somewhere above, and room around the point.
fn is_open_floor(graph: &Graph, maze: Handle<Collider>, spot: Vector3<f32>) -> bool {
    let probe = Vector3::new(spot.x, 1.0, spot.z);
    let on_maze = |hit: Option<(Vector3<f32>, Handle<Collider>)>| {
        hit.filter(|&(_, collider)| collider == maze)
            .map(|(position, _)| position)
    };
    let Some(ground) = on_maze(first_hit(graph, probe, -Vector3::y(), 1.5)) else {
        return false;
    };
    if on_maze(first_hit(graph, probe, Vector3::y(), 20.0)).is_none() {
        // Open sky: this is outside the tubes.
        return false;
    }
    let chest = Vector3::new(spot.x, ground.y + 1.0, spot.z);
    [Vector3::x(), -Vector3::x(), Vector3::z(), -Vector3::z()]
        .iter()
        .all(|dir| first_hit(graph, chest, *dir, 0.45).is_none())
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
