//! The few things that differ when the game runs in a web page instead of on the desktop. A page
//! has no files of its own to read, no threads and no system clock, and the mouse is locked only
//! when the browser agrees to it.

use std::sync::mpsc::{self, Receiver};

/// The text of the data file at `path`, or why it cannot be had. A web page has no files, so the
/// web version carries the ones the game reads itself; the models and sounds the engine loads are
/// fetched from the page's server instead.
pub fn read_to_string(path: &str) -> Result<String, String> {
    #[cfg(not(target_arch = "wasm32"))]
    return std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"));

    #[cfg(target_arch = "wasm32")]
    return CARRIED
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, text)| text.to_string())
        .ok_or_else(|| format!("{path}: not carried in the web version"));
}

/// The data files the web version carries, by the paths the game reads them from.
#[cfg(target_arch = "wasm32")]
const CARRIED: &[(&str, &str)] = &[
    ("data/dialogue/droids.json", include_str!("../data/dialogue/droids.json")),
    ("data/droid_motion.json", include_str!("../data/droid_motion.json")),
    ("data/sounds/pistol_formants.json", include_str!("../data/sounds/pistol_formants.json")),
    ("data/sounds/voice_formants.json", include_str!("../data/sounds/voice_formants.json")),
];

/// The setting called `name`, if it is set. On the desktop settings are environment variables; a
/// page has none, so there they come from the query in its address instead:
/// `index.html?MAZE_SEED=7&MAZE_SSAO=0`.
pub fn var(name: &str) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return std::env::var(name).ok();

    #[cfg(target_arch = "wasm32")]
    return web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|query| web_sys::UrlSearchParams::new_with_str(&query).ok())
        .and_then(|params| params.get(name));
}

/// Does `work` on a thread of its own, and hands back where its result will arrive. A web page
/// has no threads, so there it is done before this returns.
pub fn in_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (sender, receiver) = mpsc::channel();
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || sender.send(work()));
    #[cfg(target_arch = "wasm32")]
    let _ = sender.send(work());
    receiver
}

/// The time now, in nanoseconds since 1970, for seeding the random numbers.
pub fn nanos_now() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    return std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);

    // The browser's clock counts milliseconds.
    #[cfg(target_arch = "wasm32")]
    return (fyrox::core::js_sys::Date::now() * 1.0e6) as u64;
}

/// How big the browser shows the page.
#[cfg(target_arch = "wasm32")]
pub fn page_size() -> Option<fyrox::dpi::LogicalSize<f64>> {
    let window = web_sys::window()?;
    let width = window.inner_width().ok()?.as_f64()?;
    let height = window.inner_height().ok()?.as_f64()?;
    Some(fyrox::dpi::LogicalSize::new(width, height))
}

/// Ends the game. In a browser that goes back to the page's title, since a page cannot close.
pub fn quit(ctx: &mut fyrox::plugin::PluginContext) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
        return;
    }
    ctx.loop_controller.exit();
}

/// Whether the browser has locked the mouse to the page. It is asked for, and granted later or
/// not at all; and Escape takes it back without the page hearing the key.
#[cfg(target_arch = "wasm32")]
pub fn mouse_locked() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.pointer_lock_element())
        .is_some()
}
