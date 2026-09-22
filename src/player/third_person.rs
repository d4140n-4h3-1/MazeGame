//! Seeing the player from behind: the camera held back over the droid's shoulder, pulled in
//! wherever a wall would come between it and the head, and V to go between that and seeing
//! through the droid's own eyes.
//!
//! Holding the middle mouse button swings the camera round the droid instead of turning it, to
//! see it from any side - from first person too, which shows the droid until the button is let
//! go. Let go, the camera swings back behind it.
//!
//! Everything that moves the head - the bob, the landing, the lean, the look behind - still moves
//! it in third person; the camera is carried along behind it. The flashlight stays with the
//! droid rather than the camera, so it lights what is in front of the droid instead of the back
//! of its head.

use super::{avatar::Avatar, Player};
use fyrox::{
    core::algebra::{UnitQuaternion, Vector3},
    graph::SceneGraph,
    scene::{graph::Graph, node::Node},
};

/// How far behind the head the camera is held, in meters, how far above it, and how far out to
/// the side, over the right shoulder, so the droid does not stand in the middle of the view. The
/// body's right is its +x - see `TO_RIGHT` in [`lean`](super::lean).
pub(super) const BOOM_LENGTH: f32 = 2.6;
const BOOM_RISE: f32 = 0.3;
const BOOM_SHOULDER: f32 = 0.45;
/// How far the camera keeps from a wall behind it, in meters. The near plane is 0.1 m.
const BOOM_CLEARANCE: f32 = 0.25;
/// How quickly the camera swings back out once a wall is out of the way, like
/// [`EYE_EASING`](super::posture::EYE_EASING). It goes in at once, so no wall is ever seen
/// through.
const BOOM_EASING: f32 = 4.0;
/// With the camera pulled in closer than this to the head, in meters, the droid is hidden: all
/// that could be seen of it is the inside of its head.
const HIDE_WITHIN: f32 = 0.6;
/// The capsule round the droid for the graphics effects, in meters: how tall, and how far out
/// from its middle. Wide enough for its arms, held out a little from its sides.
const CAPSULE_HEIGHT: f32 = 1.75;
const CAPSULE_RADIUS: f32 = 0.55;
/// How quickly the camera swings back behind the droid once the middle button is let go, like
/// [`EYE_EASING`](super::posture::EYE_EASING).
const ORBIT_RETURN: f32 = 10.0;
/// How far up or down the camera can look, orbiting or not, in radians.
pub(super) const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
/// Where the flashlight is: in first person against the camera, low and to one side, and in third
/// person against the head, a little in front of the droid's face.
const FLASHLIGHT_IN_HAND: Vector3<f32> = Vector3::new(0.2, -0.2, 0.0);
const FLASHLIGHT_ON_HEAD: Vector3<f32> = Vector3::new(0.0, -0.1, 0.4);

/// How far the camera is swung round the droid with the middle mouse button, on top of where the
/// player is looking: across, and up or down, in radians.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct Orbit {
    /// Whether the middle button is down.
    pub(super) held: bool,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
}

impl Orbit {
    /// Swings the camera by `yaw` across and `pitch` up or down, keeping the view, with the
    /// player's own `looking` pitch under it, from going over the top.
    pub(super) fn swing(&mut self, yaw: f32, pitch: f32, looking: f32) {
        self.yaw += yaw;
        self.pitch = (looking + self.pitch + pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT) - looking;
    }

    /// Lets the camera swing back behind the droid while the button is up.
    pub(super) fn settle(&mut self, dt: f32) {
        if self.held {
            return;
        }
        // The short way round, however many times it went round the droid.
        self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let keep = (-ORBIT_RETURN * dt).exp();
        self.yaw *= keep;
        self.pitch *= keep;
        if self.yaw.abs() < 1.0e-4 && self.pitch.abs() < 1.0e-4 {
            (self.yaw, self.pitch) = (0.0, 0.0);
        }
    }

    /// Whether the camera is swung round the droid, or on its way back.
    pub(super) fn is_active(&self) -> bool {
        self.held || self.yaw != 0.0 || self.pitch != 0.0
    }
}

impl Player {
    /// Puts the camera for a head at `head`, turned by `turn`, both in the body's own terms: at
    /// the head in first person, or behind it in third. `aim` is where the head itself faces,
    /// which is where `turn` faces too unless the camera is swung round the droid; the
    /// flashlight goes by it.
    pub(super) fn place_camera(
        &mut self,
        graph: &mut Graph,
        head: Vector3<f32>,
        turn: UnitQuaternion<f32>,
        aim: UnitQuaternion<f32>,
        dt: f32,
    ) {
        let third_person = (self.third_person || self.orbit.is_active()) && self.avatar.is_some();
        let mut boom = Vector3::zeros();
        if third_person {
            let wanted = turn * Vector3::new(BOOM_SHOULDER, BOOM_RISE, -BOOM_LENGTH);
            let length = wanted.norm();
            let facing = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.yaw);
            let from = graph[self.body].global_position() + facing * head;
            let room = (self.distance_to_hit(
                graph,
                from,
                facing * wanted / length,
                length + BOOM_CLEARANCE,
            ) - BOOM_CLEARANCE)
                .clamp(0.0, length);
            self.boom = if room < self.boom {
                room
            } else {
                self.boom + (room - self.boom) * (1.0 - (-BOOM_EASING * dt).exp())
            };
            boom = wanted * (self.boom / length);
        }
        if let Some(avatar) = &self.avatar {
            avatar.set_visible(graph, third_person && self.boom > HIDE_WITHIN);
        }

        let camera = graph[self.camera.transmute::<Node>()].local_transform_mut();
        camera.set_position(head + boom);
        camera.set_rotation(turn);
        // The camera's own terms are the head's, moved back along the boom.
        let flashlight = if third_person {
            FLASHLIGHT_ON_HEAD - turn.inverse() * boom
        } else {
            FLASHLIGHT_IN_HAND
        };
        // A spot light shines down its own -Y; a quarter turn about X makes that the head's
        // forward, and the rest turns it from the camera's way to the head's.
        let shine = turn.inverse()
            * aim
            * UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -90f32.to_radians());
        if let Ok(light) = graph.try_get_mut(self.flashlight) {
            let transform = light.local_transform_mut();
            transform.set_position(flashlight);
            transform.set_rotation(shine);
        }
    }

    /// The droid, while it can be seen, for the graphics effects: a capsule round it, and how far
    /// it has moved since the last time this was asked.
    pub fn moving(&mut self, graph: &Graph) -> Option<fyrox_gfx::MovingThing> {
        let feet = self.position(graph) + Vector3::new(0.0, super::FEET, 0.0);
        let moved = self.last_seen.map_or(Vector3::zeros(), |last| feet - last);
        self.last_seen = Some(feet);
        let avatar = self.avatar.as_ref()?;
        if !avatar.is_visible(graph) {
            return None;
        }
        Some(fyrox_gfx::MovingThing {
            bottom: feet + Vector3::new(0.0, CAPSULE_RADIUS, 0.0),
            top: feet + Vector3::new(0.0, CAPSULE_HEIGHT - CAPSULE_RADIUS, 0.0),
            radius: CAPSULE_RADIUS,
            moved,
        })
    }

    /// Holds the middle mouse button down, or lets it go: while it is down, the mouse swings the
    /// camera round the droid.
    pub fn set_orbiting(&mut self, held: bool) {
        self.orbit.held = held;
    }

    /// Goes between seeing the droid from behind and seeing through its eyes.
    pub(super) fn toggle_view(&mut self) {
        self.third_person = !self.third_person;
    }

    /// Brings the droid into the scene, from its model, once that has loaded. Without it the
    /// player can only see through their own eyes.
    pub fn attach_avatar(
        &mut self,
        scene: &mut fyrox::scene::Scene,
        model: &fyrox::resource::model::ModelResource,
    ) {
        self.avatar = Avatar::spawn(model, scene, self.body.transmute(), super::FEET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn settled(mut orbit: Orbit) -> Orbit {
        for _ in 0..120 {
            orbit.settle(1.0 / 60.0);
        }
        orbit
    }

    #[test]
    fn it_swings_back_behind_once_let_go_and_not_before() {
        let mut orbit = Orbit {
            held: true,
            ..Default::default()
        };
        orbit.swing(2.0, 0.3, 0.0);
        let held = settled(orbit);
        assert_eq!((held.yaw, held.pitch), (2.0, 0.3), "stays while held");
        orbit.held = false;
        let back = settled(orbit);
        assert_eq!((back.yaw, back.pitch), (0.0, 0.0));
        assert!(!back.is_active());
    }

    #[test]
    fn it_goes_back_the_short_way_round() {
        // Nearly all the way round the droid: a little way back is shorter than all the rest.
        let mut orbit = Orbit {
            yaw: 2.0 * PI - 0.2,
            ..Default::default()
        };
        orbit.settle(1.0 / 60.0);
        assert!(orbit.yaw < 0.0 && orbit.yaw > -0.2, "{}", orbit.yaw);
    }

    #[test]
    fn it_never_goes_over_the_top() {
        let mut orbit = Orbit::default();
        let looking = 0.5;
        orbit.swing(0.0, 10.0, looking);
        assert!((looking + orbit.pitch - PITCH_LIMIT).abs() < 1e-5);
        orbit.swing(0.0, -20.0, looking);
        assert!((looking + orbit.pitch + PITCH_LIMIT).abs() < 1e-5);
    }
}
