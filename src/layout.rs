//! Where the player starts and where the exit goes.
//!
//! The maze comes from a model, so nothing says which parts of it are corridors. The game
//! samples the level on a grid instead: a cell is walkable when a ray dropped into it lands on the
//! floor rather than on top of a wall. Paths between cells then come from a breadth-first search,
//! which is what places the exit at the far end of the maze by walking distance, not by straight
//! line distance through the walls. The maze's inhabitants find their way about on the same grid
//! (see [`WalkGrid::routes_from`]). The grid itself is `hydroxus-ai`'s.

pub use hydroxus_ai::grid::{Rng, WalkGrid};

/// Picks a start and an exit for a round on `grid`: the start is chosen by `pick` among the
/// cells of the largest connected area, and the exit is as far from it as the maze allows.
pub fn plan_round(
    grid: &WalkGrid,
    mut pick: impl FnMut(usize) -> usize,
) -> Option<((usize, usize), (usize, usize))> {
    // Stray samples (a ledge, a gap outside the walls) form small islands of their own; the
    // round is played in the biggest connected area.
    let mut best: Vec<(usize, usize)> = Vec::new();
    let mut seen = vec![false; grid.width * grid.depth];
    for (x, z) in grid.walkable_cells() {
        if seen[z * grid.width + x] {
            continue;
        }
        let area: Vec<(usize, usize)> = grid
            .distances_from((x, z))
            .iter()
            .enumerate()
            .filter(|(_, d)| d.is_some())
            .map(|(i, _)| (i % grid.width, i / grid.width))
            .collect();
        for &(ax, az) in &area {
            seen[az * grid.width + ax] = true;
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
    let (start, _) = grid.farthest_from(seed)?;
    let (exit, _) = grid.farthest_from(start)?;
    Some((start, exit))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a grid from rows of `#` (wall) and `.` (floor).
    fn grid(rows: &[&str]) -> WalkGrid {
        let mut grid = WalkGrid::new(rows[0].len(), rows.len(), 1.0);
        for (z, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                grid.set(x, z, c == '.');
            }
        }
        grid
    }

    #[test]
    fn a_round_spans_the_longest_route() {
        let maze = grid(&[
            ".....", //
            "####.", //
            ".....", //
        ]);
        let (start, exit) = plan_round(&maze, |n| n / 2).unwrap();
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
            let (start, exit) = plan_round(&maze, |n| i % n).unwrap();
            assert!(start.0 >= 2 && exit.0 >= 2, "{start:?} {exit:?}");
        }
    }

    #[test]
    fn nothing_to_play_on_gives_no_round() {
        assert_eq!(plan_round(&grid(&["#.#"]), |_| 0), None);
    }
}
