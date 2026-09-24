//! The player: a capsule-shaped rigid body with a camera at eye height.
//!
//! The player can stand, crouch or crawl. C goes between standing and crouching (and up from a
//! crawl to a crouch); Z goes between crawling and standing (and down from a crouch to a crawl).
//! Crouching and crawling shrink the body from the top, so the feet stay on the floor, and lower
//! the eyes with it.
//!
//! The player walks, runs or sprints. Caps Lock goes between walking and running, and stays
//! where it is put; Shift sprints while it is held, from either. Speed is not picked up or put
//! down at once: the body accelerates into it and slows out of it - see `ramp` in [`movement`] -
//! and off the ground there is barely anything to push against, so a jump mostly keeps the way it
//! was going.
//!
//! Space jumps: tapped, a low jump, and held, a high one. Each press jumps once.
//!
//! Tab takes cover against the wall ahead, and A and D slide along it to its edge - see
//! [`cover`].
//!
//! Holding the right mouse button strafes: the droid keeps facing ahead whichever way it goes -
//! see [`avatar`]. A sprint slows to a run while it does, and picks up again when it is let go.
//!
//! R draws the pistol and holsters it again, and the left mouse button draws it and then fires
//! it - see [`pistol`].
//!
//! A sprint costs breath, and runs out: see [`Player::breathe`]. Out of breath, the player is
//! down to a walk until they have got some of it back.
//!
//! The head is not carried perfectly level. It dips as the knees take a landing, and rolls into
//! sideways movement and into turns.
//!
//! F switches the flashlight on and off.
//!
//! In cover at the edge of the wall, holding the key that would go on past leans: the head moves
//! out round the corner and tilts, to see past it without stepping out from behind it - see
//! [`lean`]. It never leans into a wall.
//!
//! Holding Q looks behind: the head turns round over the shoulder while the body keeps going the
//! way it was facing, so the player can see what is following them without stopping.
//!
//! The player is seen from behind, as a droid (see [`avatar`]), with the camera held back over its
//! shoulder; V goes between that and seeing through its eyes, and holding the middle mouse button
//! swings the camera round it - see [`third_person`].
//!
//! Each of these has a file of its own here, adding to [`Player`] what it needs.

pub(crate) mod avatar;
mod breath;
mod cover;
mod head;
mod input;
mod lean;
mod movement;
mod pistol;
pub(crate) mod posture;
mod third_person;
mod view;

pub use avatar::DROID_MODEL;
use avatar::{heading, Avatar, Going};
use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        camera::{Camera, CameraBuilder},
        collider::{Collider, ColliderBuilder},
        graph::{physics::RayCastOptions, Graph},
        light::{spot::SpotLightBuilder, BaseLightBuilder},
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder},
        transform::TransformBuilder,
    },
};
use head::LookBack;
use input::Keys;
use posture::Posture;
use third_person::{Orbit, BOOM_LENGTH};
use view::{DEFAULT_FOV, DEFAULT_SENSITIVITY, NEAR_PLANE};

/// Where the feet are, relative to the middle of the body when standing. The body's origin stays
/// there in every posture; only the collider and the eyes move.
const FEET: f32 = -0.85;

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    body: Handle<RigidBody>,
    collider: Handle<Collider>,
    camera: Handle<Camera>,
    flashlight: Handle<Node>,
    /// Whether the flashlight is on. It stays as the player left it from one round to the next.
    flashlight_on: bool,
    /// Whether Caps Lock has put the player into a run. Like the flashlight, it stays as the
    /// player left it from one round to the next.
    running: bool,
    /// How much breath is left, from 1 down to 0.
    stamina: f32,
    /// Whether the player has run themselves out and is walking it off.
    winded: bool,
    /// Whether the feet were on something as of this frame.
    grounded: bool,
    /// How fast the body was falling last frame, in meters per second, for the landing to read.
    fall_speed: f32,
    /// Whether this press of Space has been jumped on already: it has to be let go to jump again.
    jump_spent: bool,
    /// How long ago the body pushed off, in seconds, while Space is still held from it and it
    /// could yet be a tap.
    since_jump: Option<f32>,
    /// How far the knees are still bent under a landing, in meters.
    landing: f32,
    /// How far the head is rolled into its movement, in radians.
    roll: f32,
    /// The yaw last frame, to see how fast the player is turning.
    last_yaw: f32,
    posture: Posture,
    /// The posture the body's shape was last made for.
    shaped_for: Option<Posture>,
    /// How high the eyes are above the feet right now, on their way to the posture's height.
    eyes: f32,
    look_back: LookBack,
    /// How far the head is leaning right now, in meters, along the body's `right` (see
    /// [`Player::fit_lean`]); the tilt is taken from the same number, so the head always tips the
    /// way it is leaning.
    lean: f32,
    /// The wall the droid is in cover against, if it is.
    cover: Option<cover::Cover>,
    yaw: f32,
    pitch: f32,
    sensitivity: f32,
    fov: f32,
    /// The droid the player is seen as, once its model has loaded.
    avatar: Option<Avatar>,
    /// Whether the player is seen from behind rather than through their own eyes. It stays as
    /// the player left it from one round to the next.
    third_person: bool,
    /// How far behind the head the camera is right now, in meters: all the way back, or pulled
    /// in by a wall.
    boom: f32,
    /// How far the camera is swung round the droid with the middle mouse button.
    orbit: Orbit,
    /// Where the droid's feet were the last time the graphics effects were told.
    last_seen: Option<Vector3<f32>>,
    /// Whether the player wants the pistol out.
    armed: bool,
    /// The pistol's shots.
    bolts: pistol::Bolts,
    keys: Keys,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            body: Default::default(),
            collider: Default::default(),
            camera: Default::default(),
            flashlight: Default::default(),
            flashlight_on: true,
            running: false,
            stamina: 1.0,
            winded: false,
            grounded: false,
            fall_speed: 0.0,
            jump_spent: false,
            since_jump: None,
            landing: 0.0,
            roll: 0.0,
            last_yaw: 0.0,
            posture: Posture::Standing,
            shaped_for: None,
            eyes: Posture::Standing.eyes(),
            look_back: LookBack::default(),
            lean: 0.0,
            cover: None,
            yaw: 0.0,
            pitch: 0.0,
            sensitivity: DEFAULT_SENSITIVITY,
            fov: DEFAULT_FOV,
            avatar: None,
            third_person: true,
            boom: BOOM_LENGTH,
            orbit: Orbit::default(),
            last_seen: None,
            armed: false,
            bolts: Default::default(),
            keys: Default::default(),
        }
    }
}

impl Player {
    pub fn spawn(graph: &mut Graph) -> Self {
        // A flashlight, for the corridors the sun does not reach. A spot light shines down its
        // own -Y axis; turned a quarter turn about X, that is the camera's forward (+Z).
        let flashlight = SpotLightBuilder::new(
            BaseLightBuilder::new(
                BaseBuilder::new().with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(Vector3::new(0.2, -0.2, 0.0))
                        .with_local_rotation(UnitQuaternion::from_axis_angle(
                            &Vector3::x_axis(),
                            -90f32.to_radians(),
                        ))
                        .build(),
                ),
            )
            .with_intensity(1.5),
        )
        .with_distance(15.0)
        .with_hotspot_cone_angle(35f32.to_radians())
        .build(graph)
        .to_base();

        let camera = CameraBuilder::new(
            BaseBuilder::new()
                .with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(Vector3::new(0.0, FEET + Posture::Standing.eyes(), 0.0))
                        .build(),
                )
                .with_child(flashlight),
        )
        .with_fov(DEFAULT_FOV.to_radians())
        .with_z_near(NEAR_PLANE)
        .build(graph);

        let collider = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(Posture::Standing.shape().0)
            .with_friction(0.0)
            .build(graph);

        let body = RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(Vector3::new(0.0, 50.0, 0.0))
                        .build(),
                )
                .with_child(collider)
                .with_child(camera),
        )
        .with_locked_rotations(true)
        .with_can_sleep(false)
        // Held in place until the maze is ready and the first round moves it.
        .with_gravity_scale(0.0)
        .build(graph);

        Self {
            body,
            collider,
            camera,
            flashlight,
            bolts: pistol::Bolts::new(graph),
            ..Default::default()
        }
    }

    pub fn position(&self, graph: &Graph) -> Vector3<f32> {
        graph[self.body].global_position()
    }

    /// Where the player's feet are.
    pub fn feet(&self, graph: &Graph) -> Vector3<f32> {
        self.position(graph) + Vector3::new(0.0, FEET, 0.0)
    }

    pub fn teleport(&mut self, graph: &mut Graph, position: Vector3<f32>, yaw: f32) {
        self.start_fresh(yaw);
        self.bolts.clear(graph);
        let body = &mut graph[self.body];
        body.set_gravity_scale(1.0);
        body.set_lin_vel(Vector3::zeros());
        body.local_transform_mut().set_position(position);
    }

    /// Puts the body back the way a round starts: facing `yaw`, on its feet, on a full breath
    /// and with the head still. What the player has chosen for themselves - their gait, the
    /// flashlight, how the view is set up - is left as they left it.
    fn start_fresh(&mut self, yaw: f32) {
        self.yaw = yaw;
        self.last_yaw = yaw;
        self.pitch = 0.0;
        self.posture = Posture::Standing;
        self.eyes = Posture::Standing.eyes();
        self.look_back = LookBack::default();
        self.lean = 0.0;
        self.cover = None;
        self.stamina = 1.0;
        self.winded = false;
        self.fall_speed = 0.0;
        self.since_jump = None;
        self.landing = 0.0;
        self.roll = 0.0;
        self.boom = BOOM_LENGTH;
        // A new round puts the body somewhere else rather than moving it there.
        self.last_seen = None;
        self.armed = false;
    }

    /// Applies input for this frame. With `can_move` off the player only looks around.
    pub fn update(&mut self, graph: &mut Graph, dt: f32, can_move: bool) {
        let was_grounded = self.grounded;
        self.grounded = self.on_ground(graph);
        // Landed: the knees take whatever the body was falling at as of last frame, since the
        // solver has already taken it out of the body by now.
        if self.grounded && !was_grounded {
            self.land();
        }
        let keys = &self.keys;
        let pushing = can_move && (keys.forward || keys.back || keys.left || keys.right);
        self.breathe(dt, pushing);
        self.fit_posture(graph, dt);
        self.look_back.advance(self.keys.look_back, dt);
        if let Ok(flashlight) = graph.try_get_mut(self.flashlight) {
            if flashlight.visibility() != self.flashlight_on {
                flashlight.set_visibility(self.flashlight_on);
            }
        }
        let rotation = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.yaw);
        let right = rotation * -Vector3::x();
        self.fit_lean(graph, right, dt);

        graph[self.body]
            .local_transform_mut()
            .set_rotation(rotation);
        let skid = self.avatar.as_ref().and_then(Avatar::travel);
        if std::mem::take(&mut self.keys.take_cover) && can_move {
            self.toggle_cover(graph, rotation * Vector3::z());
        }
        let (horizontal, jumped, low) =
            self.drive(graph, rotation * Vector3::z(), right, can_move, skid, dt);

        self.carry_head(horizontal, right, dt);
        self.place_head(graph, dt);
        let gait = self.gait();
        let keys = &self.keys;
        // Which way the body is going, from the way it faces: its left is +x.
        let local = rotation.inverse() * horizontal;
        // Which way the camera looks, likewise, as of the last frame: for the pistol to follow.
        let look = rotation.inverse()
            * graph[self.camera.transmute::<Node>()]
                .look_vector()
                .try_normalize(1.0e-6)
                .unwrap_or_else(Vector3::z);
        let going = Going {
            heading: can_move.then(|| {
                self.cover_heading()
                    .unwrap_or_else(|| heading(keys.forward, keys.back, keys.left, keys.right))
            }),
            speed: horizontal.norm(),
            posture: self.posture,
            gait,
            grounded: self.grounded,
            jumped,
            low,
            cover: self.in_cover(),
            pushing,
            falling: self.fall_speed,
            // In cover, the wall sets which way the droid faces.
            strafing: can_move && self.strafing() && !self.in_cover(),
            armed: self.armed,
            trigger: can_move && keys.trigger,
            raised: can_move && keys.strafe,
            look: (look.y.clamp(-1.0, 1.0).asin(), look.x.atan2(look.z)),
            way: local.x.atan2(local.z),
        };
        self.keys.trigger = false;
        if let Some(avatar) = self.avatar.as_mut() {
            avatar.animate(graph, going, dt);
        }
        self.shoot(graph, dt);
    }

    /// Whether the droid strafes: with the right mouse button held, or the pistol out.
    fn strafing(&self) -> bool {
        self.keys.strafe || self.armed
    }

    /// How fast the player goes at `gait` in the posture they are in, in meters per second: as
    /// fast as the droid's feet go (see [`Avatar::pace`]), or without the droid, the posture's
    /// own speeds.
    fn top_speed(&self, gait: posture::Gait) -> f32 {
        self.avatar
            .as_ref()
            .and_then(|avatar| avatar.pace(self.posture, gait))
            .unwrap_or_else(|| self.posture.speed(gait))
    }

    /// How far a ray from `from` goes in `direction` before hitting something other than the
    /// player, up to `reach`.
    fn distance_to_hit(
        &self,
        graph: &Graph,
        from: Vector3<f32>,
        direction: Vector3<f32>,
        reach: f32,
    ) -> f32 {
        let head = from;
        let mut hits = Vec::new();
        graph.physics.cast_ray(
            RayCastOptions {
                ray_origin: Point3::from(head),
                ray_direction: direction,
                max_len: reach,
                groups: Default::default(),
                sort_results: true,
            },
            &mut hits,
        );
        hits.iter()
            // The ray starts inside the player's own body.
            .find(|hit| hit.collider != self.collider)
            .map_or(reach, |hit| (hit.position.coords - head).norm())
    }
}

/// Presses and releases a key, with a few key-repeat presses while it is down.
#[cfg(test)]
fn press(player: &mut Player, key: fyrox::keyboard::KeyCode) {
    for _ in 0..4 {
        player.on_key(key, true);
    }
    player.on_key(key, false);
}
