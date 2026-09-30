//! The pistol: R draws it and holsters it again; the left mouse button draws it too, and once it
//! is drawn fires it. Drawn, the droid holds it lower, at the ready, and strafes; the right mouse
//! button raises it to aim, and a shot from the ready raises it, fires as soon as it is up, and
//! lowers it again a moment later.
//!
//! A shot is a glowing green bolt that leaves the droid's muzzle as the droid fires, flies
//! straight the way the gun was pointing (see [`Avatar::shot`](super::avatar::Avatar::shot)) -
//! wherever the camera is looking - and stops at the first thing it hits. It carries the
//! muzzle's green flash with it, lighting up the droid as it leaves and everything it passes in
//! the dark. Where it hits something it stops with its tip against it and glows there a moment
//! longer, lighting up what it hit, before it is gone. What it stops at is told to the game (see
//! [`Player::struck`]), which decides what harm that does.
//!
//! The bolts are a handful of meshes made once, with the player, and shown while they fly:
//! adding or taking away a mesh has the traced shadows gather every mesh in the scene again.
//!
//! A shot sounds at the muzzle, and each bolt hums as it flies, till it stops. The sounds are
//! made from the formants in [`PISTOL_SOUNDS`] when the player is: see [`crate::formants`]. The
//! shot, and a bolt hitting something, are heard by the droids too: see [`noise`](super::noise).

use super::{noise, Player};
use crate::formants::{self, Sounds};
use fyrox::{
    core::{
        algebra::{Matrix4, UnitQuaternion, Vector3},
        color::Color,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{Material, MaterialResource},
    resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource},
    scene::{
        base::BaseBuilder,
        collider::Collider,
        graph::Graph,
        light::{point::PointLightBuilder, BaseLightBuilder},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            MeshBuilder,
        },
        node::Node,
        sound::{Sound as SoundNode, SoundBufferResource, SoundBuilder, Status},
        transform::TransformBuilder,
    },
};

/// The pistol's sounds, as formants: `shot` as a bolt leaves, and `hum` as it flies.
pub const PISTOL_SOUNDS: &str = "data/sounds/pistol_formants.json";

/// How many bolts can be in the air at once. Firing another takes the one that has flown
/// longest.
const BOLTS: usize = 8;
/// How fast a bolt flies, in meters per second.
pub const BOLT_SPEED: f32 = 40.0;
/// How far a bolt flies before it is gone, if it hits nothing, in meters.
const BOLT_RANGE: f32 = 100.0;
/// How long a bolt is, from end to end, and how thick, in meters.
const BOLT_LENGTH: f32 = 0.5;
const BOLT_WIDTH: f32 = 0.05;
/// The bolt's colour - the green of the ball at the muzzle - and how brightly it glows by itself.
const BOLT_COLOR: Color = Color::opaque(40, 255, 60);
/// The glow of the ball at the muzzle, lighting what is round it while the pistol is out: how
/// bright it is, and how far it reaches, in meters.
const MUZZLE_BRIGHTNESS: f32 = 0.6;
const MUZZLE_REACH: f32 = 1.5;
const BOLT_GLOW: f32 = 10.0;
/// How far the flash a bolt carries lights, in meters, and how brightly.
const FLASH_REACH: f32 = 5.0;
const FLASH_BRIGHTNESS: f32 = 3.0;
/// How long a bolt that has hit something glows where it hit, in seconds.
const IMPACT: f32 = 0.12;

/// A bolt hitting something: what, where, and which way it was going, one meter long.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strike {
    pub collider: Handle<Collider>,
    pub at: Vector3<f32>,
    pub way: Vector3<f32>,
}

/// A bolt in the air.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bolt {
    position: Vector3<f32>,
    /// Which way it flies, one meter long.
    direction: Vector3<f32>,
    /// How much further it can fly, in meters, and whether it glows there as if it had hit
    /// something when it gets there.
    range: f32,
    lands: bool,
    /// How much longer it glows where it hit something, in seconds, once it has.
    landed: Option<f32>,
}

/// The pistol's bolts: a few meshes, shown while they fly. Anyone with a pistol can have them -
/// the player, or the droids in the examples.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Bolts {
    /// Every bolt's mesh, and the bolt if it is flying.
    bolts: Vec<(Handle<Node>, Option<Bolt>)>,
    /// Every bolt's hum, as `bolts` has them, if there is one to hum.
    hums: Vec<Handle<Node>>,
    /// The sound of a shot, and how far off it is heard at full volume, if there is one.
    shot: Option<(SoundBufferResource, f32)>,
    /// What bolts have hit since the game last asked.
    struck: Vec<Strike>,
}

/// The sound called `name` in `sounds`, made to play, with how far off it is heard at full
/// volume; if it is there.
fn made(sounds: Option<&Sounds>, name: &str) -> Option<(SoundBufferResource, f32)> {
    let sounds = sounds?;
    let sound = sounds.sounds.get(name).or_else(|| {
        fyrox::core::log::Log::warn(format!("Pistol: {PISTOL_SOUNDS} has no {name}"));
        None
    })?;
    Some((formants::buffer(sound, sounds.sample_rate)?, sound.reach))
}

impl Bolts {
    /// Makes the bolts' meshes, out of sight until they are fired.
    pub fn new(graph: &mut Graph) -> Self {
        // Glowing all over: the standard shader glows by an emission texture, and by nothing
        // without one.
        let white = Texture::from_bytes(
            TextureKind::Rectangle {
                width: 1,
                height: 1,
            },
            TexturePixelKind::RGBA8,
            vec![255; 4],
        )
        .map(TextureResource::new_embedded);
        let mut material = Material::standard();
        material.set_property("diffuseColor", BOLT_COLOR);
        material.set_property(
            "emissionStrength",
            Vector3::new(BOLT_COLOR.r, BOLT_COLOR.g, BOLT_COLOR.b).cast::<f32>() / 255.0
                * BOLT_GLOW,
        );
        material.bind("emissionTexture", white);
        let material = MaterialResource::new_embedded(material);
        let sounds = Sounds::load(PISTOL_SOUNDS)
            .inspect_err(|error| fyrox::core::log::Log::err(format!("Pistol: silent, {error}")))
            .ok();
        let (shot, hum) = (made(sounds.as_ref(), "shot"), made(sounds.as_ref(), "hum"));
        let mut hums = Vec::new();
        // Drawn out along +Z, the way it flies.
        let shape = SurfaceResource::new_embedded(SurfaceData::make_sphere(
            8,
            8,
            0.5,
            &Matrix4::new_nonuniform_scaling(&Vector3::new(BOLT_WIDTH, BOLT_WIDTH, BOLT_LENGTH)),
        ));
        let bolts = (0..BOLTS)
            .map(|_| {
                // The flash rides with the bolt, and shows and goes with it. It lights what is
                // round the bolt, but does not scatter into a haze in the air: the bolt is what
                // glows.
                let flash: Handle<Node> = PointLightBuilder::new(
                    BaseLightBuilder::new(BaseBuilder::new())
                        .with_color(BOLT_COLOR)
                        .with_intensity(FLASH_BRIGHTNESS)
                        .with_scatter_enabled(false),
                )
                .with_radius(FLASH_REACH)
                .build(graph)
                .to_base();
                let mut base = BaseBuilder::new()
                    .with_cast_shadows(false)
                    .with_visibility(false)
                    .with_child(flash);
                // Its hum rides with it too, started as it is fired and stopped as it stops.
                if let Some((buffer, reach)) = &hum {
                    let hum = SoundBuilder::new(BaseBuilder::new())
                        .with_buffer(Some(buffer.clone()))
                        .with_looping(true)
                        .with_radius(*reach)
                        .build(graph)
                        .to_base();
                    hums.push(hum);
                    base = base.with_child(hum);
                }
                let mesh = MeshBuilder::new(base)
                    .with_surfaces(vec![SurfaceBuilder::new(shape.clone())
                        .with_material(material.clone())
                        .build()])
                    .build(graph)
                    .to_base();
                (mesh, None)
            })
            .collect();
        Self {
            bolts,
            hums,
            shot,
            struck: Vec::new(),
        }
    }

    /// Starts or stops the hum of the bolt that is `index` of `bolts`, if it has one.
    fn hum(&self, graph: &mut Graph, index: usize, on: bool) {
        let Some(&hum) = self.hums.get(index) else {
            return;
        };
        if let Ok(hum) = graph.try_get_mut_of_type::<SoundNode>(hum) {
            if on {
                hum.stop();
                hum.play();
            } else {
                hum.stop();
            }
        }
    }

    /// Fires a bolt from `from` along `direction`, one meter long.
    pub fn fire(&mut self, graph: &mut Graph, from: Vector3<f32>, direction: Vector3<f32>) {
        self.fire_for(graph, from, direction, BOLT_RANGE, false);
    }

    /// Fires a bolt from `from` along `direction`, one meter long, that flies no further than
    /// `range` - and there, if it `lands`, glows as if it had hit something.
    pub fn fire_for(
        &mut self,
        graph: &mut Graph,
        from: Vector3<f32>,
        direction: Vector3<f32>,
        range: f32,
        lands: bool,
    ) {
        // A free one, or failing that the one that has flown furthest: the least range left.
        let left = |bolt: &Option<Bolt>| bolt.map_or(f32::NEG_INFINITY, |bolt| bolt.range);
        let Some((index, (mesh, bolt))) = self
            .bolts
            .iter_mut()
            .enumerate()
            .min_by(|a, b| left(&a.1 .1).total_cmp(&left(&b.1 .1)))
        else {
            return;
        };
        *bolt = Some(Bolt {
            position: from,
            direction,
            range: range.min(BOLT_RANGE),
            lands,
            landed: None,
        });
        let node = &mut graph[*mesh];
        node.local_transform_mut()
            .set_position(from)
            .set_rotation(UnitQuaternion::face_towards(&direction, &Vector3::y()));
        node.set_visibility(true);
        self.hum(graph, index, true);
        // The shot sounds where it leaves, once, and is gone.
        if let Some((buffer, reach)) = &self.shot {
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

    /// Flies every bolt in the air on for another `dt`, stopping each at the first thing
    /// `first_hit` - from where, which way, how far - says is in its way, if anything: how far
    /// along, and what. What each hit, it returns.
    pub fn fly(
        &mut self,
        graph: &mut Graph,
        dt: f32,
        first_hit: impl Fn(&Graph, Vector3<f32>, Vector3<f32>, f32) -> Option<(f32, Handle<Collider>)>,
    ) -> Vec<Strike> {
        let mut struck = Vec::new();
        // How far each bolt gets this frame, and what it hits, if it does.
        let flights: Vec<Option<(f32, Option<Handle<Collider>>, bool)>> = self
            .bolts
            .iter()
            .map(|(_, bolt)| {
                let bolt = bolt.filter(|bolt| bolt.landed.is_none())?;
                let step = (BOLT_SPEED * dt).min(bolt.range);
                Some(
                    match first_hit(graph, bolt.position, bolt.direction, step) {
                        Some((reach, hit)) => (reach, Some(hit), true),
                        None => (step, None, step >= bolt.range),
                    },
                )
            })
            .collect();
        for (index, flight) in flights.into_iter().enumerate() {
            let (mesh, bolt) = &mut self.bolts[index];
            let Some(flying) = bolt.as_mut() else {
                continue;
            };
            // One that has hit something glows where it hit until its moment is up.
            if let Some(left) = flying.landed.as_mut() {
                *left -= dt;
                if *left <= 0.0 {
                    *bolt = None;
                    graph[*mesh].set_visibility(false);
                }
                continue;
            }
            let Some((reach, hit, stopped)) = flight else {
                continue;
            };
            struck.extend(hit.map(|collider| Strike {
                collider,
                at: flying.position + flying.direction * reach,
                way: flying.direction,
            }));
            if hit.is_some() {
                // Its tip against what it hit, its flash lighting it up; and quiet, and the first
                // to go if another is fired.
                flying.position += flying.direction * (reach - 0.5 * BOLT_LENGTH).max(0.0);
                flying.range = 0.0;
                flying.landed = Some(IMPACT);
                graph[*mesh]
                    .local_transform_mut()
                    .set_position(flying.position);
                self.hum(graph, index, false);
                continue;
            }
            if stopped && flying.lands {
                // At the end of the way it was sent, glowing there as if it had hit something.
                flying.position += flying.direction * (reach - 0.5 * BOLT_LENGTH).max(0.0);
                flying.range = 0.0;
                flying.landed = Some(IMPACT);
                graph[*mesh]
                    .local_transform_mut()
                    .set_position(flying.position);
                self.hum(graph, index, false);
                continue;
            }
            if stopped {
                *bolt = None;
                graph[*mesh].set_visibility(false);
                self.hum(graph, index, false);
                continue;
            }
            flying.position += flying.direction * reach;
            flying.range -= reach;
            graph[*mesh]
                .local_transform_mut()
                .set_position(flying.position);
        }
        struck
    }

    /// Puts every bolt out of sight, and out of the air.
    pub fn clear(&mut self, graph: &mut Graph) {
        self.struck.clear();
        for index in 0..self.bolts.len() {
            let (mesh, bolt) = &mut self.bolts[index];
            if bolt.take().is_some() {
                graph[*mesh].set_visibility(false);
                self.hum(graph, index, false);
            }
        }
    }
}

/// The muzzle's glow, put out until the pistol is drawn. Not scattering into a haze in the air:
/// the ball at the muzzle is what glows.
pub(super) fn muzzle_lamp(graph: &mut Graph) -> Handle<Node> {
    PointLightBuilder::new(
        BaseLightBuilder::new(BaseBuilder::new().with_visibility(false))
            .with_color(BOLT_COLOR)
            .with_intensity(MUZZLE_BRIGHTNESS)
            .with_scatter_enabled(false),
    )
    .with_radius(MUZZLE_REACH)
    .build(graph)
    .to_base()
}

impl Player {
    /// Fires a bolt from the muzzle the way the gun pointed, if the droid let a shot go this
    /// frame; and flies every bolt in the air on for another `dt`.
    pub(super) fn shoot(&mut self, graph: &mut Graph, dt: f32) {
        // In first person, from the pistol held in view, for the middle of the view.
        let shot = self.avatar.as_ref().and_then(|avatar| avatar.shot());
        let shot = shot.map(|shot| self.aim_from_view(graph).unwrap_or(shot));
        if let Some((muzzle, direction)) = shot {
            self.bolts.fire(graph, muzzle, direction);
            self.make_noise(muzzle, noise::SHOT_NOISE);
        }
        // Each bolt on for another `dt`: what it hits, the droids hear, and the game is told.
        let me = self.collider;
        let strikes = self.bolts.fly(graph, dt, |graph, from, direction, reach| {
            super::first_hit(graph, from, direction, reach, me)
        });
        for strike in &strikes {
            // Where the bolt stops, its tip against what it hit.
            let at = strike.at - strike.way * (0.5 * BOLT_LENGTH);
            self.make_noise(at, noise::IMPACT_NOISE);
        }
        self.bolts.struck.extend(strikes);
    }

    /// Where the pistol is pointed while it is out: from where the view is, along the middle of
    /// it, one meter long. None while it is put away.
    pub fn pistol_aim(&self, graph: &Graph) -> Option<(Vector3<f32>, Vector3<f32>)> {
        let out = self.avatar.as_ref()?.pistol_out(graph);
        let camera = &graph[self.camera];
        out.then(|| (camera.global_position(), camera.look_vector().normalize()))
    }

    /// What the pistol's bolts have hit since this was last asked, each once for every bolt.
    pub fn struck(&mut self) -> Vec<Strike> {
        std::mem::take(&mut self.bolts.struck)
    }

    /// Puts the muzzle's glow where the muzzle is that can be seen - the pistol held in first
    /// person, or the droid's - as of the last frame, or puts it out while neither is.
    pub(super) fn light_muzzle(&self, graph: &mut Graph) {
        let showing = |pistol: Handle<Node>| {
            graph
                .try_get(pistol)
                .is_ok_and(|node| node.global_visibility())
        };
        let from_view = self
            .viewmodel
            .as_ref()
            .and_then(|view| view.pistol_nodes())
            .filter(|&(pistol, _)| showing(pistol));
        let from_droid = self
            .avatar
            .as_ref()
            .and_then(|avatar| avatar.pistol_nodes())
            .filter(|&(pistol, _)| showing(pistol));
        let muzzle = from_view
            .or(from_droid)
            .and_then(|(_, muzzle)| graph.try_get(muzzle).ok())
            .map(|muzzle| muzzle.global_position());
        if let Ok(lamp) = graph.try_get_mut(self.pistol_lamp) {
            if let Some(at) = muzzle {
                lamp.local_transform_mut().set_position(at);
            }
            if lamp.visibility() != muzzle.is_some() {
                lamp.set_visibility(muzzle.is_some());
            }
        }
    }

    /// Whether the player has the pistol out, or wants it out.
    pub fn armed(&self) -> bool {
        self.armed
    }

    /// Takes the pistol out, or puts it away.
    pub(super) fn toggle_pistol(&mut self) {
        self.armed = !self.armed;
    }

    /// Pulls the trigger: draws the pistol if it is away, and otherwise fires it - once it is
    /// all the way out.
    pub fn pull_trigger(&mut self) {
        if self.armed {
            self.keys.trigger = true;
        } else {
            self.armed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bolt_flies_stops_at_what_it_hits_and_lands_where_it_was_sent() {
        let mut graph = Graph::new();
        let mut bolts = Bolts::new(&mut graph);
        let ahead = Vector3::z();
        let flying = |bolts: &Bolts| bolts.bolts.iter().filter_map(|(_, b)| *b).collect::<Vec<_>>();
        // Nothing in the way: on it goes, at its speed.
        bolts.fire(&mut graph, Vector3::zeros(), ahead);
        assert!(bolts.fly(&mut graph, 0.1, |_, _, _, _| None).is_empty());
        let bolt = flying(&bolts)[0];
        assert!((bolt.position.z - BOLT_SPEED * 0.1).abs() < 1.0e-4);
        // Something 1 m further on: it stops there, and says what it hit, and where.
        let wall = Handle::<Collider>::new(7, 1);
        let struck = bolts.fly(&mut graph, 0.1, |_, _, _, _| Some((1.0, wall)));
        assert_eq!(struck.len(), 1);
        assert_eq!(struck[0].collider, wall);
        assert!((struck[0].at.z - (BOLT_SPEED * 0.1 + 1.0)).abs() < 1.0e-4);
        assert!(flying(&bolts)[0].landed.is_some());
        // It glows there a moment, and is gone.
        bolts.fly(&mut graph, IMPACT + 0.01, |_, _, _, _| None);
        assert!(flying(&bolts).is_empty());
        // Sent 3 m to land: it glows where it gets to, having hit nothing.
        bolts.fire_for(&mut graph, Vector3::zeros(), ahead, 3.0, true);
        // 3 m at its speed is 0.075 s; after 0.1 s it is there, glowing.
        for _ in 0..5 {
            assert!(bolts.fly(&mut graph, 0.02, |_, _, _, _| None).is_empty());
        }
        let bolt = flying(&bolts)[0];
        assert!(bolt.landed.is_some());
        assert!((bolt.position.z - (3.0 - 0.5 * BOLT_LENGTH)).abs() < 1.0e-3);
        // Sent 3 m not to land: gone when it gets there.
        bolts.clear(&mut graph);
        bolts.fire_for(&mut graph, Vector3::zeros(), ahead, 3.0, false);
        for _ in 0..10 {
            bolts.fly(&mut graph, 0.02, |_, _, _, _| None);
        }
        assert!(flying(&bolts).is_empty());
    }

    #[test]
    fn the_first_pull_draws_and_the_next_fires() {
        let mut player = Player::default();
        player.pull_trigger();
        assert!(player.armed && !player.keys.trigger, "drawn, not fired");
        player.pull_trigger();
        assert!(player.keys.trigger, "fired");
    }

    #[test]
    fn r_draws_and_holsters() {
        let mut player = Player::default();
        crate::player::press(&mut player, fyrox::keyboard::KeyCode::KeyR);
        assert!(player.armed);
        crate::player::press(&mut player, fyrox::keyboard::KeyCode::KeyR);
        assert!(!player.armed);
    }

    #[test]
    fn drawn_it_strafes_and_a_sprint_only_runs() {
        use crate::player::posture::Gait;
        let mut player = Player::default();
        crate::player::hold_shift(&mut player);
        player.pull_trigger();
        assert_eq!(player.gait(), Gait::Running);
        assert!(player.strafing());
    }
}
