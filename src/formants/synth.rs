//! Makes a [`Sound`] into samples: its source - a buzz and noise - run through each of its
//! formants side by side, and the lot shaped by its loudness.
//!
//! The buzz is a sawtooth, rich in every harmonic for the formants to pick out, smoothed where
//! it jumps so that it does not alias into a whine at high pitches. Each formant is a band-pass
//! filter, retuned every few samples as its frequency glides. The noise is the same every time,
//! so a sound comes out the same every time it is made.

use super::Sound;
use std::f32::consts::TAU;

/// How many samples go by between retunings of the formants as they glide.
const RETUNE_EVERY: usize = 32;
/// How long the end of a looped sound is blended into its start, in seconds.
const LOOP_BLEND: f32 = 0.05;
/// How long a sound that does not loop takes to fade out at its very end, in seconds, so that
/// it never stops with a click.
const TAIL: f32 = 0.005;

/// A band-pass filter: a resonance at a frequency, as wide as it is told, as loud at its middle
/// as what goes in.
#[derive(Debug, Default, Clone, Copy)]
struct Resonance {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    /// What went in, and came out, the last two samples.
    x: [f32; 2],
    y: [f32; 2],
}

impl Resonance {
    /// Retunes it to `frequency`, `width` wide, both in Hz, at `sample_rate`.
    fn tune(&mut self, frequency: f32, width: f32, sample_rate: f32) {
        let nyquist = sample_rate / 2.0;
        let frequency = frequency.clamp(20.0, nyquist * 0.95);
        let w0 = TAU * frequency / sample_rate;
        let q = frequency / width.max(1.0);
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b2 = -alpha / a0;
        self.a1 = -2.0 * w0.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b2 * self.x[1] - self.a1 * self.y[0] - self.a2 * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// Smooths the jump of a sawtooth at `phase`, going up `step` a sample, so that it does not
/// alias.
fn smoothed_jump(phase: f32, step: f32) -> f32 {
    if phase < step {
        let t = phase / step;
        t + t - t * t - 1.0
    } else if phase > 1.0 - step {
        let t = (phase - 1.0) / step;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Noise from -1 to 1, the same every time for the same `seed`.
fn noise(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// `sound` as samples, one channel, at `sample_rate` a second: as loud as its `volume` at its
/// loudest, and - looped - with its end blended into its start.
pub fn make(sound: &Sound, sample_rate: u32) -> Vec<f32> {
    let rate = sample_rate as f32;
    let blend = if sound.looped { LOOP_BLEND } else { 0.0 };
    let length = (sound.length * rate).round() as usize;
    let extra = (blend * rate).round() as usize;
    let mut resonances = vec![Resonance::default(); sound.formants.len()];
    let (mut phase, mut seed) = (0.0_f32, 0x9e37_79b9_u32);
    let mut samples: Vec<f32> = (0..length + extra)
        .map(|i| {
            // Past the end of a looped sound, round again from its start.
            let time = (i % length.max(1)) as f32 / rate;
            if i % RETUNE_EVERY == 0 {
                for (resonance, formant) in resonances.iter_mut().zip(&sound.formants) {
                    resonance.tune(formant.frequency.at(time), formant.width, rate);
                }
            }
            let step = (sound.pitch.at(time) / rate).clamp(0.0, 0.5);
            phase = (phase + step).fract();
            let buzz = 2.0 * phase - 1.0 - smoothed_jump(phase, step);
            let source = buzz * sound.buzz.at(time) + noise(&mut seed) * sound.noise.at(time);
            let shaped: f32 = resonances
                .iter_mut()
                .zip(&sound.formants)
                .map(|(resonance, formant)| resonance.run(source) * formant.gain)
                .sum();
            shaped * sound.loudness.at(time)
        })
        .collect();
    if sound.looped {
        // The run past the end fades into the start, so that going round is seamless.
        for i in 0..extra.min(length) {
            let t = i as f32 / extra as f32;
            samples[i] = samples[i] * t + samples[length + i] * (1.0 - t);
        }
        samples.truncate(length);
    } else {
        let tail = ((TAIL * rate) as usize).min(samples.len());
        let start = samples.len() - tail;
        for (i, sample) in samples[start..].iter_mut().enumerate() {
            *sample *= 1.0 - i as f32 / (tail.max(2) - 1) as f32;
        }
    }
    let peak = samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
    if peak > 0.0 {
        let scale = sound.volume.clamp(0.0, 1.0) / peak;
        samples.iter_mut().for_each(|s| *s *= scale);
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formants::{Curve, Formant, Sounds};

    /// How strong `frequency` is in `samples`, at `rate`.
    fn strength(samples: &[f32], frequency: f32, rate: f32) -> f32 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, s) in samples.iter().enumerate() {
            let angle = TAU * frequency * i as f32 / rate;
            re += s * angle.cos();
            im += s * angle.sin();
        }
        (re * re + im * im).sqrt() / samples.len() as f32
    }

    fn flat(value: f32) -> Curve {
        Curve(vec![[0.0, value]])
    }

    fn noise_through(frequency: f32) -> Sound {
        Sound {
            length: 0.5,
            looped: false,
            pitch: flat(100.0),
            buzz: flat(0.0),
            noise: flat(1.0),
            loudness: flat(1.0),
            formants: vec![Formant { frequency: flat(frequency), width: 80.0, gain: 1.0 }],
            volume: 1.0,
            reach: 1.0,
        }
    }

    #[test]
    fn a_formant_lets_its_own_frequency_through_and_not_others() {
        let samples = make(&noise_through(1000.0), 44100);
        let (at, off) = (strength(&samples, 1000.0, 44100.0), strength(&samples, 3000.0, 44100.0));
        assert!(at > 5.0 * off, "{at} at the formant, {off} well away from it");
    }

    #[test]
    fn it_is_as_long_and_as_loud_as_it_says_and_the_same_every_time() {
        let sound = noise_through(800.0);
        let samples = make(&sound, 22050);
        assert_eq!(samples.len(), (0.5 * 22050.0) as usize);
        let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        assert!((peak - 1.0).abs() < 1e-5, "{peak}");
        assert_eq!(*samples.last().unwrap(), 0.0, "faded out at the very end");
        assert_eq!(samples, make(&sound, 22050));
    }

    #[test]
    fn a_buzz_sounds_at_its_pitch() {
        let sound = Sound {
            buzz: flat(1.0),
            noise: flat(0.0),
            pitch: flat(200.0),
            formants: vec![Formant { frequency: flat(400.0), width: 600.0, gain: 1.0 }],
            ..noise_through(0.0)
        };
        let samples = make(&sound, 44100);
        let (on, between) = (strength(&samples, 400.0, 44100.0), strength(&samples, 500.0, 44100.0));
        assert!(on > 5.0 * between, "{on} on a harmonic, {between} between two");
    }

    #[test]
    fn the_pistols_sounds_are_made_and_the_hum_goes_round_seamlessly() {
        let sounds = Sounds::load("data/sounds/pistol_formants.json").unwrap();
        for (name, sound) in &sounds.sounds {
            let samples = make(sound, sounds.sample_rate);
            let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
            assert!(samples.iter().all(|s| s.is_finite()), "{name}");
            assert!((peak - sound.volume).abs() < 1e-4, "{name}: {peak}");
        }
        let hum = make(&sounds.sounds["hum"], sounds.sample_rate);
        let (last, first) = (hum[hum.len() - 1], hum[0]);
        let typical = hum.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!((first - last).abs() <= typical, "the seam jumps {} ", (first - last).abs());
    }
}
