//! Leaning out round the corner the droid is in cover behind: at the edge of the wall, holding
//! the key that would go on past it - or aiming the pistol - moves the head out that way, and the
//! camera with it, keeping it clear of the walls.

use super::{Player, FEET};
use fyrox::{
    core::algebra::{UnitQuaternion, Vector3},
    scene::graph::Graph,
};

/// How far the head moves out round the corner when leaning, in meters: along the wall past its
/// end, and then across the end of it, past the line of the wall, so that the camera behind it
/// sees round the corner as the droid leans its head and the pistol out.
pub(super) const LEAN_DISTANCE: f32 = 1.0;
const LEAN_ACROSS: f32 = 0.6;
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
    /// The lean is the whole way round the corner, whichever way the body is turned by
    /// `rotation`: out to the side looking at the wall, and straight on looking along it, so the
    /// camera behind the head goes out round the corner too.
    pub(super) fn fit_lean(&mut self, graph: &Graph, rotation: UnitQuaternion<f32>, dt: f32) {
        let head = graph[self.body].global_position() + Vector3::new(0.0, FEET + self.eyes, 0.0);
        let target = self.cover_peek().map_or(Vector3::zeros(), |way| {
            let room = self.distance_to_hit(graph, head, way, LEAN_DISTANCE + LEAN_CLEARANCE);
            let out = way * lean_reach(room);
            // From out there, across the end of the wall toward its far side - as far as the wall
            // itself lets it, so only once the head is past the end.
            let across = self.cover_across().map_or(Vector3::zeros(), |across| {
                let room =
                    self.distance_to_hit(graph, head + out, across, LEAN_ACROSS + LEAN_CLEARANCE);
                across * lean_reach(room).min(LEAN_ACROSS)
            });
            rotation.inverse() * (out + across)
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
