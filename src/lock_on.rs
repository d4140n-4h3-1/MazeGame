//! The warning a sentry gives as it locks on to the player (see [`crate::inhabitants::LOCK_ON`]):
//! its weapon charging, from where it stands, for as long as it keeps them in sight. Losing sight
//! of them, it stops at once, so the player hears when they have got away. Made from the
//! formants in [`SENTRY_SOUNDS`]; without it, sentries lock on in silence.

use crate::formants::{self, Sounds};
use fyrox::{
    core::{algebra::Vector3, log::Log, pool::Handle},
    graph::SceneGraph,
    scene::{
        base::BaseBuilder,
        graph::Graph,
        node::Node,
        sound::{SoundBufferResource, SoundBuilder, Status},
        transform::TransformBuilder,
    },
};

/// Where the sentries' sounds are.
pub const SENTRY_SOUNDS: &str = "data/sounds/sentry_formants.json";

/// The charging sound, and those playing it: which droid, as an index, and the sound's node.
#[derive(Debug, Default, PartialEq)]
pub struct LockOn {
    sound: Option<(SoundBufferResource, f32)>,
    playing: Vec<(usize, Handle<Node>)>,
}

impl LockOn {
    /// Makes the sound from [`SENTRY_SOUNDS`].
    pub fn make() -> Self {
        let sound = Sounds::load(SENTRY_SOUNDS)
            .inspect_err(|error| Log::err(format!("Sentries: {error}")))
            .ok()
            .and_then(|sounds| {
                let sound = sounds.sounds.get("lock_on")?;
                Some((formants::buffer(sound, sounds.sample_rate)?, sound.reach))
            });
        Self {
            sound,
            playing: Vec::new(),
        }
    }

    /// Has the sound play from each of `locking` - a droid, as an index, locking on, and where
    /// its face is - that is not playing it already, following it about; and stops it for any
    /// no longer locking on.
    pub fn update(&mut self, graph: &mut Graph, locking: &[(usize, Vector3<f32>)]) {
        self.playing.retain(|&(n, node)| {
            let still = locking.iter().any(|&(m, _)| m == n);
            if !still && graph.is_valid_handle(node) {
                graph.remove_node(node);
            }
            still
        });
        for &(n, face) in locking {
            match self.playing.iter().find(|&&(m, _)| m == n) {
                Some(&(_, node)) => {
                    if let Ok(node) = graph.try_get_mut(node) {
                        node.local_transform_mut().set_position(face);
                    }
                }
                None => {
                    let Some((buffer, reach)) = &self.sound else {
                        continue;
                    };
                    let node = SoundBuilder::new(BaseBuilder::new().with_local_transform(
                        TransformBuilder::new().with_local_position(face).build(),
                    ))
                    .with_buffer(Some(buffer.clone()))
                    .with_radius(*reach)
                    .with_play_once(true)
                    .with_status(Status::Playing)
                    .build(graph)
                    .to_base();
                    self.playing.push((n, node));
                }
            }
        }
    }

    /// Stops every one of them, as the droids are cleared away.
    pub fn clear(&mut self, graph: &mut Graph) {
        self.update(graph, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inhabitants::LOCK_ON;

    #[test]
    fn the_charge_lasts_as_long_as_the_lock() {
        let sounds = Sounds::load(SENTRY_SOUNDS).expect("the sentries' sounds load");
        let sound = &sounds.sounds["lock_on"];
        assert!((sound.length - LOCK_ON).abs() < 1.0e-3, "{}", sound.length);
        let samples = crate::formants::synth::make(sound, sounds.sample_rate);
        assert!(samples.iter().all(|s| s.is_finite()));
        let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        assert!((peak - sound.volume).abs() < 1.0e-3, "{peak}");
    }
}
