//! The alarm: a klaxon, three rising whoops, sounded from where it is raised - by a droid the
//! player has provoked into it, or by a computer whose hack has failed - and heard through the
//! maze. Made from the formants in [`ALARM_SOUNDS`]; without them, alarms are silent.

use crate::formants::{self, Sounds};
use fyrox::{
    core::{algebra::Vector3, log::Log},
    scene::{
        base::BaseBuilder,
        graph::Graph,
        sound::{SoundBufferResource, SoundBuilder, Status},
        transform::TransformBuilder,
    },
};

/// Where the alarm's sound is.
pub const ALARM_SOUNDS: &str = "data/sounds/alarm_formants.json";

/// The alarm's sound, made to play, and how far off it is heard at full volume.
#[derive(Debug, Default, PartialEq)]
pub struct AlarmSound(Option<(SoundBufferResource, f32)>);

impl AlarmSound {
    /// Makes it from [`ALARM_SOUNDS`].
    pub fn make() -> Self {
        let sounds = Sounds::load(ALARM_SOUNDS)
            .inspect_err(|error| Log::err(format!("Alarm: {error}")))
            .ok();
        Self(sounds.and_then(|sounds| {
            let sound = sounds.sounds.get("alarm")?;
            Some((formants::buffer(sound, sounds.sample_rate)?, sound.reach))
        }))
    }

    /// Sounds it once, from `at`.
    pub fn sound(&self, graph: &mut Graph, at: Vector3<f32>) {
        let Some((buffer, reach)) = &self.0 else {
            return;
        };
        SoundBuilder::new(
            BaseBuilder::new()
                .with_local_transform(TransformBuilder::new().with_local_position(at).build()),
        )
        .with_buffer(Some(buffer.clone()))
        .with_radius(*reach)
        .with_play_once(true)
        .with_status(Status::Playing)
        .build(graph);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alarm_is_there_and_plays() {
        let sounds = Sounds::load(ALARM_SOUNDS).expect("the alarm's sound loads");
        let sound = &sounds.sounds["alarm"];
        let samples = formants::synth::make(sound, sounds.sample_rate);
        let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        assert!(samples.iter().all(|s| s.is_finite()));
        assert!((peak - sound.volume).abs() < 1.0e-3, "{peak}");
        assert!(sound.reach >= 30.0, "it carries through the maze");
    }
}
