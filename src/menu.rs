//! The pause menu: the world stops behind a dimmed screen, with buttons to carry on, to switch
//! the maze's lights, to change the options, to start again or to leave, and a reminder of the
//! controls. The options are a page of their own: the subtitles of what the droids say, in
//! System Latin and in English, each on or off. Escape there goes back to the menu.

use crate::dialogue::screen::Subtitles;
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
        BuildContext, HorizontalAlignment, Thickness, UiNode, UserInterface, VerticalAlignment,
    },
};

const CONTROLS: &str = "WASD move    Mouse look    Space jump\n\
    Caps Lock walk or run    Shift sprint\n\
    C crouch    Z crawl    Tab cover    Q look behind\n\
    Right mouse strafe    R pistol    Left mouse draw, fire\n\
    E talk    F flashlight    N new maze\n\
    [ ] turn speed    - = view width";

/// What the player picked in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Resume,
    /// Switch the maze's lights off, or back on.
    Lights,
    /// Go to the options, or back from them to the menu.
    Options,
    Back,
    /// Switch the subtitles in System Latin, or those in English, off or back on.
    LatinSubtitles,
    EnglishSubtitles,
    Restart,
    Quit,
}

#[derive(Debug, Default, PartialEq)]
pub struct PauseMenu {
    screen: Handle<Screen>,
    resume: Handle<Button>,
    lights: Handle<Button>,
    lights_label: Handle<Text>,
    options: Handle<Button>,
    restart: Handle<Button>,
    quit: Handle<Button>,
    /// The menu's own page, and the options'.
    main_page: Handle<UiNode>,
    options_page: Handle<UiNode>,
    latin: Handle<Button>,
    latin_label: Handle<Text>,
    english: Handle<Button>,
    english_label: Handle<Text>,
    back: Handle<Button>,
    open: bool,
    /// Whether the options are showing rather than the menu's own page.
    in_options: bool,
}

impl PauseMenu {
    /// Builds the menu, hidden, over everything else in `ui`. `restart` is what starting again
    /// is called: a new maze, or a new round in the same one.
    pub fn build(ui: &mut UserInterface, restart: &str) -> Self {
        let ctx = &mut ui.build_ctx();
        let paused_title = title(ctx, "Paused");
        let (resume, _) = button(ctx, "Resume");
        let (lights, lights_label) = button(ctx, &lights_text(true));
        let (options, _) = button(ctx, "Options");
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
        let main_page = page(
            ctx,
            true,
            [
                paused_title.to_base(),
                resume.to_base(),
                lights.to_base(),
                options.to_base(),
                restart.to_base(),
                quit.to_base(),
                controls.to_base(),
            ],
        );
        let options_title = title(ctx, "Options");
        let subtitles = Subtitles::default();
        let (latin, latin_label) = button(ctx, &latin_text(subtitles.latin));
        let (english, english_label) = button(ctx, &english_text(subtitles.english));
        let (back, _) = button(ctx, "Back");
        let options_page = page(
            ctx,
            false,
            [options_title.to_base(), latin.to_base(), english.to_base(), back.to_base()],
        );
        let backdrop = BorderBuilder::new(
            WidgetBuilder::new()
                .with_background(Brush::Solid(Color::from_rgba(0, 0, 0, 170)).into())
                .with_child(main_page)
                .with_child(options_page),
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
            options,
            restart,
            quit,
            main_page,
            options_page,
            latin,
            latin_label,
            english,
            english_label,
            back,
            open: false,
            in_options: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the options are showing rather than the menu's own page.
    pub fn in_options(&self) -> bool {
        self.in_options
    }

    /// Shows or hides the menu, on its own page. `can_restart` is whether there is a level to
    /// start again yet.
    pub fn set_open(&mut self, ui: &UserInterface, open: bool, can_restart: bool) {
        self.open = open;
        ui.send(self.screen, WidgetMessage::Visibility(open));
        ui.send(self.restart, WidgetMessage::Enabled(can_restart));
        self.set_in_options(ui, false);
    }

    /// Shows the options, or the menu's own page.
    pub fn set_in_options(&mut self, ui: &UserInterface, in_options: bool) {
        self.in_options = in_options;
        ui.send(self.main_page, WidgetMessage::Visibility(!in_options));
        ui.send(self.options_page, WidgetMessage::Visibility(in_options));
    }

    /// Shows which subtitles are on.
    pub fn set_subtitles(&self, ui: &UserInterface, subtitles: Subtitles) {
        ui.send(self.latin_label, TextMessage::Text(latin_text(subtitles.latin)));
        ui.send(self.english_label, TextMessage::Text(english_text(subtitles.english)));
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
            (self.options, Choice::Options),
            (self.back, Choice::Back),
            (self.latin, Choice::LatinSubtitles),
            (self.english, Choice::EnglishSubtitles),
            (self.restart, Choice::Restart),
            (self.quit, Choice::Quit),
        ]
        .into_iter()
        .find(|&(button, _)| matches!(message.data_from(button), Some(ButtonMessage::Click)))
        .map(|(_, choice)| choice)
    }
}

fn lights_text(on: bool) -> String {
    format!("Lights: {}", on_off(on))
}

fn latin_text(on: bool) -> String {
    format!("System Latin subtitles: {}", on_off(on))
}

fn english_text(on: bool) -> String {
    format!("English subtitles: {}", on_off(on))
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// The big white heading at the top of a page.
fn title(ctx: &mut BuildContext, text: &str) -> Handle<Text> {
    TextBuilder::new(
        WidgetBuilder::new()
            .with_margin(Thickness::bottom(18.0))
            .with_foreground(Brush::Solid(Color::WHITE).into()),
    )
    .with_text(text)
    .with_font_size(44.0.into())
    .with_horizontal_text_alignment(HorizontalAlignment::Center)
    .build(ctx)
}

/// A page of the menu: `items`, one above the other in the middle of the screen, showing if
/// `visible`.
fn page(
    ctx: &mut BuildContext,
    visible: bool,
    items: impl IntoIterator<Item = Handle<UiNode>>,
) -> Handle<UiNode> {
    let page = WidgetBuilder::new()
        .with_visibility(visible)
        .with_horizontal_alignment(HorizontalAlignment::Center)
        .with_vertical_alignment(VerticalAlignment::Center)
        .with_children(items);
    StackPanelBuilder::new(page).build(ctx).to_base()
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
            .with_width(340.0)
            .with_height(48.0)
            .with_margin(Thickness::uniform(6.0)),
    )
    .with_content(text)
    .build(ctx);
    (button, text)
}
