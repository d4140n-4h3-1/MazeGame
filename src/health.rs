//! The player's health: a sentry's touch takes some of it rather than deleting them outright, and
//! only with none left are they deleted. It comes back two ways: fast, from a heart picked up in
//! the maze (see [`crate::hearts`]), or slowly, a little at a time, while they rest - standing
//! still a moment.

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

    /// Another `dt` seconds on, `resting` or not: resting long enough, some comes back.
    pub fn update(&mut self, dt: f32, resting: bool) {
        self.flash = (self.flash - dt).max(0.0);
        self.rested = if resting { self.rested + dt } else { 0.0 };
        if self.rested >= REST_BEFORE && self.left > 0.0 {
            self.left = (self.left + dt / REST_TIME).min(1.0);
        }
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
}
