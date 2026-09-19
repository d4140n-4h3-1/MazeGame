//! Where the player starts and where the exit goes.
//!
//! The maze comes from a model, so nothing says which parts of it are corridors. The game
//! samples the level on a grid instead: a cell is walkable when a ray dropped into it lands on the
//! floor rather than on top of a wall. Paths between cells then come from a breadth-first search,
//! which is what places the exit at the far end of the maze by walking distance, not by straight
//! line distance through the walls.

use std::collections::VecDeque;

/// A grid of walkable cells over the maze's footprint.
#[derive(Debug, Clone, PartialEq)]
pub struct WalkGrid {
    pub width: usize,
    pub depth: usize,
    cells: Vec<bool>,
}

impl WalkGrid {
    pub fn new(width: usize, depth: usize) -> Self {
        Self {
            width,
            depth,
            cells: vec![false; width * depth],
        }
    }

    pub fn set(&mut self, x: usize, z: usize, walkable: bool) {
        self.cells[z * self.width + x] = walkable;
    }

    pub fn is_walkable(&self, x: usize, z: usize) -> bool {
        self.cells[z * self.width + x]
    }

    pub fn walkable_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.depth)
            .flat_map(move |z| (0..self.width).map(move |x| (x, z)))
            .filter(|&(x, z)| self.is_walkable(x, z))
    }

    /// Walking distance, in cells, from `start` to every cell; `None` where it cannot be reached.
    pub fn distances_from(&self, start: (usize, usize)) -> Vec<Option<u32>> {
        let mut distances = vec![None; self.cells.len()];
        if !self.is_walkable(start.0, start.1) {
            return distances;
        }
        distances[start.1 * self.width + start.0] = Some(0);
        let mut queue = VecDeque::from([start]);
        while let Some((x, z)) = queue.pop_front() {
            let next = distances[z * self.width + x].unwrap() + 1;
            let neighbours = [
                (x.wrapping_sub(1), z),
                (x + 1, z),
                (x, z.wrapping_sub(1)),
                (x, z + 1),
            ];
            for (nx, nz) in neighbours {
                if nx >= self.width || nz >= self.depth || !self.is_walkable(nx, nz) {
                    continue;
                }
                let slot = &mut distances[nz * self.width + nx];
                if slot.is_none() {
                    *slot = Some(next);
                    queue.push_back((nx, nz));
                }
            }
        }
        distances
    }

    /// Returns the reachable cell that is the longest walk away from `start`, with its distance.
    pub fn farthest_from(&self, start: (usize, usize)) -> Option<((usize, usize), u32)> {
        self.distances_from(start)
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.map(|d| ((i % self.width, i / self.width), d)))
            .max_by_key(|&(_, d)| d)
    }

    /// Picks a start and an exit for a round: the start is chosen by `pick` among the cells of the
    /// largest connected area, and the exit is as far from it as the maze allows.
    pub fn plan_round(
        &self,
        mut pick: impl FnMut(usize) -> usize,
    ) -> Option<((usize, usize), (usize, usize))> {
        // Stray samples (a ledge, a gap outside the walls) form small islands of their own; the
        // round is played in the biggest connected area.
        let mut best: Vec<(usize, usize)> = Vec::new();
        let mut seen = vec![false; self.cells.len()];
        for (x, z) in self.walkable_cells() {
            if seen[z * self.width + x] {
                continue;
            }
            let area: Vec<(usize, usize)> = self
                .distances_from((x, z))
                .iter()
                .enumerate()
                .filter(|(_, d)| d.is_some())
                .map(|(i, _)| (i % self.width, i / self.width))
                .collect();
            for &(ax, az) in &area {
                seen[az * self.width + ax] = true;
            }
            if area.len() > best.len() {
                best = area;
            }
        }
        if best.len() < 2 {
            return None;
        }

        // A start in a random spot can land in the middle of the maze, which halves the walk;
        // starting from one end of the longest route keeps every round a full crossing.
        let seed = best[pick(best.len())];
        let (start, _) = self.farthest_from(seed)?;
        let (exit, _) = self.farthest_from(start)?;
        Some((start, exit))
    }
}

/// A small deterministic random number generator, so rounds do not need another dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 33) as usize % n.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a grid from rows of `#` (wall) and `.` (floor).
    fn grid(rows: &[&str]) -> WalkGrid {
        let mut grid = WalkGrid::new(rows[0].len(), rows.len());
        for (z, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                grid.set(x, z, c == '.');
            }
        }
        grid
    }

    #[test]
    fn distances_follow_corridors_not_straight_lines() {
        let maze = grid(&[
            ".#...", //
            ".#.#.", //
            "...#.", //
        ]);
        let distances = maze.distances_from((0, 0));
        // (2, 0) is two cells away in a straight line, but six along the corridor.
        assert_eq!(distances[2], Some(6));
        assert_eq!(distances[1], None);
        assert_eq!(maze.farthest_from((0, 0)), Some(((4, 2), 10)));
    }

    #[test]
    fn a_round_spans_the_longest_route() {
        let maze = grid(&[
            ".....", //
            "####.", //
            ".....", //
        ]);
        let (start, exit) = maze.plan_round(|n| n / 2).unwrap();
        let mut ends = [start, exit];
        ends.sort();
        assert_eq!(ends, [(0, 0), (0, 2)]);
    }

    #[test]
    fn a_round_ignores_small_islands() {
        let maze = grid(&[
            ".#....", //
            "##....", //
        ]);
        for i in 0..9 {
            let (start, exit) = maze.plan_round(|n| i % n).unwrap();
            assert!(start.0 >= 2 && exit.0 >= 2, "{start:?} {exit:?}");
        }
    }

    #[test]
    fn nothing_to_play_on_gives_no_round() {
        assert_eq!(grid(&["#.#"]).plan_round(|_| 0), None);
    }

    #[test]
    fn rng_stays_in_range() {
        let mut rng = Rng::new(7);
        assert!((0..1000).all(|_| rng.below(5) < 5));
    }
}
