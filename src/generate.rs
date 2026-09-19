//! Random mazes built from tile prefabs.
//!
//! The maze is planned on a grid of junctions, then built from tiles on a finer grid of cells, one
//! tile cell per square of floor. A junction tile (the corner, the T and the crossroads) has a
//! middle cell and an arm of one cell towards each of its openings, so two junctions three cells
//! apart meet arm to arm: the cell next to each junction is its own arm. Junctions with a straight
//! run through them, and dead ends, are made of straight pipes instead, and a dead end gets a wall
//! across its far side.
//!
//! Nothing here knows what the tiles look like. Their openings are measured from the models (see
//! `TileShape`), so the plan only has to find, for each junction, the tile and the quarter turn
//! whose arms point where the maze goes.

use crate::layout::Rng;

/// How many cells apart junctions are: a middle cell and an arm on each side.
pub const JUNCTION_SPACING: i32 = 3;

/// A direction on the cell grid, as a step in x and z.
pub type Dir = (i32, i32);

pub const DIRS: [Dir; 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Turns a direction by `turns` quarter turns about the vertical axis, the way a node rotated by
/// `turns * 90` degrees about +Y turns its children: x' = x cos a + z sin a, z' = -x sin a + z cos a.
pub fn rotate(dir: Dir, turns: u32) -> Dir {
    let (mut x, mut z) = dir;
    for _ in 0..turns % 4 {
        (x, z) = (z, -x);
    }
    (x, z)
}

/// A maze on a grid of junctions: which neighbours each junction is joined to.
#[derive(Debug, Clone, PartialEq)]
pub struct Maze {
    pub width: usize,
    pub depth: usize,
    /// For each junction, whether it is joined to the neighbour in each of [`DIRS`].
    links: Vec<[bool; 4]>,
}

impl Maze {
    /// A random maze: every junction reachable from every other, with a few extra joins so that
    /// not every choice is a dead end. `loop_chance` is the chance, per dead end, of opening it
    /// into a neighbouring corridor.
    pub fn generate(width: usize, depth: usize, loop_chance: f32, rng: &mut Rng) -> Self {
        let mut maze = Self {
            width,
            depth,
            links: vec![[false; 4]; width * depth],
        };
        if width == 0 || depth == 0 {
            return maze;
        }

        // A depth-first walk that carves into a random unvisited neighbour, and backs up when
        // there is none: long winding corridors, and every junction visited exactly once.
        let mut visited = vec![false; width * depth];
        let start = (rng.below(width), rng.below(depth));
        visited[maze.index(start)] = true;
        let mut stack = vec![start];
        while let Some(&here) = stack.last() {
            let options: Vec<usize> = (0..4)
                .filter(|&d| {
                    maze.neighbour(here, d)
                        .is_some_and(|next| !visited[maze.index(next)])
                })
                .collect();
            if options.is_empty() {
                stack.pop();
                continue;
            }
            let d = options[rng.below(options.len())];
            let next = maze.neighbour(here, d).unwrap();
            maze.join(here, d);
            visited[maze.index(next)] = true;
            stack.push(next);
        }

        // Open some dead ends into a neighbour, which gives the maze a few loops.
        let chance = (loop_chance.clamp(0.0, 1.0) * 1000.0) as usize;
        for z in 0..depth {
            for x in 0..width {
                let here = (x, z);
                if maze.degree(here) != 1 || rng.below(1000) >= chance {
                    continue;
                }
                let closed: Vec<usize> = (0..4)
                    .filter(|&d| !maze.links[maze.index(here)][d] && maze.neighbour(here, d).is_some())
                    .collect();
                if !closed.is_empty() {
                    let d = closed[rng.below(closed.len())];
                    maze.join(here, d);
                }
            }
        }
        maze
    }

    fn index(&self, (x, z): (usize, usize)) -> usize {
        z * self.width + x
    }

    fn neighbour(&self, (x, z): (usize, usize), d: usize) -> Option<(usize, usize)> {
        let (dx, dz) = DIRS[d];
        let nx = x as i32 + dx;
        let nz = z as i32 + dz;
        (nx >= 0 && nz >= 0 && (nx as usize) < self.width && (nz as usize) < self.depth)
            .then_some((nx as usize, nz as usize))
    }

    fn join(&mut self, here: (usize, usize), d: usize) {
        let there = self.neighbour(here, d).expect("joined to a neighbour off the grid");
        let (i, j) = (self.index(here), self.index(there));
        self.links[i][d] = true;
        self.links[j][(d + 2) % 4] = true;
    }

    /// The directions a junction is joined in.
    pub fn openings(&self, here: (usize, usize)) -> Vec<Dir> {
        (0..4)
            .filter(|&d| self.links[self.index(here)][d])
            .map(|d| DIRS[d])
            .collect()
    }

    fn degree(&self, here: (usize, usize)) -> usize {
        self.links[self.index(here)].iter().filter(|&&l| l).count()
    }

    /// Junctions a walk can reach from the first one, which is all of them in a proper maze.
    #[cfg(test)]
    pub fn reachable(&self) -> usize {
        if self.links.is_empty() {
            return 0;
        }
        let mut seen = vec![false; self.links.len()];
        let mut stack = vec![(0, 0)];
        seen[0] = true;
        let mut count = 1;
        while let Some(here) = stack.pop() {
            for d in 0..4 {
                if !self.links[self.index(here)][d] {
                    continue;
                }
                let next = self.neighbour(here, d).unwrap();
                if !seen[self.index(next)] {
                    seen[self.index(next)] = true;
                    count += 1;
                    stack.push(next);
                }
            }
        }
        count
    }
}

/// Which way a tile's arms point, measured from its model unturned.
#[derive(Debug, Clone, PartialEq)]
pub enum TileShape {
    /// A straight piece one cell long, open at both ends along `axis` (either way along it).
    Pipe { axis: Dir },
    /// A middle cell with a one-cell arm in each of these directions, open at the arm's end.
    Junction { arms: Vec<Dir> },
}

/// The kinds of tile a maze is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileKind {
    Pipe,
    Corner,
    Tee,
    Cross,
}

/// One piece of a built maze: a tile, or a wall closing a dead end.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    /// `kind` turned by `turns` quarter turns, its middle cell at `cell`.
    Tile {
        kind: TileKind,
        cell: (i32, i32),
        turns: u32,
    },
    /// A wall across the side of `cell` that faces `dir`.
    Wall { cell: (i32, i32), dir: Dir },
}

/// The tiles, as measured.
#[derive(Debug, Clone, PartialEq)]
pub struct TileSet {
    pub pipe: TileShape,
    pub corner: TileShape,
    pub tee: TileShape,
    pub cross: TileShape,
}

impl TileSet {
    fn shape(&self, kind: TileKind) -> &TileShape {
        match kind {
            TileKind::Pipe => &self.pipe,
            TileKind::Corner => &self.corner,
            TileKind::Tee => &self.tee,
            TileKind::Cross => &self.cross,
        }
    }

    /// The quarter turns that make the tile's arms point exactly in `dirs`, if any do.
    fn turns_for(&self, kind: TileKind, dirs: &[Dir]) -> Option<u32> {
        match self.shape(kind) {
            TileShape::Junction { arms } => (0..4).find(|&turns| {
                arms.len() == dirs.len()
                    && arms.iter().all(|&arm| dirs.contains(&rotate(arm, turns)))
            }),
            TileShape::Pipe { axis } => (0..4).find(|&turns| {
                let along = rotate(*axis, turns);
                dirs.iter().all(|&d| d == along || d == (-along.0, -along.1))
            }),
        }
    }

    /// Lays out the maze in tiles. Fails, naming the problem, if a tile does not have the shape
    /// its kind needs.
    pub fn build(&self, maze: &Maze) -> Result<Vec<Piece>, String> {
        let mut pieces = Vec::new();
        for z in 0..maze.depth {
            for x in 0..maze.width {
                let dirs = maze.openings((x, z));
                let middle = (x as i32 * JUNCTION_SPACING, z as i32 * JUNCTION_SPACING);
                let beside = |d: Dir| (middle.0 + d.0, middle.1 + d.1);
                let pipe = |cell: (i32, i32), along: Dir, pieces: &mut Vec<Piece>| {
                    let turns = self
                        .turns_for(TileKind::Pipe, &[along])
                        .ok_or("the straight tile is not open at two opposite ends")?;
                    pieces.push(Piece::Tile {
                        kind: TileKind::Pipe,
                        cell,
                        turns,
                    });
                    Ok::<(), String>(())
                };
                match dirs.len() {
                    0 => {}
                    1 => {
                        // A dead end: the corridor comes in through the arm and stops at the far
                        // side of the middle cell.
                        let d = dirs[0];
                        pipe(middle, d, &mut pieces)?;
                        pipe(beside(d), d, &mut pieces)?;
                        pieces.push(Piece::Wall {
                            cell: middle,
                            dir: (-d.0, -d.1),
                        });
                    }
                    2 if dirs[0] == (-dirs[1].0, -dirs[1].1) => {
                        let d = dirs[0];
                        pipe(beside(dirs[1]), d, &mut pieces)?;
                        pipe(middle, d, &mut pieces)?;
                        pipe(beside(d), d, &mut pieces)?;
                    }
                    n => {
                        let kind = match n {
                            2 => TileKind::Corner,
                            3 => TileKind::Tee,
                            _ => TileKind::Cross,
                        };
                        let turns = self.turns_for(kind, &dirs).ok_or_else(|| {
                            format!("no turn of the {kind:?} tile has arms pointing {dirs:?}")
                        })?;
                        pieces.push(Piece::Tile {
                            kind,
                            cell: middle,
                            turns,
                        });
                    }
                }
            }
        }
        Ok(pieces)
    }
}

/// Which sides of each cell of a built maze are open: the floor plan that decides what can be
/// seen from where.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellMap {
    /// Open sides of every cell with floor, in the order of [`DIRS`].
    open: std::collections::HashMap<(i32, i32), [bool; 4]>,
}

fn dir_index(d: Dir) -> usize {
    DIRS.iter().position(|&x| x == d).expect("not a grid direction")
}

impl CellMap {
    /// The floor plan of `pieces` built from `tiles`.
    pub fn new(tiles: &TileSet, pieces: &[Piece]) -> Self {
        let mut map = Self::default();
        for piece in pieces {
            for (cell, sides) in piece_cells(tiles, piece) {
                let open = map.open.entry(cell).or_default();
                for d in sides {
                    open[dir_index(d)] = true;
                }
            }
        }
        // Walls close what the pipes they stand in left open.
        for piece in pieces {
            if let Piece::Wall { cell, dir } = piece {
                if let Some(open) = map.open.get_mut(cell) {
                    open[dir_index(*dir)] = false;
                }
            }
        }
        map
    }

    pub fn is_open(&self, cell: (i32, i32), d: Dir) -> bool {
        self.open.get(&cell).is_some_and(|open| open[dir_index(d)])
    }

    /// Every cell that can be seen from somewhere in `from`: rays are cast across the floor plan
    /// from the middle and near the corners of the cell, in every direction, and stop at the first
    /// closed side they meet. The result does not depend on where in the cell the viewer stands,
    /// so it only changes when they step into another cell. `reach` limits how many cells away a
    /// ray goes.
    pub fn visible_from(&self, from: (i32, i32), reach: f32) -> std::collections::HashSet<(i32, i32)> {
        const RAYS: usize = 360;
        const INSET: f32 = 0.45;
        let mut seen = std::collections::HashSet::from([from]);
        let origins = [
            (0.0, 0.0),
            (-INSET, -INSET),
            (INSET, -INSET),
            (-INSET, INSET),
            (INSET, INSET),
        ];
        for (ox, oz) in origins {
            let origin = (from.0 as f32 + ox, from.1 as f32 + oz);
            for i in 0..RAYS {
                let angle = (i as f32 + 0.5) / RAYS as f32 * std::f32::consts::TAU;
                self.cast(from, origin, (angle.cos(), angle.sin()), reach, &mut seen);
            }
        }
        seen
    }

    /// Walks a ray cell by cell from `origin` (in `cell`), where cell (x, z) spans x - 0.5 to
    /// x + 0.5 and z - 0.5 to z + 0.5, adding each cell it enters to `seen`.
    fn cast(
        &self,
        mut cell: (i32, i32),
        origin: (f32, f32),
        dir: (f32, f32),
        reach: f32,
        seen: &mut std::collections::HashSet<(i32, i32)>,
    ) {
        let step = (dir.0.signum() as i32, dir.1.signum() as i32);
        // How far along the ray the next vertical and horizontal cell sides are.
        let next_side = |c: i32, o: f32, d: f32, s: i32| {
            if s == 0 {
                f32::INFINITY
            } else {
                (c as f32 + 0.5 * s as f32 - o) / d
            }
        };
        let mut t_x = next_side(cell.0, origin.0, dir.0, step.0);
        let mut t_z = next_side(cell.1, origin.1, dir.1, step.1);
        let dt_x = if step.0 == 0 { f32::INFINITY } else { 1.0 / dir.0.abs() };
        let dt_z = if step.1 == 0 { f32::INFINITY } else { 1.0 / dir.1.abs() };
        loop {
            let (t, d) = if t_x < t_z {
                (t_x, (step.0, 0))
            } else {
                (t_z, (0, step.1))
            };
            if t > reach || !self.is_open(cell, d) {
                return;
            }
            cell = (cell.0 + d.0, cell.1 + d.1);
            seen.insert(cell);
            if d.0 != 0 {
                t_x += dt_x;
            } else {
                t_z += dt_z;
            }
        }
    }

    /// The cells within `depth` steps of `cells` through open sides: where a lamp can stand and
    /// still light one of `cells`.
    pub fn around(
        &self,
        cells: &std::collections::HashSet<(i32, i32)>,
        depth: u32,
    ) -> std::collections::HashSet<(i32, i32)> {
        let mut reached = cells.clone();
        let mut frontier: Vec<(i32, i32)> = cells.iter().copied().collect();
        for _ in 0..depth {
            let mut next = Vec::new();
            for cell in frontier {
                for d in DIRS {
                    let there = (cell.0 + d.0, cell.1 + d.1);
                    if self.is_open(cell, d) && reached.insert(there) {
                        next.push(there);
                    }
                }
            }
            frontier = next;
        }
        reached
    }
}

/// The cells a piece covers, each with the sides the piece leaves open.
pub fn piece_cells(tiles: &TileSet, piece: &Piece) -> Vec<((i32, i32), Vec<Dir>)> {
    match *piece {
        Piece::Tile { kind, cell, turns } => match tiles.shape(kind) {
            TileShape::Pipe { axis } => {
                let a = rotate(*axis, turns);
                vec![(cell, vec![a, (-a.0, -a.1)])]
            }
            TileShape::Junction { arms } => {
                let arms: Vec<Dir> = arms.iter().map(|&a| rotate(a, turns)).collect();
                let mut cells = vec![(cell, arms.clone())];
                for d in arms {
                    cells.push(((cell.0 + d.0, cell.1 + d.1), vec![d, (-d.0, -d.1)]));
                }
                cells
            }
        },
        Piece::Wall { cell, .. } => vec![(cell, Vec::new())],
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// The tiles as they come from the models: a pipe along x, a corner to +x and -z, a T to +x,
    /// +z and -z, a cross.
    fn tiles() -> TileSet {
        TileSet {
            pipe: TileShape::Pipe { axis: (1, 0) },
            corner: TileShape::Junction {
                arms: vec![(1, 0), (0, -1)],
            },
            tee: TileShape::Junction {
                arms: vec![(1, 0), (0, 1), (0, -1)],
            },
            cross: TileShape::Junction {
                arms: DIRS.to_vec(),
            },
        }
    }

    #[test]
    fn rotation_is_a_quarter_turn_about_y() {
        assert_eq!(rotate((1, 0), 1), (0, -1));
        assert_eq!(rotate((0, -1), 1), (-1, 0));
        assert_eq!(rotate((1, 0), 4), (1, 0));
    }

    #[test]
    fn every_junction_is_reachable() {
        for seed in 1..50 {
            let maze = Maze::generate(7, 5, 0.2, &mut Rng::new(seed));
            assert_eq!(maze.reachable(), 35, "seed {seed}");
        }
    }

    #[test]
    fn links_agree_from_both_sides() {
        let maze = Maze::generate(6, 6, 0.3, &mut Rng::new(9));
        for z in 0..6 {
            for x in 0..6 {
                for d in maze.openings((x, z)) {
                    let there = ((x as i32 + d.0) as usize, (z as i32 + d.1) as usize);
                    assert!(maze.openings(there).contains(&(-d.0, -d.1)));
                }
            }
        }
    }

    /// Where each placed tile is open, cell by cell, in the maze's cells.
    fn open_sides(tiles: &TileSet, pieces: &[Piece]) -> (HashMap<(i32, i32), HashSet<Dir>>, usize) {
        let mut open: HashMap<(i32, i32), HashSet<Dir>> = HashMap::new();
        let mut walls = 0;
        for piece in pieces {
            match piece {
                Piece::Tile { kind, cell, turns } => match tiles.shape(*kind) {
                    TileShape::Pipe { axis } => {
                        let a = rotate(*axis, *turns);
                        let sides = open.entry(*cell).or_default();
                        assert!(sides.is_empty(), "two tiles in cell {cell:?}");
                        sides.insert(a);
                        sides.insert((-a.0, -a.1));
                    }
                    TileShape::Junction { arms } => {
                        let middle = open.entry(*cell).or_default();
                        assert!(middle.is_empty(), "two tiles in cell {cell:?}");
                        for &arm in arms {
                            open.get_mut(cell).unwrap().insert(rotate(arm, *turns));
                        }
                        for &arm in arms {
                            let d = rotate(arm, *turns);
                            let arm_cell = (cell.0 + d.0, cell.1 + d.1);
                            let sides = open.entry(arm_cell).or_default();
                            assert!(sides.is_empty(), "two tiles in cell {arm_cell:?}");
                            sides.insert(d);
                            sides.insert((-d.0, -d.1));
                        }
                    }
                },
                Piece::Wall { .. } => walls += 1,
            }
        }
        (open, walls)
    }

    #[test]
    fn every_opening_meets_another() {
        let tiles = tiles();
        for seed in 1..30 {
            let maze = Maze::generate(6, 5, 0.25, &mut Rng::new(seed));
            let pieces = tiles.build(&maze).unwrap();
            let (open, walls) = open_sides(&tiles, &pieces);
            for (cell, sides) in &open {
                for d in sides {
                    let next = (cell.0 + d.0, cell.1 + d.1);
                    let meets = open.get(&next).is_some_and(|s| s.contains(&(-d.0, -d.1)));
                    // A dead end's open side towards its own wall is closed by that wall.
                    let walled = pieces.iter().any(|p| {
                        matches!(p, Piece::Wall { cell: c, dir } if c == cell && dir == d)
                    });
                    assert!(meets || walled, "seed {seed}: cell {cell:?} opens {d:?} onto nothing");
                }
            }
            let dead_ends = (0..5)
                .flat_map(|z| (0..6).map(move |x| (x, z)))
                .filter(|&j| maze.openings(j).len() == 1)
                .count();
            assert_eq!(walls, dead_ends);
        }
    }

    #[test]
    fn a_tile_of_the_wrong_shape_is_reported() {
        let mut tiles = tiles();
        tiles.tee = TileShape::Junction {
            arms: vec![(1, 0), (0, 1)],
        };
        let maze = Maze::generate(6, 6, 0.0, &mut Rng::new(3));
        assert!(tiles.build(&maze).is_err());
    }
    fn built(seed: u64) -> (TileSet, Vec<Piece>) {
        let tiles = tiles();
        let maze = Maze::generate(6, 6, 0.2, &mut Rng::new(seed));
        let pieces = tiles.build(&maze).unwrap();
        (tiles, pieces)
    }

    #[test]
    fn a_straight_corridor_is_seen_to_its_end_and_no_further() {
        // A corridor of three pipes along x, closed at both ends.
        let tiles = tiles();
        let pieces = vec![
            Piece::Tile { kind: TileKind::Pipe, cell: (0, 0), turns: 0 },
            Piece::Tile { kind: TileKind::Pipe, cell: (1, 0), turns: 0 },
            Piece::Tile { kind: TileKind::Pipe, cell: (2, 0), turns: 0 },
            Piece::Wall { cell: (0, 0), dir: (-1, 0) },
            Piece::Wall { cell: (2, 0), dir: (1, 0) },
            // Another corridor alongside, behind the wall.
            Piece::Tile { kind: TileKind::Pipe, cell: (1, 1), turns: 0 },
        ];
        let map = CellMap::new(&tiles, &pieces);
        let seen = map.visible_from((0, 0), 50.0);
        assert!(seen.contains(&(2, 0)));
        assert!(!seen.contains(&(1, 1)), "seen through a wall");
        assert!(!seen.contains(&(3, 0)), "seen past a dead end");
    }

    #[test]
    fn a_corner_hides_what_is_around_it() {
        let (tiles, pieces, middle, turns) = (1..)
            .find_map(|seed| {
                let (tiles, pieces) = built(seed);
                let corner = pieces.iter().find_map(|p| match p {
                    Piece::Tile { kind: TileKind::Corner, cell, turns } => Some((*cell, *turns)),
                    _ => None,
                })?;
                Some((tiles, pieces, corner.0, corner.1))
            })
            .unwrap();
        let map = CellMap::new(&tiles, &pieces);
        // From the end of one arm, two cells down the other arm is out of sight.
        let arms: Vec<Dir> = match &tiles.corner {
            TileShape::Junction { arms } => arms.iter().map(|&a| rotate(a, turns)).collect(),
            _ => unreachable!(),
        };
        let (a, b) = (arms[0], arms[1]);
        let viewer = (middle.0 + 2 * a.0, middle.1 + 2 * a.1);
        let hidden = (middle.0 + 3 * b.0, middle.1 + 3 * b.1);
        let seen = map.visible_from(viewer, 50.0);
        assert!(seen.contains(&(middle.0 + a.0, middle.1 + a.1)));
        assert!(!seen.contains(&hidden), "saw around the corner");
    }

    #[test]
    fn everything_seen_is_reachable_floor() {
        let (tiles, pieces) = built(11);
        let map = CellMap::new(&tiles, &pieces);
        for (x, z) in [(0, 0), (3, 3), (6, 9)] {
            for cell in map.visible_from((x, z), 50.0) {
                assert!(map.open.contains_key(&cell), "saw {cell:?}, which has no floor");
            }
        }
    }

    #[test]
    fn lamps_around_a_corner_still_count() {
        let tiles = tiles();
        let pieces = vec![
            Piece::Tile { kind: TileKind::Pipe, cell: (0, 0), turns: 0 },
            Piece::Tile { kind: TileKind::Pipe, cell: (1, 0), turns: 0 },
            Piece::Tile { kind: TileKind::Pipe, cell: (2, 0), turns: 0 },
            Piece::Tile { kind: TileKind::Pipe, cell: (3, 0), turns: 0 },
        ];
        let map = CellMap::new(&tiles, &pieces);
        let seen = std::collections::HashSet::from([(0, 0)]);
        let lit = map.around(&seen, 2);
        assert!(lit.contains(&(2, 0)));
        assert!(!lit.contains(&(3, 0)));
    }
}
