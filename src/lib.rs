//! Maze: a small maze game, played as a droid seen from behind, rendered with Vulkan.
//!
//! Every round is a new maze, put together at random from the tile models in `data/` (see
//! [`generate`] and [`tiles`]). The player starts at one end of the longest route through it and a
//! glowing exit waits at the other end; the clock runs until the player reaches it.
//!
//! Controls: WASD to move, Caps Lock to go between walking and jogging, a tap of Shift between
//! walking and running and Shift held to sprint - running and sprinting cost stamina, and running
//! out of it leaves the player walking - Space to
//! jump, mouse to look, C to crouch, Z to crawl (each toggles), hold Q to look behind, Tab to
//! take cover against a wall - A and D slide along it, and lean round its corner at the edge - F
//! for the flashlight, V for third or first person, hold the right mouse button to strafe, R to
//! draw or holster the pistol, the left mouse button to draw it and fire, E to talk to a droid
//! close by, N for a new maze, Escape to pause. Run with `cargo run` from this directory;
//! `MAZE_SIZE=<w>x<d>` sets how many junctions wide and deep the maze is, `MAZE_DEBUG=1` also prints
//! the walkable map the game made of the level, `MAZE_SEED=<n>` makes every maze identical,
//! `MAZE_WINDOWED=1` opens a window instead of filling the screen, and `MAZE_MODEL=<path>` plays a
//! fixed maze model instead, such as `data/maze_full.fbx`. In a browser these go in the page's
//! address instead, as `?MAZE_SEED=7` (see [`platform::var`]).

//!
//! The game is this library, which `src/main.rs` runs; the examples (see `examples/`) use it too,
//! such as the droids the player and the maze's inhabitants are made from ([`player::avatar`]).

mod alarm;
mod computer;
mod credits;
mod ctf;
mod culling;
mod diagnostics;
mod dialogue;
pub mod dismember;
mod drone;
mod drone_shot;
mod health;
mod hearts;
mod firewall;
mod fixtures;
mod formants;
pub mod game;
mod generate;
mod hud;
mod inhabitants;
mod inward;
mod layout;
mod level;
mod menu;
mod notes;
pub mod platform;
pub mod player;
pub mod ragdoll;
mod survey;
mod tiles;
