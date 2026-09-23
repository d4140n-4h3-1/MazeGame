//! Seeing the player from behind: the camera held back over the droid's shoulder, pulled in
//! wherever a wall would come between it and the head, and V to go between that and seeing
//! through the droid's own eyes.
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
/// How far up or down the camera can look, in radians.
pub(super) const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
/// Where the flashlight is: in first person against the camera, low and to one side, and in third
/// person against the head, a little in front of the droid's face.
const FLASHLIGHT_IN_HAND: Vector3<f32> = Vector3::new(0.2, -0.2, 0.0);
const FLASHLIGHT_ON_HEAD: Vector3<f32> = Vector3::new(0.0, -0.1, 0.4);

impl Player {
    /// Puts the camera for a head at `head`, turned by `turn`, both in the body's own terms: at
    /// the head in first person, or behind it in third.
    pub(super) fn place_camera(
        &mut self,
        graph: &mut Graph,
        head: Vector3<f32>,
        turn: UnitQuaternion<f32>,
        dt: f32,
    ) {
        let third_person = self.third_person && self.avatar.is_some();
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
        // forward.
        let shine = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -90f32.to_radians());
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
        Some(super::avatar::capsule(feet, moved))
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
        self.avatar = Avatar::spawn(model, scene, self.body.transmute(), super::FEET, false);
        if self.avatar.is_none() {
            fyrox::core::log::Log::err("Player: playing without the droid");
        }
    }
}
