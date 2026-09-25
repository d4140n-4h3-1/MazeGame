//! The pistol: R draws it and holsters it again; the left mouse button draws it too, and once it
//! is drawn fires it. Drawn, the droid holds it lower, at the ready, and strafes; the right mouse
//! button raises it to aim, and a shot from the ready raises it, fires as soon as it is up, and
//! lowers it again a moment later.
//!
//! A shot is a glowing green bolt that leaves the droid's muzzle as the droid fires, flies
//! straight the way the gun was pointing (see [`Avatar::shot`](super::avatar::Avatar::shot)) -
//! wherever the camera is looking - and stops at the first thing it hits. It carries the
//! muzzle's green flash with it, lighting up the droid as it leaves and everything it passes in
//! the dark. It does no harm to anything yet.
//!
//! The bolts are a handful of meshes made once, with the player, and shown while they fly:
//! adding or taking away a mesh has the traced shadows gather every mesh in the scene again.
//!
//! A shot sounds at the muzzle, and each bolt hums as it flies, till it stops. The sounds are
//! made from the formants in [`PISTOL_SOUNDS`] when the player is: see [`crate::formants`].

use super::Player;
use crate::formants::{synth, Sound, Sounds};
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
        graph::Graph,
        light::{point::PointLightBuilder, BaseLightBuilder},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            MeshBuilder,
        },
        node::Node,
        sound::{DataSource, Sound as SoundNode, SoundBuffer, SoundBufferResource, SoundBuilder, Status},
        transform::TransformBuilder,
    },
};

/// The pistol's sounds, as formants: `shot` as a bolt leaves, and `hum` as it flies.
pub const PISTOL_SOUNDS: &str = "data/sounds/pistol_formants.json";

/// How many bolts can be in the air at once. Firing another takes the one that has flown
/// longest.
const BOLTS: usize = 8;
/// How fast a bolt flies, in meters per second.
const BOLT_SPEED: f32 = 40.0;
/// How far a bolt flies before it is gone, if it hits nothing, in meters.
const BOLT_RANGE: f32 = 100.0;
/// How long a bolt is, from end to end, and how thick, in meters.
const BOLT_LENGTH: f32 = 0.5;
const BOLT_WIDTH: f32 = 0.05;
/// The bolt's colour - the green of the ball at the muzzle - and how brightly it glows by itself.
const BOLT_COLOR: Color = Color::opaque(40, 255, 60);
const BOLT_GLOW: f32 = 10.0;
/// How far the flash a bolt carries lights, in meters, and how brightly.
const FLASH_REACH: f32 = 5.0;
const FLASH_BRIGHTNESS: f32 = 3.0;

/// A bolt in the air.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bolt {
    position: Vector3<f32>,
    /// Which way it flies, one meter long.
    direction: Vector3<f32>,
    /// How much further it can fly, in meters.
    range: f32,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Bolts {
    /// Every bolt's mesh, and the bolt if it is flying.
    bolts: Vec<(Handle<Node>, Option<Bolt>)>,
    /// Every bolt's hum, as `bolts` has them, if there is one to hum.
    hums: Vec<Handle<Node>>,
    /// The sound of a shot, and how far off it is heard at full volume, if there is one.
    shot: Option<(SoundBufferResource, f32)>,
}

/// `sound`, made at `sample_rate` into something to play.
fn sound_buffer(sound: &Sound, sample_rate: u32) -> Option<SoundBufferResource> {
    let samples = synth::make(sound, sample_rate);
    let data = DataSource::Raw {
        sample_rate: sample_rate as usize,
        channel_count: 1,
        samples,
    };
    SoundBuffer::raw_generic(data).ok().map(SoundBufferResource::new_embedded)
}

/// The sound called `name` in `sounds`, made to play, with how far off it is heard at full
/// volume; if it is there.
fn made(sounds: Option<&Sounds>, name: &str) -> Option<(SoundBufferResource, f32)> {
    let sounds = sounds?;
    let sound = sounds.sounds.get(name).or_else(|| {
        fyrox::core::log::Log::warn(format!("Pistol: {PISTOL_SOUNDS} has no {name}"));
        None
    })?;
    Some((sound_buffer(sound, sounds.sample_rate)?, sound.reach))
}

impl Bolts {
    /// Makes the bolts' meshes, out of sight until they are fired.
    pub(super) fn new(graph: &mut Graph) -> Self {
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
            Vector3::new(BOLT_COLOR.r, BOLT_COLOR.g, BOLT_COLOR.b).cast::<f32>() / 255.0 * BOLT_GLOW,
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
        Self { bolts, hums, shot }
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
    fn fire(&mut self, graph: &mut Graph, from: Vector3<f32>, direction: Vector3<f32>) {
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
            range: BOLT_RANGE,
        });
        let node = &mut graph[*mesh];
        node.local_transform_mut()
            .set_position(from)
            .set_rotation(UnitQuaternion::face_towards(&direction, &Vector3::y()));
        node.set_visibility(true);
        self.hum(graph, index, true);
        // The shot sounds where it leaves, once, and is gone.
        if let Some((buffer, reach)) = &self.shot {
            SoundBuilder::new(BaseBuilder::new().with_local_transform(
                TransformBuilder::new().with_local_position(from).build(),
            ))
            .with_buffer(Some(buffer.clone()))
            .with_radius(*reach)
            .with_play_once(true)
            .with_status(Status::Playing)
            .build(graph);
        }
    }

    /// Puts every bolt out of sight, and out of the air.
    pub(super) fn clear(&mut self, graph: &mut Graph) {
        for index in 0..self.bolts.len() {
            let (mesh, bolt) = &mut self.bolts[index];
            if bolt.take().is_some() {
                graph[*mesh].set_visibility(false);
                self.hum(graph, index, false);
            }
        }
    }
}

impl Player {
    /// Fires a bolt from the muzzle the way the gun pointed, if the droid let a shot go this
    /// frame; and flies every bolt in the air on for another `dt`.
    pub(super) fn shoot(&mut self, graph: &mut Graph, dt: f32) {
        if let Some((muzzle, direction)) = self.avatar.as_ref().and_then(|avatar| avatar.shot()) {
            self.bolts.fire(graph, muzzle, direction);
        }
        // How far each bolt gets this frame before it hits something, if it does.
        let flights: Vec<Option<(f32, bool)>> = self
            .bolts
            .bolts
            .iter()
            .map(|(_, bolt)| {
                let bolt = (*bolt)?;
                let step = (BOLT_SPEED * dt).min(bolt.range);
                let reach = self.distance_to_hit(graph, bolt.position, bolt.direction, step);
                Some((reach, reach < step || step >= bolt.range))
            })
            .collect();
        for (index, flight) in flights.into_iter().enumerate() {
            let (mesh, bolt) = &mut self.bolts.bolts[index];
            let (Some(flying), Some((reach, stopped))) = (bolt.as_mut(), flight) else {
                continue;
            };
            if stopped {
                *bolt = None;
                graph[*mesh].set_visibility(false);
                self.bolts.hum(graph, index, false);
                continue;
            }
            flying.position += flying.direction * reach;
            flying.range -= reach;
            graph[*mesh]
                .local_transform_mut()
                .set_position(flying.position);
        }
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
        player.on_key(fyrox::keyboard::KeyCode::ShiftLeft, true);
        player.pull_trigger();
        assert_eq!(player.gait(), Gait::Running);
        assert!(player.strafing());
    }
}
