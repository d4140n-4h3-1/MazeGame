//! Capture the flag: two sides, red and blue, each with a base round its flag at one end of the
//! map (see [`crate::firewall`]). The player is red, and takes blue's flag.
//!
//! Each side has [`DROIDS`] droids and a drone of its own. Red's are the player's allies:
//! they pay the player no heed, and go after blue's. Blue's guard their end, watching for the
//! player all the while, and go after the player or red's, whichever they see. Of each side's
//! droids, the first stays by its own flag and the second makes for the other side's; each
//! side's drone patrols round its own flag. A droid comes after one of the other side it sees
//! and shoots it with its pistol; a drone fires at them. Each side's shots harm only the other
//! side: blue's harm the player too.

use fyrox::core::algebra::Vector3;

/// The map it is played on.
pub const CTF_MAP: &str = "data/arena/ctf_map.glb";

/// How many droids each side has.
pub const DROIDS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Red,
    Blue,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Red, Side::Blue];

    pub fn name(self) -> &'static str {
        match self {
            Side::Red => "red",
            Side::Blue => "blue",
        }
    }

    pub fn other(self) -> Side {
        match self {
            Side::Red => Side::Blue,
            Side::Blue => Side::Red,
        }
    }

    /// Whether it is the player's side.
    pub fn is_players(self) -> bool {
        self == Side::Red
    }
}

/// Where a side's droids and drone go: the flags, as the map has them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bases {
    pub red: Vector3<f32>,
    pub blue: Vector3<f32>,
}

impl Bases {
    pub fn flag(&self, side: Side) -> Vector3<f32> {
        match side {
            Side::Red => self.red,
            Side::Blue => self.blue,
        }
    }

    /// Where the `n`th droid of `side` keeps to: its own flag for the first, the other side's
    /// for the rest.
    pub fn post(&self, side: Side, n: usize) -> Vector3<f32> {
        match n {
            0 => self.flag(side),
            _ => self.flag(side.other()),
        }
    }
}
