//! The conversation on screen, as in Fallout 3: a panel along the bottom, with who is talking,
//! what they say and what it means, how the last check went, and the replies to pick from, the
//! one picked lit up; and the hint to talk, under the middle of the screen, while there is
//! someone close enough. The panel is coloured by the mood of what is being said: green as
//! usual, blue, yellow, orange or red (see [`Mood`]).

use super::{Mood, View};
use fyrox::{
    core::{color::Color, pool::Handle},
    gui::{
        border::{Border, BorderBuilder},
        brush::Brush,
        formatted_text::WrapMode,
        message::{MouseButton, UiMessage},
        screen::{Screen, ScreenBuilder},
        stack_panel::StackPanelBuilder,
        text::{Text, TextBuilder, TextMessage},
        widget::{WidgetBuilder, WidgetMessage},
        BuildContext, HorizontalAlignment, Thickness, UserInterface, VerticalAlignment,
    },
};

/// The most replies a line can offer; any past these are not shown.
pub const MOST_REPLIES: usize = 8;
/// How wide the panel is, in pixels.
const WIDTH: f32 = 860.0;

/// The colours of the panel in one mood: bright for what is being said and the reply picked, the
/// usual for the rest, dim for what has been said already and for the panel's edge; the panel's
/// backdrop, and the light behind the reply picked.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Palette {
    bright: Color,
    usual: Color,
    dim: Color,
    backdrop: Color,
    lit: Color,
}

/// The Pip-Boy's greens, as usual.
const GREEN: Palette = Palette {
    bright: Color::opaque(170, 255, 190),
    usual: Color::opaque(90, 225, 130),
    dim: Color::opaque(50, 130, 75),
    backdrop: Color::from_rgba(2, 12, 6, 215),
    lit: Color::from_rgba(60, 255, 130, 55),
};
const BLUE: Palette = Palette {
    bright: Color::opaque(175, 215, 255),
    usual: Color::opaque(95, 165, 255),
    dim: Color::opaque(50, 95, 150),
    backdrop: Color::from_rgba(2, 6, 14, 215),
    lit: Color::from_rgba(80, 160, 255, 55),
};
const YELLOW: Palette = Palette {
    bright: Color::opaque(255, 245, 165),
    usual: Color::opaque(235, 210, 80),
    dim: Color::opaque(140, 120, 40),
    backdrop: Color::from_rgba(12, 10, 2, 215),
    lit: Color::from_rgba(255, 220, 60, 55),
};
const ORANGE: Palette = Palette {
    bright: Color::opaque(255, 205, 155),
    usual: Color::opaque(255, 145, 50),
    dim: Color::opaque(150, 80, 30),
    backdrop: Color::from_rgba(14, 6, 2, 215),
    lit: Color::from_rgba(255, 140, 40, 55),
};
const RED: Palette = Palette {
    bright: Color::opaque(255, 175, 170),
    usual: Color::opaque(240, 70, 60),
    dim: Color::opaque(140, 40, 35),
    backdrop: Color::from_rgba(14, 2, 2, 215),
    lit: Color::from_rgba(255, 60, 50, 55),
};
const UNLIT: Color = Color::from_rgba(0, 0, 0, 0);

fn palette(mood: Mood) -> Palette {
    match mood {
        Mood::Normal => GREEN,
        Mood::Success => BLUE,
        Mood::Warning => YELLOW,
        Mood::Agitated => ORANGE,
        Mood::Hostile => RED,
    }
}

/// The colour a droid's eyes glow saying something in `mood`: that of the panel, but none - their
/// own - as usual.
pub fn eyes(mood: Mood) -> Option<Color> {
    (mood != Mood::Normal).then(|| palette(mood).usual)
}

/// Something done with the mouse to a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    /// The mouse went over the `n`th.
    Over(usize),
    /// The `n`th was clicked.
    Picked(usize),
}

#[derive(Debug, PartialEq)]
pub struct DialogueScreen {
    screen: Handle<Screen>,
    /// The panel's edge and backdrop, and the line between what is said and the replies.
    frame: Handle<Border>,
    rule: Handle<Border>,
    name: Handle<Text>,
    says: Handle<Text>,
    means: Handle<Text>,
    note: Handle<Text>,
    /// Each reply: its lit-up background, and its text.
    replies: Vec<(Handle<Border>, Handle<Text>)>,
    /// Whether each reply shown has been said before.
    said: Vec<bool>,
    selected: usize,
    /// The colours of the mood of what is being said.
    palette: Palette,
    prompt: Handle<Text>,
    open: bool,
}

impl Default for DialogueScreen {
    fn default() -> Self {
        Self {
            screen: Handle::NONE,
            frame: Handle::NONE,
            rule: Handle::NONE,
            name: Handle::NONE,
            says: Handle::NONE,
            means: Handle::NONE,
            note: Handle::NONE,
            replies: Vec::new(),
            said: Vec::new(),
            selected: 0,
            palette: GREEN,
            prompt: Handle::NONE,
            open: false,
        }
    }
}

fn text(ctx: &mut BuildContext, color: Color, size: f32, margin: Thickness) -> Handle<Text> {
    TextBuilder::new(
        WidgetBuilder::new()
            .with_margin(margin)
            .with_foreground(Brush::Solid(color).into()),
    )
    .with_font_size(size.into())
    .with_wrap(WrapMode::Word)
    .build(ctx)
}

impl DialogueScreen {
    /// Builds the panel, hidden, and the hint to talk, in `ui`.
    pub fn build(ui: &mut UserInterface) -> Self {
        let ctx = &mut ui.build_ctx();
        let name = text(ctx, GREEN.dim, 17.0, Thickness::bottom(6.0));
        let says = text(ctx, GREEN.bright, 27.0, Thickness::bottom(4.0));
        let means = text(ctx, GREEN.usual, 19.0, Thickness::bottom(4.0));
        let note = text(ctx, GREEN.bright, 17.0, Thickness::bottom(4.0));
        let rule = BorderBuilder::new(
            WidgetBuilder::new()
                .with_height(1.0)
                .with_margin(Thickness::top_bottom(8.0))
                .with_background(Brush::Solid(GREEN.dim).into()),
        )
        .with_stroke_thickness(Thickness::uniform(0.0).into())
        .build(ctx);
        let mut panel = WidgetBuilder::new()
            .with_margin(Thickness::uniform(18.0))
            .with_child(name)
            .with_child(says)
            .with_child(means)
            .with_child(note)
            .with_child(rule);
        let mut replies = Vec::new();
        for _ in 0..MOST_REPLIES {
            let label = text(
                ctx,
                GREEN.usual,
                22.0,
                Thickness {
                    left: 10.0,
                    top: 4.0,
                    right: 10.0,
                    bottom: 4.0,
                },
            );
            let reply = BorderBuilder::new(
                WidgetBuilder::new()
                    .with_visibility(false)
                    .with_background(Brush::Solid(UNLIT).into())
                    .with_child(label),
            )
            .with_stroke_thickness(Thickness::uniform(0.0).into())
            .with_corner_radius(0.0.into())
            .build(ctx);
            panel = panel.with_child(reply);
            replies.push((reply, label));
        }
        let panel = StackPanelBuilder::new(panel).build(ctx);
        let frame = BorderBuilder::new(
            WidgetBuilder::new()
                .with_width(WIDTH)
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_vertical_alignment(VerticalAlignment::Bottom)
                .with_margin(Thickness::bottom(40.0))
                .with_background(Brush::Solid(GREEN.backdrop).into())
                .with_foreground(Brush::Solid(GREEN.dim).into())
                .with_child(panel),
        )
        .with_stroke_thickness(Thickness::uniform(2.0).into())
        .with_corner_radius(0.0.into())
        .build(ctx);
        // The UI's root only gives its children the size they ask for; a screen is the size of
        // the window, so the panel sits at the bottom of it.
        let screen = ScreenBuilder::new(
            WidgetBuilder::new()
                .with_visibility(false)
                .with_child(frame),
        )
        .build(ctx);

        // Under the middle of the screen, where Fallout puts it.
        let prompt = TextBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_vertical_alignment(VerticalAlignment::Center)
                .with_margin(Thickness::top(240.0))
                .with_foreground(Brush::Solid(GREEN.usual).into()),
        )
        .with_font_size(22.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        ScreenBuilder::new(WidgetBuilder::new().with_child(prompt)).build(ctx);

        Self {
            screen,
            frame,
            rule,
            name,
            says,
            means,
            note,
            replies,
            prompt,
            ..Default::default()
        }
    }

    /// Shows or hides the panel.
    pub fn set_open(&mut self, ui: &UserInterface, open: bool) {
        self.open = open;
        ui.send(self.screen, WidgetMessage::Visibility(open));
    }

    /// Puts the hint to talk to `who` on screen, or with none takes it off.
    pub fn set_prompt(&self, ui: &UserInterface, who: Option<&str>) {
        let text = who.map_or(String::new(), |who| format!("{who}\nE) Talk"));
        ui.send(self.prompt, TextMessage::Text(text));
    }

    /// Shows `view`, from `who`, in the colours of its mood, with the first reply picked.
    pub fn show(&mut self, ui: &UserInterface, who: &str, view: &View) {
        self.colour(ui, palette(view.mood));
        ui.send(self.name, TextMessage::Text(who.to_string()));
        ui.send(self.says, TextMessage::Text(view.says.clone()));
        ui.send(self.means, TextMessage::Text(view.means.clone()));
        ui.send(self.means, WidgetMessage::Visibility(!view.means.is_empty()));
        ui.send(self.note, TextMessage::Text(view.note.clone().unwrap_or_default()));
        ui.send(self.note, WidgetMessage::Visibility(view.note.is_some()));
        self.said.clear();
        for (i, &(reply, label)) in self.replies.iter().enumerate() {
            let choice = view.choices.get(i);
            ui.send(reply, WidgetMessage::Visibility(choice.is_some()));
            if let Some(choice) = choice {
                ui.send(label, TextMessage::Text(choice.label.clone()));
                self.said.push(choice.said);
            }
        }
        self.select(ui, 0);
    }

    /// Colours the panel, and all that is in it but the replies, with `palette`; the replies are
    /// coloured as they are picked.
    fn colour(&mut self, ui: &UserInterface, palette: Palette) {
        self.palette = palette;
        let solid = |color: Color| Brush::Solid(color).into();
        ui.send(self.frame, WidgetMessage::Background(solid(palette.backdrop)));
        ui.send(self.frame, WidgetMessage::Foreground(solid(palette.dim)));
        ui.send(self.rule, WidgetMessage::Background(solid(palette.dim)));
        for (text, color) in [
            (self.name, palette.dim),
            (self.says, palette.bright),
            (self.means, palette.usual),
            (self.note, palette.bright),
        ] {
            ui.send(text, WidgetMessage::Foreground(solid(color)));
        }
    }

    /// How many replies there are to pick from.
    pub fn count(&self) -> usize {
        self.said.len()
    }

    /// The reply picked.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Picks the `n`th reply, lighting it up.
    pub fn select(&mut self, ui: &UserInterface, n: usize) {
        self.selected = n.min(self.count().saturating_sub(1));
        for (i, &(reply, label)) in self.replies.iter().take(self.count()).enumerate() {
            let lit = i == self.selected;
            let color = match (lit, self.said[i]) {
                (true, _) => self.palette.bright,
                (false, true) => self.palette.dim,
                (false, false) => self.palette.usual,
            };
            let light = if lit { self.palette.lit } else { UNLIT };
            ui.send(reply, WidgetMessage::Background(Brush::Solid(light).into()));
            ui.send(label, WidgetMessage::Foreground(Brush::Solid(color).into()));
        }
    }

    /// Picks the reply `step` on from the one picked now, round from the last to the first.
    pub fn step(&mut self, ui: &UserInterface, step: isize) {
        let count = self.count().max(1) as isize;
        let n = (self.selected as isize + step).rem_euclid(count);
        self.select(ui, n as usize);
    }

    /// What `message` does with the mouse to a reply, if anything.
    pub fn pointer(&self, message: &UiMessage) -> Option<Pointer> {
        if !self.open {
            return None;
        }
        let (i, _) = self
            .replies
            .iter()
            .take(self.count())
            .enumerate()
            .find(|(_, &(reply, label))| message.is_from(reply) || message.is_from(label))?;
        match message.data::<WidgetMessage>()? {
            WidgetMessage::MouseEnter => Some(Pointer::Over(i)),
            WidgetMessage::MouseDown {
                button: MouseButton::Left,
                ..
            } => Some(Pointer::Picked(i)),
            _ => None,
        }
    }
}
