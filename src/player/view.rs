//! Looking around with the mouse, and the view's settings: how fast it turns and how wide it is.

use super::{third_person::PITCH_LIMIT, Player};
use fyrox::{
    graph::SceneGraph,
    scene::{
        camera::{Camera, Projection},
        graph::Graph,
        node::Node,
    },
};

/// Radians the view turns per mouse count, to start with. Roughly what a shooter's default feels
/// like; [`Player::nudge_sensitivity`] changes it in game.
pub(super) const DEFAULT_SENSITIVITY: f32 = 0.0012;
const SENSITIVITY_RANGE: std::ops::RangeInclusive<f32> = 0.0002..=0.01;

/// Vertical field of view. The engine's default is 75 degrees, which is about 106 across at 16:9
/// and noticeably stretched at the edges; 60 vertical is about 90 across, the usual shooter view.
pub(super) const DEFAULT_FOV: f32 = 60.0;
const FOV_RANGE: std::ops::RangeInclusive<f32> = 45.0..=100.0;

/// How close to the camera anything is drawn, in meters. The body keeps the eyes 35 cm from any
/// wall, so this is never seen; and the depth buffer's precision everywhere scales with it -
/// the engine's default of 2.5 cm leaves distant walls with centimeters of depth error.
pub(super) const NEAR_PLANE: f32 = 0.1;

impl Player {
    /// Turns the view by a mouse movement - or, with the middle button held, swings the camera
    /// round the droid instead, leaving the droid facing as it was.
    pub fn look(&mut self, dx: f32, dy: f32) {
        let (across, up) = (-dx * self.sensitivity, dy * self.sensitivity);
        if self.orbit.held {
            self.orbit.swing(across, up, self.pitch);
            return;
        }
        self.yaw += across;
        self.pitch = (self.pitch + up).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Multiplies how far the view turns per mouse count, and returns the new value.
    pub fn nudge_sensitivity(&mut self, factor: f32) -> f32 {
        self.sensitivity =
            (self.sensitivity * factor).clamp(*SENSITIVITY_RANGE.start(), *SENSITIVITY_RANGE.end());
        self.sensitivity
    }

    /// Widens or narrows the view by `degrees`, and returns the new field of view.
    pub fn nudge_fov(&mut self, degrees: f32, graph: &mut Graph) -> f32 {
        self.fov = (self.fov + degrees).clamp(*FOV_RANGE.start(), *FOV_RANGE.end());
        if let Ok(camera) = graph.try_get_mut_of_type::<Camera>(self.camera.transmute::<Node>()) {
            if let Projection::Perspective(perspective) = camera.projection_mut() {
                perspective.fov = self.fov.to_radians();
            }
        }
        self.fov
    }

    /// How far the view turns per mouse count, and how wide it is, for showing on screen.
    pub fn look_settings(&self) -> (f32, f32) {
        (self.sensitivity, self.fov)
    }
}
