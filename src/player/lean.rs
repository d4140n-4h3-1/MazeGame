//! Leaning round corners with Ctrl: picking the side the corner is on, and keeping the head clear
//! of the walls on the way out.

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
/// How far ahead to look for a corner to lean round, in meters, and how finely.
const PEEK_AHEAD: f32 = 8.0;
const PEEK_STEP: f32 = 0.25;
/// How much further the space to one side has to reach than it does beside the player, in
/// meters, for the wall on that side to have ended: a corner.
const PEEK_OPENING: f32 = 1.5;
/// How far the rays feeling for walls to the side reach, in meters.
const PEEK_SIDEWAYS: f32 = 12.0;
/// What to multiply the body's `right` by to send the head to the player's left, and to their
/// right. They read the wrong way round, because the vector the rest of the player's code calls
/// `right` comes out running to the player's left. These are set from which way the head is seen
/// to go on screen rather than worked out from the axes, so they are the one place to change if
/// the lean ever comes out mirrored again - and see [`lean_multiplier`].
const TO_LEFT: f32 = 1.0;
const TO_RIGHT: f32 = -1.0;
/// How quickly the head moves into and out of a lean, like
/// [`EYE_EASING`](super::posture::EYE_EASING).
const LEAN_EASING: f32 = 14.0;

/// What is to one side, between the head and the wall in front.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Peek {
    /// The furthest the space on this side reaches, anywhere along the way. A wall running the
    /// whole length keeps this down to the width of the corridor; a corner lets it run away.
    open: f32,
    /// How far ahead the side first opens out past the wall running along it, if it does at all.
    opening_at: Option<f32>,
    /// How far the head can actually lean this way before it would touch something.
    reach: f32,
}

/// The side to lean to, -1 left or 1 right.
///
/// A corner is where the space to one side opens out and the space to the other does not: at a
/// left turn the left runs away down the new corridor while the right stays up against the wall.
/// That difference is what decides it, and it reads the same whether the corner is still some way
/// off or the player has walked right up to it - which is when they most want to peek, and which
/// is where asking whether the space *widens* from here on cannot help, because by then it has
/// already widened.
///
/// Only when both sides open out much the same does anything else get a say: the stem of a T or a
/// crossroads goes to the nearer corner, then to the side with more room to lean into, and
/// failing all of that to the right.
fn pick_side(left: Peek, right: Peek) -> f32 {
    if (left.open - right.open).abs() > PEEK_OPENING {
        return if left.open > right.open { -1.0 } else { 1.0 };
    }
    match (left.opening_at, right.opening_at) {
        (Some(_), None) => return -1.0,
        (None, Some(_)) => return 1.0,
        (Some(l), Some(r)) if (l - r).abs() > PEEK_STEP * 1.5 => {
            return if l < r { -1.0 } else { 1.0 };
        }
        _ => {}
    }
    if left.reach > right.reach + 0.01 {
        -1.0
    } else {
        1.0
    }
}

/// Turns [`pick_side`]'s answer, which is in the player's own terms, into the multiplier on the
/// body's `right` that actually takes the head that way.
fn lean_multiplier(pick: f32) -> f32 {
    if pick < 0.0 {
        TO_LEFT
    } else {
        TO_RIGHT
    }
}

/// How far the head leans when there is `room` beside it: a full lean, or as far as keeps it
/// clear of whatever is there.
fn lean_reach(room: f32) -> f32 {
    (room - LEAN_CLEARANCE).clamp(0.0, LEAN_DISTANCE)
}

impl Player {
    /// Moves the lean towards where it should be: out to the chosen side while Ctrl is held, as
    /// far as the room beside the head allows. `forward` is where the head faces.
    pub(super) fn fit_lean(
        &mut self,
        graph: &Graph,
        right: Vector3<f32>,
        forward: Vector3<f32>,
        dt: f32,
    ) {
        let head = graph[self.body].global_position() + Vector3::new(0.0, FEET + self.eyes, 0.0);
        if !self.keys.lean {
            self.lean_side = None;
        } else if self.lean_side.is_none() {
            self.lean_side = Some(self.choose_lean_side(graph, head, right, forward));
        }
        let mut target = 0.0;
        if let Some(side) = self.lean_side {
            let room = self.room_beside(graph, head, right * side);
            target = side * lean_reach(room);
        }
        self.lean += (target - self.lean) * (1.0 - (-LEAN_EASING * dt).exp());
    }

    /// Which side to lean to: the side the corner is on. Rays are cast out to both sides from
    /// the head and from points further and further ahead of it, up to the wall in front, which
    /// measures how much room there is either side the whole way along. [`pick_side`] reads the
    /// corner off the difference between the two sides.
    fn choose_lean_side(
        &self,
        graph: &Graph,
        head: Vector3<f32>,
        right: Vector3<f32>,
        forward: Vector3<f32>,
    ) -> f32 {
        // Up to the wall ahead, keeping clear of it.
        let ahead = self.distance_to_hit(graph, head, forward, PEEK_AHEAD) - 0.3;
        let side = |side: f32| -> Peek {
            // Always at least the one beside the head, however close the wall in front is.
            let mut across = Vec::new();
            let mut at = 0.0;
            loop {
                across.push((
                    at,
                    self.distance_to_hit(graph, head + forward * at, right * side, PEEK_SIDEWAYS),
                ));
                at += PEEK_STEP;
                if at >= ahead {
                    break;
                }
            }
            let open = across.iter().fold(0.0f32, |most, &(_, d)| most.max(d));
            // The wall's own line, taken as the narrowest the side ever gets rather than whatever
            // happens to be beside the head, which may already be the opening.
            let wall = across.iter().fold(f32::MAX, |least, &(_, d)| least.min(d));
            let opening_at = across
                .iter()
                .find(|&&(_, d)| d > wall + PEEK_OPENING)
                .map(|&(at, _)| at);
            Peek {
                open,
                opening_at,
                reach: lean_reach(self.room_beside(graph, head, right * side)),
            }
        };
        lean_multiplier(pick_side(side(TO_LEFT), side(TO_RIGHT)))
    }

    /// How far the head could move in `direction` before touching something, up to a little
    /// more than a full lean.
    fn room_beside(&self, graph: &Graph, head: Vector3<f32>, direction: Vector3<f32>) -> f32 {
        self.distance_to_hit(graph, head, direction, LEAN_DISTANCE + LEAN_CLEARANCE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A corridor 2 m across: the wall on this side is a meter off, the whole way.
    const WALL: f32 = 1.0;
    /// A side passage runs away out of sight.
    const PASSAGE: f32 = PEEK_SIDEWAYS;

    fn peek(open: f32, opening_at: Option<f32>, reach: f32) -> Peek {
        Peek {
            open,
            opening_at,
            reach,
        }
    }

    /// A wall down one side, with nothing to peek round.
    fn blank() -> Peek {
        peek(WALL, None, 0.4)
    }

    /// A corner coming up `at` meters ahead.
    fn corner(at: f32) -> Peek {
        peek(PASSAGE, Some(at), 0.4)
    }

    #[test]
    fn a_left_corner_peeks_left_and_a_right_corner_peeks_right() {
        assert_eq!(pick_side(corner(2.0), blank()), -1.0, "corner on the left");
        assert_eq!(pick_side(blank(), corner(2.0)), 1.0, "corner on the right");
    }

    #[test]
    fn the_head_goes_to_the_side_that_was_picked() {
        // The two sides are opposite ways along the same vector, whichever way round they are.
        assert_eq!(TO_LEFT, -TO_RIGHT);
        assert_eq!(lean_multiplier(pick_side(corner(2.0), blank())), TO_LEFT);
        assert_eq!(lean_multiplier(pick_side(blank(), corner(2.0))), TO_RIGHT);
        // And the scans are labelled with the same two, so what `pick_side` is told is the left
        // is what the head leans to when it answers left.
        assert_eq!(lean_multiplier(-1.0), TO_LEFT);
        assert_eq!(lean_multiplier(1.0), TO_RIGHT);
    }

    #[test]
    fn it_still_knows_the_side_standing_right_at_the_corner() {
        // Walked up to a left turn and stopped in the mouth of it: the left is wide open from
        // the head onwards, so nothing about it *widens* from here - it has already widened.
        let standing_in_it = peek(PASSAGE, None, 0.4);
        assert_eq!(pick_side(standing_in_it, blank()), -1.0, "left turn, leaning left");
        assert_eq!(pick_side(blank(), standing_in_it), 1.0, "right turn, leaning right");
    }

    #[test]
    fn it_leans_round_the_corner_even_pressed_against_that_wall() {
        // Up against the right-hand wall at a right turn: barely any room to lean, still right.
        assert_eq!(pick_side(blank(), peek(PASSAGE, Some(1.0), 0.15)), 1.0);
    }

    #[test]
    fn a_doorway_on_the_wrong_side_does_not_pull_the_lean_off_the_corner() {
        // A recess a meter deep on the right, a real corridor on the left. The recess is nearer,
        // but it is not what anyone is going to come round.
        let recess = peek(WALL + 1.0, Some(0.5), 0.4);
        assert_eq!(pick_side(corner(3.0), recess), -1.0);
    }

    #[test]
    fn the_nearer_corner_wins_when_both_sides_open_out_alike() {
        // A crossroads: the choice is not about which side is more open, so it falls to which
        // corner is nearer.
        assert_eq!(pick_side(corner(1.0), corner(5.0)), -1.0);
        assert_eq!(pick_side(corner(5.0), corner(1.0)), 1.0);
    }

    #[test]
    fn with_no_corner_or_two_alike_it_leans_into_the_room() {
        assert_eq!(pick_side(blank(), peek(WALL, None, 0.1)), -1.0, "more room on the left");
        assert_eq!(pick_side(blank(), blank()), 1.0, "a blank corridor: right");
        // The stem of a T: a corner both ways at once, at the same distance.
        assert_eq!(
            pick_side(peek(PASSAGE, Some(2.0), 0.1), peek(PASSAGE, Some(2.0), 0.4)),
            1.0
        );
    }

    #[test]
    fn a_hair_more_room_on_one_side_is_not_a_corner() {
        // Standing off-centre in a straight corridor is not something to lean round, so the
        // openness has to differ by a real margin before it counts.
        let (near, far) = (peek(0.6, None, 0.2), peek(1.4, None, 0.4));
        assert_eq!(pick_side(near, far), 1.0, "falls through to the room, not the gap");
        assert_eq!(pick_side(far, near), -1.0);
    }

    #[test]
    fn a_lean_stops_short_of_a_wall() {
        assert_eq!(lean_reach(10.0), LEAN_DISTANCE, "open space: a full lean");
        assert!((lean_reach(0.35) - 0.15).abs() < 1e-6, "keeps its distance");
        assert_eq!(lean_reach(0.1), 0.0, "already against it: no lean");
    }
}
