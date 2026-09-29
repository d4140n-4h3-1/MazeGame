//! Machine speech: System Latin as a drone says it, which no human would hear as speech at all.
//!
//! Each letter is a sound of its own, as the letter would be spoken, but made by a machine: a
//! vowel is a beep on a note of its own, `m`, `n`, `l` and `r` a low hum, a hissing letter - `s`,
//! `f`, `v`, `z`, `h` - a rasping buzz, and a stop - `p`, `t`, `k` and the rest - a click. Words
//! are a moment apart and sentences longer, and a question chirps up at its end. So the drone
//! speaks the language, word for word and at its pace, but what comes out is beeps, hums and
//! buzzes. What each sounds like is in `data/sounds/drone_voice.json`, whose `about` says what
//! everything in it means.

use super::{Curve, Formant, Sound};
use serde::Deserialize;
use std::collections::HashMap;

/// Where the drones' voice comes from.
pub const DRONE_VOICE: &str = "data/sounds/drone_voice.json";

/// A sound a group of letters is made as: its pitch and where its formant sits, in Hz, how long
/// it lasts, in seconds, and how much noise goes with its buzz, from 0.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Tone {
    pub pitch: f32,
    pub formant: f32,
    pub length: f32,
    #[serde(default)]
    pub noise: f32,
}

/// How long the gaps are, in seconds: between letters, words and sentences.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Gaps {
    pub letter: f32,
    pub word: f32,
    pub stop: f32,
}

/// The drones' voice, as the file has it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Chirps {
    pub sample_rate: u32,
    /// Each vowel's note, in Hz, and how long a beep lasts.
    pub notes: HashMap<String, f32>,
    pub beep: f32,
    pub hum: Tone,
    pub buzz: Tone,
    pub click: Tone,
    pub gaps: Gaps,
    /// How far a question's last chirp rises, as a part of its note.
    pub rise: f32,
    pub volume: f32,
    /// How far off it is heard at full volume, in meters.
    pub reach: f32,
}

/// How narrow the formant a beep or a hum rings on is, in Hz, and how wide the one a buzz or a
/// click rasps through.
const NARROW: f32 = 60.0;
const WIDE: f32 = 900.0;
/// How long each sound takes to come up and die away, in seconds, so that none starts or stops
/// with a click of its own.
const EDGE: f32 = 0.004;

/// What a letter is made as.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Chirp {
    Beep(f32),
    Hum,
    Buzz,
    Click,
    Word,
    Stop { question: bool },
}

/// `c` in lower case and without its macron.
fn plain(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'ā' => 'a',
        'ē' => 'e',
        'ī' => 'i',
        'ō' => 'o',
        'ū' => 'u',
        'ȳ' => 'y',
        c => c,
    }
}

impl Chirps {
    /// The voice in the file at `path`.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = crate::platform::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))
    }

    /// `text` as the chirps it is said as.
    fn spell(&self, text: &str) -> Vec<Chirp> {
        let mut chirps = Vec::new();
        for c in text.chars().map(plain) {
            let chirp = match c {
                'j' => self.notes.get("i").map(|&note| Chirp::Beep(note)),
                'w' => self.notes.get("u").map(|&note| Chirp::Beep(note)),
                'm' | 'n' | 'l' | 'r' => Some(Chirp::Hum),
                's' | 'f' | 'v' | 'z' | 'h' | 'x' => Some(Chirp::Buzz),
                'p' | 't' | 'k' | 'b' | 'd' | 'g' | 'c' | 'q' => Some(Chirp::Click),
                '.' | '!' => Some(Chirp::Stop { question: false }),
                '?' => Some(Chirp::Stop { question: true }),
                ',' | ':' | ';' => Some(Chirp::Word),
                c if c.is_whitespace() => Some(Chirp::Word),
                c => self.notes.get(c.to_string().as_str()).map(|&note| Chirp::Beep(note)),
            };
            if let Some(chirp) = chirp {
                // One gap where several would run together.
                let gap = |chirp: &Chirp| matches!(chirp, Chirp::Word | Chirp::Stop { .. });
                match chirps.last_mut() {
                    Some(last) if gap(last) && gap(&chirp) => {
                        if matches!(chirp, Chirp::Stop { .. }) {
                            *last = chirp;
                        }
                    }
                    None if gap(&chirp) => (),
                    _ => chirps.push(chirp),
                }
            }
        }
        chirps
    }

    /// `text` said by a drone, every note `pitch` times its own, as a sound to make.
    pub fn say(&self, text: &str, pitch: f32) -> Sound {
        let (mut pitches, mut buzzes, mut noises, mut loudness) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut rings, mut rasps) = (Vec::new(), Vec::new());
        let mut time = 0.0_f32;
        let mut last_note = self.notes.values().copied().fold(0.0, f32::max) * pitch;
        let chirps = self.spell(text);
        for (n, chirp) in chirps.iter().enumerate() {
            let (tone_pitch, ring, rasp, buzz, noise, length) = match *chirp {
                Chirp::Beep(note) => {
                    let note = note * pitch;
                    last_note = note;
                    (note, note, note * 2.0, 1.0, 0.0, self.beep)
                }
                Chirp::Hum => {
                    let hum = &self.hum;
                    (hum.pitch * pitch, hum.formant * pitch, hum.formant * 2.0, 1.0, hum.noise, hum.length)
                }
                Chirp::Buzz => {
                    let buzz = &self.buzz;
                    (buzz.pitch * pitch, buzz.pitch * pitch, buzz.formant, 1.0, buzz.noise, buzz.length)
                }
                Chirp::Click => {
                    let click = &self.click;
                    (click.pitch * pitch, click.formant, click.formant, 0.0, 1.0, click.length)
                }
                Chirp::Word => {
                    time += self.gaps.word;
                    continue;
                }
                Chirp::Stop { question } => {
                    // A question chirps up once more at its end, from its last note.
                    if question {
                        let (from, to) = (last_note, last_note * (1.0 + self.rise));
                        let start = time + self.gaps.letter;
                        for (at, note) in [(start, from), (start + self.beep * 1.5, to)] {
                            pitches.push([at, note]);
                            rings.push([at, note]);
                            rasps.push([at, note * 2.0]);
                        }
                        buzzes.push([start, 1.0]);
                        noises.push([start, 0.0]);
                        loudness.extend([
                            [start, 0.0],
                            [start + EDGE, 1.0],
                            [start + self.beep * 1.5 - EDGE, 1.0],
                            [start + self.beep * 1.5, 0.0],
                        ]);
                        time = start + self.beep * 1.5;
                    }
                    if n + 1 < chirps.len() {
                        time += self.gaps.stop;
                    }
                    continue;
                }
            };
            let start = time + self.gaps.letter;
            let end = start + length;
            // Held where it is for its length: each curve steps to it as it starts.
            for at in [start, end] {
                pitches.push([at, tone_pitch]);
                rings.push([at, ring]);
                rasps.push([at, rasp]);
                buzzes.push([at, buzz]);
                noises.push([at, noise]);
            }
            loudness.extend([[start, 0.0], [start + EDGE, 1.0], [end - EDGE, 1.0], [end, 0.0]]);
            time = end;
        }
        Sound {
            length: time + self.gaps.letter,
            looped: false,
            pitch: Curve(pitches),
            buzz: Curve(buzzes),
            noise: Curve(noises),
            loudness: Curve(loudness),
            formants: vec![
                Formant {
                    frequency: Curve(rings),
                    width: NARROW,
                    gain: 1.0,
                },
                Formant {
                    frequency: Curve(rasps),
                    width: WIDE,
                    gain: 0.35,
                },
            ],
            volume: self.volume,
            reach: self.reach,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chirps() -> Chirps {
        Chirps::load(DRONE_VOICE).expect("the drones' voice")
    }

    #[test]
    fn each_letter_is_a_beep_a_hum_a_buzz_or_a_click() {
        let chirps = chirps();
        let spelt = chirps.spell("Sta, mo?");
        let a = chirps.notes["a"];
        let o = chirps.notes["o"];
        assert_eq!(
            spelt,
            [
                Chirp::Buzz,
                Chirp::Click,
                Chirp::Beep(a),
                Chirp::Word,
                Chirp::Hum,
                Chirp::Beep(o),
                Chirp::Stop { question: true },
            ]
        );
    }

    #[test]
    fn every_vowel_has_a_note_of_its_own() {
        let chirps = chirps();
        let mut notes: Vec<f32> = "aeiouy".chars().map(|v| chirps.notes[&v.to_string()]).collect();
        notes.sort_by(f32::total_cmp);
        notes.dedup();
        assert_eq!(notes.len(), 6);
    }

    #[test]
    fn a_line_is_said_as_long_as_its_letters_and_gaps_and_is_heard() {
        let chirps = chirps();
        let short = chirps.say("Sta.", 1.0);
        let long = chirps.say("Intrusor detectum. Sta.", 1.0);
        assert!(long.length > short.length * 3.0, "{} against {}", long.length, short.length);
        let samples = super::super::synth::make(&short, chirps.sample_rate);
        assert!(samples.iter().any(|s| s.abs() > 0.1), "silent");
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        // Higher all through for a higher pitch.
        let high = chirps.say("Sta.", 1.5);
        assert!(high.pitch.at(short.length * 0.6) > short.pitch.at(short.length * 0.6));
    }
}
