//! Maze: a small maze game, played as a droid seen from behind, rendered with Vulkan.
//!
//! Every round is a new maze, put together at random from the tile models in `data/` (see
//! [`generate`] and [`tiles`]). The player starts at one end of the longest route through it and a
//! glowing exit waits at the other end; the clock runs until the player reaches it.
//!
//! Controls: WASD to move, Caps Lock to go between walking and running, Shift to sprint while
//! it is held - which costs breath, and leaves the player walking once it runs out - Space to
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

mod computer;
mod culling;
mod diagnostics;
mod dialogue;
mod dismember;
mod fixtures;
mod formants;
mod game;
mod generate;
mod hud;
mod inhabitants;
mod inward;
mod layout;
mod level;
mod menu;
mod platform;
mod ragdoll;
mod player;
mod survey;
mod tiles;

use fyrox::{
    engine::{executor::Executor, GraphicsContextParams},
    event_loop::EventLoop,
    window::{Fullscreen, WindowAttributes},
};
use fyrox_gfx::GraphicsEffects;
use game::MazeGame;

fn main() {
    // Assets are looked up next to this crate, wherever it is started from.
    let _ = std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"));

    let mut executor = Executor::from_params(
        Some(EventLoop::new().unwrap()),
        GraphicsContextParams {
            window_attributes: window_attributes(),
            // MAZE_VSYNC=0 uncaps the frame rate, which is how the cost of a frame is measured.
            vsync: platform::var("MAZE_VSYNC").as_deref() != Some("0"),
            msaa_sample_count: None,
            // With OpenGL compiled out, the default constructor is the wgpu one.
            graphics_server_constructor: Default::default(),
            named_objects: false,
        },
    );
    let effects = graphics_effects();
    // The game tells the effects what moves, so anti-aliasing does not leave a ghost behind it.
    let moving = effects.moving_things();
    executor.add_plugin(effects);
    executor.add_plugin(MazeGame::new(moving));
    executor.run()
}

/// The game fills the screen it starts on, borderless, so the monitor keeps its own resolution.
/// MAZE_WINDOWED=1 opens it in an ordinary window instead. In a browser it fills the page, which
/// the page's own style sees to.
fn window_attributes() -> WindowAttributes {
    let mut attributes = WindowAttributes::default().with_title("Maze").with_resizable(true);
    if platform::var("MAZE_WINDOWED").as_deref() != Some("1") && cfg!(not(target_arch = "wasm32")) {
        attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }
    // The renderer is made before the page lays out the canvas, which it cannot draw into while
    // it has no size.
    #[cfg(target_arch = "wasm32")]
    if let Some(size) = platform::page_size() {
        attributes = attributes.with_inner_size(size);
    }
    attributes
}

/// Refractive glass for the ceiling panes, softer shadow edges, anti-aliasing, a budget that keeps
/// shadow maps for the nearest lamps only (while shadows are not traced), and ambient occlusion
/// reaching far enough to shade the corners of corridors this wide - each of which the
/// environment can change.
fn graphics_effects() -> GraphicsEffects {
    let mut effects = GraphicsEffects::default()
        .with_ambient_occlusion(fyrox_gfx::AmbientOcclusion::reaching(0.9));
    // MAZE_SHADOW_BUDGET=0 lets every lamp in range draw shadow maps, which stops their shadows
    // switching on and off as the nearest four change, at the cost of a shadow map per lamp. It
    // only matters when shadows are not traced.
    if platform::var("MAZE_SHADOW_BUDGET").as_deref() == Some("0") {
        effects = effects.without_shadow_budget();
    }
    // MAZE_SSAO=0 turns ambient occlusion off. It is computed from what is on screen, so the
    // darkening it adds in corners changes with where the camera looks.
    if platform::var("MAZE_SSAO").as_deref() == Some("0") {
        effects = effects.with_ambient_occlusion(fyrox_gfx::AmbientOcclusion::off());
    }
    // Every light's shadows traced against the geometry, in place of shadow maps: every lamp
    // shadows what it lights, at any distance, instead of the nearest few. MAZE_RT=0 goes back
    // to shadow maps, as does a graphics card without ray tracing. The edges are soft, from a few
    // rays per pixel spread over each light; MAZE_HARD_SHADOWS=1 traces one ray and keeps them
    // sharp.
    if platform::var("MAZE_RT").as_deref() != Some("0") {
        let shadows = if platform::var("MAZE_HARD_SHADOWS").as_deref() == Some("1") {
            fyrox_gfx::RayTracedShadows::hard()
        } else {
            fyrox_gfx::RayTracedShadows::default()
        };
        effects = effects.with_ray_traced_shadows(shadows);
    }
    // MAZE_REFLECTIONS=0 turns the floor reflections off.
    if platform::var("MAZE_REFLECTIONS").as_deref() != Some("0") {
        effects = effects.with_reflections(fyrox_gfx::Reflections {
            // A corridor floor is not a mirror: enough to catch the lamps overhead.
            strength: 0.25,
            reach: 8.0,
            ..Default::default()
        });
    }
    effects
}
