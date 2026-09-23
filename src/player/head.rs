//! The head: how it is carried along with the body - dipping under a landing, rolling into
//! movement and turns - and turned round to look behind. This is
//! where everything that moves the camera comes together.

use super::{
    lean::{LEAN_DIP, LEAN_DISTANCE, LEAN_TILT},
    posture::Gait,
    Player, FEET,
};
use fyrox::{
    core::algebra::{UnitQuaternion, Vector3},
    scene::graph::Graph,
};

/// How far the knees give on landing, in meters per meter per second of fall, and the most they
/// can give however far the drop.
const LANDING_DIP: f32 = 0.03;
const LANDING_DIP_LIMIT: f32 = 0.18;
/// How quickly the knees straighten again afterwards, like
/// [`EYE_EASING`](super::posture::EYE_EASING).
const LANDING_EASING: f32 = 6.0;
/// How slowly the player has to be falling for the landing to pass unnoticed, in meters per
/// second: stepping off a kerb does not buckle the knees.
const LANDING_SOFT: f32 = 1.5;

/// How far the head rolls into running flat out sideways, in degrees.
const STRAFE_ROLL: f32 = 1.6;
/// How far it rolls into a turn, in degrees per radian per second of turning, and the most a turn
/// can roll it: a flick of the mouse is not a lean.
const TURN_ROLL: f32 = 1.2;
const TURN_ROLL_LIMIT: f32 = 3.0;
/// How quickly the roll follows what the body is doing, like
/// [`EYE_EASING`](super::posture::EYE_EASING).
const ROLL_EASING: f32 = 8.0;

/// How long the head takes to turn round to look behind, and back, in seconds.
const LOOK_BACK_TIME: f32 = 0.2;

/// How far the knees give under a landing at `fall_speed`, in meters. A gentle touch down costs
/// nothing; beyond that it goes with the speed of the fall, up to a limit.
fn landing_dip(fall_speed: f32) -> f32 {
    ((fall_speed - LANDING_SOFT).max(0.0) * LANDING_DIP).min(LANDING_DIP_LIMIT)
}

/// How far the head rolls, in degrees, for a body moving `sideways` meters per second out of a
/// top speed of `flat_out`, while turning at `yaw_rate` radians per second. Positive is to the
/// right, the same way round as a lean. Both lean into the movement, the way a body puts itself
/// over the foot it is about to need; a turn's share is capped, so that spinning on the spot
/// tips the horizon rather than rolling it over.
fn roll_degrees(sideways: f32, flat_out: f32, yaw_rate: f32) -> f32 {
    let strafe = (sideways / flat_out).clamp(-1.0, 1.0) * STRAFE_ROLL;
    // Turning right takes the yaw down, and rolls the head right.
    let turn = (-yaw_rate * TURN_ROLL).clamp(-TURN_ROLL_LIMIT, TURN_ROLL_LIMIT);
    strafe + turn
}

/// How far round the head is turned to look behind, from 0 (ahead) to 1 (behind).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct LookBack(f32);

impl LookBack {
    /// Turns the head towards behind while `held`, and back ahead otherwise.
    pub(super) fn advance(&mut self, held: bool, dt: f32) {
        let step = dt / LOOK_BACK_TIME;
        self.0 = if held { self.0 + step } else { self.0 - step }.clamp(0.0, 1.0);
    }

    /// How far the head is turned, in radians. Eased at both ends, so the turn starts and stops
    /// smoothly rather than snapping.
    pub(super) fn angle(self) -> f32 {
        let t = self.0;
        std::f32::consts::PI * t * t * (3.0 - 2.0 * t)
    }
}

impl Player {
    /// Bends the knees under a landing, at the speed the body was falling.
    pub(super) fn land(&mut self) {
        self.landing = landing_dip(self.fall_speed).max(self.landing);
    }

    /// Moves the head along with the body for this frame: the knees straightening after a
    /// landing, and the roll into where it is going. `horizontal` is how fast the body is
    /// travelling along the floor, and `right` is its right. The head does not bob in step with
    /// the stride: the camera is held steady, and in third person the droid's own cycles show it.
    pub(super) fn carry_head(&mut self, horizontal: Vector3<f32>, right: Vector3<f32>, dt: f32) {
        self.landing *= (-LANDING_EASING * dt).exp();

        let yaw_rate = if dt > 0.0 {
            (self.yaw - self.last_yaw) / dt
        } else {
            0.0
        };
        self.last_yaw = self.yaw;
        let wanted = roll_degrees(
            horizontal.dot(&right),
            self.top_speed(Gait::Sprinting),
            yaw_rate,
        )
        .to_radians();
        self.roll += (wanted - self.roll) * (1.0 - (-ROLL_EASING * dt).exp());
    }

    /// Puts the head where it is: at the eyes' height, moved by the landing and the lean, and turned by the look behind, the pitch, the lean and the roll. The camera goes
    /// there, or behind it - see [`Player::place_camera`].
    pub(super) fn place_head(&mut self, graph: &mut Graph, dt: f32) {
        let leaning = self.lean / LEAN_DISTANCE;
        // The head turns on the body; walking goes by the body, so it carries on ahead. A lean
        // moves it out to the side (the body's right is its -x) and tilts it the same way.
        let head = Vector3::new(
            -self.lean,
            FEET + self.eyes - LEAN_DIP * leaning.abs() - self.landing,
            0.0,
        );
        let tilt = UnitQuaternion::from_axis_angle(
            &Vector3::z_axis(),
            leaning * LEAN_TILT.to_radians() + self.roll,
        );
        let turn = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.look_back.angle())
            * UnitQuaternion::from_axis_angle(&Vector3::x_axis(), self.pitch)
            * tilt;
        self.place_camera(graph, head, turn, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fyrox::keyboard::KeyCode;

    #[test]
    fn the_knees_give_with_the_speed_of_the_fall_but_only_so_far() {
        assert_eq!(landing_dip(1.0), 0.0, "stepping down costs nothing");
        assert!(landing_dip(5.0) > landing_dip(3.0), "a longer drop dips further");
        assert_eq!(landing_dip(1000.0), LANDING_DIP_LIMIT, "and no further than that");
    }

    #[test]
    fn the_head_rolls_into_its_movement() {
        // Strafing right rolls right, the same way round as a lean to the right.
        assert!(roll_degrees(5.0, 6.5, 0.0) > 0.0);
        assert!(roll_degrees(-5.0, 6.5, 0.0) < 0.0);
        assert_eq!(roll_degrees(0.0, 6.5, 0.0), 0.0, "straight ahead, on the level");
        // Turning right takes the yaw down.
        assert!(roll_degrees(0.0, 6.5, -1.0) > 0.0, "turning right rolls right");
        assert!(roll_degrees(0.0, 6.5, 1.0) < 0.0);
        // Strafing right while turning right rolls further than either alone.
        let both = roll_degrees(5.0, 6.5, -1.0);
        assert!(both > roll_degrees(5.0, 6.5, 0.0) && both > roll_degrees(0.0, 6.5, -1.0));
    }

    #[test]
    fn a_flick_of_the_mouse_does_not_roll_the_horizon_over() {
        let flick = roll_degrees(0.0, 6.5, -50.0);
        assert_eq!(flick, TURN_ROLL_LIMIT);
        // And the whole roll stays modest even doing everything at once.
        assert!(roll_degrees(100.0, 6.5, -50.0) < 5.0);
    }

    #[test]
    fn holding_q_turns_the_head_round_and_letting_go_turns_it_back() {
        let mut player = Player::default();
        player.on_key(KeyCode::KeyQ, true);
        for _ in 0..30 {
            player.look_back.advance(player.keys.look_back, 1.0 / 60.0);
        }
        assert!((player.look_back.angle() - std::f32::consts::PI).abs() < 1e-5, "behind");
        player.on_key(KeyCode::KeyQ, false);
        for _ in 0..30 {
            player.look_back.advance(player.keys.look_back, 1.0 / 60.0);
        }
        assert_eq!(player.look_back.angle(), 0.0, "ahead again");
    }

    #[test]
    fn the_turn_starts_and_ends_gently() {
        let angle = |t: f32| LookBack(t).angle();
        // Slow near the ends, fast in the middle.
        assert!(angle(0.1) - angle(0.0) < angle(0.55) - angle(0.45));
        assert!(angle(1.0) - angle(0.9) < angle(0.55) - angle(0.45));
    }
}
