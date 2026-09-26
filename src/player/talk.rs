//! Talking to another droid, as in Fallout 3: the player's droid turns to face them and looks
//! level, and the camera leaves its shoulder - or its eyes - for a close-up of the other droid's
//! face, with the view narrowed as a long lens would. Done talking, it goes back.
//!
//! The flashlight stays in the droid's hands however far the camera goes, so it goes on lighting
//! what the droid faces: the face being talked to.
//!
//! Using a computer, the camera goes in the same way to its screen, square on and closer, with the
//! screen in the middle of the view.

use super::{avatar::wrap, Player, FEET};
use fyrox::{
    core::algebra::{UnitQuaternion, Vector2, Vector3},
    graph::SceneGraph,
    scene::{
        camera::{Camera, Projection},
        graph::Graph,
        node::Node,
    },
};

/// How far in front of a computer's screen the camera is, in meters: near enough that the screen
/// fills most of the view up and down.
const SCREEN_CLOSE_UP: f32 = 0.6;
/// How far in front of the face the camera is, in meters. It looks level and square on to the
/// face, neither up nor down nor from one side, from low enough that the face is up above the
/// conversation along the bottom of the screen.
const CLOSE_UP: f32 = 0.85;
/// How far up the view the face is: 0 in the middle, 1 at the top.
const FACE_UP_VIEW: f32 = 0.5;
/// How wide the view is in the close-up, in degrees up and down.
const CLOSE_UP_FOV: f32 = 40.0;
/// How long the camera takes to go in to the close-up, or back out, in seconds.
const CLOSE_UP_TIME: f32 = 0.6;
/// How quickly the close-up follows the face as it moves, and how quickly the droid turns to
/// face it, like [`EYE_EASING`](super::posture::EYE_EASING). The face moves a little all the
/// time, as the droid idles; followed at once, the view would shake with it.
const FACE_EASING: f32 = 5.0;
const TURN_EASING: f32 = 6.0;
/// How high up a droid a line of sight to it goes, in meters above its feet, and how far into
/// it a ray can stop - on its own body - and still count as reaching it.
const DROID_MIDDLE: f32 = 1.2;
const DROID_DEPTH: f32 = 0.5;

/// How a close-up frames what it looks at: from how far, how far up the view it puts it, and from
/// which way - out from a screen, square on - or else from wherever the player is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Framing {
    distance: f32,
    up_view: f32,
    out: Option<Vector3<f32>>,
}

impl Default for Framing {
    /// A face's.
    fn default() -> Self {
        Self {
            distance: CLOSE_UP,
            up_view: FACE_UP_VIEW,
            out: None,
        }
    }
}

/// Who the player is talking to, and how far the camera has gone in to them.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct Talk {
    /// The face being talked to, across the world, while the player is talking.
    face: Option<Vector3<f32>>,
    /// Where the close-up is looking: the face, followed smoothly, and kept for going back out.
    looking: Vector3<f32>,
    /// How far in to the close-up the camera is, from 0 to 1.
    through: f32,
    /// Whether the view was last narrowed for the close-up, so it is widened again once.
    narrowed: bool,
    framing: Framing,
}

impl Talk {
    /// Stops talking, with the camera back where it was at once: for a new round, somewhere
    /// else.
    pub(super) fn cut(&mut self) {
        self.face = None;
        self.through = 0.0;
    }
}

/// How far `through` goes in, from 0 to 1: slowly at either end.
fn ease(through: f32) -> f32 {
    through * through * (3.0 - 2.0 * through)
}

/// Where the camera is for a close-up of a face at `face`, talked to from `from`, and how it
/// is turned, across the world: level, and as far below the face as puts it as far up the view as
/// `framing` says - for a face, [`FACE_UP_VIEW`] of the way up.
fn close_up(
    face: Vector3<f32>,
    from: Vector3<f32>,
    framing: Framing,
) -> (Vector3<f32>, UnitQuaternion<f32>) {
    let out = framing
        .out
        .map(|out| Vector3::new(out.x, 0.0, out.z))
        .unwrap_or_else(|| Vector3::new(from.x - face.x, 0.0, from.z - face.z))
        .try_normalize(1.0e-4)
        .unwrap_or(Vector3::z());
    let below = framing.distance * (CLOSE_UP_FOV.to_radians() / 2.0).tan() * framing.up_view;
    let at = face + out * framing.distance - Vector3::y() * below;
    (at, UnitQuaternion::face_towards(&-out, &Vector3::y()))
}

impl Player {
    /// Talks to the droid whose face is at `face`, across the world, or with none stops talking.
    /// While talking, it is told again every frame, as the face moves.
    pub fn talk_to(&mut self, face: Option<Vector3<f32>>) {
        self.close_up_on(face, Framing::default());
    }

    /// Uses the computer whose screen's middle is at `screen`, across the world, facing the way
    /// that goes with it; or with none stops using it. The camera goes in square on to the screen,
    /// with it in the middle of the view.
    pub fn use_screen(&mut self, screen: Option<(Vector3<f32>, Vector3<f32>)>) {
        let framing = Framing {
            distance: SCREEN_CLOSE_UP,
            up_view: 0.0,
            out: screen.map(|(_, facing)| facing),
        };
        self.close_up_on(screen.map(|(middle, _)| middle), framing);
    }

    fn close_up_on(&mut self, face: Option<Vector3<f32>>, framing: Framing) {
        if let (Some(face), None) = (face, self.talk.face) {
            if self.talk.through == 0.0 {
                self.talk.looking = face;
            }
        }
        self.talk.face = face;
        if face.is_some() {
            self.talk.framing = framing;
        }
    }

    /// Turns the body to face whoever the player is talking to, if they are, and levels the
    /// head, for another `dt`.
    pub(super) fn face_speaker(&mut self, graph: &Graph, dt: f32) {
        let Some(face) = self.talk.face else {
            return;
        };
        self.talk.looking += (face - self.talk.looking) * (1.0 - (-FACE_EASING * dt).exp());
        let to = face - graph[self.body].global_position();
        if to.x.abs() + to.z.abs() > 1.0e-3 {
            let turn = 1.0 - (-TURN_EASING * dt).exp();
            self.yaw += wrap(to.x.atan2(to.z) - self.yaw) * turn;
            self.pitch *= 1.0 - turn;
        }
    }

    /// Moves the camera, which would be at `at` turned by `turn` - with the head at `head`, all
    /// in the body's own terms - as far in to the close-up as it has got after another `dt`.
    /// Returns where that puts it and how it is turned, and how far in it is, from 0 to 1.
    pub(super) fn close_in(
        &mut self,
        graph: &mut Graph,
        head: Vector3<f32>,
        at: Vector3<f32>,
        turn: UnitQuaternion<f32>,
        dt: f32,
    ) -> (Vector3<f32>, UnitQuaternion<f32>, f32) {
        let wanted = if self.talk.face.is_some() { 1.0 } else { 0.0 };
        let step = dt / CLOSE_UP_TIME;
        self.talk.through += (wanted - self.talk.through).clamp(-step, step);
        let t = ease(self.talk.through);

        if t > 0.0 || self.talk.narrowed {
            let fov = self.fov + (CLOSE_UP_FOV - self.fov) * t;
            if let Ok(camera) = graph.try_get_mut_of_type::<Camera>(self.camera.transmute::<Node>())
            {
                if let Projection::Perspective(perspective) = camera.projection_mut() {
                    perspective.fov = fov.to_radians();
                }
            }
            self.talk.narrowed = t > 0.0;
        }
        if t == 0.0 {
            return (at, turn, 0.0);
        }

        // Across the world, then back into the body's own terms.
        let body = graph[self.body].global_position();
        let facing = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.yaw);
        let (there, looking) = close_up(self.talk.looking, body + facing * head, self.talk.framing);
        let there = facing.inverse() * (there - body);
        let looking = facing.inverse() * looking;
        let turned = turn.try_slerp(&looking, t, 1.0e-6).unwrap_or(looking);
        (at.lerp(&there, t), turned, t)
    }

    /// Where `point`, across the world, is in a view `size` big, in pixels from its top left, as
    /// the camera is now; none if it is not in front of the camera. Worked out from where the
    /// camera is and how it is turned - it looks along its +Z, up its +Y, with the right of the
    /// view along its -X - and how wide its view is, since the camera's own matrices are only
    /// worked out as it is drawn.
    pub fn on_screen(&self, graph: &Graph, point: Vector3<f32>, size: Vector2<f32>) -> Option<Vector2<f32>> {
        let camera = graph
            .try_get_of_type::<Camera>(self.camera.transmute::<Node>())
            .ok()?;
        let Projection::Perspective(perspective) = camera.projection() else {
            return None;
        };
        let transform = camera.global_transform();
        let axis = |i: usize| {
            Vector3::new(transform[(0, i)], transform[(1, i)], transform[(2, i)])
                .try_normalize(1.0e-6)
                .unwrap_or_default()
        };
        let off = point - camera.global_position();
        let (right, up, ahead) = (-off.dot(&axis(0)), off.dot(&axis(1)), off.dot(&axis(2)));
        if ahead <= 1.0e-3 || size.y <= 0.0 {
            return None;
        }
        let half_up = (perspective.fov / 2.0).tan() * ahead;
        let half_across = half_up * size.x / size.y;
        Some(Vector2::new(
            (1.0 + right / half_across) * 0.5 * size.x,
            (1.0 - up / half_up) * 0.5 * size.y,
        ))
    }

    /// Which way the player looks, along the ground.
    pub fn ahead(&self) -> Vector3<f32> {
        Vector3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// Whether nothing but the droid itself is in the way from the player's eyes to the middle
    /// of a droid standing at `feet`.
    pub fn can_see(&self, graph: &Graph, feet: Vector3<f32>) -> bool {
        let eyes = graph[self.body].global_position() + Vector3::new(0.0, FEET + self.eyes, 0.0);
        let way = feet + Vector3::new(0.0, DROID_MIDDLE, 0.0) - eyes;
        let length = way.norm();
        length < 1.0e-3
            || self.distance_to_hit(graph, eyes, way / length, length) > length - DROID_DEPTH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_close_up_looks_level_at_the_face_from_below_it() {
        let face = Vector3::new(0.0, 1.6, 0.0);
        let from = Vector3::new(0.0, 1.5, 2.0);
        let (at, turn) = close_up(face, from, Framing::default());
        assert!((at.z - CLOSE_UP).abs() < 1.0e-4, "out towards whoever is talking: {at:?}");
        let looking = turn * Vector3::z();
        assert!((looking + Vector3::z()).norm() < 1.0e-4, "level and square on: {looking:?}");
        // Where the face is up the view: how far above the camera, over how far half the view
        // reaches up at that distance.
        let up = (face.y - at.y) / (CLOSE_UP * (CLOSE_UP_FOV.to_radians() / 2.0).tan());
        assert!((up - FACE_UP_VIEW).abs() < 1.0e-4, "{up}");
    }

    #[test]
    fn the_camera_goes_in_and_out_slowly_at_either_end() {
        assert_eq!((ease(0.0), ease(1.0)), (0.0, 1.0));
        assert!(ease(0.1) < 0.1 && ease(0.9) > 0.9);
    }
}
