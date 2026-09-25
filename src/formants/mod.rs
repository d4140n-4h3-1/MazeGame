//! Sounds made from formants, as a voice is: a source - a buzz at a pitch, and noise - shaped by
//! a few resonances, the formants, which give it its colour, the way the mouth gives a vowel its
//! own. Each sound is described in a file of its own kind (see `data/sounds/pistol_formants.json`,
//! whose `about` says what everything in it means), and made into samples here when the game
//! starts; [`synth`] does the making.
//!
//! Everything that changes over a sound - the pitch, how much buzz and noise there is, how loud
//! it is, where each formant sits - is a [`Curve`]: points in time, joined by straight lines.

pub mod synth;

use serde::Deserialize;
use std::collections::HashMap;

/// A value over time: `[seconds, value]` points, joined by straight lines, and held flat before
/// the first and after the last. With no points at all it is 0.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Curve(pub Vec<[f32; 2]>);

impl Curve {
    /// The value `time` seconds in.
    pub fn at(&self, time: f32) -> f32 {
        let points = &self.0;
        let Some(first) = points.first() else {
            return 0.0;
        };
        if time <= first[0] {
            return first[1];
        }
        for pair in points.windows(2) {
            let ([t0, v0], [t1, v1]) = (pair[0], pair[1]);
            if time <= t1 {
                let span = t1 - t0;
                return if span > 0.0 {
                    v0 + (v1 - v0) * (time - t0) / span
                } else {
                    v1
                };
            }
        }
        points.last().map_or(0.0, |last| last[1])
    }
}

/// A resonance shaping a sound: where it sits, in Hz, how wide it is, in Hz, and how loud.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Formant {
    pub frequency: Curve,
    pub width: f32,
    pub gain: f32,
}

/// One sound, as the file has it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Sound {
    /// How long it is, in seconds.
    pub length: f32,
    /// Whether it plays over and over, its end blended into its start.
    #[serde(default)]
    pub looped: bool,
    /// The buzz's pitch, in Hz, and how much of the buzz and of the noise there is.
    pub pitch: Curve,
    pub buzz: Curve,
    pub noise: Curve,
    /// How loud the whole sound is over its length, before `volume`.
    pub loudness: Curve,
    pub formants: Vec<Formant>,
    /// How loud its loudest moment is, from 0 to 1.
    pub volume: f32,
    /// How far off it is heard at full volume, in meters, before it starts to fade.
    pub reach: f32,
}

/// A file of sounds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Sounds {
    /// Samples a second to make them at.
    pub sample_rate: u32,
    pub sounds: HashMap<String, Sound>,
}

impl Sounds {
    /// The sounds in the file at `path`.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_joins_its_points_and_holds_at_the_ends() {
        let curve = Curve(vec![[0.1, 10.0], [0.3, 30.0], [0.3, 50.0], [0.5, 0.0]]);
        assert_eq!(curve.at(0.0), 10.0, "held before the first");
        assert!((curve.at(0.2) - 20.0).abs() < 1e-4, "halfway");
        assert_eq!(curve.at(0.3), 30.0, "a jump takes the first of the two");
        assert!((curve.at(0.4) - 25.0).abs() < 1e-4, "after the jump");
        assert_eq!(curve.at(9.0), 0.0, "held after the last");
        assert_eq!(Curve::default().at(1.0), 0.0, "nothing is silence");
    }

    #[test]
    fn the_pistols_sounds_load() {
        let sounds = Sounds::load("data/sounds/pistol_formants.json").unwrap();
        for name in ["shot", "hum"] {
            let sound = &sounds.sounds[name];
            assert!(sound.length > 0.0 && !sound.formants.is_empty(), "{name}");
        }
        assert!(sounds.sounds["hum"].looped && !sounds.sounds["shot"].looped);
    }
}
