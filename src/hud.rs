//! What is written on screen: a status line in the corner, a banner across the middle for the
//! end of a round and for anything that went wrong, the droids' alert at the top in the middle,
//! as in Metal Gear and Fallout, the player's stamina in the bottom left corner, and, with the
//! pistol out, its ammo in the bottom right: the cyber pistol's is endless, shown as ∞.
//!
//! The alert is coloured by its phase - ALERT red, EVASION amber, CAUTION yellow - and ALERT and
//! CAUTION blink; EVASION and CAUTION count down the seconds they have left. The player's health
//! is a red bar in the bottom middle, and the screen flashes red as they are hit. The stamina is a bar,
//! green, yellow once it runs low, and red and blinking while the player is winded. Sentries have
//! bars like it over their heads (see [`Hud::show_breath_bars`]).

use crate::{computer::FONT, inhabitants::Alert};
use fyrox::{
    asset::untyped::ResourceKind,
    core::{algebra::Vector2, color::Color, pool::Handle, uuid::Uuid},
    gui::{
        border::{Border, BorderBuilder},
        brush::Brush,
        font::{Font, FontResource, FontStyles},
        grid::{Column, GridBuilder, Row},
        screen::ScreenBuilder,
        stack_panel::StackPanelBuilder,
        text::{Text, TextBuilder, TextMessage},
        widget::{WidgetBuilder, WidgetMessage},
        HorizontalAlignment, Orientation, Thickness, UiNode, UserInterface, VerticalAlignment,
    },
    plugin::PluginContext,
};

/// How long a note stays on screen, in seconds.
const NOTE_TIME: f32 = 3.0;
/// How often what blinks goes round, in seconds, and how much of that it is showing.
const BLINK: f32 = 0.6;
const BLINK_ON: f32 = 0.4;
/// The alert's colour in each phase.
const ALERT_RED: Color = Color::opaque(255, 45, 45);
const EVASION_AMBER: Color = Color::opaque(255, 150, 30);
const CAUTION_YELLOW: Color = Color::opaque(255, 225, 40);
/// The stamina bar: how big it is inside its frame, in pixels, and its colours - with plenty left,
/// running low, and winded - and below how much it counts as running low.
const BAR: (f32, f32) = (180.0, 10.0);
const STAMINA_GREEN: Color = Color::opaque(90, 230, 120);
const STAMINA_LOW: f32 = 0.35;
/// The health bar's colour, and below how much of it it blinks; and how red the screen goes as
/// the player is hit, out of 255.
const HEALTH_RED: Color = Color::opaque(230, 50, 70);
const HEALTH_LOW: f32 = 0.34;
const HURT_RED: u8 = 90;

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
        /// How much health is left, from 0 to 1, and how long is left of the red flash of a hit,
        /// in seconds.
        health: (f32, f32),
        /// Whether the pistol is out.
        armed: bool,
        mouse_captured: bool,
        /// How the droids hunting the player are going about it, if any are, and how many seconds
        /// that has left.
        alarm: Option<(Alert, f32)>,
    },
}

#[derive(Debug, Default, PartialEq)]
pub struct Hud {
    status: Handle<Text>,
    banner: Handle<Text>,
    /// The alert at the top, and the stamina bar - its panel and what fills it.
    alert: Handle<Text>,
    stamina: Handle<UiNode>,
    stamina_fill: Handle<Border>,
    /// The health bar - its panel and what fills it - and the red over the whole screen as the
    /// player is hit.
    health: Handle<UiNode>,
    health_fill: Handle<Border>,
    hurt: Handle<UiNode>,
    shown_health: Option<(f32, f32)>,
    /// The pistol's ammo, and whether it is showing.
    ammo: Handle<UiNode>,
    shown_ammo: bool,
    /// How long it has been blinking, in seconds, and what was last put on screen, so that each
    /// is only sent when it changes.
    blink: f32,
    shown_alert: Option<(String, Color)>,
    shown_stamina: Option<(f32, Color)>,
    /// The bars over the sentries' heads - each a frame and what fills it - of which those not
    /// needed are hidden; and where they go.
    breath_bars: Vec<(Handle<UiNode>, Handle<Border>)>,
    screen: Handle<UiNode>,
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
        let ctx = &mut ui.build_ctx();
        let alert = TextBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_vertical_alignment(VerticalAlignment::Top)
                // Under the status line, which starts in the same row.
                .with_margin(Thickness::top(44.0))
                .with_visibility(false),
        )
        .with_font_size(34.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        let stamina_fill = BorderBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Left)
                .with_margin(Thickness::uniform(3.0))
                .with_width(BAR.0)
                .with_height(BAR.1)
                .with_background(Brush::Solid(STAMINA_GREEN).into()),
        )
        .with_stroke_thickness(Thickness::zero().into())
        .build(ctx);
        let frame = BorderBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Left)
                .with_width(BAR.0 + 6.0)
                .with_height(BAR.1 + 6.0)
                .with_foreground(Brush::Solid(Color::opaque(200, 200, 200)).into())
                .with_background(Brush::Solid(Color::from_rgba(0, 0, 0, 120)).into())
                .with_child(stamina_fill),
        )
        .with_stroke_thickness(Thickness::uniform(1.0).into())
        .build(ctx);
        let label = |ctx: &mut _, text: &str, alignment| {
            TextBuilder::new(
                WidgetBuilder::new()
                    .with_horizontal_alignment(alignment)
                    .with_margin(Thickness::bottom(3.0))
                    .with_foreground(Brush::Solid(Color::WHITE).into()),
            )
            .with_font_size(16.0.into())
            .with_text(text)
            .build(ctx)
        };
        let stamina_label = label(ctx, "STAMINA", HorizontalAlignment::Left);
        let health_fill = BorderBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Left)
                .with_margin(Thickness::uniform(3.0))
                .with_width(BAR.0)
                .with_height(BAR.1)
                .with_background(Brush::Solid(HEALTH_RED).into()),
        )
        .with_stroke_thickness(Thickness::zero().into())
        .build(ctx);
        let health_frame = BorderBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_width(BAR.0 + 6.0)
                .with_height(BAR.1 + 6.0)
                .with_foreground(Brush::Solid(Color::opaque(200, 200, 200)).into())
                .with_background(Brush::Solid(Color::from_rgba(0, 0, 0, 120)).into())
                .with_child(health_fill),
        )
        .with_stroke_thickness(Thickness::uniform(1.0).into())
        .build(ctx);
        let health_label = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::bottom(3.0))
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_font_size(16.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .with_text("HEALTH")
        .build(ctx);
        let health = StackPanelBuilder::new(
            // Across the whole bottom, with what is in it in the middle: the screen puts a child
            // asking for less than all of it in its corner.
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Stretch)
                .with_vertical_alignment(VerticalAlignment::Bottom)
                .with_margin(Thickness::uniform(18.0))
                .with_visibility(false)
                .with_child(health_label)
                .with_child(health_frame),
        )
        .with_orientation(Orientation::Vertical)
        .build(ctx)
        .to_base();
        let stamina = StackPanelBuilder::new(
            WidgetBuilder::new()
                .on_column(0)
                .with_horizontal_alignment(HorizontalAlignment::Left)
                .with_vertical_alignment(VerticalAlignment::Bottom)
                .with_visibility(false)
                .with_child(stamina_label)
                .with_child(frame),
        )
        .with_orientation(Orientation::Vertical)
        .build(ctx)
        .to_base();
        // The pistol's ammo: endless. In DejaVu Sans Mono, which has the sign for it.
        let pistol_label = label(ctx, "CYBER PISTOL", HorizontalAlignment::Right);
        let mut endless = TextBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Right)
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_font_size(56.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Right)
        .with_text("∞");
        if let Ok(font) = Font::from_memory(FONT, 1024, FontStyles::default(), Vec::new()) {
            endless = endless.with_font(FontResource::new_ok(
                Uuid::new_v4(),
                ResourceKind::Embedded,
                font,
            ));
        }
        let endless = endless.build(ctx);
        let ammo = StackPanelBuilder::new(
            WidgetBuilder::new()
                .on_column(2)
                .with_horizontal_alignment(HorizontalAlignment::Right)
                .with_vertical_alignment(VerticalAlignment::Bottom)
                .with_visibility(false)
                .with_child(pistol_label)
                .with_child(endless),
        )
        .with_orientation(Orientation::Vertical)
        .build(ctx)
        .to_base();
        // Along the bottom of the window: the stamina on the left, the ammo on the right; the
        // health, by itself, in the middle.
        let bottom = GridBuilder::new(
            WidgetBuilder::new()
                .with_vertical_alignment(VerticalAlignment::Bottom)
                .with_margin(Thickness::uniform(18.0))
                .with_child(stamina)
                .with_child(ammo),
        )
        .add_row(Row::auto())
        .add_column(Column::auto())
        .add_column(Column::stretch())
        .add_column(Column::auto())
        .build(ctx);
        // The UI's root only gives its children the size they ask for, which for text is the text
        // itself, in the corner. A screen is the size of the window, so in one the banner is
        // centered on the window, the alert at the top in the middle, the stamina in the bottom
        // left corner and the ammo in the bottom right.
        // Red over everything, clear until the player is hit.
        let hurt = BorderBuilder::new(
            WidgetBuilder::new()
                .with_visibility(false)
                .with_background(Brush::Solid(Color::from_rgba(255, 0, 0, 0)).into()),
        )
        .with_stroke_thickness(Thickness::zero().into())
        .build(ctx)
        .to_base();
        let screen = ScreenBuilder::new(
            WidgetBuilder::new()
                .with_child(hurt)
                .with_child(health)
                .with_child(banner)
                .with_child(alert)
                .with_child(bottom),
        )
        .build(ctx)
        .to_base();
        Self {
            status,
            banner,
            alert,
            stamina,
            stamina_fill,
            health,
            health_fill,
            hurt,
            ammo,
            screen,
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

    /// Rewrites the status line, and puts up the alert and the stamina, `dt` seconds after the
    /// last time.
    pub fn update(&mut self, ui: &UserInterface, dt: f32, status: Status) {
        self.note_time = (self.note_time - dt).max(0.0);
        self.blink = (self.blink + dt) % BLINK;
        let blinking_on = self.blink < BLINK_ON;
        let (text, alarm, breath, health, armed) = match status {
            Status::Loading => ("Loading the maze...".to_string(), None, None, None, false),
            Status::Blank => (String::new(), None, None, None, false),
            Status::Round {
                time,
                best,
                breath,
                health,
                armed,
                mouse_captured,
                alarm,
            } => {
                let mut text = format!("Time {}", format_time(time));
                if let Some(best) = best {
                    text += &format!("    Best {}", format_time(best));
                }
                if !mouse_captured {
                    text += "    (click to look around)";
                }
                if self.note_time > 0.0 {
                    text += &format!("\n{}", self.note);
                }
                (text, alarm, Some(breath), Some(health), armed)
            }
        };
        ui.send(self.status, TextMessage::Text(text));

        let alert = alarm.and_then(|(alert, left)| alert_shown(alert, left, blinking_on));
        if alert != self.shown_alert {
            ui.send(self.alert, WidgetMessage::Visibility(alert.is_some()));
            if let Some((text, colour)) = &alert {
                ui.send(self.alert, TextMessage::Text(text.clone()));
                ui.send(self.alert, WidgetMessage::Foreground(Brush::Solid(*colour).into()));
            }
            self.shown_alert = alert;
        }

        let stamina = breath.map(|(left, winded)| stamina_shown(left, winded, blinking_on));
        if stamina != self.shown_stamina {
            ui.send(self.stamina, WidgetMessage::Visibility(stamina.is_some()));
            if let Some((left, colour)) = stamina {
                ui.send(self.stamina_fill, WidgetMessage::Width(BAR.0 * left));
                ui.send(self.stamina_fill, WidgetMessage::Background(Brush::Solid(colour).into()));
            }
            self.shown_stamina = stamina;
        }

        // Rounded, so that it is only sent as it changes enough to see.
        let health = health.map(|(left, flash)| {
            ((left.clamp(0.0, 1.0) * 200.0).round() / 200.0, (flash * 20.0).round() / 20.0)
        });
        if health != self.shown_health {
            ui.send(self.health, WidgetMessage::Visibility(health.is_some()));
            if let Some((left, flash)) = health {
                ui.send(self.health_fill, WidgetMessage::Width(BAR.0 * left));
                let colour = match left < HEALTH_LOW && blinking_on {
                    true => ALERT_RED,
                    false => HEALTH_RED,
                };
                ui.send(self.health_fill, WidgetMessage::Background(Brush::Solid(colour).into()));
                let red = (flash / crate::health::FLASH * HURT_RED as f32) as u8;
                ui.send(self.hurt, WidgetMessage::Visibility(red > 0));
                ui.send(
                    self.hurt,
                    WidgetMessage::Background(Brush::Solid(Color::from_rgba(255, 0, 0, red)).into()),
                );
            } else {
                ui.send(self.hurt, WidgetMessage::Visibility(false));
            }
            self.shown_health = health;
        }

        if armed != self.shown_ammo {
            ui.send(self.ammo, WidgetMessage::Visibility(armed));
            self.shown_ammo = armed;
        }
    }
}

/// A stamina bar over a droid's head: where the middle of its bottom edge is on screen, in
/// pixels; how wide it is; and how much breath the droid has left, from 0 to 1, and whether it has
/// run out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreathBar {
    pub at: Vector2<f32>,
    pub width: f32,
    pub left: f32,
    pub winded: bool,
}

impl Hud {
    /// Shows `bars` over the droids' heads, looking as the player's does, and hides the rest.
    pub fn show_breath_bars(&mut self, ui: &mut UserInterface, bars: &[BreathBar]) {
        let blinking_on = self.blink < BLINK_ON;
        while self.breath_bars.len() < bars.len() {
            let bar = breath_bar(ui);
            ui.send(bar.0, WidgetMessage::LinkWith(self.screen));
            self.breath_bars.push(bar);
        }
        for (n, &(frame, fill)) in self.breath_bars.iter().enumerate() {
            let Some(bar) = bars.get(n) else {
                ui.send(frame, WidgetMessage::Visibility(false));
                continue;
            };
            let width = bar.width.clamp(BREATH_BAR_WIDTH.0, BREATH_BAR_WIDTH.1);
            let height = (width * BAR.1 / BAR.0 * 2.0).max(4.0);
            let (left, colour) = stamina_shown(bar.left, bar.winded, blinking_on);
            ui.send(frame, WidgetMessage::Visibility(true));
            ui.send(frame, WidgetMessage::Width(width + 4.0));
            ui.send(frame, WidgetMessage::Height(height + 4.0));
            ui.send(
                frame,
                WidgetMessage::Margin(Thickness {
                    left: bar.at.x - 0.5 * width - 2.0,
                    top: bar.at.y - height - 4.0,
                    right: 0.0,
                    bottom: 0.0,
                }),
            );
            ui.send(fill, WidgetMessage::Width(width * left));
            ui.send(fill, WidgetMessage::Height(height));
            ui.send(fill, WidgetMessage::Background(Brush::Solid(colour).into()));
        }
    }
}

/// How wide a bar over a droid's head can get on screen, in pixels, from least to most.
const BREATH_BAR_WIDTH: (f32, f32) = (40.0, 140.0);

/// A stamina bar for over a droid's head, hidden: its frame, and what fills it.
fn breath_bar(ui: &mut UserInterface) -> (Handle<UiNode>, Handle<Border>) {
    let ctx = &mut ui.build_ctx();
    let fill = BorderBuilder::new(
        WidgetBuilder::new()
            .with_horizontal_alignment(HorizontalAlignment::Left)
            .with_margin(Thickness::uniform(2.0))
            .with_background(Brush::Solid(STAMINA_GREEN).into()),
    )
    .with_stroke_thickness(Thickness::zero().into())
    .build(ctx);
    let frame = BorderBuilder::new(
        WidgetBuilder::new()
            .with_horizontal_alignment(HorizontalAlignment::Left)
            .with_vertical_alignment(VerticalAlignment::Top)
            .with_visibility(false)
            .with_foreground(Brush::Solid(Color::opaque(200, 200, 200)).into())
            .with_background(Brush::Solid(Color::from_rgba(0, 0, 0, 120)).into())
            .with_child(fill),
    )
    .with_stroke_thickness(Thickness::uniform(1.0).into())
    .build(ctx)
    .to_base();
    (frame, fill)
}

/// What the alert says and in what colour, `left` seconds from the end of `alert`, with what
/// blinks showing or not: None while ALERT and CAUTION blink off.
fn alert_shown(alert: Alert, left: f32, blinking_on: bool) -> Option<(String, Color)> {
    let seconds = left.max(0.0).ceil();
    match alert {
        Alert::Alert => blinking_on.then(|| ("ALERT".to_string(), ALERT_RED)),
        Alert::Evasion => Some((format!("EVASION  {seconds:.0}"), EVASION_AMBER)),
        Alert::Caution => blinking_on.then(|| (format!("CAUTION  {seconds:.0}"), CAUTION_YELLOW)),
    }
}

/// How full the stamina bar is and its colour, with `left` of it, winded or not, and with what
/// blinks showing or not. The bar only goes dark, blinking, while winded.
fn stamina_shown(left: f32, winded: bool, blinking_on: bool) -> (f32, Color) {
    let left = (left.clamp(0.0, 1.0) * 100.0).round() / 100.0;
    let colour = match (winded, left < STAMINA_LOW) {
        (true, _) if blinking_on => ALERT_RED,
        (true, _) => Color::opaque(110, 20, 20),
        (false, true) => CAUTION_YELLOW,
        (false, false) => STAMINA_GREEN,
    };
    (left, colour)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_and_caution_blink_and_evasion_counts_down_steadily() {
        assert_eq!(alert_shown(Alert::Alert, 0.0, true), Some(("ALERT".into(), ALERT_RED)));
        assert_eq!(alert_shown(Alert::Alert, 0.0, false), None);
        assert_eq!(
            alert_shown(Alert::Evasion, 11.2, false),
            Some(("EVASION  12".into(), EVASION_AMBER)),
            "EVASION does not blink"
        );
        assert_eq!(
            alert_shown(Alert::Caution, 29.9, true),
            Some(("CAUTION  30".into(), CAUTION_YELLOW))
        );
        assert_eq!(alert_shown(Alert::Caution, 29.9, false), None);
    }

    #[test]
    fn the_stamina_bar_warns_as_it_runs_low_and_blinks_when_winded() {
        assert_eq!(stamina_shown(1.0, false, true), (1.0, STAMINA_GREEN));
        assert_eq!(stamina_shown(0.2, false, true).1, CAUTION_YELLOW);
        assert_eq!(stamina_shown(0.1, true, true).1, ALERT_RED);
        assert_ne!(stamina_shown(0.1, true, false).1, ALERT_RED, "blinking");
        assert_eq!(stamina_shown(1.4, false, true).0, 1.0, "never past full");
    }
}
