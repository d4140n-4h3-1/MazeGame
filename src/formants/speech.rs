//! Speech: System Latin read aloud by a voice made from formants, as the maze's droids speak it.
//!
//! Latin is written as it sounds, so reading it takes only a few rules: `c`, `k` and `q` are all
//! a k, `qu` is a k with a w after it, `x` is a k and an s, `ph` is an f, `th` and `ch` are a t
//! and a k, an `i` starting a word before a vowel is a y, and a letter doubled is held longer.
//! Each sound - see `data/sounds/voice_formants.json`, whose `about` says what everything in it
//! means - has where its three formants sit, how long it lasts, and how much buzz and noise it
//! is made of; a stop, such as a p or a t, is a moment's silence and then a burst of noise. The
//! formants glide from one sound to the next as a mouth does.
//!
//! The voice speaks each word a moment apart and each vowel on a note of its own, stepping up and
//! down about a line that falls over the sentence - the way a machine would read - and falls
//! further at a full stop, or rises at a question. Each kind of droid has a voice of its own: its
//! pitch, how quickly it speaks, how big it sounds, how much its notes step about, and how
//! breathy it is. How a droid feels raises or lowers its pitch: see [`Voices::mood_pitch`].

use super::{Curve, Formant, Sound};
use crate::dialogue::Mood;
use serde::Deserialize;
use std::collections::HashMap;

/// Where each droid's voice comes from.
pub const VOICES: &str = "data/sounds/voice_formants.json";

/// One sound of speech.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Phone {
    /// Where its three formants sit, in Hz, for a voice of size 1.
    pub formants: [f32; 3],
    /// How long it lasts, in seconds, at a speed of 1.
    pub length: f32,
    #[serde(default)]
    pub buzz: f32,
    #[serde(default)]
    pub noise: f32,
    pub loudness: f32,
    /// Whether it is a stop: silence, then a burst of its noise.
    #[serde(default)]
    pub stop: bool,
}

/// How someone speaks.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Voice {
    /// Its pitch, in Hz, in the middle of a sentence.
    pub pitch: f32,
    /// How quickly it speaks: 2 is twice as fast.
    pub speed: f32,
    /// How big it sounds: every formant is this many times as high, so under 1 is bigger.
    pub size: f32,
    /// How far its notes step up and down, and how far a sentence falls over its length, as
    /// parts of its pitch.
    pub lilt: f32,
    pub fall: f32,
    /// How far a question rises at its end, as a part of its pitch.
    pub rise: f32,
    /// How much noise goes along with the buzz, from 0.
    pub breath: f32,
    pub volume: f32,
    /// How far off it is heard at full volume, in meters.
    pub reach: f32,
}

/// How long the gaps are, in seconds, at a speed of 1.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Pauses {
    pub word: f32,
    pub comma: f32,
    pub stop: f32,
}

/// The file of voices: the sounds of speech, and everyone's voice.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Voices {
    pub sample_rate: u32,
    /// How wide each of the three formants is, in Hz, and how loud.
    pub widths: [f32; 3],
    pub gains: [f32; 3],
    pub pauses: Pauses,
    pub phones: HashMap<String, Phone>,
    pub voices: HashMap<String, Voice>,
    /// How many times its usual pitch a voice speaks at in each mood; 1 for any not given.
    #[serde(default)]
    pub moods: HashMap<Mood, f32>,
}

/// What a piece of text is to be spoken as.
#[derive(Debug, Clone, PartialEq)]
enum Unit {
    /// A sound, held this many times as long as usual.
    Phone(&'static str, f32),
    /// The gap between two words.
    Word,
    Comma,
    /// The end of a sentence, and whether it asks something.
    End { question: bool },
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

/// `c` without its macron, and in lower case.
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

/// `text` as the sounds and gaps it is spoken as.
fn spell(text: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    let push = |units: &mut Vec<Unit>, phone: &'static str| {
        // A doubled letter is one sound, held longer.
        if let Some(Unit::Phone(last, hold)) = units.last_mut() {
            if *last == phone && !is_vowel(phone.chars().next().unwrap_or(' ')) {
                *hold = 1.6;
                return;
            }
        }
        units.push(Unit::Phone(phone, 1.0));
    };
    let chars: Vec<char> = text.chars().map(plain).collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or(' ');
        let word_start = i == 0 || !chars[i - 1].is_alphabetic();
        match c {
            'q' => {
                push(&mut units, "k");
                if next == 'u' {
                    push(&mut units, "w");
                    i += 1;
                }
            }
            'c' | 'k' => {
                push(&mut units, "k");
                if next == 'h' {
                    i += 1;
                }
            }
            'x' => {
                push(&mut units, "k");
                push(&mut units, "s");
            }
            'p' if next == 'h' => {
                push(&mut units, "f");
                i += 1;
            }
            't' if next == 'h' => {
                push(&mut units, "t");
                i += 1;
            }
            'i' if word_start && is_vowel(next) => push(&mut units, "j"),
            'a' => push(&mut units, "a"),
            'b' => push(&mut units, "b"),
            'd' => push(&mut units, "d"),
            'e' => push(&mut units, "e"),
            'f' => push(&mut units, "f"),
            'g' => push(&mut units, "g"),
            'h' => push(&mut units, "h"),
            'i' => push(&mut units, "i"),
            'j' => push(&mut units, "j"),
            'l' => push(&mut units, "l"),
            'm' => push(&mut units, "m"),
            'n' => push(&mut units, "n"),
            'o' => push(&mut units, "o"),
            'p' => push(&mut units, "p"),
            'r' => push(&mut units, "r"),
            's' => push(&mut units, "s"),
            't' => push(&mut units, "t"),
            'u' => push(&mut units, "u"),
            'v' => push(&mut units, "v"),
            'w' => push(&mut units, "w"),
            'y' => push(&mut units, "y"),
            'z' => push(&mut units, "z"),
            ',' | ':' | ';' => units.push(Unit::Comma),
            '.' | '!' => units.push(Unit::End { question: false }),
            '?' => units.push(Unit::End { question: true }),
            _ if c.is_whitespace() => {
                if matches!(units.last(), Some(Unit::Phone(..))) {
                    units.push(Unit::Word);
                }
            }
            _ => (),
        }
        i += 1;
    }
    if matches!(units.last(), Some(Unit::Phone(..) | Unit::Word)) {
        units.push(Unit::End { question: false });
    }
    units
}

/// A moment in the speech: where each curve is at a time.
#[derive(Debug, Clone, Copy)]
struct Key {
    time: f32,
    formants: [f32; 3],
    buzz: f32,
    noise: f32,
    loudness: f32,
    /// The vowel it belongs to, counted from the start, if it belongs to one; and the sentence
    /// it is in.
    vowel: Option<usize>,
    sentence: usize,
}

impl Voices {
    /// The voices in the file at `path`.
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))
    }

    /// The voice called `name`, or failing that the one called `default`.
    pub fn voice(&self, name: &str) -> Option<&Voice> {
        self.voices.get(name).or_else(|| self.voices.get("default"))
    }

    /// How many times its usual pitch a voice speaks at in `mood`.
    pub fn mood_pitch(&self, mood: Mood) -> f32 {
        self.moods.get(&mood).copied().unwrap_or(1.0)
    }

    /// `text` spoken by `voice`, its pitch `pitch` times its own, as a sound to make.
    pub fn speak(&self, text: &str, voice: &Voice, pitch: f32) -> Sound {
        let speed = voice.speed.max(0.1);
        let mut keys: Vec<Key> = Vec::new();
        let (mut time, mut vowels, mut sentence) = (0.0_f32, 0, 0);
        // A gap: silent, the formants held where they were.
        let silence = |keys: &mut Vec<Key>, time: &mut f32, length: f32, sentence: usize| {
            let formants = keys.last().map_or([500.0, 1500.0, 2500.0], |key| key.formants);
            for at in [*time + 0.01, *time + length - 0.01] {
                keys.push(Key {
                    time: at,
                    formants,
                    buzz: 0.0,
                    noise: 0.0,
                    loudness: 0.0,
                    vowel: None,
                    sentence,
                });
            }
            *time += length;
        };
        for unit in spell(text) {
            let phone = match unit {
                Unit::Phone(name, hold) => self.phones.get(name).map(|phone| (name, phone, hold)),
                Unit::Word => {
                    silence(&mut keys, &mut time, self.pauses.word / speed, sentence);
                    None
                }
                Unit::Comma => {
                    silence(&mut keys, &mut time, self.pauses.comma / speed, sentence);
                    None
                }
                Unit::End { .. } => {
                    silence(&mut keys, &mut time, self.pauses.stop / speed, sentence);
                    sentence += 1;
                    None
                }
            };
            let Some((name, phone, hold)) = phone else {
                continue;
            };
            let length = (phone.length * hold / speed).max(if phone.stop { 0.05 } else { 0.02 });
            let formants = phone.formants.map(|f| f * voice.size);
            let vowel = is_vowel(name.chars().next().unwrap_or(' ')).then(|| {
                vowels += 1;
                vowels - 1
            });
            let voiced = phone.buzz > 0.0;
            let key = |time: f32, buzz: f32, noise: f32, loudness: f32| Key {
                time,
                formants,
                buzz,
                noise: noise + if voiced { voice.breath } else { 0.0 },
                loudness,
                vowel,
                sentence,
            };
            if phone.stop {
                // Closed, murmuring if it is voiced, then the burst as it opens.
                let murmur = if voiced { 0.08 } else { 0.0 };
                keys.push(key(time + 0.01, phone.buzz, 0.0, murmur));
                keys.push(key(time + length - 0.025, phone.buzz, 0.0, murmur));
                keys.push(key(time + length - 0.02, 0.0, phone.noise, phone.loudness));
                keys.push(key(time + length - 0.005, 0.0, phone.noise, phone.loudness * 0.5));
            } else {
                for at in [0.25, 0.75] {
                    keys.push(key(time + length * at, phone.buzz, phone.noise, phone.loudness));
                }
            }
            time += length;
        }

        let mut sound = Sound {
            length: time + 0.05,
            looped: false,
            pitch: Curve::default(),
            buzz: Curve::default(),
            noise: Curve::default(),
            loudness: Curve(vec![[0.0, 0.0]]),
            formants: (0..3)
                .map(|i| Formant {
                    frequency: Curve::default(),
                    width: self.widths[i] * voice.size,
                    gain: self.gains[i],
                })
                .collect(),
            volume: voice.volume,
            reach: voice.reach,
        };
        let questions: Vec<bool> = spell(text)
            .iter()
            .filter_map(|unit| match unit {
                Unit::End { question } => Some(*question),
                _ => None,
            })
            .collect();
        let spans: Vec<(f32, f32)> = (0..sentence.max(1))
            .map(|s| {
                // What is spoken of it, not the gap after.
                let times = keys
                    .iter()
                    .filter(|key| key.sentence == s && key.loudness > 0.0)
                    .map(|key| key.time);
                let start = times.clone().fold(f32::INFINITY, f32::min);
                (start, times.fold(start, f32::max))
            })
            .collect();
        // Each vowel on a note of its own: on a line falling over the sentence - further at its
        // end, or rising at a question - and stepped up and down about it, one vowel to the next.
        let note = |key: &Key, at: f32| {
            let (start, end) = spans.get(key.sentence).copied().unwrap_or((0.0, 1.0));
            let through = ((at - start) / (end - start).max(1.0e-3)).clamp(0.0, 1.0);
            let ending = ((through - 0.7) / 0.3).max(0.0);
            let question = questions.get(key.sentence).copied().unwrap_or(false);
            let line = 1.0 + voice.fall * (0.5 - through)
                + if question {
                    voice.rise * ending
                } else {
                    -voice.fall * ending
                };
            let step = match key.vowel {
                Some(n) => voice.lilt * if n % 2 == 0 { 0.5 } else { -0.5 },
                None => 0.0,
            };
            voice.pitch * pitch * (line + step)
        };
        let mut held = voice.pitch * pitch;
        for (i, key) in keys.iter().enumerate() {
            // A vowel holds one note from end to end, the one for where it ends; everything else
            // keeps the last one.
            if key.vowel.is_some() {
                let end = keys[i..]
                    .iter()
                    .take_while(|k| k.vowel == key.vowel)
                    .last()
                    .map_or(key.time, |k| k.time);
                held = note(key, end);
            }
            sound.pitch.0.push([key.time, held]);
            sound.buzz.0.push([key.time, key.buzz]);
            sound.noise.0.push([key.time, key.noise]);
            sound.loudness.0.push([key.time, key.loudness]);
            for (formant, &frequency) in sound.formants.iter_mut().zip(&key.formants) {
                formant.frequency.0.push([key.time, frequency]);
            }
        }
        sound.loudness.0.push([sound.length, 0.0]);
        sound
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formants::synth;

    fn voices() -> Voices {
        Voices::load(VOICES).unwrap()
    }

    fn phones(text: &str) -> Vec<&'static str> {
        spell(text)
            .into_iter()
            .filter_map(|unit| match unit {
                Unit::Phone(phone, _) => Some(phone),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn latin_is_read_as_it_is_written() {
        assert_eq!(phones("Quis"), ["k", "w", "i", "s"]);
        assert_eq!(phones("exitus"), ["e", "k", "s", "i", "t", "u", "s"]);
        assert_eq!(phones("Codex"), ["k", "o", "d", "e", "k", "s"]);
        assert_eq!(phones("iam"), ["j", "a", "m"]);
        assert_eq!(phones("patrōlium"), ["p", "a", "t", "r", "o", "l", "i", "u", "m"]);
        assert_eq!(phones("schema"), ["s", "k", "e", "m", "a"]);
    }

    #[test]
    fn a_doubled_letter_is_held_longer_and_sentences_end() {
        let units = spell("Accipatum");
        assert_eq!(units[1], Unit::Phone("k", 1.6));
        assert_eq!(units.last(), Some(&Unit::End { question: false }));
        assert_eq!(spell("Ubi?").last(), Some(&Unit::End { question: true }));
    }

    #[test]
    fn every_letter_has_a_sound_and_every_droid_a_voice() {
        let voices = voices();
        for phone in phones("abcdefghijklmnopqrstuvwxyz") {
            assert!(voices.phones.contains_key(phone), "{phone}");
        }
        let script = crate::dialogue::Script::load(crate::dialogue::SCRIPT).unwrap();
        for character in &script.characters {
            assert!(voices.voices.contains_key(&character.name), "{}", character.name);
        }
    }

    #[test]
    fn a_mood_raises_or_lowers_the_voice() {
        let voices = voices();
        assert_eq!(voices.mood_pitch(Mood::Normal), 1.0);
        assert!(voices.mood_pitch(Mood::Agitated) > 1.0, "agitated, higher");
        assert!(voices.mood_pitch(Mood::Hostile) < 1.0, "hostile, lower");
        let voice = voices.voice("default").unwrap();
        let first = |pitch: f32| voices.speak("Sta.", voice, pitch).pitch.0[0][1];
        let agitated = voices.mood_pitch(Mood::Agitated);
        assert!((first(agitated) - first(1.0) * agitated).abs() < 1.0e-3);
    }

    #[test]
    fn a_question_rises_and_a_statement_falls() {
        let voices = voices();
        let voice = voices.voice("default").unwrap();
        let pitch_at_end = |text: &str| {
            let sound = voices.speak(text, voice, 1.0);
            let last = sound.pitch.0.iter().rev().find(|p| p[1] > 0.0).unwrap()[1];
            (sound.pitch.0[0][1], last)
        };
        let (start, end) = pitch_at_end("Quis esas tu?");
        assert!(end > start, "{start} to {end}");
        let (start, end) = pitch_at_end("Hostis detectum.");
        assert!(end < start, "{start} to {end}");
    }

    #[test]
    fn a_line_is_spoken_in_about_the_time_it_should_take() {
        let voices = voices();
        for (name, voice) in &voices.voices {
            let sound = voices.speak("Sta. Eso defendator codex quattuor septem. Quis esas?", voice, 1.0);
            assert!(sound.length > 1.5 && sound.length < 8.0, "{name}: {}", sound.length);
            let samples = synth::make(&sound, voices.sample_rate);
            assert!(samples.iter().all(|s| s.is_finite()), "{name}");
            let times = sound.loudness.0.windows(2).all(|w| w[0][0] <= w[1][0]);
            assert!(times, "{name}: the curves go forward in time");
        }
    }
}
