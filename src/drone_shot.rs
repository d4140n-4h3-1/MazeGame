//! The drone's shots: plasma bolts, a glowing ball with a tail and two small rings turning about
//! it, made in Blender ([`SHOT_MODEL`]). One leaves the drone's eye each time it fires, at where
//! the player is, flies straight at [`SPEED`] - slow enough to step out of the way of from a few
//! steps off - and stops at the first thing it hits, glowing there a moment longer. Hitting the
//! player, it hurts them: the game is told (see [`Shots::update`]).
//!
//! It glows, and lights what it passes, in the colour of the drone's mood as it fired. Like the
//! pistol's bolts, a handful are made once and shown while they fly: adding or taking away a mesh
//! has the traced shadows gather every mesh in the scene again. It sounds as the pistol's bolts do
//! (see [`PISTOL_SOUNDS`]): a shot as it leaves, and a hum as it flies.

use crate::{
    drone::{claim_glows, light_colour},
    fixtures::{DIFFUSE_COLOR, EMISSION_STRENGTH},
    formants::{self, Sounds},
    player::PISTOL_SOUNDS,
};
use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector3},
        log::Log,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{MaterialProperty, MaterialResource},
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        animation::AnimationPlayer,
        base::BaseBuilder,
        collider::Collider,
        graph::{physics::RayCastOptions, Graph},
        light::{
            point::{PointLight, PointLightBuilder},
            BaseLightBuilder,
        },
        mesh::Mesh,
        node::Node,
        sound::{Sound as SoundNode, SoundBufferResource, SoundBuilder, Status},
        transform::TransformBuilder,
        Scene,
    },
};

/// The shot's model.
pub const SHOT_MODEL: &str = "data/drone_shot.glb";

/// How many can be in the air at once, from every drone. Firing another takes the one that has
/// flown longest.
const SHOTS: usize = 8;
/// How fast one flies, in meters per second, and how far before it is gone, if it hits nothing.
const SPEED: f32 = 12.0;
const RANGE: f32 = 40.0;
/// How brightly it glows: as many times as its colour at strength 1, and its core brighter still.
const GLOW: f32 = 3.0;
/// How far the light it carries reaches, in meters, and how brightly.
const FLASH_REACH: f32 = 4.0;
const FLASH_BRIGHTNESS: f32 = 2.5;
/// How long one that has hit something glows where it hit, in seconds; and how far back from what
/// it hit its middle stops, in meters - its ball is 0.12 m across.
const IMPACT: f32 = 0.15;
const STOP_SHORT: f32 = 0.06;

/// A shot in the air, or glowing where it hit.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flight {
    position: Vector3<f32>,
    /// Which way it flies, one meter long.
    direction: Vector3<f32>,
    /// How much further it can fly, in meters.
    range: f32,
    /// How much longer it glows where it hit something, in seconds, once it has.
    landed: Option<f32>,
}

/// One of the shots made: its model, the light and hum it carries, its own copies of its glowing
/// materials with their own strengths, and its flight, if it is flying.
#[derive(Debug, Clone, PartialEq)]
struct Shot {
    root: Handle<Node>,
    flash: Handle<Node>,
    hum: Option<Handle<Node>>,
    glows: Vec<(MaterialResource, f32)>,
    flight: Option<Flight>,
}

/// The drone's shots.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Shots {
    shots: Vec<Shot>,
    /// The sound of a shot leaving, and how far off it is heard at full volume, if there is one.
    sound: Option<(SoundBufferResource, f32)>,
}

/// The sound called `name` in `sounds`, made to play, with how far off it is heard at full
/// volume; if it is there.
fn made(sounds: Option<&Sounds>, name: &str) -> Option<(SoundBufferResource, f32)> {
    let sound = sounds?.sounds.get(name)?;
    Some((formants::buffer(sound, sounds?.sample_rate)?, sound.reach))
}

impl Shots {
    /// Makes the shots from their `model` in `scene`, out of sight until they are fired.
    pub fn spawn(model: &ModelResource, scene: &mut Scene) -> Self {
        let sounds = Sounds::load(PISTOL_SOUNDS)
            .inspect_err(|error| Log::err(format!("Drone shots: silent, {error}")))
            .ok();
        let (sound, hum) = (made(sounds.as_ref(), "shot"), made(sounds.as_ref(), "hum"));
        let shots = (0..SHOTS)
            .map(|_| {
                let root = model.instantiate(scene);
                let graph = &mut scene.graph;
                graph[root].set_visibility(false);
                let nodes: Vec<Handle<Node>> = graph.traverse_handle_iter(root).collect();
                for &node in &nodes {
                    if graph[node].cast::<Mesh>().is_some() {
                        graph[node].set_cast_shadows(false);
                    }
                    // Its rings turn all the while.
                    if let Ok(player) = graph.try_get_mut_of_type::<AnimationPlayer>(node) {
                        for animation in player.animations_mut().get_value_mut_silent().iter_mut() {
                            animation.set_loop(true).set_enabled(true);
                        }
                    }
                }
                // Each material's strength as made, the core's brighter than the rest.
                let glows = claim_glows(graph, &nodes)
                    .into_iter()
                    .map(|(material, _)| {
                        let strength = match crate::fixtures::glow_strength(&material.data_ref()) {
                            Some(MaterialProperty::Vector3(glow)) => glow.max(),
                            Some(MaterialProperty::Float(glow)) => glow,
                            _ => 1.0,
                        };
                        (material, strength)
                    })
                    .collect();
                // The light rides with it, but does not scatter into a haze in the air: the shot
                // is what glows.
                let flash = PointLightBuilder::new(
                    BaseLightBuilder::new(BaseBuilder::new())
                        .with_intensity(FLASH_BRIGHTNESS)
                        .with_scatter_enabled(false),
                )
                .with_radius(FLASH_REACH)
                .build(graph)
                .to_base();
                graph.link_nodes(flash, root);
                let hum = hum.as_ref().map(|(buffer, reach)| {
                    let hum = SoundBuilder::new(BaseBuilder::new())
                        .with_buffer(Some(buffer.clone()))
                        .with_looping(true)
                        .with_radius(*reach)
                        .build(graph)
                        .to_base();
                    graph.link_nodes(hum, root);
                    hum
                });
                Shot {
                    root,
                    flash,
                    hum,
                    glows,
                    flight: None,
                }
            })
            .collect::<Vec<_>>();
        Log::info(format!("Drone shots: {} made", shots.len()));
        Self { shots, sound }
    }

    /// Fires a shot from `from` at `at`, glowing `colour` (strength 1 at its brightest).
    pub fn fire(&mut self, graph: &mut Graph, from: Vector3<f32>, at: Vector3<f32>, colour: Vector3<f32>) {
        let Some(direction) = (at - from).try_normalize(1.0e-4) else {
            return;
        };
        // A free one, or failing that the one that has flown furthest: the least range left.
        let left = |shot: &Shot| shot.flight.map_or(f32::NEG_INFINITY, |flight| flight.range);
        let Some(shot) = self.shots.iter_mut().min_by(|a, b| left(a).total_cmp(&left(b))) else {
            return;
        };
        shot.flight = Some(Flight {
            position: from,
            direction,
            range: RANGE,
            landed: None,
        });
        for (material, strength) in &shot.glows {
            let mut material = material.data_ref();
            material.set_property(DIFFUSE_COLOR, light_colour(colour));
            material.set_property(EMISSION_STRENGTH, MaterialProperty::Vector3(colour * *strength * GLOW));
        }
        if let Ok(flash) = graph.try_get_mut_of_type::<PointLight>(shot.flash) {
            flash.base_light_mut().set_color(light_colour(colour));
        }
        let node = &mut graph[shot.root];
        node.local_transform_mut()
            .set_position(from)
            .set_rotation(UnitQuaternion::face_towards(&direction, &Vector3::y()));
        node.set_visibility(true);
        hum(graph, shot.hum, true);
        // The shot sounds where it leaves, once, and is gone.
        if let Some((buffer, reach)) = &self.sound {
            SoundBuilder::new(
                BaseBuilder::new().with_local_transform(
                    TransformBuilder::new().with_local_position(from).build(),
                ),
            )
            .with_buffer(Some(buffer.clone()))
            .with_radius(*reach)
            .with_play_once(true)
            .with_status(Status::Playing)
            .build(graph);
        }
    }

    /// Flies every shot in the air on for another `dt`, through the `drones`' bodies. How many
    /// hit `player`, the player's collider, this time.
    pub fn update(
        &mut self,
        graph: &mut Graph,
        dt: f32,
        player: Handle<Collider>,
        drones: &[Handle<Collider>],
    ) -> usize {
        let mut hits = 0;
        for shot in &mut self.shots {
            let Some(flight) = shot.flight.as_mut() else {
                continue;
            };
            // One that has hit something glows where it hit until its moment is up.
            if let Some(left) = flight.landed.as_mut() {
                *left -= dt;
                if *left <= 0.0 {
                    shot.flight = None;
                    graph[shot.root].set_visibility(false);
                }
                continue;
            }
            let step = (SPEED * dt).min(flight.range);
            let mut found = Vec::new();
            graph.physics.cast_ray(
                RayCastOptions {
                    ray_origin: Point3::from(flight.position),
                    ray_direction: flight.direction,
                    max_len: step,
                    groups: Default::default(),
                    sort_results: true,
                },
                &mut found,
            );
            if let Some(hit) = found.iter().find(|hit| !drones.contains(&hit.collider)) {
                hits += usize::from(hit.collider == player);
                let reach = (hit.position.coords - flight.position).norm();
                flight.position += flight.direction * (reach - STOP_SHORT).max(0.0);
                flight.range = 0.0;
                flight.landed = Some(IMPACT);
                graph[shot.root]
                    .local_transform_mut()
                    .set_position(flight.position);
                hum(graph, shot.hum, false);
                continue;
            }
            if step >= flight.range {
                shot.flight = None;
                graph[shot.root].set_visibility(false);
                hum(graph, shot.hum, false);
                continue;
            }
            flight.position += flight.direction * step;
            flight.range -= step;
            graph[shot.root]
                .local_transform_mut()
                .set_position(flight.position);
        }
        hits
    }

    /// Puts every shot out of sight, and out of the air.
    pub fn clear(&mut self, graph: &mut Graph) {
        for shot in &mut self.shots {
            if shot.flight.take().is_some() {
                graph[shot.root].set_visibility(false);
                hum(graph, shot.hum, false);
            }
        }
    }
}

/// Starts or stops a shot's hum, if it has one.
fn hum(graph: &mut Graph, hum: Option<Handle<Node>>, on: bool) {
    if let Some(hum) = hum.and_then(|hum| graph.try_get_mut_of_type::<SoundNode>(hum).ok()) {
        hum.stop();
        if on {
            hum.play();
        }
    }
}
