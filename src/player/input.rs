//! The keys the player moves with, and the toggles among them: Caps Lock, C, Z, F, V, Tab and R
//! each act once per press, however long the key is held. Shift acts by how long it is held: a
//! tap switches running on or off, and held down it sprints.

use super::{
    posture::{Gait, SPRINT_HOLD},
    Player,
};
use fyrox::keyboard::KeyCode;

#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Keys {
    pub(super) forward: bool,
    pub(super) back: bool,
    pub(super) left: bool,
    pub(super) right: bool,
    /// How long Shift has been held down, in seconds; None while it is up.
    pub(super) shift_held: Option<f32>,
    /// Whether Caps Lock is down, so that key repeat while it is held does not toggle again.
    gait_toggle: bool,
    pub(super) jump: bool,
    /// Whether C and Z are down, so that key repeat while one is held does not toggle again.
    crouch: bool,
    crawl: bool,
    pub(super) look_back: bool,
    /// Whether F is down, so that key repeat does not switch the flashlight again.
    flashlight: bool,
    /// Whether V is down, so that key repeat does not switch the view again.
    view: bool,
    /// Whether Tab is down, so that key repeat does not take cover or let go again.
    cover: bool,
    /// Whether Tab has been pressed since the last update, to take cover or let go.
    pub(super) take_cover: bool,
    /// Whether the right mouse button is down, to strafe.
    pub(super) strafe: bool,
    /// Whether R is down, so that key repeat does not draw or holster the pistol again.
    pistol: bool,
    /// Whether the trigger has been pulled since the last update.
    pub(super) trigger: bool,
}

impl Player {
    pub fn on_key(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::KeyW | KeyCode::ArrowUp => self.keys.forward = pressed,
            KeyCode::KeyS | KeyCode::ArrowDown => self.keys.back = pressed,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.keys.left = pressed,
            KeyCode::KeyD | KeyCode::ArrowRight => self.keys.right = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => match (pressed, self.keys.shift_held) {
                (true, None) => self.keys.shift_held = Some(0.0),
                // Let go before it sprinted: a tap, which switches running on or off.
                (false, Some(held)) => {
                    if held < SPRINT_HOLD {
                        self.pace = switched(self.pace, Gait::Running);
                    }
                    self.keys.shift_held = None;
                }
                _ => (),
            },
            KeyCode::CapsLock => {
                if pressed && !self.keys.gait_toggle {
                    self.pace = switched(self.pace, Gait::Jogging);
                }
                self.keys.gait_toggle = pressed;
            }
            KeyCode::Space => self.keys.jump = pressed,
            KeyCode::KeyC => {
                if pressed && !self.keys.crouch {
                    self.posture = self.posture.crouch_toggled();
                }
                self.keys.crouch = pressed;
            }
            KeyCode::KeyQ => self.keys.look_back = pressed,
            KeyCode::KeyF => {
                if pressed && !self.keys.flashlight {
                    self.flashlight_on = !self.flashlight_on;
                }
                self.keys.flashlight = pressed;
            }
            KeyCode::KeyV => {
                if pressed && !self.keys.view {
                    self.toggle_view();
                }
                self.keys.view = pressed;
            }
            KeyCode::Tab => {
                if pressed && !self.keys.cover {
                    self.keys.take_cover = true;
                }
                self.keys.cover = pressed;
            }
            KeyCode::KeyR => {
                if pressed && !self.keys.pistol {
                    self.toggle_pistol();
                }
                self.keys.pistol = pressed;
            }
            KeyCode::KeyZ => {
                if pressed && !self.keys.crawl {
                    self.posture = self.posture.crawl_toggled();
                }
                self.keys.crawl = pressed;
            }
            _ => (),
        }
    }

    /// Counts another `dt` seconds of Shift being held down, if it is.
    pub(super) fn hold_keys(&mut self, dt: f32) {
        if let Some(held) = self.keys.shift_held.as_mut() {
            *held += dt;
        }
    }

    /// Holds the right mouse button down, or lets it go: while it is down, the droid strafes,
    /// facing ahead whichever way it goes.
    pub fn set_strafing(&mut self, held: bool) {
        self.keys.strafe = held;
    }

    pub fn release_keys(&mut self) {
        self.keys = Keys::default();
        self.orbit.held = false;
    }
}

/// The pace a toggle for `to` leaves the player in, from `pace`: `to`, or back to a walk if they
/// were at it already.
fn switched(pace: Gait, to: Gait) -> Gait {
    if pace == to {
        Gait::Walking
    } else {
        to
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{hold_shift, posture::Posture, press};

    #[test]
    fn c_toggles_crouching() {
        let mut player = Player::default();
        press(&mut player, KeyCode::KeyC);
        assert_eq!(player.posture, Posture::Crouching);
        press(&mut player, KeyCode::KeyC);
        assert_eq!(player.posture, Posture::Standing);
    }

    #[test]
    fn z_toggles_crawling() {
        let mut player = Player::default();
        press(&mut player, KeyCode::KeyZ);
        assert_eq!(player.posture, Posture::Crawling);
        press(&mut player, KeyCode::KeyZ);
        assert_eq!(player.posture, Posture::Standing);
    }

    #[test]
    fn c_from_a_crawl_crouches_and_z_from_a_crouch_crawls() {
        let mut player = Player::default();
        press(&mut player, KeyCode::KeyZ);
        press(&mut player, KeyCode::KeyC);
        assert_eq!(player.posture, Posture::Crouching);
        press(&mut player, KeyCode::KeyZ);
        assert_eq!(player.posture, Posture::Crawling);
    }

    #[test]
    fn a_key_let_go_while_the_window_was_away_still_works_after() {
        let mut player = Player::default();
        // Pressed, then the window lost focus before the key came up.
        player.on_key(KeyCode::KeyC, true);
        player.release_keys();
        press(&mut player, KeyCode::KeyC);
        assert_eq!(player.posture, Posture::Standing, "down, then up again");
    }

    #[test]
    fn f_switches_the_flashlight_off_and_on_once_per_press() {
        let mut player = Player::default();
        assert!(!player.flashlight_on, "off to start with");
        press(&mut player, KeyCode::KeyF);
        assert!(player.flashlight_on);
        press(&mut player, KeyCode::KeyF);
        assert!(!player.flashlight_on);
    }

    #[test]
    fn v_goes_between_third_and_first_person_once_per_press() {
        let mut player = Player::default();
        assert!(player.third_person, "seen from behind to start with");
        press(&mut player, KeyCode::KeyV);
        assert!(!player.third_person);
        press(&mut player, KeyCode::KeyV);
        assert!(player.third_person);
    }

    #[test]
    fn caps_lock_goes_between_walking_and_jogging_once_per_press() {
        let mut player = Player::default();
        assert_eq!(player.gait(), Gait::Walking, "walking to start with");
        press(&mut player, KeyCode::CapsLock);
        assert_eq!(player.gait(), Gait::Jogging);
        press(&mut player, KeyCode::CapsLock);
        assert_eq!(player.gait(), Gait::Walking, "back to a walk");
    }

    #[test]
    fn a_tap_of_shift_goes_between_walking_and_running() {
        let mut player = Player::default();
        press(&mut player, KeyCode::ShiftLeft);
        assert_eq!(player.gait(), Gait::Running);
        press(&mut player, KeyCode::ShiftLeft);
        assert_eq!(player.gait(), Gait::Walking, "back to a walk");
        // From a jog, a tap runs, and Caps Lock from a run jogs.
        press(&mut player, KeyCode::CapsLock);
        press(&mut player, KeyCode::ShiftLeft);
        assert_eq!(player.gait(), Gait::Running);
        press(&mut player, KeyCode::CapsLock);
        assert_eq!(player.gait(), Gait::Jogging);
    }

    #[test]
    fn shift_held_sprints_from_any_pace_and_leaves_it_as_it_was() {
        let mut player = Player::default();
        player.on_key(KeyCode::ShiftLeft, true);
        assert_eq!(player.gait(), Gait::Walking, "not yet held long enough");
        player.hold_keys(SPRINT_HOLD);
        assert_eq!(player.gait(), Gait::Sprinting, "sprinting from a walk");
        player.on_key(KeyCode::ShiftLeft, false);
        assert_eq!(player.gait(), Gait::Walking, "a sprint is not a tap");

        for pace in [KeyCode::CapsLock, KeyCode::ShiftLeft] {
            let mut player = Player::default();
            press(&mut player, pace);
            let before = player.gait();
            hold_shift(&mut player);
            assert_eq!(player.gait(), Gait::Sprinting, "sprinting from {before:?}");
            player.on_key(KeyCode::ShiftLeft, false);
            assert_eq!(player.gait(), before, "back once Shift is let go");
        }
    }

    #[test]
    fn strafing_anything_faster_than_a_walk_is_a_jog() {
        let mut player = Player::default();
        hold_shift(&mut player);
        player.set_strafing(true);
        assert_eq!(player.gait(), Gait::Jogging, "a sprint jogs, strafing");
        player.set_strafing(false);
        assert_eq!(player.gait(), Gait::Sprinting, "Shift is still held");

        let mut player = Player::default();
        press(&mut player, KeyCode::ShiftLeft);
        player.set_strafing(true);
        assert_eq!(player.gait(), Gait::Jogging, "a run jogs, strafing");
    }

    #[test]
    fn the_pace_survives_the_window_losing_focus() {
        let mut player = Player::default();
        press(&mut player, KeyCode::CapsLock);
        hold_shift(&mut player);
        player.release_keys();
        assert_eq!(
            player.gait(),
            Gait::Jogging,
            "still jogging, and Shift is no longer held"
        );
    }
}
