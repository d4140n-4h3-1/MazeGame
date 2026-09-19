//! The pause menu: the world stops behind a dimmed screen, with buttons to carry on, to switch
//! the maze's lights, to start again or to leave, and a reminder of the controls.

use fyrox::{
    core::{color::Color, pool::Handle},
    gui::{
        border::BorderBuilder,
        brush::Brush,
        button::{Button, ButtonBuilder, ButtonMessage},
        message::UiMessage,
        screen::{Screen, ScreenBuilder},
        stack_panel::StackPanelBuilder,
        text::{Text, TextBuilder, TextMessage},
        widget::{WidgetBuilder, WidgetMessage},
        BuildContext, HorizontalAlignment, Thickness, UserInterface, VerticalAlignment,
    },
};

const CONTROLS: &str = "WASD move    Mouse look    Space jump\n\
    Caps Lock walk or run    Shift sprint\n\
    C crouch    Z crawl    Ctrl lean    Q look behind\n\
    F flashlight    R new maze\n\
    [ ] turn speed    - = view width";

/// What the player picked in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Resume,
    /// Switch the maze's lights off, or back on.
    Lights,
    Restart,
    Quit,
}

#[derive(Debug, Default, PartialEq)]
pub struct PauseMenu {
    screen: Handle<Screen>,
    resume: Handle<Button>,
    lights: Handle<Button>,
    lights_label: Handle<Text>,
    restart: Handle<Button>,
    quit: Handle<Button>,
    open: bool,
}

impl PauseMenu {
    /// Builds the menu, hidden, over everything else in `ui`. `restart` is what starting again
    /// is called: a new maze, or a new round in the same one.
    pub fn build(ui: &mut UserInterface, restart: &str) -> Self {
        let ctx = &mut ui.build_ctx();
        let title = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::bottom(18.0))
                .with_foreground(Brush::Solid(Color::WHITE).into()),
        )
        .with_text("Paused")
        .with_font_size(44.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        let (resume, _) = button(ctx, "Resume");
        let (lights, lights_label) = button(ctx, &lights_text(true));
        let (restart, _) = button(ctx, restart);
        let (quit, _) = button(ctx, "Quit");
        let controls = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::top(24.0))
                .with_foreground(Brush::Solid(Color::opaque(190, 190, 200)).into()),
        )
        .with_text(CONTROLS)
        .with_font_size(16.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        let panel = StackPanelBuilder::new(
            WidgetBuilder::new()
                .with_horizontal_alignment(HorizontalAlignment::Center)
                .with_vertical_alignment(VerticalAlignment::Center)
                .with_child(title)
                .with_child(resume)
                .with_child(lights)
                .with_child(restart)
                .with_child(quit)
                .with_child(controls),
        )
        .build(ctx);
        let backdrop = BorderBuilder::new(
            WidgetBuilder::new()
                .with_background(Brush::Solid(Color::from_rgba(0, 0, 0, 170)).into())
                .with_child(panel),
        )
        .with_stroke_thickness(Thickness::uniform(0.0).into())
        .build(ctx);
        // The UI's root only gives its children the size they ask for; a screen is the size of
        // the window, so the backdrop covers all of it.
        let screen = ScreenBuilder::new(
            WidgetBuilder::new()
                .with_visibility(false)
                .with_child(backdrop),
        )
        .build(ctx);
        Self {
            screen,
            resume,
            lights,
            lights_label,
            restart,
            quit,
            open: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows or hides the menu. `can_restart` is whether there is a level to start again yet.
    pub fn set_open(&mut self, ui: &UserInterface, open: bool, can_restart: bool) {
        self.open = open;
        ui.send(self.screen, WidgetMessage::Visibility(open));
        ui.send(self.restart, WidgetMessage::Enabled(can_restart));
    }

    /// Shows whether the lights are on.
    pub fn set_lights(&self, ui: &UserInterface, on: bool) {
        ui.send(self.lights_label, TextMessage::Text(lights_text(on)));
    }

    /// What `message` picks from the menu, if anything.
    pub fn choice(&self, message: &UiMessage) -> Option<Choice> {
        [
            (self.resume, Choice::Resume),
            (self.lights, Choice::Lights),
            (self.restart, Choice::Restart),
            (self.quit, Choice::Quit),
        ]
        .into_iter()
        .find(|&(button, _)| matches!(message.data_from(button), Some(ButtonMessage::Click)))
        .map(|(_, choice)| choice)
    }
}

fn lights_text(on: bool) -> String {
    format!("Lights: {}", if on { "on" } else { "off" })
}

/// A button of the menu, and the text on it.
fn button(ctx: &mut BuildContext, label: &str) -> (Handle<Button>, Handle<Text>) {
    let text = TextBuilder::new(WidgetBuilder::new())
        .with_text(label)
        .with_font_size(24.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .with_vertical_text_alignment(VerticalAlignment::Center)
        .build(ctx);
    let button = ButtonBuilder::new(
        WidgetBuilder::new()
            .with_width(280.0)
            .with_height(48.0)
            .with_margin(Thickness::uniform(6.0)),
    )
    .with_content(text)
    .build(ctx);
    (button, text)
}
