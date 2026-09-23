//! Leaning out round the corner the droid is in cover behind: at the edge of the wall, holding
//! the key that would go on past it moves the head out that way, keeping it clear of the walls.

use super::{Player, FEET};
use fyrox::{core::algebra::Vector3, scene::graph::Graph};

/// How far the head moves out to the side when leaning, in meters.
pub(super) const LEAN_DISTANCE: f32 = 0.4;
/// How far the head tilts at a full lean, in degrees.
pub(super) const LEAN_TILT: f32 = 12.0;
/// How much lower the eyes are at a full lean: the body bends to the side.
pub(super) const LEAN_DIP: f32 = 0.06;
/// How far the head stays from a wall it leans towards, in meters. The camera's near plane is
/// 0.1 m, so this keeps the wall from being cut open.
const LEAN_CLEARANCE: f32 = 0.2;
/// How quickly the head moves into and out of a lean, like
/// [`EYE_EASING`](super::posture::EYE_EASING).
const LEAN_EASING: f32 = 14.0;

/// How far the head leans when there is `room` beside it: a full lean, or as far as keeps it
/// clear of whatever is there.
fn lean_reach(room: f32) -> f32 {
    (room - LEAN_CLEARANCE).clamp(0.0, LEAN_DISTANCE)
}

impl Player {
    /// Moves the lean towards where it should be: out round the corner while the droid is at
    /// the edge of the wall it is in cover against with a key held that way, as far as the room
    /// beside the head allows, and back in otherwise.
    ///
    /// The lean is along the body's `right`, as the head moves, so it is as much of the way round
    /// the corner as lies across the body: all of it looking at the wall, none looking along it.
    pub(super) fn fit_lean(&mut self, graph: &Graph, right: Vector3<f32>, dt: f32) {
        let head = graph[self.body].global_position() + Vector3::new(0.0, FEET + self.eyes, 0.0);
        let target = self.cover_peek().map_or(0.0, |way| {
            let room = self.distance_to_hit(graph, head, way, LEAN_DISTANCE + LEAN_CLEARANCE);
            way.dot(&right) * lean_reach(room)
        });
        self.lean += (target - self.lean) * (1.0 - (-LEAN_EASING * dt).exp());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lean_stops_short_of_a_wall() {
        assert_eq!(lean_reach(10.0), LEAN_DISTANCE, "open space: a full lean");
        assert!((lean_reach(0.35) - 0.15).abs() < 1e-6, "keeps its distance");
        assert_eq!(lean_reach(0.1), 0.0, "already against it: no lean");
    }
}
