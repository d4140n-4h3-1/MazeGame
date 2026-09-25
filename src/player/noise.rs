//! What the player makes heard, for the droids hunting them to hear (see
//! [`crate::inhabitants`]): footfalls running and sprinting, landing from a fall, and the pistol
//! - its shot where it is fired, and its bolt where it hits, which can draw a droid away from
//! the player. Walking, crouched or crawling, the player makes no sound.
//!
//! Each noise is where it is made and how far off it carries, in meters along the corridors.

use super::{posture::Gait, posture::Posture, Player};
use fyrox::core::algebra::Vector3;

/// How far off footfalls carry, running and sprinting, in meters.
const RUN_NOISE: f32 = 5.0;
const SPRINT_NOISE: f32 = 10.0;
/// How far off landing from a fall carries, in meters, and how fast the body has to be falling,
/// in meters per second, for it to make any.
const LANDING_NOISE: f32 = 6.0;
const LOUD_LANDING: f32 = 3.0;
/// How far off the pistol's shot carries, and a bolt hitting something, in meters.
pub(super) const SHOT_NOISE: f32 = 12.0;
pub(super) const IMPACT_NOISE: f32 = 8.0;
/// How often footfalls are heard, in seconds: about a step's time.
const FOOTFALL: f32 = 0.3;
/// How fast the body has to be going, in meters per second, for its feet to be heard.
const HEARD_MOVING: f32 = 0.5;

#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Noises {
    /// What has been made heard since the game last asked: where, and how far off it carries.
    made: Vec<(Vector3<f32>, f32)>,
    /// How long until the next footfall is heard, in seconds.
    next_footfall: f32,
}

/// How far off footfalls carry at `gait` in `posture`: nowhere, but for running and sprinting
/// on their feet.
fn footfall(posture: Posture, gait: Gait) -> f32 {
    match (posture, gait) {
        (Posture::Standing, Gait::Sprinting) => SPRINT_NOISE,
        (Posture::Standing, Gait::Running) => RUN_NOISE,
        _ => 0.0,
    }
}

impl Player {
    /// Makes `loudness` heard at `at`.
    pub(super) fn make_noise(&mut self, at: Vector3<f32>, loudness: f32) {
        if loudness > 0.0 {
            self.noises.made.push((at, loudness));
        }
    }

    /// The feet at `feet`, going `speed` along the ground at `gait`, for another `dt`: heard a
    /// step at a time, if they are loud enough to be.
    pub(super) fn footfalls(&mut self, feet: Vector3<f32>, speed: f32, gait: Gait, dt: f32) {
        self.noises.next_footfall -= dt;
        if !self.grounded || speed < HEARD_MOVING {
            return;
        }
        if self.noises.next_footfall <= 0.0 {
            self.noises.next_footfall = FOOTFALL;
            self.make_noise(feet, footfall(self.posture, gait));
        }
    }

    /// Landing at `feet`, falling as fast as the body was: heard if it was a hard landing.
    pub(super) fn thud(&mut self, feet: Vector3<f32>) {
        if self.fall_speed > LOUD_LANDING {
            self.make_noise(feet, LANDING_NOISE);
        }
    }

    /// What the player has made heard since this was last asked: where, and how far off it
    /// carries, in meters.
    pub fn noises(&mut self) -> Vec<(Vector3<f32>, f32)> {
        std::mem::take(&mut self.noises.made)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_running_and_sprinting_on_their_feet_are_heard() {
        assert_eq!(footfall(Posture::Standing, Gait::Walking), 0.0);
        let (sprint, run) = (
            footfall(Posture::Standing, Gait::Sprinting),
            footfall(Posture::Standing, Gait::Running),
        );
        assert!(sprint > run && run > 0.0);
        assert_eq!(footfall(Posture::Crouching, Gait::Sprinting), 0.0);
        assert_eq!(footfall(Posture::Crawling, Gait::Running), 0.0);
    }

    #[test]
    fn a_step_is_heard_once_per_footfall() {
        let mut player = Player {
            grounded: true,
            ..Default::default()
        };
        let feet = Vector3::zeros();
        for _ in 0..30 {
            player.footfalls(feet, 3.0, Gait::Sprinting, 0.01);
        }
        assert_eq!(player.noises().len(), 1, "a third of a second, one step");
        player.footfalls(feet, 1.0, Gait::Walking, 1.0);
        assert!(player.noises().is_empty(), "walking is quiet");
    }
}
