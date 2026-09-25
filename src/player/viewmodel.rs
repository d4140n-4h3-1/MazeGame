//! The pistol in first person, held as in Call of Duty: a copy of the droid's own pistol, carried
//! by the camera low and to the right, pointing a little in towards the middle of the view.
//! Holding the right mouse button brings it up into the middle to aim, with the screen on top of
//! it - crosshair and all - square in the middle of the view, as a sight.
//!
//! It comes up from below as the droid draws its pistol and goes back down as it holsters it,
//! at the same moments the droid's own pistol shows and goes. Each shot kicks it back and up,
//! and it settles again. Right up against a wall it is tucked down out of the way, rather than
//! pushed into it.
//!
//! Its parts do what the droid's own do: the ball at the muzzle tumbles, and the screen flashes
//! as the trigger is pulled. A shot leaves its muzzle, not the droid's, for wherever the middle
//! of the view is looking: see [`Player::aim_from_view`].

use super::{avatar::Avatar, Player};
use fyrox::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{base::BaseBuilder, graph::Graph, node::Node, pivot::PivotBuilder},
};

/// How big the pistol is held, as a scale on the model's own pistol: 24 cm long.
const SIZE: f32 = 0.1;
/// Where it is held from the camera, in the camera's own terms - +z ahead, +y up, and -x to the
/// right - at the hip, and raised to aim. Raised, the middle of its screen is on the middle of
/// the view: the screen sits this far above the pistol's middle line and this far back from its
/// middle, in the model's own terms.
const HIP: Vector3<f32> = Vector3::new(-0.13, -0.17, 0.3);
const SCREEN_ABOVE: f32 = 1.73;
const SCREEN_BEHIND: f32 = 0.52;
const SIGHT_DISTANCE: f32 = 0.28;
/// How far off the pistol at the hip points to cross the middle of the view, in meters.
const CONVERGE: f32 = 8.0;
/// How much lower and nearer it is put away, in meters, and how far its muzzle is dipped, in
/// radians.
const PUT_AWAY: Vector3<f32> = Vector3::new(0.0, -0.25, -0.1);
const PUT_AWAY_DIP: f32 = 50.0 * std::f32::consts::PI / 180.0;
/// How long it takes to come up or go down, in seconds.
const RAISE_TIME: f32 = 0.2;
/// How quickly it comes up to aim, or goes back to the hip, like
/// [`EYE_EASING`](super::posture::EYE_EASING).
const AIM_EASING: f32 = 14.0;
/// How far a shot kicks it back, in meters, and its muzzle up, in radians, and how quickly it
/// settles again, like a rate.
const KICK_BACK: f32 = 0.04;
const KICK_UP: f32 = 8.0 * std::f32::consts::PI / 180.0;
const KICK_SETTLE: f32 = 12.0;
/// How near a wall ahead has to be, in meters, for the pistol to start tucking down out of the
/// way, and how much nearer it has to be for it to be all the way down.
const TUCK_FROM: f32 = 0.5;
const TUCK_OVER: f32 = 0.25;
/// How quickly it tucks down and comes back up, like a rate.
const TUCK_EASING: f32 = 10.0;
/// How far out from its middle line it is, for the graphics effects, in meters.
const RADIUS: f32 = 0.1;

/// The pistol held in first person.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Viewmodel {
    /// What the camera carries, turned and moved to hold the pistol.
    grip: Handle<Node>,
    /// The copy of the pistol in it, and its muzzle.
    pistol: Handle<Node>,
    muzzle: Handle<Node>,
    /// Every part of the droid's own pistol below the pistol itself, with the part of the copy
    /// that does what it does.
    parts: Vec<(Handle<Node>, Handle<Node>)>,
    /// How far up it is, from 0 put away to 1 held; how far in to aim, from 0 at the hip to 1
    /// raised; how much of a shot's kick is left, from 1 to 0; and how far it is tucked down out
    /// of a wall's way, from 0 to 1.
    up: f32,
    aim: f32,
    kick: f32,
    tuck: f32,
    /// Where its middle was, across the world, the last time the graphics effects were told.
    last_seen: Option<Vector3<f32>>,
}

/// Where the pistol is held, and how it is turned, `aim` of the way in from the hip to aiming.
fn held(aim: f32) -> (Vector3<f32>, UnitQuaternion<f32>) {
    let sights = Vector3::new(
        0.0,
        -SCREEN_ABOVE * SIZE,
        SIGHT_DISTANCE + SCREEN_BEHIND * SIZE,
    );
    // At the hip it points in and up at the middle of the view, some way off.
    let hip_turn = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), (-HIP.x).atan2(CONVERGE))
        * UnitQuaternion::from_axis_angle(&Vector3::x_axis(), HIP.y.atan2(CONVERGE));
    let at = HIP + (sights - HIP) * aim;
    let turn = hip_turn.slerp(&UnitQuaternion::identity(), aim);
    (at, turn)
}

impl Viewmodel {
    /// A copy of `avatar`'s pistol, carried by `camera`, out of sight until it is drawn. None if
    /// the droid has no pistol.
    pub(super) fn new(graph: &mut Graph, avatar: &Avatar, camera: Handle<Node>) -> Option<Self> {
        let (original, original_muzzle) = avatar.pistol_nodes()?;
        let (pistol, map) = graph.copy_node_inplace(original, &mut |_, _| true);
        let mut muzzle = original_muzzle;
        if !map.try_map(&mut muzzle) {
            return None;
        }
        let parts = graph
            .traverse_handle_iter(original)
            .skip(1)
            .filter_map(|part| {
                let mut copy = part;
                map.try_map(&mut copy).then_some((part, copy))
            })
            .collect();
        let grip = PivotBuilder::new(BaseBuilder::new().with_visibility(false))
            .build(graph)
            .to_base();
        graph.link_nodes(grip, camera);
        graph.link_nodes(pistol, grip);
        // Its barrel runs along its +x; turned a quarter turn round, that is the camera's ahead.
        let node = &mut graph[pistol];
        node.set_visibility(true);
        node.local_transform_mut()
            .set_position(Vector3::zeros())
            .set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::y_axis(),
                -std::f32::consts::FRAC_PI_2,
            ))
            .set_scale(Vector3::repeat(SIZE));
        Some(Self {
            grip,
            pistol,
            muzzle,
            parts,
            up: 0.0,
            aim: 0.0,
            kick: 0.0,
            tuck: 0.0,
            last_seen: None,
        })
    }

    /// Whether it can be seen.
    fn shown(&self, graph: &Graph) -> bool {
        graph[self.grip].visibility()
    }

    /// Holds it for another `dt`: `seen` while the view is from the droid's own eyes, up while
    /// the droid's pistol is `out`, `raised` to aim or at the hip, kicked by a shot `fired` this
    /// frame, and tucked out of the way of a wall `room` meters ahead.
    #[allow(clippy::too_many_arguments)]
    fn hold(
        &mut self,
        graph: &mut Graph,
        seen: bool,
        out: bool,
        raised: bool,
        fired: bool,
        room: f32,
        dt: f32,
    ) {
        let step = dt / RAISE_TIME;
        self.up = (self.up + if out { step } else { -step }).clamp(0.0, 1.0);
        let ease = |rate: f32| 1.0 - (-rate * dt).exp();
        let aim = if raised && out { 1.0 } else { 0.0 };
        self.aim += (aim - self.aim) * ease(AIM_EASING);
        self.kick = if fired { 1.0 } else { self.kick * (-KICK_SETTLE * dt).exp() };
        let tuck = ((TUCK_FROM - room) / TUCK_OVER).clamp(0.0, 1.0);
        self.tuck += (tuck - self.tuck) * ease(TUCK_EASING);

        let shown = seen && self.up > 0.0;
        if graph[self.grip].visibility() != shown {
            graph[self.grip].set_visibility(shown);
        }
        if !shown {
            self.last_seen = None;
            return;
        }
        let (at, turn) = held(self.aim);
        // Put away, or tucked down out of a wall's way: lower, nearer, and dipped. Smoothed at
        // both ends, so it eases up and down.
        let away = 1.0 - self.up.min(1.0 - self.tuck);
        let away = away * away * (3.0 - 2.0 * away);
        let dip = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), PUT_AWAY_DIP * away);
        let kick = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), -KICK_UP * self.kick);
        let transform = graph[self.grip].local_transform_mut();
        transform
            .set_position(at + PUT_AWAY * away + Vector3::new(0.0, 0.0, -KICK_BACK * self.kick))
            .set_rotation(turn * dip * kick);
        // Every part of it as the droid's own: the ball at the muzzle tumbling, and the screen
        // lit.
        for &(part, copy) in &self.parts {
            let (rotation, visible) = {
                let part = &graph[part];
                (**part.local_transform().rotation(), part.visibility())
            };
            let copy = &mut graph[copy];
            copy.local_transform_mut().set_rotation(rotation);
            if copy.visibility() != visible {
                copy.set_visibility(visible);
            }
        }
    }
}

impl Player {
    /// Makes the pistol to hold in first person, from the droid's.
    pub(super) fn make_viewmodel(&mut self, graph: &mut Graph) {
        self.viewmodel = self
            .avatar
            .as_ref()
            .and_then(|avatar| Viewmodel::new(graph, avatar, self.camera.transmute()));
    }

    /// Holds the pistol in first person for another `dt`, for a shot that left this frame if
    /// one did.
    pub(super) fn hold_pistol(&mut self, graph: &mut Graph, fired: bool, dt: f32) {
        let Some(avatar) = self.avatar.as_ref() else {
            return;
        };
        let out = avatar.pistol_out(graph);
        let camera = &graph[self.camera];
        let (eye, ahead) = (camera.global_position(), camera.look_vector());
        let room = self.distance_to_hit(graph, eye, ahead.normalize(), TUCK_FROM);
        let (seen, raised) = (self.in_own_eyes, self.keys.strafe);
        if let Some(viewmodel) = self.viewmodel.as_mut() {
            viewmodel.hold(graph, seen, out, raised, fired, room, dt);
        }
    }

    /// Where a shot leaves, and which way it goes, one meter long, while the pistol is held in
    /// first person: from its muzzle, for whatever the middle of the view is looking at. None
    /// while it is not.
    pub(super) fn aim_from_view(&self, graph: &Graph) -> Option<(Vector3<f32>, Vector3<f32>)> {
        let viewmodel = self.viewmodel.as_ref().filter(|v| v.shown(graph))?;
        let camera = &graph[self.camera];
        let (eye, ahead) = (camera.global_position(), camera.look_vector().normalize());
        const FAR: f32 = 100.0;
        let target = eye + ahead * self.distance_to_hit(graph, eye, ahead, FAR);
        let muzzle = graph[viewmodel.muzzle].global_position();
        let way = (target - muzzle).try_normalize(1.0e-4).unwrap_or(ahead);
        Some((muzzle, way))
    }

    /// The pistol held in first person, while it can be seen, for the graphics effects: a
    /// capsule round it, and how far it has moved since the last time this was asked.
    pub(super) fn viewmodel_moving(&mut self, graph: &Graph) -> Option<fyrox_gfx::MovingThing> {
        let viewmodel = self.viewmodel.as_mut()?;
        if !viewmodel.shown(graph) {
            return None;
        }
        let (back, front) = (
            graph[viewmodel.pistol].global_position(),
            graph[viewmodel.muzzle].global_position(),
        );
        let middle = (back + front) * 0.5;
        let moved = viewmodel.last_seen.map_or(Vector3::zeros(), |last| middle - last);
        viewmodel.last_seen = Some(middle);
        Some(fyrox_gfx::MovingThing {
            bottom: back,
            top: front,
            radius: RADIUS,
            moved,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_the_hip_it_is_low_and_right_and_points_in_at_the_middle() {
        let (at, turn) = held(0.0);
        assert!(at.x < 0.0 && at.y < 0.0, "low, and to the right (-x): {at:?}");
        let ahead = turn * Vector3::z();
        let crosses = at + ahead * (CONVERGE / ahead.z);
        assert!(crosses.xy().norm() < 0.02, "crosses the middle some way off: {crosses:?}");
    }

    #[test]
    fn raised_its_screen_is_in_the_middle_of_the_view() {
        let (at, turn) = held(1.0);
        // The screen's middle, in the pistol's own terms, turned the way the pistol is held in
        // the grip: its +x ahead.
        let quarter = -std::f32::consts::FRAC_PI_2;
        let to_screen = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), quarter)
            * Vector3::new(-SCREEN_BEHIND, SCREEN_ABOVE, 0.0)
            * SIZE;
        let screen = at + turn * to_screen;
        assert!(screen.xy().norm() < 1.0e-4, "{screen:?}");
        assert!((screen.z - SIGHT_DISTANCE).abs() < 1.0e-4);
    }
}
