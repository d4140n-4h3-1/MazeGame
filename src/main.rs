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
//! draw or holster the pistol, the left mouse button to draw it and fire, N for a new maze,
//! Escape to pause. Run with `cargo run` from this directory;
//! `MAZE_SIZE=<w>x<d>` sets how many junctions wide and deep the maze is, `MAZE_DEBUG=1` also prints
//! the walkable map the game made of the level, `MAZE_SEED=<n>` makes every maze identical, and
//! `MAZE_MODEL=<path>` plays a fixed maze model instead, such as `data/maze_full.fbx`.

mod culling;
mod diagnostics;
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
mod player;
mod survey;
mod tiles;

use fyrox::{
    engine::{executor::Executor, GraphicsContextParams},
    event_loop::EventLoop,
};
use fyrox_gfx::GraphicsEffects;
use game::MazeGame;

fn main() {
    // Assets are looked up next to this crate, wherever it is started from.
    let _ = std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"));

    let mut executor = Executor::from_params(
        Some(EventLoop::new().unwrap()),
        GraphicsContextParams {
            window_attributes: Default::default(),
            // MAZE_VSYNC=0 uncaps the frame rate, which is how the cost of a frame is measured.
            vsync: std::env::var("MAZE_VSYNC").as_deref() != Ok("0"),
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
    if std::env::var("MAZE_SHADOW_BUDGET").as_deref() == Ok("0") {
        effects = effects.without_shadow_budget();
    }
    // MAZE_SSAO=0 turns ambient occlusion off. It is computed from what is on screen, so the
    // darkening it adds in corners changes with where the camera looks.
    if std::env::var("MAZE_SSAO").as_deref() == Ok("0") {
        effects = effects.with_ambient_occlusion(fyrox_gfx::AmbientOcclusion::off());
    }
    // Every light's shadows traced against the geometry, in place of shadow maps: every lamp
    // shadows what it lights, at any distance, instead of the nearest few. MAZE_RT=0 goes back
    // to shadow maps, as does a graphics card without ray tracing. The edges are soft, from a few
    // rays per pixel spread over each light; MAZE_HARD_SHADOWS=1 traces one ray and keeps them
    // sharp.
    if std::env::var("MAZE_RT").as_deref() != Ok("0") {
        let shadows = if std::env::var("MAZE_HARD_SHADOWS").as_deref() == Ok("1") {
            fyrox_gfx::RayTracedShadows::hard()
        } else {
            fyrox_gfx::RayTracedShadows::default()
        };
        effects = effects.with_ray_traced_shadows(shadows);
    }
    // MAZE_REFLECTIONS=0 turns the floor reflections off.
    if std::env::var("MAZE_REFLECTIONS").as_deref() != Ok("0") {
        effects = effects.with_reflections(fyrox_gfx::Reflections {
            // A corridor floor is not a mirror: enough to catch the lamps overhead.
            strength: 0.25,
            reach: 8.0,
            ..Default::default()
        });
    }
    effects
}
