//! The maze's inhabitants: droids like the player's, going about the maze by themselves.
//!
//! Each one wanders. It picks somewhere a good walk away, follows the cheapest route there over
//! the survey's grid - which keeps to the middle of the corridors (see
//! [`WalkGrid::routes_from`]) - and when it gets there stands idling a while before setting off
//! again. It walks at the droid's own walking pace, so its feet keep to the floor, and slows down
//! to turn.
//!
//! They have bodies, so the player cannot walk through them, and they make way for each other.
//! One walking veers to its right round anyone ahead of it, so that two meeting head on pass
//! each other on the left, and to the left instead if a wall is in the way; one standing about
//! that sees another coming straight at it steps out of its way. Only someone right in front of
//! it stops it, and kept waiting too long, it goes somewhere else instead.
//!
//! Only those the player could see are drawn. Like the player's droid, they cast no shadows.

use crate::{
    layout::{Rng, WalkGrid},
    level::Level,
    player::{
        avatar::{self, Avatar, Going},
        posture::{Gait, Posture},
    },
    survey,
};
use fyrox::{
    core::{algebra::Vector3, log::Log, pool::Handle},
    graph::SceneGraph,
    resource::model::ModelResource,
    scene::{
        base::BaseBuilder,
        collider::{ColliderBuilder, ColliderShape},
        graph::Graph,
        node::Node,
        rigidbody::{RigidBody, RigidBodyBuilder, RigidBodyType},
        transform::TransformBuilder,
        Scene,
    },
};

/// How many droids live in a maze, unless MAZE_INHABITANTS says otherwise.
const COUNT: usize = 6;
/// Their bodies: a capsule this far out from its middle line, with its middle this far above the
/// feet - 1.7 m tall, like the player's.
const RADIUS: f32 = 0.3;
const MIDDLE: f32 = 0.85;
/// How far from the player they are put down, in steps across the grid's half-meter cells
/// (see [`WalkGrid::routes_from`]): out of sight to begin with.
const AWAY_FROM_PLAYER: f32 = 24.0;
/// How far a droid goes each time it sets off, in steps across the grid, from least to most:
/// never just round the corner, never across the whole maze.
const TRIP: (f32, f32) = (30.0, 160.0);
/// How long it stands still between trips, in seconds, from least to most.
const REST: (f32, f32) = (2.0, 10.0);
/// How near the next point on its route it has to get, in meters, before making for the one
/// after: that much of every corner is cut.
const REACHED: f32 = 0.6;
/// How near the end of its route counts as there, in meters.
const ARRIVED: f32 = 0.1;
/// How quickly it gets up to speed and slows down, in meters per second per second.
const ACCELERATION: f32 = 3.0;
/// How far ahead, in meters, a droid starts to veer round anyone in its way.
const AVOID_RANGE: f32 = 2.5;
/// How far to either side of its way, in meters, someone has to be not to be in it.
const PASSING: f32 = 1.0;
/// How hard it veers round someone just in front of it: how far to the side for every meter
/// ahead, less the further off they are.
const VEER: f32 = 1.2;
/// How close, in meters, someone straight ahead has to be for a droid to stop for them.
const KEEP_CLEAR: f32 = 0.8;
/// How far ahead, in meters, it makes sure there is floor before veering that way.
const LOOK_AHEAD: f32 = 0.6;
/// How near, in meters, a droid coming straight at one standing about has to be for that one to
/// step out of its way, and how far it steps.
const YIELD_RANGE: f32 = 2.2;
const STEP_ASIDE: f32 = 0.8;
/// How long, in seconds, a droid waits for someone in its way before going somewhere else.
const PATIENCE: f32 = 3.0;
/// How quickly its feet follow the floor up and down, like
/// [`EYE_EASING`](crate::player::posture::EYE_EASING).
const FLOOR_EASING: f32 = 10.0;
/// Its walking pace, in meters per second, if the droid has no walk to go by.
const FALLBACK_PACE: f32 = 1.4;

#[derive(Debug, Clone, PartialEq)]
struct Inhabitant {
    body: Handle<Node>,
    avatar: Avatar,
    /// Where its feet are.
    feet: Vector3<f32>,
    /// How fast it is going along the ground, in meters per second.
    speed: f32,
    /// Which way it is making for, in radians, left positive from the world's +z.
    heading: f32,
    /// The rest of its route, as points on the floor, the next one last.
    route: Vec<Vector3<f32>>,
    /// How long it has left to stand still, in seconds.
    resting: f32,
    /// How long it has been kept waiting by someone in its way, in seconds.
    waiting: f32,
    /// Where its feet were the last time the graphics effects were told.
    last_seen: Option<Vector3<f32>>,
}

/// Everyone who lives in the maze.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Inhabitants {
    droids: Vec<Inhabitant>,
    /// Whether they have been put into this round's maze yet.
    populated: bool,
}

/// A number from `range.0` to `range.1`.
fn between(rng: &mut Rng, range: (f32, f32)) -> f32 {
    range.0 + (range.1 - range.0) * rng.below(1001) as f32 / 1000.0
}

/// `vector` along the ground.
fn flat(vector: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(vector.x, 0.0, vector.z)
}

/// Which way `heading` faces, along the ground: left positive from the world's +z.
fn forward(heading: f32) -> Vector3<f32> {
    Vector3::new(heading.sin(), 0.0, heading.cos())
}

/// A droid's right, going `ahead`.
fn right_of(ahead: Vector3<f32>) -> Vector3<f32> {
    Vector3::new(-ahead.z, 0.0, ahead.x)
}

/// Which way a droid at `feet` should go to make its way `ahead` round `others` - each where
/// they are, and which way they are walking, if they are - veering to its right, or failing that
/// to its left, but only where `floor_at` says there is floor. And whether someone is right in
/// front of it, so that it has to stop.
fn make_way(
    feet: Vector3<f32>,
    ahead: Vector3<f32>,
    others: impl Iterator<Item = (Vector3<f32>, Option<Vector3<f32>>)>,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> (Vector3<f32>, bool) {
    let right = right_of(ahead);
    let mut veer: f32 = 0.0;
    let mut blocked = false;
    for (there, _) in others {
        let to_them = flat(there - feet);
        let distance = to_them.norm();
        let (along, across) = (to_them.dot(&ahead), to_them.dot(&right));
        if distance > AVOID_RANGE || along <= 0.0 || across.abs() > PASSING {
            continue;
        }
        veer = veer.max(VEER * (1.0 - distance / AVOID_RANGE));
        blocked |= distance < KEEP_CLEAR && along > 0.8 * distance;
    }
    if veer == 0.0 {
        return (ahead, blocked);
    }
    // Right if there is room, then left, and less sharply before not at all.
    for side in [veer, 0.5 * veer, -veer, -0.5 * veer] {
        let way = (ahead + right * side).normalize();
        if floor_at(feet + way * LOOK_AHEAD) {
            // Veering hard, it gets past whoever is in front of it rather than waiting for them.
            return (way, blocked && side.abs() < 0.5 * VEER);
        }
    }
    (ahead, blocked)
}

/// Where a droid standing about at `feet` should step to, to get out of the way of any of
/// `others` walking straight at it: out to the side they are not veering to, or failing that
/// the other side, wherever `floor_at` says there is floor.
fn step_aside(
    feet: Vector3<f32>,
    others: impl Iterator<Item = (Vector3<f32>, Option<Vector3<f32>>)>,
    floor_at: impl Fn(Vector3<f32>) -> bool,
) -> Option<Vector3<f32>> {
    for (there, walking) in others {
        let Some(going) = walking else {
            continue;
        };
        let to_me = flat(feet - there);
        let distance = to_me.norm();
        let (along, across) = (to_me.dot(&going), to_me.dot(&right_of(going)));
        if distance > YIELD_RANGE || along <= 0.0 || across.abs() > PASSING {
            continue;
        }
        // They veer to their right, so out to their left.
        let left = -right_of(going);
        return [left, -left]
            .into_iter()
            .map(|side| feet + side * STEP_ASIDE)
            .find(|&spot| floor_at(spot) && floor_at(feet + (spot - feet) * 0.5));
    }
    None
}

/// The middle of a cell of the grid, on its floor.
fn on_floor(grid: &WalkGrid, origin: Vector3<f32>, (x, z): (usize, usize)) -> Vector3<f32> {
    survey::cell_center(origin, x, z) + Vector3::new(0.0, grid.floor(x, z), 0.0)
}

/// A route from `feet` to somewhere a trip away, as points on the floor, the first one last. Empty
/// if there is nowhere to go.
fn plan(grid: &WalkGrid, origin: Vector3<f32>, feet: Vector3<f32>, rng: &mut Rng) -> Vec<Vector3<f32>> {
    let Some(from) =
        survey::cell_at(grid, origin, feet).filter(|&(x, z)| grid.is_walkable(x, z))
    else {
        return Vec::new();
    };
    let routes = grid.routes_from(from, TRIP.1);
    let reached = |range: (f32, f32)| -> Vec<usize> {
        routes
            .costs
            .iter()
            .enumerate()
            .filter(|(_, cost)| cost.is_some_and(|c| c >= range.0 && c <= range.1))
            .map(|(i, _)| i)
            .collect()
    };
    // Somewhere a trip away; failing that, in a small space, anywhere else at all.
    let mut choices = reached(TRIP);
    if choices.is_empty() {
        choices = reached((f32::MIN_POSITIVE, f32::INFINITY));
    }
    if choices.is_empty() {
        return Vec::new();
    }
    let goal = choices[rng.below(choices.len())];
    let Some(path) = routes.path_to((goal % grid.width, goal / grid.width)) else {
        return Vec::new();
    };
    path.into_iter()
        .skip(1)
        .rev()
        .map(|cell| on_floor(grid, origin, cell))
        .collect()
}

impl Inhabitants {
    /// Whether they have been put into this round's maze yet.
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Takes everyone out of the scene, to be put into the next round afresh.
    pub fn clear(&mut self, graph: &mut Graph) {
        for droid in self.droids.drain(..) {
            if graph.is_valid_handle(droid.body) {
                graph.remove_node(droid.body);
            }
        }
        self.populated = false;
    }

    /// Puts the maze's inhabitants into `scene`, as droids from `model`, on the floor of `grid`
    /// whose corner is at `origin`, well away from the `player`'s feet.
    pub fn populate(
        &mut self,
        scene: &mut Scene,
        model: &ModelResource,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: Vector3<f32>,
        rng: &mut Rng,
    ) {
        self.clear(&mut scene.graph);
        self.populated = true;
        let count = std::env::var("MAZE_INHABITANTS")
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(COUNT);
        let Some(start) = survey::cell_at(grid, origin, player)
            .filter(|&(x, z)| grid.is_walkable(x, z))
            .or_else(|| survey::nearest_walkable(grid, origin, player))
        else {
            return;
        };
        let routes = grid.routes_from(start, f32::INFINITY);
        let mut places: Vec<(usize, usize)> = routes
            .costs
            .iter()
            .enumerate()
            .filter(|(_, cost)| cost.is_some_and(|c| c >= AWAY_FROM_PLAYER))
            .map(|(i, _)| (i % grid.width, i / grid.width))
            .collect();

        for _ in 0..count {
            if places.is_empty() {
                break;
            }
            let feet = on_floor(grid, origin, places.swap_remove(rng.below(places.len())));
            let collider = ColliderBuilder::new(BaseBuilder::new())
                .with_shape(ColliderShape::capsule_y(MIDDLE - RADIUS, RADIUS))
                .build(&mut scene.graph);
            let body = RigidBodyBuilder::new(
                BaseBuilder::new().with_child(collider).with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(feet + Vector3::new(0.0, MIDDLE, 0.0))
                        .build(),
                ),
            )
            .with_body_type(RigidBodyType::KinematicPositionBased)
            .build(&mut scene.graph)
            .to_base();
            let Some(avatar) = Avatar::spawn(model, scene, body, -MIDDLE, true) else {
                scene.graph.remove_node(body);
                break;
            };
            self.droids.push(Inhabitant {
                body,
                avatar,
                feet,
                speed: 0.0,
                heading: between(rng, (-std::f32::consts::PI, std::f32::consts::PI)),
                route: Vec::new(),
                // Not all setting off at once.
                resting: between(rng, REST),
                waiting: 0.0,
                last_seen: None,
            });
        }
        Log::info(format!("Maze: {} inhabitants", self.droids.len()));
    }

    /// Moves everyone along for another `dt`, over `grid` whose corner is at `origin`, making
    /// way for each other and for the `player`'s feet.
    pub fn update(
        &mut self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        player: Vector3<f32>,
        rng: &mut Rng,
        dt: f32,
    ) {
        let floor_at = |spot: Vector3<f32>| {
            survey::cell_at(grid, origin, spot).is_some_and(|(x, z)| grid.is_walkable(x, z))
        };
        // Where everyone is, and which way those walking are going; the player last.
        let everyone: Vec<(Vector3<f32>, Option<Vector3<f32>>)> = self
            .droids
            .iter()
            .map(|droid| {
                let walking = (droid.speed > 0.1).then(|| forward(droid.heading));
                (droid.feet, walking)
            })
            .chain(std::iter::once((player, None)))
            .collect();
        for (me, droid) in self.droids.iter_mut().enumerate() {
            let others = everyone
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != me)
                .map(|(_, &other)| other);
            droid.resting = (droid.resting - dt).max(0.0);
            if droid.route.is_empty() {
                if let Some(aside) = step_aside(droid.feet, others.clone(), floor_at) {
                    droid.route = vec![aside];
                } else if droid.resting == 0.0 {
                    droid.route = plan(grid, origin, droid.feet, rng);
                    if droid.route.is_empty() {
                        droid.resting = REST.0;
                    }
                }
            }
            // Past each point on the way, bar the last, as soon as it is near - or already
            // behind, having been put off course making way for someone.
            while let [.., after, next] = droid.route[..] {
                if flat(next - droid.feet).norm() < REACHED
                    || flat(after - droid.feet).norm() < flat(after - next).norm()
                {
                    droid.route.pop();
                } else {
                    break;
                }
            }

            let mut wanted = 0.0;
            if let Some(&next) = droid.route.last() {
                let to = flat(next - droid.feet);
                if droid.route.len() == 1 && to.norm() < ARRIVED {
                    droid.route.clear();
                    droid.resting = between(rng, REST);
                } else {
                    let (way, blocked) = make_way(droid.feet, to.normalize(), others, floor_at);
                    droid.heading = way.x.atan2(way.z);
                    if blocked {
                        droid.waiting += dt;
                        if droid.waiting > PATIENCE {
                            droid.route.clear();
                            droid.waiting = 0.0;
                            droid.resting = between(rng, (0.5, 2.0));
                        }
                    } else {
                        droid.waiting = 0.0;
                        let pace = droid
                            .avatar
                            .pace(Posture::Standing, Gait::Walking)
                            .unwrap_or(FALLBACK_PACE);
                        // Slower the further it has yet to turn, and slowing down to stop at the
                        // end of the way.
                        let off = droid.heading - droid.avatar.facing();
                        let left: f32 = droid
                            .route
                            .windows(2)
                            .map(|pair| flat(pair[0] - pair[1]).norm())
                            .sum::<f32>()
                            + to.norm();
                        wanted =
                            (pace * off.cos().max(0.0)).min((2.0 * ACCELERATION * left).sqrt());
                    }
                }
            }
            let step = ACCELERATION * dt;
            droid.speed += (wanted - droid.speed).clamp(-step, step);
            droid.feet += forward(droid.heading) * (droid.speed * dt);
            if let Some((x, z)) = survey::cell_at(grid, origin, droid.feet) {
                if grid.is_walkable(x, z) {
                    let floor = grid.floor(x, z);
                    droid.feet.y += (floor - droid.feet.y) * (1.0 - (-FLOOR_EASING * dt).exp());
                }
            }

            if let Ok(body) = graph.try_get_mut_of_type::<RigidBody>(droid.body) {
                body.set_next_kinematic_translation(droid.feet + Vector3::new(0.0, MIDDLE, 0.0));
            }
            let going = Going {
                heading: Some(droid.heading),
                speed: droid.speed,
                posture: Posture::Standing,
                gait: Gait::Walking,
                grounded: true,
                jumped: false,
                low: false,
                falling: 0.0,
                cover: false,
            };
            droid.avatar.animate(graph, going, dt);
        }
    }

    /// Draws only those the player could see from where they are in `level`.
    pub fn show(&self, graph: &mut Graph, level: &Level) {
        for droid in &self.droids {
            droid.avatar.set_visible(graph, level.can_see(droid.feet));
        }
    }

    /// The droid nearest `to` of those that can be seen, for the graphics effects: a capsule
    /// round it, and how far it has moved since the last time this was asked.
    pub fn moving(&mut self, graph: &Graph, to: Vector3<f32>) -> Option<fyrox_gfx::MovingThing> {
        let mut nearest: Option<(f32, fyrox_gfx::MovingThing)> = None;
        for droid in &mut self.droids {
            let moved = droid
                .last_seen
                .map_or(Vector3::zeros(), |last| droid.feet - last);
            droid.last_seen = Some(droid.feet);
            let distance = (droid.feet - to).norm();
            if droid.avatar.is_visible(graph) && nearest.is_none_or(|(d, _)| distance < d) {
                nearest = Some((distance, avatar::capsule(droid.feet, moved)));
            }
        }
        nearest.map(|(_, thing)| thing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHEAD: Vector3<f32> = Vector3::new(0.0, 0.0, 1.0);
    const EVERYWHERE: fn(Vector3<f32>) -> bool = |_| true;

    fn standing(x: f32, z: f32) -> (Vector3<f32>, Option<Vector3<f32>>) {
        (Vector3::new(x, 0.0, z), None)
    }

    #[test]
    fn with_nobody_about_it_goes_straight_on() {
        let (way, blocked) = make_way(Vector3::zeros(), AHEAD, std::iter::empty(), EVERYWHERE);
        assert_eq!((way, blocked), (AHEAD, false));
        // Someone behind, or well off to one side, is not in the way either.
        let others = [standing(0.0, -1.0), standing(2.0, 1.0)];
        let (way, _) = make_way(Vector3::zeros(), AHEAD, others.into_iter(), EVERYWHERE);
        assert_eq!(way, AHEAD);
    }

    #[test]
    fn it_veers_right_round_someone_ahead_and_harder_the_nearer() {
        let right = right_of(AHEAD);
        let (far, _) = make_way(Vector3::zeros(), AHEAD, [standing(0.0, 2.0)].into_iter(), EVERYWHERE);
        let (near, blocked) =
            make_way(Vector3::zeros(), AHEAD, [standing(0.0, 1.0)].into_iter(), EVERYWHERE);
        assert!(far.dot(&right) > 0.0 && near.dot(&right) > far.dot(&right));
        assert!(!blocked);
    }

    #[test]
    fn with_a_wall_on_the_right_it_veers_left() {
        let right = right_of(AHEAD);
        let floor = |spot: Vector3<f32>| spot.dot(&right_of(AHEAD)) < 0.1;
        let (way, _) = make_way(Vector3::zeros(), AHEAD, [standing(0.0, 1.0)].into_iter(), floor);
        assert!(way.dot(&right) < 0.0);
    }

    #[test]
    fn hemmed_in_with_someone_right_in_front_it_stops() {
        let floor = |spot: Vector3<f32>| spot.x.abs() < 0.05;
        let (_, blocked) = make_way(Vector3::zeros(), AHEAD, [standing(0.0, 0.5)].into_iter(), floor);
        assert!(blocked);
    }

    #[test]
    fn standing_about_it_steps_out_of_the_way_of_someone_coming_at_it() {
        let coming = (Vector3::new(0.0, 0.0, -1.5), Some(AHEAD));
        let spot = step_aside(Vector3::zeros(), [coming].into_iter(), EVERYWHERE).unwrap();
        // Out to their left, since they veer to their right.
        assert!(spot.dot(&right_of(AHEAD)) < -0.5);
        // Nobody walking at it, or someone walking away, leaves it be.
        let going_away = (Vector3::new(0.0, 0.0, -1.5), Some(-AHEAD));
        assert_eq!(step_aside(Vector3::zeros(), [going_away].into_iter(), EVERYWHERE), None);
        let waiting = standing(0.0, -1.5);
        assert_eq!(step_aside(Vector3::zeros(), [waiting].into_iter(), EVERYWHERE), None);
    }
}
