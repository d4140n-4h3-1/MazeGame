//! Checks on the renderer: that it is the one this game is built for, and how it is doing.

use fyrox::{
    core::log::Log,
    engine::{GraphicsContext, InitializedGraphicsContext},
    plugin::PluginContext,
};
use fyrox_graphics_wgpu::server::WgpuGraphicsServer;

/// The graphics API the game is built for: Vulkan on the desktop, and in a browser WebGL 2, the
/// only one wgpu has there that every browser runs.
const EXPECTED: wgpu::Backend = if cfg!(target_arch = "wasm32") {
    wgpu::Backend::Gl
} else {
    wgpu::Backend::Vulkan
};

/// Whether the renderer is running on [`EXPECTED`]. Logs which it is, or why not.
pub fn has_expected_backend(graphics_context: &InitializedGraphicsContext) -> bool {
    let server = graphics_context.renderer.server.clone();
    match server.as_any().downcast_ref::<WgpuGraphicsServer>() {
        Some(server) => {
            let info = server.state.adapter.get_info();
            if info.backend != EXPECTED {
                Log::err(format!(
                    "Expected {EXPECTED:?}, but wgpu picked {:?} on {}. Exiting.",
                    info.backend, info.name
                ));
                return false;
            }
            Log::info(format!("Rendering with {EXPECTED:?} on {}", info.name));
            true
        }
        None => {
            Log::err("The renderer is not the wgpu one; build this crate on its own.");
            false
        }
    }
}

/// With `MAZE_DEBUG`, logs what the renderer did once a second, including the slowest frame of
/// that second - which is what a stutter looks like in numbers.
#[derive(Debug, Default, PartialEq)]
pub struct FrameStats {
    worst_frame: f32,
}

impl FrameStats {
    /// Takes in the frame just drawn, and logs the second's statistics when it is up.
    pub fn update(&mut self, ctx: &PluginContext) {
        if crate::platform::var("MAZE_DEBUG").is_none() {
            return;
        }
        if let GraphicsContext::Initialized(graphics_context) = &*ctx.graphics_context {
            let stats = graphics_context.renderer.get_statistics();
            self.worst_frame = self.worst_frame.max(stats.pure_frame_time);
            if (ctx.elapsed_time - ctx.dt).floor() == ctx.elapsed_time.floor() {
                return;
            }
            let worst = std::mem::take(&mut self.worst_frame);
            Log::info(format!(
                "Stats: {} FPS, frame {:.2} ms (capped {:.2} ms), {} draw calls, {} triangles,                  {} point shadow maps, {} spot shadow maps, {} traced shadows, {} point lights, {} spot lights",
                stats.frames_per_second,
                stats.pure_frame_time * 1000.0,
                worst * 1000.0,
                stats.geometry.draw_calls,
                stats.geometry.triangles_rendered,
                stats.lighting.point_shadow_maps_rendered,
                stats.lighting.spot_shadow_maps_rendered,
                stats.lighting.traced_shadows_rendered,
                stats.lighting.point_lights_rendered,
                stats.lighting.spot_lights_rendered,
            ));
        }
    }
}
