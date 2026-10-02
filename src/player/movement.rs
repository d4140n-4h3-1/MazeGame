//! Moving the body: speeding up and slowing down along the floor, jumping, stepping up stairs and
//! down them on its feet, and feeling for the floor under the feet.

use super::{posture::Posture, Player, FEET};
use fyrox::{
    core::{algebra::Vector3, pool::Handle},
    graph::SceneGraph,
    scene::{
        collider::Collider,
        graph::Graph,
        node::Node,
        rigidbody::{RigidBody, RigidBodyType},
    },
};

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
/// The highest step the body walks up without a jump, in meters: a stair's, not a crate's.
const STEP_UP: f32 = 0.4;
/// The deepest step the body walks down without falling, in meters: as high as it walks up.
const STEP_DOWN: f32 = STEP_UP;
/// The lowest that is worth stepping up rather than sliding over, in meters.
const STEP_LEAST: f32 = 0.02;
/// How far out from its middle the body feels ahead for a step, in meters: its radius and a
/// little more.
const STEP_REACH: f32 = 0.45;
/// How high over the feet the body feels for a step in its way, in meters: the bottom of the
/// capsule curves up away from a step's edge, which a ray any higher would miss.
const ANKLE: f32 = 0.05;
/// How long the body is carried over a step's edge, at most, in seconds: long enough to walk
/// its middle out over the step at a crawl.
const OVER_STEP: f32 = 1.5;
/// Below this speed along the floor, in meters per second, the body has stopped on the edge.
const STOPPED: f32 = 0.05;

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

/// Whether `collider` stands still: a floor, a wall or a step, not anyone's body.
fn fixed(graph: &Graph, collider: Handle<Collider>) -> bool {
    let body = graph[collider.transmute::<Node>()].parent();
    graph
        .try_get_of_type::<RigidBody>(body)
        .is_ok_and(|body| body.body_type() == RigidBodyType::Static)
}

impl Player {
    /// Whether the feet have something under them. A ray straight down from a little above them,
    /// reaching a little below: the body's own collider is passed over, so it can start inside it.
    pub(super) fn on_ground(&self, graph: &Graph) -> bool {
        let feet = graph[self.body].global_position() + Vector3::new(0.0, FEET + GROUND_REACH, 0.0);
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
            None => ramp(Vector3::new(velocity.x, 0.0, velocity.z), target, push, dt),
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
        if jumped {
            self.stepping = None;
            graph[self.body].set_gravity_scale(1.0);
        } else if self.grounded {
            self.step_up(graph, horizontal, dt);
        }
        (horizontal, jumped, low)
    }

    /// Lifts the body up onto a stair in its way as it walks into it: something that stands still
    /// just ahead at the ankles, with a top no higher than [`STEP_UP`] and room over that to go
    /// on. The capsule's round bottom would only push against the step's edge; the eyes come up
    /// after it over a moment (see `place_head`).
    fn step_up(&mut self, graph: &mut Graph, horizontal: Vector3<f32>, dt: f32) {
        let Some(way) = horizontal.try_normalize(0.1) else {
            return;
        };
        let feet = graph[self.body].global_position() + Vector3::new(0.0, FEET, 0.0);
        let reach = STEP_REACH + horizontal.norm() * dt;
        let ankle = feet + Vector3::new(0.0, ANKLE, 0.0);
        let Some((ahead, wall)) = self.first_hit(graph, ankle, way, reach) else {
            return;
        };
        if !fixed(graph, wall) {
            return;
        }
        // Onto the top of it, just past its edge.
        let over = feet + way * (ahead + 0.05) + Vector3::new(0.0, STEP_UP + ANKLE, 0.0);
        let Some((down, _)) = self.first_hit(graph, over, -Vector3::y(), STEP_UP + ANKLE) else {
            return;
        };
        let rise = STEP_UP + ANKLE - down;
        if !(STEP_LEAST..=STEP_UP).contains(&rise) {
            return;
        }
        // With room to go on over it: a wall would stop a ray at the step's height too.
        let above = feet + Vector3::new(0.0, rise + ANKLE, 0.0);
        if self.first_hit(graph, above, way, reach).is_some() {
            return;
        }
        // Up, and held there - without its weight - until its middle is out over the step:
        // the ground is felt for straight under it, and its round bottom would only drop back
        // onto the edge (see `carry_over_step`).
        let body = &mut graph[self.body];
        let mut velocity = body.lin_vel();
        velocity.y = 0.0;
        body.set_lin_vel(velocity);
        body.set_gravity_scale(0.0);
        let at = **body.local_transform().position();
        body.local_transform_mut().set_position(at + Vector3::new(0.0, rise, 0.0));
        self.stepped += rise;
        self.stepping = Some(0.0);
    }

    /// Whether, off the floor, the body has only walked off the edge of a step: not in a jump,
    /// not going up, with a step that stands still no more than [`STEP_DOWN`] under its feet. It
    /// comes down onto it of its own weight, but the feet are on the stairs all the while - it
    /// does not fall, or land.
    pub(super) fn step_below(&self, graph: &Graph) -> bool {
        if self.since_jump.is_some() || graph[self.body].lin_vel().y > 0.1 {
            return false;
        }
        let feet = graph[self.body].global_position() + Vector3::new(0.0, FEET + GROUND_REACH, 0.0);
        self.first_hit(graph, feet, -Vector3::y(), GROUND_REACH * 2.0 + STEP_DOWN)
            .is_some_and(|(_, floor)| fixed(graph, floor))
    }

    /// Carries the body on over the edge of the step it was lifted onto, weightless, until it
    /// stands on the step, stops, or [`OVER_STEP`] runs out; then it has its weight again.
    pub(super) fn carry_over_step(&mut self, graph: &mut Graph, dt: f32) {
        let Some(since) = self.stepping.as_mut() else {
            return;
        };
        *since += dt;
        let velocity = graph[self.body].lin_vel();
        let stopped = Vector3::new(velocity.x, 0.0, velocity.z).norm() < STOPPED;
        if self.grounded || stopped || *since > OVER_STEP {
            self.stepping = None;
            graph[self.body].set_gravity_scale(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fyrox::{
        core::algebra::Vector2,
        scene::{
            base::BaseBuilder,
            collider::{ColliderBuilder, ColliderShape},
            graph::GraphUpdateSwitches,
            rigidbody::RigidBodyBuilder,
            transform::TransformBuilder,
        },
    };

    /// A box standing still, from `min` to `max`.
    fn block(graph: &mut Graph, min: Vector3<f32>, max: Vector3<f32>) {
        let collider = ColliderBuilder::new(
            BaseBuilder::new().with_local_transform(
                TransformBuilder::new().with_local_position((min + max) / 2.0).build(),
            ),
        )
        .with_shape(ColliderShape::cuboid((max.x - min.x) / 2.0, (max.y - min.y) / 2.0, (max.z - min.z) / 2.0))
        .build(graph);
        RigidBodyBuilder::new(BaseBuilder::new().with_child(collider))
            .with_body_type(RigidBodyType::Static)
            .build(graph);
    }

    /// The player walking forward along +z for `seconds` over a floor, from just short of
    /// whatever `ahead` puts in the way at z = 1: where their feet end up.
    fn walk_into(ahead: impl FnOnce(&mut Graph), seconds: f32) -> Vector3<f32> {
        let mut graph = Graph::new();
        block(&mut graph, Vector3::new(-5.0, -1.0, -5.0), Vector3::new(5.0, 0.0, 20.0));
        ahead(&mut graph);
        let mut player = Player::spawn(&mut graph);
        player.teleport(&mut graph, Vector3::new(0.0, -FEET + 0.01, 0.0), 0.0);
        player.on_key(fyrox::keyboard::KeyCode::KeyW, true);
        let dt = 1.0 / 60.0;
        for _ in 0..(seconds / dt) as usize {
            player.update(&mut graph, dt, true);
            graph.update(Vector2::new(800.0, 600.0), dt, GraphUpdateSwitches::default());
        }
        player.feet(&graph)
    }

    #[test]
    fn it_walks_up_stairs() {
        // Twelve steps of 0.25 m, 0.35 m deep, up to a floor 3 m up: a flight from the arena.
        let feet = walk_into(
            |graph| {
                for n in 0..12 {
                    let z = 1.0 + n as f32 * 0.35;
                    block(graph, Vector3::new(-1.25, 0.0, z), Vector3::new(1.25, 0.25 * (n + 1) as f32, 20.0));
                }
            },
            12.0,
        );
        assert!(feet.y > 2.9, "up to the top: {feet:?}");
        assert!(feet.z > 1.0 + 12.0 * 0.35, "and on along it: {feet:?}");
        assert!(feet.y < 3.1, "and no higher: {feet:?}");
    }

    /// Twelve steps of 0.25 m, 0.35 m deep, down from a floor 3 m up from z = 1, as from the arena;
    /// or, `ledge`, a sheer drop there instead. The player walks forward along +z from the top for
    /// `seconds`: where their feet end up, and how many frames they were off their feet.
    fn walk_down(ledge: bool, seconds: f32) -> (Vector3<f32>, usize) {
        let mut graph = Graph::new();
        block(&mut graph, Vector3::new(-5.0, -1.0, -5.0), Vector3::new(5.0, 0.0, 20.0));
        block(&mut graph, Vector3::new(-1.25, 0.0, -5.0), Vector3::new(1.25, 3.0, 1.0));
        if !ledge {
            for n in 0..12 {
                let z = 1.0 + n as f32 * 0.35;
                block(&mut graph, Vector3::new(-1.25, 0.0, z), Vector3::new(1.25, 3.0 - 0.25 * (n + 1) as f32, z + 0.35));
            }
        }
        let mut player = Player::spawn(&mut graph);
        player.teleport(&mut graph, Vector3::new(0.0, 3.0 - FEET + 0.01, 0.0), 0.0);
        let dt = 1.0 / 60.0;
        let mut off = 0;
        for frame in 0..(seconds / dt) as usize {
            // Settled on the top before setting off.
            player.on_key(fyrox::keyboard::KeyCode::KeyW, frame >= 30);
            player.update(&mut graph, dt, true);
            graph.update(Vector2::new(800.0, 600.0), dt, GraphUpdateSwitches::default());
            off += usize::from(frame >= 30 && !player.grounded);
        }
        (player.feet(&graph), off)
    }

    #[test]
    fn it_walks_down_stairs_on_its_feet() {
        let (feet, off) = walk_down(false, 12.0);
        assert!(feet.y < 0.1 && feet.z > 1.0 + 12.0 * 0.35, "down to the bottom: {feet:?}");
        assert_eq!(off, 0, "never off its feet on the way");
    }

    #[test]
    fn walking_off_a_ledge_is_a_fall() {
        let (feet, off) = walk_down(true, 5.0);
        assert!(feet.y < 0.1, "down on the floor: {feet:?}");
        assert!(off > 10, "off its feet as it fell: {off} frames");
    }

    #[test]
    fn a_crate_is_not_a_step() {
        let feet = walk_into(|graph| block(graph, Vector3::new(-1.25, 0.0, 1.0), Vector3::new(1.25, 1.0, 2.0)), 2.0);
        assert!(feet.y < 0.1 && feet.z < 1.0, "stopped against it: {feet:?}");
    }
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
        assert!(
            (seconds - 0.75).abs() < 0.05,
            "{seconds} s to 6 m/s at 8 m/s^2"
        );
        // Twice the speed off a standstill is twice the wait.
        let (twice, _) = ramp_to(Vector3::zeros(), forward(12.0), 8.0);
        assert!((twice - 2.0 * seconds).abs() < 0.05, "{twice} s");
    }

    #[test]
    fn it_arrives_at_the_target_without_overshooting_it() {
        let (_, fastest) = ramp_to(Vector3::zeros(), forward(5.0), 8.0);
        assert!(fastest <= 5.0 + 1e-5, "overshot to {fastest} m/s");
        // A step longer than the whole gap lands on the target rather than flying past it.
        assert_eq!(
            ramp(Vector3::zeros(), forward(5.0), 8.0, 10.0),
            forward(5.0)
        );
    }

    #[test]
    fn stopping_is_quicker_than_starting() {
        let (starting, _) = ramp_to(Vector3::zeros(), forward(5.0), 8.0);
        let (stopping, _) = ramp_to(forward(5.0), Vector3::zeros(), 8.0);
        assert!(
            stopping < starting,
            "{stopping} s to stop, {starting} s to start"
        );
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
        let air = ramp(
            Vector3::zeros(),
            target,
            STANDING_ACCELERATION * AIR_CONTROL,
            dt,
        );
        assert!(
            air.norm() < ground.norm() * 0.5,
            "a jump can still be steered, barely"
        );
        assert!(air.norm() > 0.0, "but not steered at all is being on rails");
    }
}
