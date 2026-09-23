//! Moving the body: speeding up and slowing down along the floor, jumping, and feeling for the
//! floor under the feet.

use super::{posture::Posture, Player, FEET};
use fyrox::{core::algebra::Vector3, scene::graph::Graph};

/// How much harder the player slows down than speeds up: stopping only needs the feet planted,
/// where getting going has to push a whole body along.
const BRAKING: f32 = 1.6;
/// How fast a jump leaves the ground, in meters per second: a high jump, about a meter.
const JUMP_SPEED: f32 = 4.5;
/// How long Space can be held, in seconds, and still be a tap, for a low jump.
const TAP: f32 = 0.15;
/// How fast a low jump is still rising once Space is let go, in meters per second, at most: it
/// tops out about half a meter up.
const LOW_JUMP_SPEED: f32 = 1.5;
/// How far below the feet to look for a floor, in meters. Slack enough that resting on one, with
/// the small overlaps the solver leaves, still reads as standing on it.
const GROUND_REACH: f32 = 0.15;
/// The share of the usual acceleration the player has in the air. Feet push against a floor, not
/// against air; but a jump that cannot be steered at all feels like being on rails, so not zero.
const AIR_CONTROL: f32 = 0.15;

/// Moves `velocity` towards `target` by as much as `acceleration` allows in `dt`, and no further.
///
/// A straight line towards the target rather than an exponential ease: real legs add speed at a
/// steady rate and then arrive, instead of creeping up on top speed for ever. Because it works on
/// the velocity as a vector, turning while moving sweeps round at the same rate, so a change of
/// direction at a sprint carries wide rather than pivoting on the spot.
fn ramp(velocity: Vector3<f32>, target: Vector3<f32>, acceleration: f32, dt: f32) -> Vector3<f32> {
    // Slowing down is quicker than speeding up, whether that is stopping or dropping from a
    // sprint to a walk.
    let rate = if target.norm_squared() < velocity.norm_squared() {
        acceleration * BRAKING
    } else {
        acceleration
    };
    let change = target - velocity;
    let distance = change.norm();
    let step = rate * dt;
    if distance <= step {
        target
    } else {
        velocity + change.scale(step / distance)
    }
}

impl Player {
    /// Whether the feet have something under them. A ray straight down from a little above them,
    /// reaching a little below: the body's own collider is passed over, so it can start inside it.
    pub(super) fn on_ground(&self, graph: &Graph) -> bool {
        let feet =
            graph[self.body].global_position() + Vector3::new(0.0, FEET + GROUND_REACH, 0.0);
        let reach = GROUND_REACH * 2.0;
        self.distance_to_hit(graph, feet, -Vector3::y(), reach) < reach
    }

    /// Pushes the body the way the keys ask for this frame, and jumps if they ask for that.
    /// `forward` and `right` are the body's own. A droid's `skid` under way carries the body
    /// instead, at its own speed, while the feet are on the ground. Returns how fast the body is
    /// now travelling along the floor, whether it jumped, and whether the jump it is in turned
    /// out to be a tap, and so a low one.
    pub(super) fn drive(
        &mut self,
        graph: &mut Graph,
        forward: Vector3<f32>,
        right: Vector3<f32>,
        can_move: bool,
        skid: Option<Vector3<f32>>,
        dt: f32,
    ) -> (Vector3<f32>, bool, bool) {
        let mut wish = Vector3::zeros();
        if can_move {
            let keys = &self.keys;
            if keys.forward {
                wish += forward;
            }
            if keys.back {
                wish -= forward;
            }
            if keys.right {
                wish += right;
            }
            if keys.left {
                wish -= right;
            }
        }
        let speed = self.top_speed(self.gait());
        // In cover, the wall has its say in where the body goes.
        let target = self.keep_cover(graph, wish, speed).unwrap_or_else(|| {
            wish.try_normalize(f32::EPSILON)
                .map_or(Vector3::zeros(), |dir| dir.scale(speed))
        });

        let body = &mut graph[self.body];
        let mut velocity = body.lin_vel();
        self.fall_speed = (-velocity.y).max(0.0);
        // Starting from what the body is actually doing, not from what it was asked for last
        // frame, so that a wall it has been pushed to a stop against has to be accelerated away
        // from again. In the air there is next to nothing to push with.
        let push = if self.grounded {
            self.posture.acceleration()
        } else {
            self.posture.acceleration() * AIR_CONTROL
        };
        let horizontal = match skid.filter(|_| self.grounded) {
            Some(skid) => Vector3::new(skid.x, 0.0, skid.z),
            None => ramp(
                Vector3::new(velocity.x, 0.0, velocity.z),
                target,
                push,
                dt,
            ),
        };
        velocity.x = horizontal.x;
        velocity.z = horizontal.z;
        // Every jump starts high. Let go of quickly, it is cut short into a low one.
        let mut low = false;
        if let Some(since) = self.since_jump.as_mut() {
            *since += dt;
            if !self.keys.jump {
                if *since < TAP && velocity.y > LOW_JUMP_SPEED {
                    velocity.y = LOW_JUMP_SPEED;
                    low = true;
                }
                self.since_jump = None;
            } else if *since >= TAP {
                self.since_jump = None;
            }
        }
        self.jump_spent &= self.keys.jump;
        let jumped = can_move
            && self.keys.jump
            && !self.jump_spent
            && self.posture == Posture::Standing
            && self.grounded;
        if jumped {
            self.cover = None;
            velocity.y = JUMP_SPEED;
            self.jump_spent = true;
            self.since_jump = Some(0.0);
        }
        body.set_lin_vel(velocity);
        (horizontal, jumped, low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::posture::STANDING_ACCELERATION;

    /// Runs `ramp` at 60 Hz until the velocity settles on `target`, and returns how long it took
    /// in seconds, along with the top speed seen on the way.
    fn ramp_to(from: Vector3<f32>, target: Vector3<f32>, acceleration: f32) -> (f32, f32) {
        let dt = 1.0 / 60.0;
        let mut velocity = from;
        let mut fastest: f32 = velocity.norm();
        for step in 1..600 {
            velocity = ramp(velocity, target, acceleration, dt);
            fastest = fastest.max(velocity.norm());
            if velocity == target {
                return (step as f32 * dt, fastest);
            }
        }
        panic!("never got to {target:?} from {from:?}");
    }

    fn forward(speed: f32) -> Vector3<f32> {
        Vector3::new(0.0, 0.0, speed)
    }

    #[test]
    fn getting_up_to_speed_takes_about_the_speed_over_the_acceleration() {
        let (seconds, _) = ramp_to(Vector3::zeros(), forward(6.0), 8.0);
        assert!((seconds - 0.75).abs() < 0.05, "{seconds} s to 6 m/s at 8 m/s^2");
        // Twice the speed off a standstill is twice the wait.
        let (twice, _) = ramp_to(Vector3::zeros(), forward(12.0), 8.0);
        assert!((twice - 2.0 * seconds).abs() < 0.05, "{twice} s");
    }

    #[test]
    fn it_arrives_at_the_target_without_overshooting_it() {
        let (_, fastest) = ramp_to(Vector3::zeros(), forward(5.0), 8.0);
        assert!(fastest <= 5.0 + 1e-5, "overshot to {fastest} m/s");
        // A step longer than the whole gap lands on the target rather than flying past it.
        assert_eq!(ramp(Vector3::zeros(), forward(5.0), 8.0, 10.0), forward(5.0));
    }

    #[test]
    fn stopping_is_quicker_than_starting() {
        let (starting, _) = ramp_to(Vector3::zeros(), forward(5.0), 8.0);
        let (stopping, _) = ramp_to(forward(5.0), Vector3::zeros(), 8.0);
        assert!(stopping < starting, "{stopping} s to stop, {starting} s to start");
        assert!((starting / stopping - BRAKING).abs() < 0.1);
        // Dropping from a sprint to a walk brakes too, rather than easing down.
        let (slowing, _) = ramp_to(forward(6.5), forward(2.9), 8.0);
        let (speeding, _) = ramp_to(forward(2.9), forward(6.5), 8.0);
        assert!(slowing < speeding, "{slowing} s down, {speeding} s up");
    }

    #[test]
    fn turning_at_speed_carries_wide() {
        // Hard about, at a speed that takes a moment to turn round.
        let speed = 5.0;
        let mut velocity = forward(speed);
        velocity = ramp(velocity, -forward(speed), 8.0, 1.0 / 60.0);
        assert!(velocity.z < speed, "still going as fast as it was forwards");
        assert!(velocity.z > 0.0, "snapped round instead of carrying on");
    }

    #[test]
    fn there_is_far_less_to_push_against_in_the_air() {
        let dt = 1.0 / 60.0;
        let target = Vector3::new(5.0, 0.0, 0.0);
        let ground = ramp(Vector3::zeros(), target, STANDING_ACCELERATION, dt);
        let air = ramp(Vector3::zeros(), target, STANDING_ACCELERATION * AIR_CONTROL, dt);
        assert!(air.norm() < ground.norm() * 0.5, "a jump can still be steered, barely");
        assert!(air.norm() > 0.0, "but not steered at all is being on rails");
    }
}
