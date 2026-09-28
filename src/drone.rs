//! A drone, for now only to be seen working: it hovers a few steps in front of where the player
//! starts, facing them, and goes through each of its animations in turn - idling, patrolling,
//! scanning, firing twice and dying - and then again from the start.
//!
//! Its model, [`DRONE_MODEL`], is made in Blender: `drone_root`, which the animations move about,
//! with the eye, the shell and two rings under it. The rings' spin is in the animations, but not
//! its glow, which is played here to go with each: what its green materials glow with, over the
//! animation's frames at 24 a second ([`glow`]).
//!
//! It lights what is round it too, green, as brightly as it glows: a lamp just in front of its
//! eye, whose shadows are traced as every light's are. Not inside it, where the drone's own shell
//! and eye, which are traced with the rest of the scene, would shut the light in.

use crate::{
    fixtures::{glow_strength, EMISSION_STRENGTH},
    formants::Curve,
    layout::WalkGrid,
    survey,
};
use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector3},
        color::Color,
        log::Log,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{MaterialProperty, MaterialResource},
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        animation::{Animation, AnimationPlayer},
        base::BaseBuilder,
        graph::Graph,
        light::{
            point::{PointLight, PointLightBuilder},
            BaseLightBuilder,
        },
        mesh::Mesh,
        node::Node,
        Scene,
    },
};

/// The drone's model.
pub const DRONE_MODEL: &str = "data/drone.glb";
/// How much the model is scaled: as made it is 6.5 m across its rings, and 1.3 m in the game.
const SCALE: f32 = 0.2;
/// How high it hovers, in meters above the floor; and how far in front of the player it goes,
/// the first of these with floor all the way to it.
const HOVER: f32 = 1.7;
const AHEAD: [f32; 4] = [3.5, 3.0, 2.5, 2.0];
/// Its lamp: where it is, in the model's own terms along the way it faces - just clear of the
/// front of its eye, which reaches 1.4 out from the middle; its colour; how bright it is for each
/// strength of [`glow`]; and how far it reaches, in meters.
const LAMP_AT: f32 = 1.7;
const LAMP_COLOUR: Color = Color::opaque(40, 255, 80);
const LAMP_BRIGHTNESS: f32 = 1.5;
const LAMP_REACH: f32 = 5.0;
/// Its node that the animations move about, which the lamp goes along with.
const BODY: &str = "drone_root";
/// The material property that says how much like metal a surface is, from 0 to 1.
const METALLIC_FACTOR: &str = "metallicFactor";
/// The animations' frames a second.
const FPS: f32 = 24.0;
/// How many times as strong its glow is in the game as [`glow`] has it, as made in Blender: the
/// engine only glows round what is brighter than 1.01, and green light counts for less than
/// three quarters of how bright it is, so that as made its green would only be coloured.
const BRIGHTNESS: f32 = 3.0;

/// Its animations, in the order it goes through them, each with how many times it plays.
const SHOWN: [(&str, u32); 6] = [
    ("drone_idle", 2),
    ("drone_patrol", 3),
    ("drone_scan", 1),
    ("drone_fire", 1),
    ("drone_fire", 1),
    ("drone_death", 1),
];

/// How strongly its green materials glow `frame` frames into the animation called `name`: as
/// many times as bright as green light of strength 1.
pub fn glow(name: &str, frame: f32) -> f32 {
    let curve = match name {
        // A slow breath, in and out once a loop of 97 frames.
        "drone_idle" => {
            return 1.2 - 0.3 * (frame / 97.0 * std::f32::consts::TAU).cos();
        }
        "drone_scan" => Curve(vec![
            [77.0, 1.0],
            [78.0, 5.0],
            [81.0, 2.0],
            [84.0, 5.0],
            [87.0, 2.0],
            [90.0, 3.0],
        ]),
        "drone_fire" => Curve(vec![[0.0, 1.0], [2.0, 8.0], [14.0, 1.0]]),
        // Flickering out, and then fading.
        "drone_death" => {
            let mut points = vec![[0.0, 1.0]];
            points.extend((0..=10).map(|k| [1.0 + 2.0 * k as f32, if k % 2 == 0 { 4.0 } else { 0.0 }]));
            points.push([40.0, 0.0]);
            Curve(points)
        }
        _ => return 1.0,
    };
    curve.at(frame)
}

/// The drone in the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Drone {
    root: Handle<Node>,
    player: Handle<Node>,
    /// What the animations move about, and its lamp.
    body: Handle<Node>,
    lamp: Handle<Node>,
    /// Its animations by name.
    animations: Vec<(String, Handle<Animation>)>,
    /// Its own copies of the materials that glow, and which way their glow's colour goes.
    glows: Vec<(MaterialResource, Vector3<f32>)>,
    /// Which of [`SHOWN`] it is playing, and how many times it has played it through.
    shown: usize,
    played: u32,
    /// How far into the animation it was last frame, in seconds, to tell when it comes round.
    last: f32,
}

impl Drone {
    /// Puts the drone into `scene` from its `model`, out of sight until it is placed. None if the
    /// model has no animations.
    pub fn spawn(model: &ModelResource, scene: &mut Scene) -> Option<Self> {
        let root = model.instantiate(scene);
        let graph = &mut scene.graph;
        graph[root]
            .local_transform_mut()
            .set_scale(Vector3::repeat(SCALE));
        graph[root].set_visibility(false);
        let nodes: Vec<Handle<Node>> = graph.traverse_handle_iter(root).collect();
        let Some(player) = nodes
            .iter()
            .copied()
            .find(|&node| graph[node].cast::<AnimationPlayer>().is_some())
        else {
            Log::err(format!("Drone: {DRONE_MODEL} has no animations"));
            graph.remove_node(root);
            return None;
        };
        // Its own shell and rings, just behind its lamp, would throw shadows across the corridor
        // from shadow maps. Traced shadows leave it out, as they are gathered once; so do these.
        for &node in &nodes {
            if graph[node].cast::<Mesh>().is_some() {
                graph[node].set_cast_shadows(false);
            }
        }
        let body = graph.find_by_name(root, BODY).map_or(root, |(body, _)| body);
        // Not scattering into a haze in the air: the drone is what glows.
        let lamp = PointLightBuilder::new(
            BaseLightBuilder::new(BaseBuilder::new().with_visibility(false))
                .with_color(LAMP_COLOUR)
                .with_intensity(LAMP_BRIGHTNESS)
                .with_scatter_enabled(false),
        )
        .with_radius(LAMP_REACH)
        .build(graph)
        .to_base();
        let glows = claim_glows(graph, &nodes);
        Log::info(format!("Drone: {} glowing materials", glows.len()));
        let animations = graph
            .try_get_mut_of_type::<AnimationPlayer>(player)
            .ok()?
            .animations_mut()
            .get_value_mut_silent()
            .pair_iter_mut()
            .map(|(handle, animation)| {
                animation.set_enabled(false);
                (animation.name().to_owned(), handle)
            })
            .collect::<Vec<_>>();
        for (name, _) in SHOWN {
            if !animations.iter().any(|(had, _)| had == name) {
                Log::warn(format!("Drone: {DRONE_MODEL} has no {name}"));
            }
        }
        let mut drone = Self {
            root,
            player,
            body,
            lamp,
            animations,
            glows,
            shown: SHOWN.len() - 1,
            played: 0,
            last: 0.0,
        };
        drone.next(graph);
        Some(drone)
    }

    /// Puts it hovering in front of the player, whose feet are at `feet` and who faces `ahead`,
    /// over the floor of `grid` whose corner is at `origin`, facing them. Whether there was
    /// floor in front of them for it.
    pub fn place(
        &self,
        graph: &mut Graph,
        (grid, origin): (&WalkGrid, Vector3<f32>),
        feet: Vector3<f32>,
        ahead: Vector3<f32>,
    ) -> bool {
        let floor_at = |at: Vector3<f32>| {
            survey::cell_at(grid, origin, at).filter(|&(x, z)| grid.is_walkable(x, z))
        };
        // Floor all the way there, so that it is not through a wall.
        let clear = |distance: f32| {
            let steps = (distance / (survey::CELL_SIZE * 0.5)).ceil() as usize;
            (0..=steps).all(|k| floor_at(feet + ahead * (distance * k as f32 / steps as f32)).is_some())
        };
        let Some(distance) = AHEAD.into_iter().find(|&d| clear(d)) else {
            Log::warn("Drone: no floor in front of the player to hover over");
            return false;
        };
        let at = feet + ahead * distance;
        let floor = floor_at(at).map_or(feet.y, |(x, z)| grid.floor(x, z));
        let facing = -ahead;
        graph[self.root]
            .local_transform_mut()
            .set_position(Vector3::new(at.x, floor + HOVER, at.z))
            .set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::y_axis(),
                facing.x.atan2(facing.z),
            ));
        graph[self.root].set_visibility(true);
        graph[self.lamp].set_visibility(true);
        Log::info(format!("Drone: hovering {distance:.1} m in front of the player"));
        true
    }

    /// The animation of [`SHOWN`] it is playing.
    fn playing(&self) -> Option<Handle<Animation>> {
        let (name, _) = SHOWN[self.shown];
        self.animations
            .iter()
            .find(|(had, _)| had == name)
            .map(|&(_, handle)| handle)
    }

    /// Goes on to the next of [`SHOWN`] from the start.
    fn next(&mut self, graph: &mut Graph) {
        let before = self.playing();
        self.shown = (self.shown + 1) % SHOWN.len();
        self.played = 0;
        self.last = 0.0;
        let now = self.playing();
        let Ok(player) = graph.try_get_mut_of_type::<AnimationPlayer>(self.player) else {
            return;
        };
        let animations = player.animations_mut().get_value_mut_silent();
        if let Some(before) = before.and_then(|h| animations.try_get_mut(h).ok()) {
            before.set_enabled(false);
        }
        if let Some(now) = now.and_then(|h| animations.try_get_mut(h).ok()) {
            now.set_loop(true).set_enabled(true).rewind();
        }
        Log::info(format!("Drone: {}", SHOWN[self.shown].0));
    }

    /// Glows along with the animation it is playing, and goes on to the next once it has played
    /// it through as many times as it is to. The engine plays the animation itself.
    pub fn update(&mut self, graph: &mut Graph) {
        let Some(handle) = self.playing() else {
            self.next(graph);
            return;
        };
        let time = graph
            .try_get_of_type::<AnimationPlayer>(self.player)
            .ok()
            .and_then(|player| player.animations().try_get(handle).ok())
            .map(|animation| animation.time_position());
        let Some(time) = time else {
            return;
        };
        // It has come round to the start again.
        if time < self.last {
            self.played += 1;
            if self.played >= SHOWN[self.shown].1 {
                self.next(graph);
                return;
            }
        }
        self.last = time;
        let strength = BRIGHTNESS * glow(SHOWN[self.shown].0, time * FPS);
        for (material, colour) in &self.glows {
            material
                .data_ref()
                .set_property(EMISSION_STRENGTH, MaterialProperty::Vector3(colour * strength));
        }
        // The lamp, in front of the eye wherever the animation has it, as bright as the glow.
        let at = graph[self.body]
            .global_transform()
            .transform_point(&Point3::new(0.0, 0.0, LAMP_AT))
            .coords;
        graph[self.lamp].local_transform_mut().set_position(at);
        if let Ok(lamp) = graph.try_get_mut_of_type::<PointLight>(self.lamp) {
            lamp.base_light_mut().set_intensity(LAMP_BRIGHTNESS * glow(SHOWN[self.shown].0, time * FPS));
        }
    }
}

/// Gives the drone its own copy of each material under `nodes` that glows, so that its glow can
/// change without changing the model's, with which way the colour of its glow goes, at strength 1.
fn claim_glows(graph: &mut Graph, nodes: &[Handle<Node>]) -> Vec<(MaterialResource, Vector3<f32>)> {
    let mut claimed: Vec<(u64, MaterialResource, Vector3<f32>)> = Vec::new();
    for &node in nodes {
        let Some(mesh) = graph[node].cast_mut::<Mesh>() else {
            continue;
        };
        for surface in mesh.surfaces_mut() {
            let original = surface.material().clone();
            let key = original.key();
            if let Some((_, copy, _)) = claimed.iter().find(|(had, _, _)| *had == key) {
                surface.set_material(copy.clone());
                continue;
            }
            let state = original.state();
            let Some(material) = state.data_ref() else {
                continue;
            };
            let colour = match glow_strength(material) {
                Some(MaterialProperty::Vector3(glow)) => glow / glow.max(),
                Some(_) => Vector3::new(0.0, 1.0, 0.0),
                None => continue,
            };
            let mut copy = material.clone();
            // Not metal: the engine lights a metal only by what it reflects, and puts out any glow
            // it has with the rest of its own colour. The model says nothing, which glTF takes
            // for all metal.
            copy.set_property(METALLIC_FACTOR, MaterialProperty::Float(0.0));
            let copy = MaterialResource::new_embedded(copy);
            drop(state);
            surface.set_material(copy.clone());
            claimed.push((key, copy, colour));
        }
    }
    claimed.into_iter().map(|(_, copy, colour)| (copy, colour)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_green_glows_past_the_engines_threshold_whenever_it_is_lit() {
        // As the engine weighs green light (Rec. 709).
        let brightest = |strength: f32| 0.7152 * BRIGHTNESS * strength;
        for f in 0..97 {
            assert!(brightest(glow("drone_idle", f as f32)) > 1.01, "idle at {f}");
        }
        assert!(brightest(glow("drone_patrol", 0.0)) > 1.01);
    }

    #[test]
    fn its_glow_follows_each_animation() {
        // Idle breathes between 0.9 and 1.5.
        let idle: Vec<f32> = (0..97).map(|f| glow("drone_idle", f as f32)).collect();
        let (low, high) = idle.iter().fold((f32::MAX, f32::MIN), |(l, h), &g| (l.min(g), h.max(g)));
        assert!((low - 0.9).abs() < 0.01 && (high - 1.5).abs() < 0.01, "{low} {high}");
        assert_eq!(glow("drone_patrol", 30.0), 1.0);
        // Scan pulses at the end.
        assert_eq!(glow("drone_scan", 40.0), 1.0);
        assert_eq!(glow("drone_scan", 78.0), 5.0);
        assert_eq!(glow("drone_scan", 81.0), 2.0);
        assert_eq!(glow("drone_scan", 84.0), 5.0);
        assert_eq!(glow("drone_scan", 96.0), 3.0);
        // Fire flashes.
        assert_eq!(glow("drone_fire", 2.0), 8.0);
        assert_eq!(glow("drone_fire", 14.0), 1.0);
        // Death flickers, then goes out.
        assert_eq!(glow("drone_death", 1.0), 4.0);
        assert_eq!(glow("drone_death", 3.0), 0.0);
        assert_eq!(glow("drone_death", 21.0), 4.0);
        assert_eq!(glow("drone_death", 40.0), 0.0);
        assert_eq!(glow("drone_death", 60.0), 0.0);
    }
}
