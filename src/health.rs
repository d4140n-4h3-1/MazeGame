//! The player's health: a sentry's touch takes some of it rather than deleting them outright, and
//! only with none left are they deleted. It comes back two ways: fast, from a heart picked up in
//! the maze (see [`crate::hearts`]), or slowly, a little at a time, while they rest - standing
//! still a moment.
//!
//! Each is heard ([`HealthSounds`], from [`HEALTH_SOUNDS`]): a jolt as they are hit, a chime as
//! they pick up a heart, a soft swell as resting starts to heal them, and two notes once they
//! are whole again.

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

/// Where the health's sounds are.
pub const HEALTH_SOUNDS: &str = "data/sounds/health_formants.json";

/// How much of their health a sentry's touch takes, out of 1.
pub const HIT: f32 = 1.0 / 3.0;
/// How much a heart gives back, out of 1.
pub const HEART: f32 = 0.5;
/// How long the player has to stand still before health starts coming back, in seconds, and how
/// long it then takes to come all the way back.
pub const REST_BEFORE: f32 = 2.0;
pub const REST_TIME: f32 = 45.0;
/// How long, in seconds, the screen flashes red after a hit.
pub const FLASH: f32 = 0.5;

/// The player's health, and how long they have been resting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Health {
    /// From 1, full, down to 0.
    pub left: f32,
    rested: f32,
    /// How long is left of the red flash of the last hit, in seconds.
    pub flash: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            left: 1.0,
            rested: 0.0,
            flash: 0.0,
        }
    }
}

impl Health {
    /// Takes a hit's worth. Whether that was the last of it.
    pub fn hit(&mut self) -> bool {
        self.left = (self.left - HIT).max(0.0);
        self.rested = 0.0;
        self.flash = FLASH;
        self.left <= 1.0e-4
    }

    /// Whether there is room for a heart.
    pub fn hurt(&self) -> bool {
        self.left < 1.0
    }

    /// Takes a heart's worth back.
    pub fn heart(&mut self) {
        self.left = (self.left + HEART).min(1.0);
    }

    /// Takes a heart's worth back, and says whether that made it whole.
    pub fn heart_and_whole(&mut self) -> bool {
        self.heart();
        self.left >= 1.0
    }

    /// Another `dt` seconds on, `resting` or not: resting long enough, some comes back. Whether
    /// it started to just now, or it is whole again just now.
    pub fn update(&mut self, dt: f32, resting: bool) -> Option<Healing> {
        self.flash = (self.flash - dt).max(0.0);
        let before = self.rested;
        self.rested = if resting { self.rested + dt } else { 0.0 };
        if self.rested < REST_BEFORE || self.left <= 0.0 || self.left >= 1.0 {
            return None;
        }
        self.left = (self.left + dt / REST_TIME).min(1.0);
        match () {
            _ if self.left >= 1.0 => Some(Healing::Whole),
            _ if before < REST_BEFORE => Some(Healing::Started),
            _ => None,
        }
    }
}

/// What came of resting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Healing {
    /// It has started to bring health back.
    Started,
    /// Health is all back.
    Whole,
}

/// What each is heard as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    Hit,
    Heart,
    Healing,
    Whole,
}

/// A sound made to play, and how far off it is heard at full volume.
type Made = Option<(SoundBufferResource, f32)>;

/// The health's sounds.
#[derive(Debug, Default, PartialEq)]
pub struct HealthSounds {
    hit: Made,
    heart: Made,
    healing: Made,
    whole: Made,
}

impl HealthSounds {
    /// Makes them from [`HEALTH_SOUNDS`]; the player's health is silent without it.
    pub fn make() -> Self {
        let sounds = Sounds::load(HEALTH_SOUNDS)
            .inspect_err(|error| Log::err(format!("Health: {error}")))
            .ok();
        let made = |name: &str| {
            let sounds = sounds.as_ref()?;
            let sound = sounds.sounds.get(name)?;
            Some((formants::buffer(sound, sounds.sample_rate)?, sound.reach))
        };
        Self {
            hit: made("hit"),
            heart: made("heart"),
            healing: made("healing"),
            whole: made("full"),
        }
    }

    /// Plays `heard` once at `at` - where the player is.
    pub fn play(&self, graph: &mut Graph, heard: Heard, at: Vector3<f32>) {
        let made = match heard {
            Heard::Hit => &self.hit,
            Heard::Heart => &self.heart,
            Heard::Healing => &self.healing,
            Heard::Whole => &self.whole,
        };
        let Some((buffer, reach)) = made else {
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
    fn three_touches_and_they_are_deleted() {
        let mut health = Health::default();
        assert!(!health.hit());
        assert!(!health.hit());
        assert!(health.hit());
    }

    #[test]
    fn a_heart_heals_fast_and_rest_slowly() {
        let mut health = Health::default();
        health.hit();
        health.hit();
        let low = health.left;
        health.heart();
        assert!((health.left - (low + HEART)).abs() < 1.0e-5);
        // Moving, nothing comes back; resting, it does, but only after a moment, and slowly.
        let mut rested = Health { left: low, ..Health::default() };
        let dt = 0.1;
        for _ in 0..100 {
            rested.update(dt, false);
        }
        assert_eq!(rested.left, low);
        for _ in 0..(REST_BEFORE / dt) as u32 - 1 {
            rested.update(dt, true);
        }
        assert_eq!(rested.left, low);
        for _ in 0..(10.0 / dt) as u32 {
            rested.update(dt, true);
        }
        let gained = rested.left - low;
        assert!(gained > 0.15 && gained < 0.3, "{gained}");
        for _ in 0..(REST_TIME / dt) as u32 {
            rested.update(dt, true);
        }
        assert_eq!(rested.left, 1.0);
    }

    #[test]
    fn resting_says_when_it_starts_to_heal_and_when_health_is_whole() {
        let mut health = Health { left: 0.99, ..Health::default() };
        let dt = 0.1;
        let said: Vec<Healing> = (0..((REST_BEFORE + 2.0) / dt) as u32)
            .filter_map(|_| health.update(dt, true))
            .collect();
        assert_eq!(said, vec![Healing::Started, Healing::Whole]);
        // Whole, resting says nothing more.
        assert_eq!(health.update(dt, true), None);
    }

    #[test]
    fn every_sound_is_there_and_plays() {
        let sounds = Sounds::load(HEALTH_SOUNDS).expect("the health's sounds load");
        for name in ["hit", "heart", "healing", "full"] {
            let sound = &sounds.sounds[name];
            let samples = formants::synth::make(sound, sounds.sample_rate);
            let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
            assert!(samples.iter().all(|s| s.is_finite()), "{name}");
            assert!((peak - sound.volume).abs() < 1.0e-3, "{name}: {peak}");
        }
    }
}
