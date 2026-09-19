//! Breath: a sprint spends it, and anything less gets it back.

use super::{posture::Gait, Player};

/// How long a sprint lasts on a full breath, in seconds, and how long it takes to get all of it
/// back at a walk or a standstill. Getting it back is the slower half by a good way.
const SPRINT_TIME: f32 = 6.0;
const RECOVER_TIME: f32 = 12.0;
/// The share of the usual recovery the player gets while running rather than walking.
const RECOVER_RUNNING: f32 = 0.5;
/// How much breath has to come back before the player can sprint again, out of 1. Being winded
/// costs more than the moment it takes to draw one breath.
const RECOVERED: f32 = 0.35;

impl Player {
    /// How much breath is left, from 1 down to 0, and whether the player has run out of it - for
    /// showing on screen.
    pub fn breath(&self) -> (f32, bool) {
        (self.stamina, self.winded)
    }

    /// Spends breath on a sprint and gets it back the rest of the time. Running it out leaves the
    /// player winded, and walking - which is all [`Player::gait`] will then let them do - until
    /// enough of it is back.
    pub(super) fn breathe(&mut self, dt: f32, moving: bool) {
        if moving && self.gait() == Gait::Sprinting {
            self.stamina = (self.stamina - dt / SPRINT_TIME).max(0.0);
            self.winded |= self.stamina == 0.0;
            return;
        }
        // Still hard work, just not as hard: a run gets the breath back more slowly than a walk.
        let rate = if moving && self.gait() == Gait::Running {
            RECOVER_RUNNING
        } else {
            1.0
        };
        self.stamina = (self.stamina + rate * dt / RECOVER_TIME).min(1.0);
        self.winded &= self.stamina < RECOVERED;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{posture::Posture, press};
    use fyrox::keyboard::KeyCode;

    /// Breathes for `seconds`, and returns how much breath is left.
    fn breathe_for(player: &mut Player, seconds: f32, moving: bool) -> f32 {
        let dt = 1.0 / 60.0;
        for _ in 0..(seconds / dt) as u32 {
            player.breathe(dt, moving);
        }
        player.stamina
    }

    fn sprinting() -> Player {
        let mut player = Player::default();
        player.on_key(KeyCode::ShiftLeft, true);
        player
    }

    #[test]
    fn a_sprint_runs_the_breath_out_in_about_the_time_it_should() {
        let mut player = sprinting();
        assert_eq!(player.gait(), Gait::Sprinting);
        assert!(breathe_for(&mut player, SPRINT_TIME * 0.5, true) > 0.4, "half gone by halfway");

        let dt = 1.0 / 60.0;
        let mut seconds = SPRINT_TIME * 0.5;
        while !player.winded && seconds < SPRINT_TIME * 3.0 {
            player.breathe(dt, true);
            seconds += dt;
        }
        assert!(player.winded, "never ran out");
        assert!((seconds - SPRINT_TIME).abs() < 0.1, "{seconds} s of sprinting on a full breath");
        // And once it has run out, it starts coming back: being winded is already a walk.
        assert!(breathe_for(&mut player, 1.0, true) > 0.0);
    }

    #[test]
    fn standing_still_with_shift_held_costs_nothing() {
        let mut player = sprinting();
        assert_eq!(breathe_for(&mut player, SPRINT_TIME * 2.0, false), 1.0);
        assert!(!player.winded);
    }

    #[test]
    fn being_winded_forces_a_walk_until_enough_breath_is_back() {
        let mut player = sprinting();
        // Latched into a run as well, to show that being winded overrules that too.
        press(&mut player, KeyCode::CapsLock);
        breathe_for(&mut player, SPRINT_TIME * 1.1, true);
        assert!(player.winded);
        assert_eq!(player.gait(), Gait::Walking, "no sprinting, and no running either");

        // A moment's rest is not enough to set off again.
        breathe_for(&mut player, RECOVER_TIME * RECOVERED * 0.5, true);
        assert!(player.winded, "still blowing");
        breathe_for(&mut player, RECOVER_TIME * RECOVERED * 0.7, true);
        assert!(!player.winded, "got its breath back");
        assert_eq!(player.gait(), Gait::Sprinting, "Shift is still held");
    }

    #[test]
    fn breath_comes_back_more_slowly_at_a_run_than_at_a_walk() {
        let spent = |running: bool| {
            let mut player = Player {
                stamina: 0.0,
                running,
                ..Default::default()
            };
            breathe_for(&mut player, 1.0, true)
        };
        assert!(spent(true) < spent(false));
        assert!((spent(true) / spent(false) - RECOVER_RUNNING).abs() < 1e-3);
    }

    #[test]
    fn a_new_round_starts_on_a_full_breath_but_keeps_the_gait() {
        let mut player = sprinting();
        press(&mut player, KeyCode::CapsLock);
        press(&mut player, KeyCode::KeyZ);
        breathe_for(&mut player, SPRINT_TIME * 1.1, true);
        assert!(player.winded);

        player.start_fresh(1.0);
        assert_eq!(player.stamina, 1.0);
        assert!(!player.winded);
        assert_eq!(player.posture, Posture::Standing, "back on its feet");
        assert_eq!(player.yaw, 1.0);
        assert_eq!(player.gait(), Gait::Sprinting, "Shift and the latch are the player's own");
        player.on_key(KeyCode::ShiftLeft, false);
        assert_eq!(player.gait(), Gait::Running, "still latched into a run");
    }
}
