//! The menus.
//!
//! The main menu comes first, over a blank screen: which game to play - the maze, or capture the
//! flag on one of its maps, picked on a page of their own - or to leave.
//!
//! The pause menu: the world stops behind a dimmed screen, with buttons to carry on, to switch
//! the maze's lights, to change the options, to start again, to go back to the main menu or to
//! leave, and a reminder of the controls. The options are a page of their own: the subtitles of
//! what the droids say, in System Latin and in English, each on or off. Escape there goes back to
//! the menu.
//!
//! Every menu works from the keyboard as well as the mouse: the arrow keys, or W and S, go up and
//! down its buttons - the one picked shows in its own colour - and Enter or Space presses it.

use crate::{ctf::MAPS, dialogue::screen::Subtitles};
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
    keyboard::KeyCode,
};

const CONTROLS: &str = "WASD move    Mouse look    Space jump\n\
    Caps Lock walk or run    Shift sprint\n\
    C crouch    Z crawl    Tab cover    Q look behind\n\
    Right mouse strafe    R pistol    Left mouse draw, fire\n\
    E talk    F flashlight    N new maze\n\
    [ ] turn speed    - = view width";

/// The colour of the button picked from the keyboard, and of the rest.
const PICKED: Color = Color::opaque(110, 225, 255);
const UNPICKED: Color = Color::WHITE;

/// Going up and down a menu's buttons from the keyboard, page by page. The one picked on the
/// page showing is in its own colour and has the keyboard's focus, so that Enter or Space
/// presses it as a click would; a button clicked with the mouse is picked with it.
#[derive(Debug, Default, PartialEq)]
struct Picking {
    /// Each page's buttons, top to bottom, with the text on each.
    pages: Vec<Vec<(Handle<Button>, Handle<Text>)>>,
    page: usize,
    at: usize,
    /// The button with the keyboard's focus, if it is one of these.
    focused: Handle<Button>,
}

impl Picking {
    fn picked(&self) -> Option<(Handle<Button>, Handle<Text>)> {
        self.pages.get(self.page)?.get(self.at).copied()
    }

    /// Shows `page` as picked from, from its first button that `can` be pressed.
    fn show(&mut self, ui: &UserInterface, page: usize, can: impl Fn(Handle<Button>) -> bool) {
        self.leave(ui);
        self.page = page;
        self.at = 0;
        match self.picked() {
            Some((button, _)) if !can(button) => self.step(ui, 1, can),
            _ => self.light(ui),
        }
    }

    /// Picks the next button `by` down - up, for -1 - that `can` be pressed, going round from
    /// the bottom to the top.
    fn step(&mut self, ui: &UserInterface, by: isize, can: impl Fn(Handle<Button>) -> bool) {
        let Some(page) = self.pages.get(self.page) else {
            return;
        };
        let n = page.len() as isize;
        let mut at = self.at as isize;
        for _ in 0..n {
            at = (at + by).rem_euclid(n);
            if can(page[at as usize].0) {
                break;
            }
        }
        self.at = at as usize;
        self.light(ui);
    }

    /// Shows the picked button in its colour, the rest of the page's in theirs, and gives it
    /// the keyboard's focus.
    fn light(&self, ui: &UserInterface) {
        self.colour(ui);
        if let Some((button, _)) = self.picked() {
            ui.send(button, WidgetMessage::Focus);
        }
    }

    /// Shows the picked button in its colour, and the rest of the page's in theirs.
    fn colour(&self, ui: &UserInterface) {
        let Some(page) = self.pages.get(self.page) else {
            return;
        };
        for (n, &(_, text)) in page.iter().enumerate() {
            let colour = if n == self.at { PICKED } else { UNPICKED };
            ui.send(text, WidgetMessage::Foreground(Brush::Solid(colour).into()));
        }
    }

    /// Lets go of the page: nothing on it picked or focused.
    fn leave(&mut self, ui: &UserInterface) {
        if let Some((button, text)) = self.picked() {
            ui.send(text, WidgetMessage::Foreground(Brush::Solid(UNPICKED).into()));
            ui.send(button, WidgetMessage::Unfocus);
        }
    }

    /// Follows the keyboard's focus as `message` moves it: a click focuses what it clicks. Only
    /// follows it, never moving it, so that word of an older move coming in late is soon put
    /// right by word of the newer one.
    fn observe(&mut self, ui: &UserInterface, message: &UiMessage) {
        let Some(page) = self.pages.get(self.page) else {
            return;
        };
        for (n, &(button, _)) in page.iter().enumerate() {
            match message.data_from::<WidgetMessage>(button) {
                Some(WidgetMessage::Focus) => {
                    self.focused = button;
                    if n != self.at {
                        self.at = n;
                        self.colour(ui);
                    }
                }
                Some(WidgetMessage::Unfocus) if self.focused == button => self.focused = Handle::NONE,
                _ => (),
            }
        }
    }

    /// Goes up or down for the arrow keys, or W and S; and presses the picked button for Enter or
    /// Space, unless it has the keyboard's focus, which presses it already. Whether `code` was
    /// one of those.
    fn key(&mut self, ui: &UserInterface, code: KeyCode, can: impl Fn(Handle<Button>) -> bool) -> bool {
        match code {
            KeyCode::ArrowUp | KeyCode::KeyW => self.step(ui, -1, can),
            KeyCode::ArrowDown | KeyCode::KeyS => self.step(ui, 1, can),
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => {
                if let Some((button, _)) = self.picked().filter(|&(button, _)| button != self.focused && can(button)) {
                    ui.post(button, ButtonMessage::Click);
                }
            }
            _ => return false,
        }
        true
    }
}

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
    /// Go back to the main menu, leaving the game under way.
    MainMenu,
    Quit,
}

/// Which game the player picked in the main menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Maze,
    /// Capture the flag, on the map of [`MAPS`] at this index.
    CaptureTheFlag(usize),
}

/// What the player picked in the main menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Play(Game),
    /// Go to the maps for capture the flag, or back from them to the menu.
    Maps,
    Back,
    Quit,
}

/// The main menu: which game to play, or to leave; and for capture the flag, a page of its own
/// for which map.
#[derive(Debug, Default, PartialEq)]
pub struct MainMenu {
    screen: Handle<Screen>,
    maze: Handle<Button>,
    ctf: Handle<Button>,
    quit: Handle<Button>,
    /// The menu's own page, and the maps'.
    main_page: Handle<UiNode>,
    maps_page: Handle<UiNode>,
    /// A button for each of [`MAPS`], in order.
    maps: Vec<Handle<Button>>,
    back: Handle<Button>,
    picking: Picking,
}

impl MainMenu {
    /// Builds the menu, hidden, over everything else in `ui`, on a backdrop nothing shows through.
    pub fn build(ui: &mut UserInterface) -> Self {
        let ctx = &mut ui.build_ctx();
        let heading = title(ctx, "Ruptura Systematis");
        let maze_button = button(ctx, "Maze");
        let ctf_button = button(ctx, "Capture the Flag");
        let quit_button = button(ctx, "Quit");
        let ((maze, _), (ctf, _), (quit, _)) = (maze_button, ctf_button, quit_button);
        let about = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::top(24.0))
                .with_foreground(Brush::Solid(Color::opaque(190, 190, 200)).into()),
        )
        .with_text(
            "Maze: find the way out of a new maze each round.\n\
             Capture the Flag: hack red's firewall, take their flag and bring it home,\n\
             with blue's droids and drone on your side, on the map of your choice.",
        )
        .with_font_size(16.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        let items = [heading.to_base(), maze.to_base(), ctf.to_base(), quit.to_base(), about.to_base()];
        let main_page = page(ctx, true, items);
        let maps_title = title(ctx, "Capture the Flag");
        let map_buttons: Vec<_> = MAPS.iter().map(|map| button(ctx, map.name)).collect();
        let maps: Vec<_> = map_buttons.iter().map(|&(map, _)| map).collect();
        let back_button = button(ctx, "Back");
        let back = back_button.0;
        let maps_about = TextBuilder::new(
            WidgetBuilder::new()
                .with_margin(Thickness::top(24.0))
                .with_foreground(Brush::Solid(Color::opaque(190, 190, 200)).into()),
        )
        .with_text(maps_text())
        .with_font_size(16.0.into())
        .with_horizontal_text_alignment(HorizontalAlignment::Center)
        .build(ctx);
        let items = std::iter::once(maps_title.to_base())
            .chain(maps.iter().map(|map| map.to_base()))
            .chain([back.to_base(), maps_about.to_base()]);
        let maps_page = page(ctx, false, items);
        let backdrop = BorderBuilder::new(
            WidgetBuilder::new()
                .with_background(Brush::Solid(Color::opaque(8, 10, 14)).into())
                .with_child(main_page)
                .with_child(maps_page),
        )
        .with_stroke_thickness(Thickness::uniform(0.0).into())
        .build(ctx);
        let screen = ScreenBuilder::new(WidgetBuilder::new().with_visibility(false).with_child(backdrop))
            .build(ctx);
        Self {
            screen,
            maze,
            ctf,
            quit,
            main_page,
            maps_page,
            maps,
            back,
            picking: Picking {
                pages: vec![
                    vec![maze_button, ctf_button, quit_button],
                    map_buttons.into_iter().chain([back_button]).collect(),
                ],
                ..Picking::default()
            },
        }
    }

    /// Shows or hides the menu, on its own page.
    pub fn set_open(&mut self, ui: &UserInterface, open: bool) {
        ui.send(self.screen, WidgetMessage::Visibility(open));
        self.show_maps(ui, false);
        if !open {
            self.picking.leave(ui);
        }
    }

    /// Shows the maps for capture the flag, or the menu's own page.
    pub fn show_maps(&mut self, ui: &UserInterface, maps: bool) {
        ui.send(self.main_page, WidgetMessage::Visibility(!maps));
        ui.send(self.maps_page, WidgetMessage::Visibility(maps));
        self.picking.show(ui, usize::from(maps), |_| true);
    }

    /// Goes up and down the menu, or presses what is picked, for `code`: whether it did.
    pub fn key(&mut self, ui: &UserInterface, code: KeyCode) -> bool {
        self.picking.key(ui, code, |_| true)
    }

    /// Follows the keyboard's focus as `message` moves it.
    pub fn observe(&mut self, ui: &UserInterface, message: &UiMessage) {
        self.picking.observe(ui, message);
    }

    /// What `message` picks from the menu, if anything.
    pub fn choice(&self, message: &UiMessage) -> Option<Start> {
        let maps = (self.maps.iter().enumerate()).map(|(n, &map)| (map, Start::Play(Game::CaptureTheFlag(n))));
        [
            (self.maze, Start::Play(Game::Maze)),
            (self.ctf, Start::Maps),
            (self.back, Start::Back),
            (self.quit, Start::Quit),
        ]
        .into_iter()
        .chain(maps)
        .find(|&(button, _)| matches!(message.data_from(button), Some(ButtonMessage::Click)))
        .map(|(_, choice)| choice)
    }
}

/// A line about each map, under the maps' buttons.
fn maps_text() -> String {
    let lines: Vec<_> = MAPS.iter().map(|map| format!("{}: {}", map.name, map.about)).collect();
    lines.join("\n")
}

#[derive(Debug, Default, PartialEq)]
pub struct PauseMenu {
    screen: Handle<Screen>,
    resume: Handle<Button>,
    lights: Handle<Button>,
    lights_label: Handle<Text>,
    options: Handle<Button>,
    restart: Handle<Button>,
    main_menu: Handle<Button>,
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
    /// Whether there is a level to start again yet.
    can_restart: bool,
    picking: Picking,
}

impl PauseMenu {
    /// Builds the menu, hidden, over everything else in `ui`. `restart` is what starting again
    /// is called: a new maze, or a new round in the same one.
    pub fn build(ui: &mut UserInterface, restart: &str) -> Self {
        let ctx = &mut ui.build_ctx();
        let paused_title = title(ctx, "Paused");
        let main_buttons = [
            button(ctx, "Resume"),
            button(ctx, &lights_text(true)),
            button(ctx, "Options"),
            button(ctx, restart),
            button(ctx, "Main menu"),
            button(ctx, "Quit"),
        ];
        let [(resume, _), (lights, lights_label), (options, _), (restart, _), (main_menu, _), (quit, _)] =
            main_buttons;
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
                main_menu.to_base(),
                quit.to_base(),
                controls.to_base(),
            ],
        );
        let options_title = title(ctx, "Options");
        let subtitles = Subtitles::default();
        let options_buttons = [
            button(ctx, &latin_text(subtitles.latin)),
            button(ctx, &english_text(subtitles.english)),
            button(ctx, "Back"),
        ];
        let [(latin, latin_label), (english, english_label), (back, _)] = options_buttons;
        let options_page = page(
            ctx,
            false,
            [
                options_title.to_base(),
                latin.to_base(),
                english.to_base(),
                back.to_base(),
            ],
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
            main_menu,
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
            can_restart: false,
            picking: Picking {
                pages: vec![main_buttons.to_vec(), options_buttons.to_vec()],
                ..Picking::default()
            },
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
        self.can_restart = can_restart;
        ui.send(self.screen, WidgetMessage::Visibility(open));
        ui.send(self.restart, WidgetMessage::Enabled(can_restart));
        self.set_in_options(ui, false);
        if !open {
            self.picking.leave(ui);
        }
    }

    /// Shows the options, or the menu's own page.
    pub fn set_in_options(&mut self, ui: &UserInterface, in_options: bool) {
        self.in_options = in_options;
        ui.send(self.main_page, WidgetMessage::Visibility(!in_options));
        ui.send(self.options_page, WidgetMessage::Visibility(in_options));
        let (restart, can_restart) = (self.restart, self.can_restart);
        self.picking.show(ui, usize::from(in_options), |button| button != restart || can_restart);
    }

    /// Goes up and down the menu, or presses what is picked, for `code`, while it is open:
    /// whether it did.
    pub fn key(&mut self, ui: &UserInterface, code: KeyCode) -> bool {
        let (restart, can_restart) = (self.restart, self.can_restart);
        self.open && self.picking.key(ui, code, |button| button != restart || can_restart)
    }

    /// Follows the keyboard's focus as `message` moves it.
    pub fn observe(&mut self, ui: &UserInterface, message: &UiMessage) {
        self.picking.observe(ui, message);
    }

    /// Shows which subtitles are on.
    pub fn set_subtitles(&self, ui: &UserInterface, subtitles: Subtitles) {
        ui.send(
            self.latin_label,
            TextMessage::Text(latin_text(subtitles.latin)),
        );
        ui.send(
            self.english_label,
            TextMessage::Text(english_text(subtitles.english)),
        );
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
            (self.main_menu, Choice::MainMenu),
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

#[cfg(test)]
mod tests {
    use super::*;
    use fyrox::core::algebra::Vector2;

    fn click(button: Handle<Button>) -> UiMessage {
        UiMessage::from_widget(button, ButtonMessage::Click)
    }

    #[test]
    fn capture_the_flag_opens_the_maps_and_each_map_plays_its_own() {
        let mut ui = UserInterface::new(Vector2::new(800.0, 600.0));
        let menu = MainMenu::build(&mut ui);
        assert_eq!(menu.maps.len(), MAPS.len());
        assert_eq!(menu.choice(&click(menu.ctf)), Some(Start::Maps));
        assert_eq!(menu.choice(&click(menu.back)), Some(Start::Back));
        assert_eq!(menu.choice(&click(menu.maze)), Some(Start::Play(Game::Maze)));
        for (n, &map) in menu.maps.iter().enumerate() {
            assert_eq!(menu.choice(&click(map)), Some(Start::Play(Game::CaptureTheFlag(n))));
        }
    }

    /// Hands every message waiting in `ui` to both menus, as the game does: what they make of
    /// each, and the buttons clicked.
    fn settle(ui: &mut UserInterface, main: &mut MainMenu, pause: &mut PauseMenu) -> Vec<Handle<Button>> {
        ui.update(Vector2::new(800.0, 600.0), 0.016, &Default::default());
        let mut clicked = Vec::new();
        while let Some(message) = ui.poll_message() {
            main.observe(ui, &message);
            pause.observe(ui, &message);
            if matches!(message.data::<ButtonMessage>(), Some(ButtonMessage::Click))
                && message.direction() == fyrox::gui::message::MessageDirection::FromWidget
            {
                clicked.push(message.destination().transmute());
            }
        }
        clicked
    }

    /// Enter pressed on the keyboard, through the UI as the window has it, and to the menu.
    fn enter(ui: &mut UserInterface, menu: &mut MainMenu) {
        ui.process_os_event(&fyrox::gui::message::OsEvent::KeyboardInput {
            button: fyrox::gui::message::KeyCode::Enter,
            state: fyrox::gui::message::ButtonState::Pressed,
            text: String::new(),
        });
        menu.key(ui, KeyCode::Enter);
    }

    #[test]
    fn the_keys_go_up_and_down_the_main_menu_and_enter_presses_once() {
        let mut ui = UserInterface::new(Vector2::new(800.0, 600.0));
        let mut main = MainMenu::build(&mut ui);
        let mut pause = PauseMenu::build(&mut ui, "Start again");
        main.set_open(&ui, true);
        settle(&mut ui, &mut main, &mut pause);
        assert_eq!(main.picking.focused, main.maze, "opened, the first is picked and focused");
        assert!(main.key(&ui, KeyCode::ArrowDown));
        settle(&mut ui, &mut main, &mut pause);
        assert_eq!(main.picking.focused, main.ctf);
        enter(&mut ui, &mut main);
        assert_eq!(settle(&mut ui, &mut main, &mut pause), [main.ctf], "pressed once");
        // On the maps: down past the last goes round to the first.
        main.show_maps(&ui, true);
        settle(&mut ui, &mut main, &mut pause);
        for _ in 0..MAPS.len() + 1 {
            main.key(&ui, KeyCode::KeyS);
        }
        settle(&mut ui, &mut main, &mut pause);
        assert_eq!(main.picking.focused, main.maps[0]);
        assert!(!main.key(&ui, KeyCode::KeyQ), "other keys are the game's");
    }

    #[test]
    fn enter_presses_what_is_picked_even_without_the_focus() {
        let mut ui = UserInterface::new(Vector2::new(800.0, 600.0));
        let mut main = MainMenu::build(&mut ui);
        let mut pause = PauseMenu::build(&mut ui, "Start again");
        main.set_open(&ui, true);
        settle(&mut ui, &mut main, &mut pause);
        // Clicked away from the buttons, say.
        ui.send(main.maze, WidgetMessage::Unfocus);
        settle(&mut ui, &mut main, &mut pause);
        enter(&mut ui, &mut main);
        assert_eq!(settle(&mut ui, &mut main, &mut pause), [main.maze]);
    }

    #[test]
    fn the_pause_menu_steps_over_starting_again_with_nothing_to_start_again() {
        let mut ui = UserInterface::new(Vector2::new(800.0, 600.0));
        let mut main = MainMenu::build(&mut ui);
        let mut pause = PauseMenu::build(&mut ui, "Start again");
        assert!(!pause.key(&ui, KeyCode::ArrowDown), "closed, it leaves the keys be");
        pause.set_open(&ui, true, false);
        settle(&mut ui, &mut main, &mut pause);
        for _ in 0..3 {
            pause.key(&ui, KeyCode::ArrowDown);
        }
        settle(&mut ui, &mut main, &mut pause);
        assert_eq!(pause.picking.focused, pause.main_menu, "past Start again");
        // Up from the top goes round to Quit.
        pause.set_open(&ui, true, true);
        pause.key(&ui, KeyCode::ArrowUp);
        settle(&mut ui, &mut main, &mut pause);
        assert_eq!(pause.picking.focused, pause.quit);
        pause.set_open(&ui, false, true);
        settle(&mut ui, &mut main, &mut pause);
        assert_eq!(pause.picking.focused, Handle::<Button>::NONE, "closed, nothing keeps the focus");
    }

    #[test]
    fn the_maps_show_in_place_of_the_menus_own_page() {
        let mut ui = UserInterface::new(Vector2::new(800.0, 600.0));
        let mut menu = MainMenu::build(&mut ui);
        let (main_page, maps_page) = (menu.main_page, menu.maps_page);
        let showing = |ui: &mut UserInterface| {
            while ui.poll_message().is_some() {}
            (ui[main_page].visibility(), ui[maps_page].visibility())
        };
        assert_eq!(showing(&mut ui), (true, false));
        menu.show_maps(&ui, true);
        assert_eq!(showing(&mut ui), (false, true));
        // Opened again, it is back on its own page.
        menu.set_open(&ui, true);
        assert_eq!(showing(&mut ui), (true, false));
    }
}
