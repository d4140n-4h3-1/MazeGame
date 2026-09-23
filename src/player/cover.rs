//! Taking cover with Tab: the droid goes up against the wall it is facing and keeps to it.
//!
//! In cover, the keys that would take the droid along the wall slide it along instead, and the
//! droid faces the way it is sliding. It stops where the wall ends: the edge of the wall is the
//! corner it takes cover behind, and holding the key that would go on past it leans out round
//! it (see [`lean`](super::lean)). It follows the wall round as it curves, however the camera is
//! turned.
//!
//! Tab again lets go of the wall, and so does pushing away from it, jumping, or the wall coming
//! to an end behind it rather than to one side.

use super::Player;
use fyrox::{
    core::{
        algebra::{Point3, Vector3},
        pool::Handle,
    },
    graph::SceneGraph,
    scene::{
        collider::Collider,
        graph::{physics::RayCastOptions, Graph},
        node::Node,
        rigidbody::{RigidBody, RigidBodyType},
    },
};

/// How far off a wall can be, in meters from the body's middle, for Tab to take cover against it.
const REACH: f32 = 1.6;
/// Which ways to look for a wall, in radians either side of straight ahead.
const LOOKING: [f32; 5] = [0.0, -0.4, 0.4, -0.8, 0.8];
/// How far the body's middle keeps from the wall in cover, in meters: its own width, 0.35 m, and
/// a little more.
const GAP: f32 = 0.4;
/// How far behind the body, in meters, the wall can be and still be the one it is in cover against.
const HOLD: f32 = GAP + 0.4;
/// How far ahead along the wall, in meters, it has to go on for the droid to slide that way: as
/// close to the edge as it gets.
const EDGE: f32 = 0.3;
/// How quickly the body closes up to the wall, like a rate: the share of the gap closed each
/// second, near enough.
const HUG: f32 = 8.0;
/// How squarely away from the wall the keys have to push, as the cosine of the angle, to let go.
const LET_GO: f32 = 0.7;
/// How steep a surface has to be to be a wall to take cover against: its normal can point this
/// far up or down, as a sine, and no further.
const UPRIGHT: f32 = 0.5;

/// Up against a wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Cover {
    /// Straight out of the wall, along the ground.
    normal: Vector3<f32>,
    /// Which way along the wall the droid faces: 1 with the wall on its right, -1 on its left.
    facing: f32,
    /// Which way round the corner to lean, along the ground, while the droid is at the edge of
    /// the wall with a key held that would take it past.
    peek: Option<Vector3<f32>>,
}

/// `vector` along the ground.
fn flat(vector: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(vector.x, 0.0, vector.z)
}

/// Along a wall facing out along `normal`, one way.
fn along(normal: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-normal.z, 0.0, normal.x)
}

/// What the keys `wish` the body to do against a wall facing out along `normal`: how hard they
/// push along it, one way or the other, and whether they push away from it, letting go.
fn read_keys(wish: Vector3<f32>, normal: Vector3<f32>) -> (f32, bool) {
    let wish = flat(wish);
    let length = wish.norm();
    if length < 1.0e-3 {
        return (0.0, false);
    }
    (wish.dot(&along(normal)) / length, wish.dot(&normal) > LET_GO * length)
}

/// How fast the body closes on the wall, in meters per second, from `gap` off it: out if it is
/// too close, in if it is too far, and no faster than `speed`.
fn closing(gap: f32, speed: f32) -> f32 {
    ((GAP - gap) * HUG).clamp(-speed, speed)
}

impl Player {
    /// Whether the droid is in cover.
    pub(super) fn in_cover(&self) -> bool {
        self.cover.is_some()
    }

    /// Takes cover against the wall the body faces, if there is one near enough, or lets go of
    /// the one it is in cover against. `forward` is the way the body faces.
    pub(super) fn toggle_cover(&mut self, graph: &Graph, forward: Vector3<f32>) {
        if self.cover.take().is_some() || !self.grounded {
            return;
        }
        let middle = graph[self.body].global_position();
        let forward = flat(forward).try_normalize(1.0e-6).unwrap_or_else(Vector3::z);
        let nearest = LOOKING
            .iter()
            .filter_map(|&angle| {
                let (sin, cos) = angle.sin_cos();
                let way = Vector3::new(
                    forward.x * cos + forward.z * sin,
                    0.0,
                    forward.z * cos - forward.x * sin,
                );
                self.wall(graph, middle, way, REACH)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, normal)) = nearest {
            // Facing along the wall the way the body was already turned.
            let facing = if forward.dot(&along(normal)) < 0.0 { -1.0 } else { 1.0 };
            self.cover = Some(Cover {
                normal,
                facing,
                peek: None,
            });
        }
    }

    /// Where the body should be going in cover, given the way the keys `wish` it to at `speed`:
    /// along the wall and up against it, and no further than the wall goes. None out of cover -
    /// or on letting go of it just now, pushing away or with no wall left behind.
    pub(super) fn keep_cover(
        &mut self,
        graph: &Graph,
        wish: Vector3<f32>,
        speed: f32,
    ) -> Option<Vector3<f32>> {
        let cover = self.cover?;
        let middle = graph[self.body].global_position();
        let (push, letting_go) = read_keys(wish, cover.normal);
        // The wall again, straight behind; it can curve.
        let wall = self.wall(graph, middle, -cover.normal, HOLD);
        let Some((gap, normal)) = wall.filter(|_| !letting_go && self.grounded) else {
            self.cover = None;
            return None;
        };
        let sideways = along(normal);
        let mut facing = cover.facing;
        let mut going = 0.0;
        let mut peek = None;
        if push.abs() > 0.1 {
            facing = push.signum();
            // Only while the wall goes on that way. Where it ends is the corner, to lean round.
            let ahead = middle + sideways * (facing * EDGE);
            if self.wall(graph, ahead, -normal, HOLD).is_some() {
                going = facing * speed * push.abs();
            } else {
                peek = Some(sideways * facing);
            }
        }
        self.cover = Some(Cover {
            normal,
            facing,
            peek,
        });
        Some(sideways * going + normal * closing(gap, speed.max(1.0)))
    }

    /// Which way to lean round the corner, along the ground: while the droid is at the edge of
    /// the wall it is in cover against, with a key held that would take it on past.
    pub(super) fn cover_peek(&self) -> Option<Vector3<f32>> {
        self.cover?.peek
    }

    /// Which way the droid faces in cover, like
    /// [`heading`](super::avatar::heading): along the wall.
    pub(super) fn cover_heading(&self) -> Option<f32> {
        let cover = self.cover?;
        let way = along(cover.normal) * cover.facing;
        Some(super::avatar::wrap(way.x.atan2(way.z) - self.yaw))
    }

    /// The nearest wall of the maze from `from` in `direction`, within `reach`: how far off it
    /// is, and which way it faces, along the ground and back towards `from`. Only the maze's own
    /// walls count - not the floor, and not anyone standing about.
    fn wall(
        &self,
        graph: &Graph,
        from: Vector3<f32>,
        direction: Vector3<f32>,
        reach: f32,
    ) -> Option<(f32, Vector3<f32>)> {
        let mut hits = Vec::new();
        graph.physics.cast_ray(
            RayCastOptions {
                ray_origin: Point3::from(from),
                ray_direction: direction,
                max_len: reach,
                groups: Default::default(),
                sort_results: true,
            },
            &mut hits,
        );
        let fixed = |collider: Handle<Collider>| {
            let body = graph[collider.transmute::<Node>()].parent();
            graph
                .try_get_of_type::<RigidBody>(body)
                .is_ok_and(|body| body.body_type() == RigidBodyType::Static)
        };
        let hit = hits
            .iter()
            .find(|hit| hit.collider != self.collider && fixed(hit.collider))?;
        if hit.normal.y.abs() > UPRIGHT {
            return None;
        }
        // Either side of a wall may come back: the one facing `from` is wanted.
        let normal = flat(hit.normal).try_normalize(1.0e-6)?;
        let normal = if normal.dot(&direction) > 0.0 { -normal } else { normal };
        Some(((hit.position.coords - from).norm(), normal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall facing out along +z.
    const OUT: Vector3<f32> = Vector3::new(0.0, 0.0, 1.0);

    #[test]
    fn keys_along_the_wall_slide_and_keys_away_from_it_let_go() {
        let sideways = along(OUT);
        assert_eq!(read_keys(sideways, OUT), (1.0, false));
        assert_eq!(read_keys(-sideways, OUT), (-1.0, false));
        assert!(read_keys(OUT, OUT).1, "straight away");
        assert_eq!(read_keys(-OUT, OUT), (0.0, false), "into the wall goes nowhere");
        assert_eq!(read_keys(Vector3::zeros(), OUT), (0.0, false));
        // Mostly along it, a little away: still sliding.
        let (push, letting_go) = read_keys(sideways + OUT * 0.5, OUT);
        assert!(push > 0.8 && !letting_go);
    }

    #[test]
    fn it_closes_up_to_the_wall_and_no_closer() {
        assert!(closing(GAP + 0.3, 2.0) < 0.0, "too far: in towards it");
        assert!(closing(GAP - 0.1, 2.0) > 0.0, "too close: out from it");
        assert_eq!(closing(GAP, 2.0), 0.0);
        assert_eq!(closing(GAP + 5.0, 2.0), -2.0, "no faster than it can walk");
    }
}
