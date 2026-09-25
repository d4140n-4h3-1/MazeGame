//! What is written on screen: a status line in the corner, and a banner across the middle for
//! the end of a round and for anything that went wrong.

use fyrox::{
    core::{algebra::Vector2, color::Color, pool::Handle},
    gui::{
        brush::Brush,
        screen::ScreenBuilder,
        text::{Text, TextBuilder, TextMessage},
        widget::WidgetBuilder,
        HorizontalAlignment, Thickness, UserInterface, VerticalAlignment,
    },
    plugin::PluginContext,
};

/// How long a note stays on screen, in seconds.
const NOTE_TIME: f32 = 3.0;

/// What the status line is about.
pub enum Status {
    Loading,
    /// Nothing: the banner says what there is to say.
    Blank,
    Round {
        time: f32,
        best: Option<f32>,
        /// How much breath is left, from 0 to 1, and whether the player has run out of it.
        breath: (f32, bool),
        mouse_captured: bool,
        /// How the droids hunting the player are going about it, if any are.
        alarm: Option<String>,
    },
}

#[derive(Debug, Default, PartialEq)]
pub struct Hud {
    status: Handle<Text>,
    banner: Handle<Text>,
    /// A message shown for a moment, such as the view settings while they are being changed.
    note: String,
    note_time: f32,
}

impl Hud {
    pub fn build(ctx: &mut PluginContext) -> Self {
        // The engine starts with no user interface; this one becomes the first, and the engine
        // keeps it the size of the window.
        ctx.user_interfaces
            .add(UserInterface::new(Vector2::new(1280.0, 720.0)));
        let ui = ctx.user_interfaces.first_mut();
        let status = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::uniform(12.0))
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_font_size(22.0.into())
        .with_text("Loading the maze...")
        .build(&mut ui.build_ctx());
        let banner = TextBuilder::new(
            WidgetBuilder::new().with_foreground(Brush::Solid(Color::opaque(120, 255, 150)).into()),
        )
        .with_font_size(40.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .with_vertical_text_alignment(VerticalAlignment::Center)
        .build(&mut ui.build_ctx());
        // The UI's root only gives its children the size they ask for, which for text is the text
        // itself, in the corner. A screen is the size of the window, so in one the banner is
        // centered on the window.
        ScreenBuilder::new(WidgetBuilder::new().with_child(banner)).build(&mut ui.build_ctx());
        Self {
            status,
            banner,
            ..Default::default()
        }
    }

    pub fn set_banner(&self, ui: &UserInterface, text: &str) {
        ui.send(self.banner, TextMessage::Text(text.to_owned()));
    }

    /// Puts `note` under the status line for a few seconds.
    pub fn show_note(&mut self, note: String) {
        self.note = note;
        self.note_time = NOTE_TIME;
    }

    pub fn note(&self) -> &str {
        &self.note
    }

    /// Rewrites the status line, `dt` seconds after the last time.
    pub fn update(&mut self, ui: &UserInterface, dt: f32, status: Status) {
        self.note_time = (self.note_time - dt).max(0.0);
        let text = match status {
            Status::Loading => "Loading the maze...".to_string(),
            Status::Blank => String::new(),
            Status::Round {
                time,
                best,
                breath: (breath, winded),
                mouse_captured,
                alarm,
            } => {
                let mut text = format!("Time {}", format_time(time));
                if let Some(alarm) = alarm {
                    text += &format!("    {alarm}");
                }
                if let Some(best) = best {
                    text += &format!("    Best {}", format_time(best));
                }
                // Only worth the room it takes once some of it has been spent.
                if breath < 1.0 {
                    text += &format!(
                        "    {} {}",
                        if winded { "Winded" } else { "Breath" },
                        breath_bar(breath)
                    );
                }
                if !mouse_captured {
                    text += "    (click to look around)";
                }
                if self.note_time > 0.0 {
                    text += &format!("\n{}", self.note);
                }
                text
            }
        };
        ui.send(self.status, TextMessage::Text(text));
    }
}

/// How much breath is left, drawn as a bar of ten. Rounded down, so that a bar with anything
/// left in it means there is something left to spend.
fn breath_bar(breath: f32) -> String {
    let filled = (breath.clamp(0.0, 1.0) * 10.0).floor() as usize;
    format!("[{}{}]", "|".repeat(filled), ".".repeat(10 - filled))
}

pub fn format_time(seconds: f32) -> String {
    let whole = seconds as u32;
    format!(
        "{}:{:02}.{}",
        whole / 60,
        whole % 60,
        ((seconds.fract()) * 10.0) as u32
    )
}
