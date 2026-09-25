//! The conversation on screen, as in Fallout 3: a green-on-black panel along the bottom, with
//! who is talking, what they say and what it means, how the last check went, and the replies to
//! pick from, the one picked lit up; and the hint to talk, under the middle of the screen, while
//! there is someone close enough.

use super::View;
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

/// The Pip-Boy's greens: bright for what is being said and the reply picked, the usual for the
/// rest, dim for what has been said already and for the panel's edge.
const BRIGHT: Color = Color::opaque(170, 255, 190);
const GREEN: Color = Color::opaque(90, 225, 130);
const DIM: Color = Color::opaque(50, 130, 75);
const BACKDROP: Color = Color::from_rgba(2, 12, 6, 215);
const LIT: Color = Color::from_rgba(60, 255, 130, 55);
const UNLIT: Color = Color::from_rgba(0, 0, 0, 0);

/// Something done with the mouse to a reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    /// The mouse went over the `n`th.
    Over(usize),
    /// The `n`th was clicked.
    Picked(usize),
}

#[derive(Debug, Default, PartialEq)]
pub struct DialogueScreen {
    screen: Handle<Screen>,
    name: Handle<Text>,
    says: Handle<Text>,
    means: Handle<Text>,
    note: Handle<Text>,
    /// Each reply: its lit-up background, and its text.
    replies: Vec<(Handle<Border>, Handle<Text>)>,
    /// Whether each reply shown has been said before.
    said: Vec<bool>,
    selected: usize,
    prompt: Handle<Text>,
    open: bool,
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
        let name = text(ctx, DIM, 17.0, Thickness::bottom(6.0));
        let says = text(ctx, BRIGHT, 27.0, Thickness::bottom(4.0));
        let means = text(ctx, GREEN, 19.0, Thickness::bottom(4.0));
        let note = text(ctx, BRIGHT, 17.0, Thickness::bottom(4.0));
        let rule = BorderBuilder::new(
            WidgetBuilder::new()
                .with_height(1.0)
                .with_margin(Thickness::top_bottom(8.0))
                .with_background(Brush::Solid(DIM).into()),
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
                GREEN,
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
                .with_background(Brush::Solid(BACKDROP).into())
                .with_foreground(Brush::Solid(DIM).into())
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
                .with_foreground(Brush::Solid(GREEN).into()),
        )
        .with_font_size(22.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        ScreenBuilder::new(WidgetBuilder::new().with_child(prompt)).build(ctx);

        Self {
            screen,
            name,
            says,
            means,
            note,
            replies,
            said: Vec::new(),
            selected: 0,
            prompt,
            open: false,
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

    /// Shows `view`, from `who`, with the first reply picked.
    pub fn show(&mut self, ui: &UserInterface, who: &str, view: &View) {
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
                (true, _) => BRIGHT,
                (false, true) => DIM,
                (false, false) => GREEN,
            };
            ui.send(reply, WidgetMessage::Background(Brush::Solid(if lit { LIT } else { UNLIT }).into()));
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
