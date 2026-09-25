//! How the player holds themselves - standing, crouching or crawling - and how fast they go in
//! each: walking, running or sprinting.

use super::{Player, FEET};
use fyrox::{
    core::algebra::Vector3,
    scene::{collider::ColliderShape, graph::Graph},
};

/// How fast each posture moves, in meters per second: walking, running and sprinting. Every
/// posture does all three, and each posture is slower than the one above it at every gait.
///
/// These are only for when the droid has not loaded. With it, the droid's feet set the speeds -
/// see [`Avatar::pace`](super::avatar::Avatar::pace) - and these are roughly what they come to.
const STANDING_SPEEDS: (f32, f32, f32) = (0.6, 2.0, 4.1);
const CROUCHING_SPEEDS: (f32, f32, f32) = (0.55, 0.75, 0.95);
const CRAWLING_SPEEDS: (f32, f32, f32) = (0.3, 0.4, 0.45);
/// How hard each posture can change how fast it is going, in meters per second squared.
/// Standing, that is next to no time to a walk and half a second to a sprint; crouched
/// or down on the floor there is much less to push off with.
pub(super) const STANDING_ACCELERATION: f32 = 8.0;
const CROUCHING_ACCELERATION: f32 = 5.0;
const CRAWLING_ACCELERATION: f32 = 2.5;
/// How quickly the eyes move to a new height: the share of the way left covered per second,
/// roughly - it takes about a quarter of a second to get there.
pub(super) const EYE_EASING: f32 = 12.0;

/// How the player holds themselves.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Posture {
    #[default]
    Standing,
    Crouching,
    Crawling,
}

impl Posture {
    /// What the crouch key (C) turns this into.
    pub(super) fn crouch_toggled(self) -> Self {
        match self {
            Posture::Standing | Posture::Crawling => Posture::Crouching,
            Posture::Crouching => Posture::Standing,
        }
    }

    /// What the crawl key (Z) turns this into.
    pub(super) fn crawl_toggled(self) -> Self {
        match self {
            Posture::Standing | Posture::Crouching => Posture::Crawling,
            Posture::Crawling => Posture::Standing,
        }
    }

    /// The body's shape, and how far its middle is above the feet.
    pub(super) fn shape(self) -> (ColliderShape, f32) {
        match self {
            // 1.7 m tall.
            Posture::Standing => (ColliderShape::capsule_y(0.5, 0.35), 0.85),
            // 1.1 m tall.
            Posture::Crouching => (ColliderShape::capsule_y(0.2, 0.35), 0.55),
            // 0.6 m tall: low enough to pass under most things.
            Posture::Crawling => (ColliderShape::ball(0.3), 0.3),
        }
    }

    /// How high the eyes are above the feet.
    pub(super) fn eyes(self) -> f32 {
        match self {
            Posture::Standing => 1.6,
            Posture::Crouching => 1.05,
            Posture::Crawling => 0.45,
        }
    }

    pub(super) fn speed(self, gait: Gait) -> f32 {
        let (walk, run, sprint) = match self {
            Posture::Standing => STANDING_SPEEDS,
            Posture::Crouching => CROUCHING_SPEEDS,
            Posture::Crawling => CRAWLING_SPEEDS,
        };
        match gait {
            Gait::Walking => walk,
            Gait::Running => run,
            Gait::Sprinting => sprint,
        }
    }

    pub(super) fn acceleration(self) -> f32 {
        match self {
            Posture::Standing => STANDING_ACCELERATION,
            Posture::Crouching => CROUCHING_ACCELERATION,
            Posture::Crawling => CRAWLING_ACCELERATION,
        }
    }
}

/// How fast the player is going, apart from their posture.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gait {
    #[default]
    Walking,
    Running,
    Sprinting,
}

impl Player {
    /// How the player holds themselves.
    pub(crate) fn posture(&self) -> Posture {
        self.posture
    }

    /// How fast the player is going: sprinting while Shift is held, and otherwise whichever of
    /// walking and running Caps Lock last left them in. Strafing, or with the pistol out, Shift
    /// only runs. Out of breath, or edging along a wall in cover, they only walk.
    pub(super) fn gait(&self) -> Gait {
        match (self.winded || self.in_cover(), self.keys.sprint, self.running) {
            (true, _, _) => Gait::Walking,
            (false, true, _) if self.strafing() => Gait::Running,
            (false, true, _) => Gait::Sprinting,
            (false, false, true) => Gait::Running,
            (false, false, false) => Gait::Walking,
        }
    }

    /// Fits the body's shape and the eyes to the posture.
    pub(super) fn fit_posture(&mut self, graph: &mut Graph, dt: f32) {
        if self.shaped_for != Some(self.posture) {
            let (shape, middle) = self.posture.shape();
            let collider = &mut graph[self.collider];
            collider.set_shape(shape);
            // Shrunk from the top: the bottom of the shape stays at the feet.
            collider
                .local_transform_mut()
                .set_position(Vector3::new(0.0, FEET + middle, 0.0));
            self.shaped_for = Some(self.posture);
        }
        let target = self.posture.eyes();
        self.eyes += (target - self.eyes) * (1.0 - (-EYE_EASING * dt).exp());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lower_posture_gets_going_more_slowly() {
        let postures = [Posture::Standing, Posture::Crouching, Posture::Crawling];
        for pair in postures.windows(2) {
            assert!(
                pair[1].acceleration() < pair[0].acceleration(),
                "{:?} gets going faster than {:?}",
                pair[1],
                pair[0]
            );
        }
    }

    #[test]
    fn each_gait_is_faster_in_every_posture_and_lower_is_slower() {
        let postures = [Posture::Standing, Posture::Crouching, Posture::Crawling];
        let gaits = [Gait::Walking, Gait::Running, Gait::Sprinting];
        for posture in postures {
            for pair in gaits.windows(2) {
                assert!(
                    posture.speed(pair[1]) > posture.speed(pair[0]),
                    "{posture:?}: {:?} is not faster than {:?}",
                    pair[1],
                    pair[0]
                );
            }
        }
        for pair in postures.windows(2) {
            for gait in gaits {
                assert!(
                    pair[1].speed(gait) < pair[0].speed(gait),
                    "{:?} is not slower than {:?} when {gait:?}",
                    pair[1],
                    pair[0]
                );
            }
        }
    }

    #[test]
    fn every_posture_keeps_its_feet_on_the_floor() {
        for posture in [Posture::Standing, Posture::Crouching, Posture::Crawling] {
            let (shape, middle) = posture.shape();
            let half_height = match shape {
                ColliderShape::Capsule(c) => (c.end - c.begin).norm() / 2.0 + c.radius,
                ColliderShape::Ball(b) => b.radius,
                _ => unreachable!(),
            };
            assert!((middle - half_height).abs() < 1e-5, "{posture:?}");
            assert!(posture.eyes() < 2.0 * half_height, "{posture:?}: eyes above the head");
        }
    }
}
