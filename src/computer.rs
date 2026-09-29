//! A computer in the maze to hack, as in Welcome to the Game: walk up to it and press E, and the
//! view goes in close on its screen, where a terminal in green text asks for a breach. Each
//! breach is a run of commands to type, one at a time, each against the clock of a trace: typed
//! out exactly before the trace completes, the next comes up; a wrong key does not go in, and
//! costs the trace time. Six typed, access is granted. If the trace completes first, access is
//! denied, and Enter starts again at once. Tab walks away.
//!
//! The monitor's frame glows red while the computer is locked and blue once it is cleared. For
//! now it only stands near where the player starts, to try the hacking out; it opens nothing.
//!
//! The terminal is on the screen itself while the player uses the computer: an interface of its
//! own, in DejaVu Sans Mono, drawn into a texture the screen glows with. The engine leaves such a
//! texture blank if the scene looks for it before the interface has first drawn into it, so each
//! use gets a fresh texture, and the screen only takes it up once the interface has drawn into it
//! ([`ScreenTerminal`]). Otherwise the screen just glows a faint green. With
//! MAZE_TERMINAL_OVERLAY=1 the terminal comes up over the screen instead, as a panel of the
//! game's own interface, as big as the screen is in the view.
//!
//! Once cleared, the computer's files are open to read: notes and diary entries, shared out among
//! the maze's computers (see [`crate::notes`]). Up and down pick one, Enter opens it, up and down
//! scroll it and Backspace goes back to the listing.
//!
//! Every key beeps from the screen, made from the formants in [`COMPUTER_SOUNDS`]: lightly for
//! one that goes in, low and buzzing for one that does not, twice rising for a command typed out.

use crate::{
    dismember::SPILL,
    formants::{self, Sounds},
    layout::{Rng, WalkGrid},
    notes::{self, Entry},
    ragdoll::CHARACTERS,
    survey::{self, CELL_SIZE},
};
use fyrox::{
    asset::untyped::ResourceKind,
    core::{
        algebra::{Point3, UnitQuaternion, Vector2, Vector3},
        color::Color,
        pool::Handle,
        uuid::Uuid,
    },
    graph::SceneGraph,
    gui::{
        border::{Border, BorderBuilder},
        brush::Brush,
        font::{Font, FontResource, FontStyles},
        grid::{Column, GridBuilder, Row},
        text::{Text, TextBuilder, TextMessage},
        widget::{WidgetBuilder, WidgetMessage},
        HorizontalAlignment, Thickness, UiContainer, UserInterface, VerticalAlignment,
    },
    material::{Material, MaterialResource},
    resource::{
        model::{ModelResource, ModelResourceExtension},
        texture::{TextureResource, TextureResourceExtension},
    },
    scene::{
        base::BaseBuilder,
        collider::{BitMask, Collider, ColliderBuilder, ColliderShape, InteractionGroups},
        graph::{physics::RayCastOptions, Graph},
        mesh::{
            buffer::{VertexAttributeUsage, VertexReadTrait},
            Mesh,
        },
        node::Node,
        rigidbody::{RigidBodyBuilder, RigidBodyType},
        sound::{SoundBufferResource, SoundBuilder, Status},
        transform::TransformBuilder,
        Scene,
    },
};

/// The computer's model: a monitor and keyboard floating against a wall, made in Blender from
/// unlockables/computer.blend by build_computer.py. It faces its +Z, from the foot of the wall
/// straight below the middle of the monitor's back.
pub const COMPUTER_MODEL: &str = "data/computer.glb";
/// The beeps it makes as it is typed at.
pub const COMPUTER_SOUNDS: &str = "data/sounds/computer_formants.json";
/// How far a key's beep goes up or down from the next key's, as a share of its pitch, so that
/// typing does not drone.
const BEEP_SPREAD: f64 = 0.03;
/// Its screen, and the rest of the monitor, whose frame glows.
const SCREEN: &str = "computer_screen";
const FRAME: &str = "computer_frame";
/// The terminal's font: DejaVu Sans Mono (see data/fonts/DejaVuSansMono.LICENSE), carried in the
/// game itself.
pub(crate) static FONT: &[u8] = include_bytes!("../data/fonts/DejaVuSansMono.ttf");

/// The terminal panel, which covers the screen as the camera sees it: how many lines of height it
/// has room for, the output with the box typed into and the keys under that; how many characters
/// go across it, at most; how far in the text is from its edges, as a share of its height; and,
/// for the font, how wide a character is and how far apart the lines are, for its size.
const PANEL_LINES: f32 = 17.0;
const PANEL_COLUMNS: f32 = 40.0;
const PANEL_MARGIN: f32 = 0.05;
const CHAR_WIDTH: f32 = 0.61;
const LINE_SPACING: f32 = 1.25;
/// The terminal's colours: its background, and its greens - what is written, what is still to be
/// typed, what stands out - and the white a wrong key flashes; and the faint green the screen
/// glows with.
const BACKGROUND: [u8; 3] = [0, 6, 2];
const GREEN: [u8; 3] = [60, 255, 110];
const DIM: [u8; 3] = [29, 122, 58];
const BRIGHT: [u8; 3] = [141, 255, 176];
const WHITE: [u8; 3] = [255, 255, 255];
///
/// The engine lights what glows by its colour as well - a surface glows as brightly as its glow
/// times its own colour - so a black surface cannot glow at all: the screen idles a faint green
/// as its colour, and the terminal on it is its colour as well as its glow.
const SCREEN_COLOUR: Color = Color::opaque(0, 44, 18);
const SCREEN_GLOW: f32 = 1.0;
/// The terminal on the screen itself: its picture, in pixels, as wide for its height as the
/// screen; how brightly the screen glows with it; and how many frames the interface draws into a
/// fresh picture before the screen takes it up.
const PICTURE: (u32, u32) = (720, 468);
const PICTURE_GLOW: f32 = 1.4;
const DRAWN_BEFORE_SHOWN: u32 = 2;
/// The frame's colour, locked and cleared - red and blue - and how brightly it glows with it.
const LOCKED_COLOUR: Color = Color::opaque(255, 28, 28);
const CLEARED_COLOUR: Color = Color::opaque(40, 110, 255);
const FRAME_GLOW: f32 = 2.0;

/// The room the monitor and keyboard take up against the wall, in meters: half as wide as they
/// are, half as high, and half as far out from the wall; and how high the middle of that is.
const BULK_HALF: Vector3<f32> = Vector3::new(0.29, 0.24, 0.1);
const BULK_HEIGHT: f32 = 1.23;
/// How far behind the cell it goes by a wall is looked for, in meters, and how high up.
const WALL_REACH: f32 = 2.0;
const WALL_LOOK_HEIGHT: f32 = 1.2;
/// How far off the wall the monitor's back floats, in meters.
const WALL_CLEARANCE: f32 = 0.005;
/// How far from the screen, along the ground, the player can use it, and how far in front of it
/// they must be.
const REACH: f32 = 1.6;
const IN_FRONT: f32 = 0.2;
/// Where it goes: on open floor this many cells of walking from the start, nearest first, with a
/// wall right behind and this many cells of floor in front.
const FROM_START: (u32, u32) = (3, 16);
const ROOM_IN_FRONT: usize = 4;
/// Where the rest go: at least this many cells of walking from the start, and at least this many
/// cells from each other as the crow flies, if there is room for that.
const AWAY_FROM_START: u32 = 20;
const APART: f32 = 60.0;
/// How many files the listing shows at once; and how many lines of a file show at once, and how
/// wide they are, in characters.
const LISTED: usize = 7;
pub const PAGE: usize = 10;
const PAGE_WIDTH: usize = 38;

/// The commands a breach is typed from, as a hacker would type them.
const COMMANDS: &[&str] = &[
    "ssh root@node7f.maze",
    "sudo chmod 777 /sys/core",
    "nmap -sS 10.0.7.0/24",
    "cat /etc/shadow",
    "inject --payload=ghost.bin",
    "decrypt kernel.key",
    "bypass auth.pam",
    "kill -9 1337",
    "mount /dev/sdb1 /mnt/vault",
    "exec rootkit.sh",
    "tail -f /var/log/trace",
    "ping -f 10.0.7.1",
    "override firewall.cfg",
    "spoof mac 0e:1a:77:c3",
    "grep -r passwd /home",
    "ncat -lvp 4444",
    "rm -rf /var/log/audit",
    "hexdump core.img",
    "unlock --node=7F3A",
    "export PATH=/tmp/x",
];
/// How many commands a breach takes; how long the trace gives each, in seconds, however long it
/// is; and how much a wrong key takes off it.
const BREACH_LINES: usize = 6;
const LINE_TIME: f32 = 15.0;
const WRONG_KEY: f32 = 0.5;
/// How long a wrong key shows, and how often the cursor blinks, in seconds.
const FLASH: f32 = 0.25;
const BLINK: f32 = 0.5;
/// How little of the trace has to be left for its bar to flash, as a share of all of it.
const URGENT: f32 = 0.3;
/// How the terminal panel comes and goes: how long it takes to fade in or out, in seconds; how
/// fast a new screen types itself out, in characters a second; and how long a wrong key jolts it
/// for, in seconds, and how far, as a share of its height.
const FADE: f32 = 0.25;
const TYPE_OUT: f32 = 240.0;
const JOLT: f32 = 0.15;
const JOLT_SIZE: f32 = 0.012;

/// How far the hack has got.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Waiting for a breach.
    #[default]
    Locked,
    /// Typing the commands against the trace.
    Breaching,
    /// The trace completed first.
    Denied,
    /// Every command typed.
    Cleared,
}

/// What a key typed at the terminal did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keystroke {
    /// Nothing: there is no breach under way.
    Ignored,
    /// It went in.
    Right,
    /// It went in, and finished the command.
    Line,
    /// It was not the next character, and cost the trace time.
    Wrong,
}

/// A hack: the commands of the breach under way, how far through them it is, and the trace.
#[derive(Debug, Clone, PartialEq)]
pub struct Hack {
    stage: Stage,
    lines: Vec<&'static str>,
    /// Which command is being typed, and how many of its characters are.
    at: usize,
    typed: usize,
    /// How long the trace has left, and how long it had, in seconds.
    left: f32,
    limit: f32,
    /// How long ago a key went wrong, as a countdown.
    flash: f32,
    dice: u64,
    /// The node it says it is.
    node: [u8; 3],
    /// Its files, to read once it is cleared; which is picked in the listing; and which is open,
    /// if any, wrapped to the screen, and how far down it has been scrolled, in lines.
    files: Vec<Entry>,
    picked: usize,
    reading: Option<(usize, Vec<String>)>,
    scroll: usize,
}

/// What the terminal is showing, for it to type itself out afresh when that changes: the stage
/// of the hack, and which file is open, if any.
pub type View = (Stage, Option<usize>);

impl Hack {
    pub fn new(seed: u64) -> Self {
        Self {
            stage: Stage::Locked,
            lines: Vec::new(),
            at: 0,
            typed: 0,
            left: 0.0,
            limit: 0.0,
            flash: 0.0,
            dice: seed | 1,
            node: [(seed >> 8) as u8, (seed >> 24) as u8, (seed >> 40) as u8],
            files: Vec::new(),
            picked: 0,
            reading: None,
            scroll: 0,
        }
    }

    /// What it is showing, for the terminal.
    pub fn view(&self) -> View {
        (self.stage, self.reading.as_ref().map(|(n, _)| *n))
    }

    /// Gives it `files` to read once it is cleared, in place of any it had.
    pub fn set_files(&mut self, files: Vec<Entry>) {
        self.files = files;
        self.picked = 0;
        self.reading = None;
        self.scroll = 0;
    }

    /// Up or down `by`, once it is cleared: through the listing, or down the file that is open.
    /// Whether it went anywhere.
    pub fn step(&mut self, by: i32) -> bool {
        if self.stage != Stage::Cleared {
            return false;
        }
        let (at, most) = match &self.reading {
            Some((_, lines)) => (&mut self.scroll, lines.len().saturating_sub(PAGE)),
            None => (&mut self.picked, self.files.len().saturating_sub(1)),
        };
        let to = at.saturating_add_signed(by as isize).min(most);
        std::mem::replace(at, to) != to
    }

    /// Backspace, once it is cleared: closes the file that is open. Whether there was one.
    pub fn back(&mut self) -> bool {
        self.stage == Stage::Cleared && self.reading.take().is_some()
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// A number below `n`, from the hack's dice.
    fn below(&mut self, n: usize) -> usize {
        // xorshift64*
        self.dice ^= self.dice >> 12;
        self.dice ^= self.dice << 25;
        self.dice ^= self.dice >> 27;
        ((self.dice.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize) % n
    }

    /// Enter: starts a breach, from locked or denied, or once cleared opens the file picked in
    /// the listing. Nothing otherwise. Whether it did.
    pub fn enter(&mut self) -> bool {
        if self.stage == Stage::Cleared {
            let Some(file) = self.files.get(self.picked).filter(|_| self.reading.is_none()) else {
                return false;
            };
            self.reading = Some((self.picked, notes::wrap(&file.text, PAGE_WIDTH)));
            self.scroll = 0;
            return true;
        }
        if !matches!(self.stage, Stage::Locked | Stage::Denied) {
            return false;
        }
        let mut lines: Vec<&'static str> = Vec::with_capacity(BREACH_LINES);
        while lines.len() < BREACH_LINES.min(COMMANDS.len()) {
            let line = COMMANDS[self.below(COMMANDS.len())];
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
        self.lines = lines;
        self.stage = Stage::Breaching;
        self.at = 0;
        self.next_line();
        true
    }

    fn next_line(&mut self) {
        self.typed = 0;
        self.limit = LINE_TIME;
        self.left = self.limit;
    }

    /// `c` typed: the next character of the command goes in if it is that, and otherwise the
    /// trace is that much further on.
    pub fn type_char(&mut self, c: char) -> Keystroke {
        if self.stage != Stage::Breaching {
            return Keystroke::Ignored;
        }
        let Some(line) = self.lines.get(self.at) else {
            return Keystroke::Ignored;
        };
        if line.chars().nth(self.typed) == Some(c) {
            self.typed += 1;
            if self.typed < line.chars().count() {
                return Keystroke::Right;
            }
            self.at += 1;
            if self.at == self.lines.len() {
                self.stage = Stage::Cleared;
            } else {
                self.next_line();
            }
            Keystroke::Line
        } else {
            self.flash = FLASH;
            self.left -= WRONG_KEY;
            if self.left <= 0.0 {
                self.stage = Stage::Denied;
            }
            Keystroke::Wrong
        }
    }

    /// Runs the trace on for another `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.flash = (self.flash - dt).max(0.0);
        if self.stage == Stage::Breaching {
            self.left -= dt;
            if self.left <= 0.0 {
                self.left = 0.0;
                self.stage = Stage::Denied;
            }
        }
    }

    /// What the terminal shows, with the cursor showing or not: its output, line by line, each a
    /// run of pieces of text in their colours; what has been typed, in the box below it; and what
    /// the keys do, under that.
    fn screen(&self, cursor: bool) -> Screen {
        let bar = |left: f32, limit: f32| {
            const CELLS: usize = 20;
            let full = ((left / limit.max(1.0e-3)) * CELLS as f32)
                .ceil()
                .clamp(0.0, CELLS as f32) as usize;
            format!("{}{}", "█".repeat(full), "░".repeat(CELLS - full))
        };
        let cursor_mark = if cursor { "█" } else { " " };
        let plain = |text: &str| vec![(text.to_string(), GREEN)];
        let bright = |text: &str| vec![(text.to_string(), BRIGHT)];
        let [a, b, c] = self.node;
        let mut output: Vec<Line> = vec![
            plain("MAZE-NET SECURE TERMINAL v2.3"),
            plain(&format!("NODE {a:02X}:{b:02X}:{c:02X}")),
            Vec::new(),
        ];
        // The box holds only what the player types, and it only takes typing mid-breach.
        let mut typed = String::new();
        match self.stage {
            Stage::Locked => {
                output.extend([
                    plain("STATUS ...... LOCKED"),
                    Vec::new(),
                    plain("> ROOT ACCESS REQUIRED"),
                    plain("> TYPE EACH COMMAND"),
                    plain("  BEFORE THE TRACE COMPLETES"),
                    Vec::new(),
                    bright("PRESS ENTER TO BREACH"),
                ]);
            }
            Stage::Breaching => {
                output.push(plain(&format!(
                    "BREACH {}/{}",
                    self.at + 1,
                    self.lines.len()
                )));
                output.push(Vec::new());
                for done in &self.lines[..self.at] {
                    output.push(vec![(format!("$ {done}  OK"), DIM)]);
                }
                // The command to type, lit up as far as it has been typed.
                let line = self.lines[self.at];
                let split = line
                    .char_indices()
                    .nth(self.typed)
                    .map_or(line.len(), |(i, _)| i);
                let (done, rest) = line.split_at(split);
                let rest_colour = if self.flash > 0.0 { WHITE } else { GREEN };
                output.push(vec![
                    ("> ".to_string(), GREEN),
                    (done.to_string(), BRIGHT),
                    (rest.to_string(), rest_colour),
                ]);
                output.push(Vec::new());
                // Running out, the trace bar flashes, in time with the cursor.
                let urgent = self.left < URGENT * self.limit;
                let bar_colour = if urgent && cursor { WHITE } else { BRIGHT };
                let err = if self.flash > 0.0 { "  ERR" } else { "" };
                output.push(vec![
                    ("TRACE ".to_string(), GREEN),
                    (bar(self.left, self.limit), bar_colour),
                    (format!(" {:.1}s", self.left), GREEN),
                    (err.to_string(), WHITE),
                ]);
                typed = done.to_string();
            }
            Stage::Denied => {
                output.extend([
                    plain("TRACE COMPLETE"),
                    Vec::new(),
                    bright("ACCESS DENIED"),
                    Vec::new(),
                    plain("CONNECTION LOGGED"),
                    Vec::new(),
                    bright("PRESS ENTER TO RETRY"),
                ]);
            }
            Stage::Cleared => match &self.reading {
                // The file open: its name, when it was written or what it is, and as much of it
                // as fits from where it has been scrolled to.
                Some((n, lines)) => {
                    let file = &self.files[*n];
                    let about = file.date.clone().unwrap_or_else(|| file.kind.to_uppercase());
                    output = vec![
                        bright(&clipped(&file.title, PANEL_COLUMNS as usize)),
                        vec![(clipped(&about, PANEL_COLUMNS as usize), DIM)],
                        Vec::new(),
                    ];
                    let end = (self.scroll + PAGE).min(lines.len());
                    output.extend(lines[self.scroll..end].iter().map(|line| plain(line)));
                    typed = clipped(&format!("cat {}", file.title), PANEL_COLUMNS as usize - 3);
                }
                // The listing, as much of it as fits round the one picked.
                None => {
                    output.extend([bright("ACCESS GRANTED"), Vec::new()]);
                    if self.files.is_empty() {
                        output.push(plain("NO FILES"));
                    } else {
                        output.push(plain(&format!("FILES: {}", self.files.len())));
                        let first = self.picked.saturating_sub(LISTED - 1);
                        for (n, file) in self.files.iter().enumerate().skip(first).take(LISTED) {
                            let (mark, colour) = match n == self.picked {
                                true => ("> ", BRIGHT),
                                false => ("  ", GREEN),
                            };
                            let (title, kind) = (clipped(&file.title, 24), clipped(&file.kind, 8));
                            let row = format!("{mark}{title:<24} {:>8}", kind.to_uppercase());
                            output.push(vec![(row, colour)]);
                        }
                    }
                    typed = "ls".to_string();
                }
            },
        }
        let input = vec![
            ("$ ".to_string(), GREEN),
            (typed, BRIGHT),
            (cursor_mark.to_string(), GREEN),
        ];
        let keys = match self.stage {
            Stage::Locked => "ENTER  BREACH    TAB  LEAVE",
            Stage::Breaching => "TYPE THE COMMAND    TAB  LEAVE",
            Stage::Denied => "ENTER  RETRY    TAB  LEAVE",
            Stage::Cleared => match &self.reading {
                Some((_, lines)) if self.scroll + PAGE < lines.len() => {
                    "↑↓ SCROLL  BKSP BACK  TAB LEAVE  ▼ MORE"
                }
                Some(_) => "↑↓ SCROLL  BKSP BACK  TAB LEAVE",
                None if self.files.is_empty() => "TAB  LEAVE",
                None => "↑↓ PICK  ENTER READ  TAB LEAVE",
            },
        };
        Screen {
            output,
            input,
            keys: vec![(keys.to_string(), DIM)],
        }
    }
}

/// The letter `c` as it would have been typed without Caps Lock: a capital with `shift` held and
/// small without, whichever way Caps Lock is. Anything but a letter is left as it is.
pub fn without_caps_lock(c: char, shift: bool) -> char {
    match (c.is_alphabetic(), shift) {
        (false, _) => c,
        (true, true) => c.to_uppercase().next().unwrap_or(c),
        (true, false) => c.to_lowercase().next().unwrap_or(c),
    }
}

/// `text`, cut down to at most `width` characters.
fn clipped(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

/// What the terminal shows: its output, what has been typed in the box below it, and what the keys
/// do, under that.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Screen {
    output: Vec<Line>,
    input: Line,
    keys: Line,
}

/// A line of the terminal: pieces of text, each in its colour.
pub type Line = Vec<(String, [u8; 3])>;

/// What the terminal panel is to show: the screen of a hack, where the corners of the computer's
/// screen are in the view, what it is showing, and whether a key has just gone wrong.
pub type Showing = (Screen, [Vector2<f32>; 4], View, bool);

/// The terminal panel, shown over the computer's screen while the player uses it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Terminal {
    panel: Handle<Border>,
    /// Its output, the box typed into and what is typed in it, and what the keys do.
    text: Handle<Text>,
    input_box: Handle<Border>,
    input: Handle<Text>,
    keys: Handle<Text>,
    /// Whether it is showing, what it shows, and where it is and how big, in pixels, so that each
    /// is only sent when it changes.
    open: bool,
    shown: Screen,
    place: [f32; 4],
    /// How far it has faded in, from 0 to 1, and what it showed last, to fade out with.
    faded: f32,
    last: Option<(Screen, [Vector2<f32>; 4])>,
    /// What it shows, and how many characters of it have typed themselves out so far.
    view: Option<View>,
    typed_out: f32,
    /// How long the jolt of a wrong key has left, in seconds, and whether a key was wrong last
    /// frame.
    jolt: f32,
    wrong: bool,
}

impl Terminal {
    /// Builds the panel, hidden, in `ui`.
    pub fn build(ui: &mut UserInterface) -> Self {
        let font = Font::from_memory(FONT, 1024, FontStyles::default(), Vec::new())
            .ok()
            .map(|font| FontResource::new_ok(Uuid::new_v4(), ResourceKind::Embedded, font));
        let ctx = &mut ui.build_ctx();
        let colour = |[r, g, b]: [u8; 3]| Brush::Solid(Color::opaque(r, g, b));
        let mut text_on = |row: usize, margin: Thickness| {
            let mut text = TextBuilder::new(
                WidgetBuilder::new()
                    .on_row(row)
                    .with_margin(margin)
                    .with_foreground(colour(GREEN).into()),
            );
            if let Some(font) = font.clone() {
                text = text.with_font(font);
            }
            text.build(ctx)
        };
        // The output fills the panel down to the box typed into; the keys go under the box.
        let text = text_on(0, Thickness::zero());
        let input = text_on(0, Thickness::zero());
        let keys = text_on(2, Thickness::zero());
        let input_box = BorderBuilder::new(
            WidgetBuilder::new()
                .on_row(1)
                .with_foreground(colour(DIM).into())
                .with_child(input),
        )
        .with_stroke_thickness(Thickness::uniform(1.0).into())
        .build(ctx);
        let text_grid = GridBuilder::new(
            WidgetBuilder::new()
                .with_child(text)
                .with_child(input_box)
                .with_child(keys),
        )
        .add_row(Row::stretch())
        .add_row(Row::auto())
        .add_row(Row::auto())
        .add_column(Column::stretch())
        .build(ctx);
        let [r, g, b] = BACKGROUND;
        // Put where it goes by how far it is from the top left of the window.
        let panel = BorderBuilder::new(
            WidgetBuilder::new()
                .with_visibility(false)
                .with_hit_test_visibility(false)
                .with_horizontal_alignment(HorizontalAlignment::Left)
                .with_vertical_alignment(VerticalAlignment::Top)
                .with_background(Brush::Solid(Color::opaque(r, g, b)).into())
                .with_foreground(Brush::Solid(Color::opaque(DIM[0], DIM[1], DIM[2])).into())
                .with_child(text_grid),
        )
        .with_stroke_thickness(Thickness::uniform(2.0).into())
        .build(ctx);
        Self {
            panel,
            text,
            input_box,
            input,
            keys,
            ..Default::default()
        }
    }

    /// Shows `lines` of a hack at `stage` over the screen, whose corners are at `corners` in the
    /// view, in pixels from its top left, with a key just gone `wrong` or not; or with none puts
    /// the panel away. The panel covers the screen, and the text is as big as fits in it. It
    /// fades in and out over another `dt`, each new stage types itself out, and a wrong key jolts
    /// it.
    pub fn show(&mut self, ui: &UserInterface, shown: Option<Showing>, dt: f32) {
        let wanted = if shown.is_some() { 1.0 } else { 0.0 };
        self.faded += (wanted - self.faded).clamp(-dt / FADE, dt / FADE);
        let open = self.faded > 0.0;
        if open != self.open {
            ui.send(self.panel, WidgetMessage::Visibility(open));
            self.open = open;
        }
        if !open {
            self.last = None;
            self.view = None;
            return;
        }
        ui.send(self.panel, WidgetMessage::Opacity(Some(self.faded)));
        let (screen, corners) = match shown {
            Some((screen, corners, view, wrong)) => {
                if self.view != Some(view) {
                    self.view = Some(view);
                    self.typed_out = 0.0;
                }
                if wrong && !self.wrong {
                    self.jolt = JOLT;
                }
                self.wrong = wrong;
                self.last = Some((screen.clone(), corners));
                (screen, corners)
            }
            // Fading out, as it last was.
            None => match self.last.clone() {
                Some(last) => last,
                None => return,
            },
        };
        self.typed_out += TYPE_OUT * dt;
        self.jolt = (self.jolt - dt).max(0.0);

        let (left, top) = corners
            .iter()
            .fold((f32::MAX, f32::MAX), |(x, y), c| (x.min(c.x), y.min(c.y)));
        let (right, bottom) = corners
            .iter()
            .fold((f32::MIN, f32::MIN), |(x, y), c| (x.max(c.x), y.max(c.y)));
        let (width, height) = (
            (right - left).round().max(1.0),
            (bottom - top).round().max(1.0),
        );
        let shake = if self.jolt > 0.0 {
            (self.jolt / JOLT * std::f32::consts::PI * 6.0).sin() * JOLT_SIZE * height
        } else {
            0.0
        };
        let place = [(left + shake).round(), top.round(), width, height];
        if place != self.place {
            let [x, y, width, height] = place;
            ui.send(
                self.panel,
                WidgetMessage::Margin(Thickness {
                    left: x,
                    top: y,
                    right: 0.0,
                    bottom: 0.0,
                }),
            );
            if [width, height] != [self.place[2], self.place[3]] {
                ui.send(self.panel, WidgetMessage::Width(width));
                ui.send(self.panel, WidgetMessage::Height(height));
                let (margin, size) = fitted(width, height);
                // The whole panel in from its edges, and a little room round what is typed.
                ui.send(
                    self.text,
                    WidgetMessage::Margin(Thickness {
                        left: margin,
                        top: margin,
                        right: margin,
                        bottom: 0.0,
                    }),
                );
                ui.send(
                    self.input_box,
                    WidgetMessage::Margin(Thickness {
                        left: margin,
                        top: 0.0,
                        right: margin,
                        bottom: 0.0,
                    }),
                );
                ui.send(
                    self.input,
                    WidgetMessage::Margin(Thickness::uniform(0.3 * size)),
                );
                ui.send(
                    self.keys,
                    WidgetMessage::Margin(Thickness {
                        left: margin,
                        top: 0.3 * size,
                        right: margin,
                        bottom: margin,
                    }),
                );
                for text in [self.text, self.input, self.keys] {
                    ui.send(text, TextMessage::FontSize(size.into()));
                }
            }
            self.place = place;
        }
        // The output types itself out; what is typed shows at once.
        let screen = Screen {
            output: typed_out(&screen.output, self.typed_out as usize),
            ..screen
        };
        if screen.output != self.shown.output {
            ui.send(self.text, TextMessage::BBCode(bbcode(&screen.output)));
        }
        if screen.input != self.shown.input {
            ui.send(
                self.input,
                TextMessage::BBCode(bbcode(std::slice::from_ref(&screen.input))),
            );
        }
        if screen.keys != self.shown.keys {
            ui.send(
                self.keys,
                TextMessage::BBCode(bbcode(std::slice::from_ref(&screen.keys))),
            );
        }
        self.shown = screen;
    }
}

/// The first `count` characters of `lines`, as a screen types itself out: what comes after is not
/// there yet.
fn typed_out(lines: &[Line], mut count: usize) -> Vec<Line> {
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let mut kept = Vec::new();
        for (piece, colour) in line {
            let length = piece.chars().count();
            if count >= length {
                kept.push((piece.clone(), *colour));
                count -= length;
            } else {
                kept.push((piece.chars().take(count).collect(), *colour));
                count = 0;
                break;
            }
        }
        out.push(kept);
        if count == 0 {
            break;
        }
    }
    out
}

/// The terminal on the computer's screen itself: an interface of its own, drawn into a picture the
/// screen glows with, and how many frames it has drawn into the picture it has now.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScreenTerminal {
    ui: Handle<UserInterface>,
    terminal: Terminal,
    picture: Option<TextureResource>,
    drawn: u32,
}

impl ScreenTerminal {
    /// Lets go of the picture it was drawing into, for the next screen it shows on to take up a
    /// fresh one.
    pub fn forget(&mut self) {
        self.picture = None;
    }

    /// Builds its interface, with nothing on it, among `uis`.
    pub fn build(uis: &mut UiContainer) -> Self {
        let (width, height) = PICTURE;
        let mut ui = UserInterface::new(Vector2::new(width as f32, height as f32));
        // Always into a picture, never onto the window.
        ui.render_target = Some(TextureResource::new_render_target(width, height));
        let terminal = Terminal::build(&mut ui);
        Self {
            ui: uis.add(ui),
            terminal,
            picture: None,
            drawn: 0,
        }
    }

    /// Shows `shown` - the screen of a hack, its stage, and whether a key has just gone wrong - on
    /// the screen of `computer`, or with none takes it off, for another `dt`.
    pub fn show(
        &mut self,
        uis: &mut UiContainer,
        computer: &Computer,
        shown: Option<(Screen, View, bool)>,
        dt: f32,
    ) {
        match (&shown, &self.picture) {
            // A fresh picture each time, which the scene has never looked for.
            (Some(_), None) => {
                let (width, height) = PICTURE;
                let picture = TextureResource::new_render_target(width, height);
                if let Ok(ui) = uis.try_get_mut(self.ui) {
                    ui.render_target = Some(picture.clone());
                }
                self.picture = Some(picture);
                self.drawn = 0;
            }
            (None, Some(_)) => {
                computer.show_on_screen(None);
                self.picture = None;
            }
            _ => (),
        }
        let (width, height) = (PICTURE.0 as f32, PICTURE.1 as f32);
        let corners = [
            Vector2::new(0.0, 0.0),
            Vector2::new(width, 0.0),
            Vector2::new(0.0, height),
            Vector2::new(width, height),
        ];
        if let Ok(ui) = uis.try_get(self.ui) {
            let shown = shown.map(|(screen, view, wrong)| (screen, corners, view, wrong));
            self.terminal.show(ui, shown, dt);
        }
        // The screen takes the picture up once the interface has drawn into it.
        if self.picture.is_some() {
            self.drawn += 1;
            if self.drawn == DRAWN_BEFORE_SHOWN {
                computer.show_on_screen(self.picture.clone());
            }
        }
    }
}

/// How far in from the edges of a panel `width` by `height` pixels the text goes, and how big it
/// is, so that [`PANEL_LINES`] lines of [`PANEL_COLUMNS`] characters fit in it.
fn fitted(width: f32, height: f32) -> (f32, f32) {
    let margin = height * PANEL_MARGIN;
    let size = ((height - 2.0 * margin) / (PANEL_LINES * LINE_SPACING))
        .min((width - 2.0 * margin) / (PANEL_COLUMNS * CHAR_WIDTH))
        .max(1.0);
    (margin, size)
}

/// `lines` as BBCode, each piece of text in its colour.
fn bbcode(lines: &[Line]) -> String {
    let mut text = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            text.push('\n');
        }
        for (piece, [r, g, b]) in line {
            if !piece.is_empty() {
                text.push_str(&format!("[c=#{r:02x}{g:02x}{b:02x}]{piece}[/c]"));
            }
        }
    }
    text
}

/// A beep made to play, and how far off it is heard at full volume.
type Beep = Option<(SoundBufferResource, f32)>;

/// The terminal's beeps: for a key that goes in, one that does not, a command typed out, and
/// Enter starting a breach or opening a file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Beeps {
    key: Beep,
    wrong: Beep,
    line: Beep,
    enter: Beep,
}

impl Beeps {
    /// Makes them from [`COMPUTER_SOUNDS`]; the computers are silent without it.
    pub fn make() -> Self {
        let sounds = Sounds::load(COMPUTER_SOUNDS)
            .inspect_err(|error| fyrox::core::log::Log::err(format!("Computer: {error}")))
            .ok();
        let made = |name: &str| {
            let sounds = sounds.as_ref()?;
            let Some(sound) = sounds.sounds.get(name) else {
                fyrox::core::log::Log::warn(format!("Computer: {COMPUTER_SOUNDS} has no {name}"));
                return None;
            };
            Some((formants::buffer(sound, sounds.sample_rate)?, sound.reach))
        };
        Self {
            key: made("key"),
            wrong: made("wrong"),
            line: made("line"),
            enter: made("enter"),
        }
    }
}

/// The computer in the maze.
#[derive(Debug, Clone, PartialEq)]
pub struct Computer {
    /// The static body it stands in, which the model hangs off, and what stands in the way; and
    /// its frame's glow.
    body: Handle<Node>,
    collider: Handle<Collider>,
    frame: MaterialResource,
    /// What its screen glows with.
    glass: MaterialResource,
    screen: Handle<Node>,
    /// The middle of the screen, and its corners, in the screen's own terms.
    screen_middle: Vector3<f32>,
    screen_corners: [Vector3<f32>; 4],
    hack: Hack,
    beeps: Beeps,
    /// Whether it has been put somewhere in this round's maze.
    placed: bool,
    /// How long the cursor has been blinking, in seconds.
    blink: f32,
    lit: Option<Stage>,
}

impl Computer {
    /// Puts the computer into `scene` from its `model`, out of the way until it is placed, beeping
    /// with `beeps`. None if the model is not the computer.
    pub fn spawn(
        model: &ModelResource,
        scene: &mut Scene,
        seed: u64,
        beeps: &Beeps,
    ) -> Option<Self> {
        let root = model.instantiate(scene);
        let graph = &mut scene.graph;
        let find = |graph: &Graph, name: &str| graph.find_by_name(root, name).map(|(node, _)| node);
        let (Some(screen), Some(frame_node)) = (find(graph, SCREEN), find(graph, FRAME)) else {
            fyrox::core::log::Log::err(format!(
                "Computer: {COMPUTER_MODEL} has no {SCREEN} or {FRAME}"
            ));
            graph.remove_node(root);
            return None;
        };
        // The screen glows a faint green, and the frame red or blue: each with its own copy of
        // the model's material, whose shader the engine lights as the model was made.
        let own = |graph: &Graph, node: Handle<Node>| {
            let mesh = graph[node].cast::<Mesh>()?;
            let original = mesh.surfaces().first()?.material();
            let state = original.state();
            let material: Material = state.data_ref()?.clone();
            Some(material)
        };
        let mut glass = own(graph, screen).unwrap_or_else(Material::standard);
        glass.set_property("diffuseColor", SCREEN_COLOUR);
        glass.set_property("emissionStrength", Vector3::repeat(SCREEN_GLOW));
        let glass = MaterialResource::new_embedded(glass);
        let mut glow = own(graph, frame_node).unwrap_or_else(Material::standard);
        glow.set_property("diffuseColor", LOCKED_COLOUR);
        glow.set_property("emissionStrength", Vector3::repeat(FRAME_GLOW));
        let frame = MaterialResource::new_embedded(glow);
        let mut middle = Vector3::zeros();
        let (mut low, mut high) = (Vector3::repeat(f32::MAX), Vector3::repeat(f32::MIN));
        if let Some(mesh) = graph[screen].cast_mut::<Mesh>() {
            let mut count = 0.0;
            for surface in mesh.surfaces_mut() {
                surface.set_material(glass.clone());
                let data = surface.data();
                let data = data.data_ref();
                for vertex in data.vertex_buffer.iter() {
                    if let Ok(position) = vertex.read_3_f32(VertexAttributeUsage::Position) {
                        middle += position;
                        count += 1.0;
                        low = low.inf(&position);
                        high = high.sup(&position);
                    }
                }
            }
            middle /= f32::max(count, 1.0);
        }
        if let Some(mesh) = graph[frame_node].cast_mut::<Mesh>() {
            for surface in mesh.surfaces_mut() {
                surface.set_material(frame.clone());
            }
        }
        for node in graph.traverse_handle_iter(root).collect::<Vec<_>>() {
            if graph[node].cast::<Mesh>().is_some() {
                graph[node].set_cast_shadows(false);
            }
        }

        // The monitor and keyboard stand in the way of anyone walking into them.
        let collider = ColliderBuilder::new(
            BaseBuilder::new().with_local_transform(
                TransformBuilder::new()
                    .with_local_position(Vector3::new(0.0, BULK_HEIGHT, BULK_HALF.z))
                    .build(),
            ),
        )
        .with_shape(ColliderShape::cuboid(BULK_HALF.x, BULK_HALF.y, BULK_HALF.z))
        .build(graph);
        let body = RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_name("computer")
                .with_child(collider)
                .with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(Vector3::new(0.0, -1000.0, 0.0))
                        .build(),
                ),
        )
        .with_body_type(RigidBodyType::Static)
        .build(graph)
        .to_base();
        graph.link_nodes(root, body);
        Some(Self {
            body,
            collider,
            glass: glass.clone(),
            frame,
            screen,
            screen_middle: middle,
            // Flat across its own x and y.
            screen_corners: [
                Vector3::new(low.x, low.y, middle.z),
                Vector3::new(high.x, low.y, middle.z),
                Vector3::new(low.x, high.y, middle.z),
                Vector3::new(high.x, high.y, middle.z),
            ],
            hack: Hack::new(seed),
            beeps: beeps.clone(),
            placed: false,
            blink: 0.0,
            lit: None,
        })
    }

    /// Puts it at `spot` in the maze whose `grid` has its corner at `origin`, against the wall
    /// behind, locked, with no files; and takes the floor it stands on out of `grid`, so that
    /// the droids walk round it. The wall is found past any of the computers' `colliders`, which
    /// may still be where they were last round.
    pub fn place(
        &mut self,
        graph: &mut Graph,
        grid: &mut WalkGrid,
        origin: Vector3<f32>,
        (cell, facing): Spot,
        colliders: &[Handle<Collider>],
    ) {
        self.hack = Hack::new(self.hack.dice.rotate_left(17) ^ 0x5bd1_e995);
        self.lit = None;
        self.placed = true;
        let across = (-facing.1, facing.0);
        let at = survey::cell_center(origin, cell.0, cell.1);
        let floor = grid.floor(cell.0, cell.1);
        let yaw = (facing.0 as f32).atan2(facing.1 as f32);
        // Its back just off the wall behind the cell - [`WALL_CLEARANCE`] short of as far as a ray
        // from the cell goes before it hits something that is not anyone, nor the computer
        // itself where it was last round.
        let back = Vector3::new(-facing.0 as f32, 0.0, -facing.1 as f32);
        let from = Vector3::new(at.x, floor + WALL_LOOK_HEIGHT, at.z);
        let mut hits = Vec::new();
        graph.physics.cast_ray(
            RayCastOptions {
                ray_origin: Point3::from(from),
                ray_direction: back,
                max_len: WALL_REACH,
                groups: InteractionGroups::new(BitMask(u32::MAX), BitMask(!(CHARACTERS | SPILL))),
                sort_results: true,
            },
            &mut hits,
        );
        let wall = hits
            .iter()
            .find(|hit| hit.collider != self.collider && !colliders.contains(&hit.collider))
            .map_or(0.5 * CELL_SIZE, |hit| hit.toi);
        let at = from + back * (wall - WALL_CLEARANCE);
        fyrox::core::log::Log::info(format!(
            "Computer: in cell {cell:?} facing {facing:?}, the wall {wall:.2} m behind; at {at:?}, {} hits",
            hits.len()
        ));
        graph[self.body]
            .local_transform_mut()
            .set_position(Vector3::new(at.x, floor, at.z))
            .set_rotation(UnitQuaternion::from_axis_angle(&Vector3::y_axis(), yaw));
        for k in [-1, 0, 1] {
            let x = cell.0 as i64 + across.0 * k;
            let z = cell.1 as i64 + across.1 * k;
            if x >= 0 && z >= 0 && (x as usize) < grid.width && (z as usize) < grid.depth {
                grid.set(x as usize, z as usize, false);
            }
        }
    }

    /// Takes it out of the maze, for a round with nowhere to put it.
    pub fn hide(&mut self, graph: &mut Graph) {
        graph[self.body]
            .local_transform_mut()
            .set_position(Vector3::new(0.0, -1000.0, 0.0));
        self.placed = false;
    }

    /// What stands in the way of anyone walking into it.
    pub fn collider(&self) -> Handle<Collider> {
        self.collider
    }

    /// Gives it `files` to read once it is cleared.
    pub fn set_files(&mut self, files: Vec<Entry>) {
        self.hack.set_files(files);
    }

    /// Up or down `by` through its files, or down the one open, beeping if it went anywhere.
    pub fn step(&mut self, graph: &mut Graph, by: i32) {
        if self.hack.step(by) {
            self.beep(graph, &self.beeps.key, 1.0);
        }
    }

    /// Back to the listing of its files from the one open, beeping if there was one.
    pub fn back(&mut self, graph: &mut Graph) {
        if self.hack.back() {
            self.beep(graph, &self.beeps.key, 1.0);
        }
    }

    /// Where the middle of its screen is across the world, and which way the screen faces.
    pub fn screen(&self, graph: &Graph) -> (Vector3<f32>, Vector3<f32>) {
        let transform = graph[self.screen].global_transform();
        let middle = transform
            .transform_point(&Point3::from(self.screen_middle))
            .coords;
        let facing = graph[self.body]
            .global_transform()
            .transform_vector(&Vector3::z());
        (
            middle,
            facing.try_normalize(1.0e-6).unwrap_or_else(Vector3::z),
        )
    }

    /// How high the floor it stands on is, across the world.
    pub fn floor(&self, graph: &Graph) -> f32 {
        graph[self.body].global_position().y
    }

    /// Where the corners of its screen are, across the world.
    pub fn screen_corners(&self, graph: &Graph) -> [Vector3<f32>; 4] {
        let transform = graph[self.screen].global_transform();
        self.screen_corners
            .map(|corner| transform.transform_point(&Point3::from(corner)).coords)
    }

    /// Whether someone standing at `feet` is close enough in front of it to use it.
    pub fn within_reach(&self, graph: &Graph, feet: Vector3<f32>) -> bool {
        if !self.placed {
            return false;
        }
        let (middle, facing) = self.screen(graph);
        let off = Vector3::new(feet.x - middle.x, 0.0, feet.z - middle.z);
        off.norm() < REACH && off.dot(&facing) > IN_FRONT
    }

    pub fn cleared(&self) -> bool {
        self.hack.stage() == Stage::Cleared
    }

    /// Enter, beeping if it starts a breach or opens a file.
    pub fn enter(&mut self, graph: &mut Graph) {
        if self.hack.enter() {
            self.beep(graph, &self.beeps.enter, 1.0);
        }
    }

    /// `c` typed, beeping as it goes in or does not: each character a little higher or lower
    /// than the next.
    pub fn type_char(&mut self, graph: &mut Graph, c: char) {
        let beep = match self.hack.type_char(c) {
            Keystroke::Ignored => return,
            Keystroke::Right => &self.beeps.key,
            Keystroke::Line => &self.beeps.line,
            Keystroke::Wrong => &self.beeps.wrong,
        };
        let pitch = 1.0 + (c as u32 % 5) as f64 / 2.0 * BEEP_SPREAD - BEEP_SPREAD;
        self.beep(graph, beep, pitch);
    }

    /// Plays `beep` once from the screen, `pitch` times as high as it was made.
    fn beep(&self, graph: &mut Graph, beep: &Beep, pitch: f64) {
        let Some((buffer, reach)) = beep else {
            return;
        };
        let (middle, _) = self.screen(graph);
        SoundBuilder::new(
            BaseBuilder::new()
                .with_local_transform(TransformBuilder::new().with_local_position(middle).build()),
        )
        .with_buffer(Some(buffer.clone()))
        .with_radius(*reach)
        .with_pitch(pitch)
        .with_play_once(true)
        .with_status(Status::Playing)
        .build(graph);
    }

    /// Has its screen glow with `picture`, the terminal's, or with none its own faint green.
    pub fn show_on_screen(&self, picture: Option<TextureResource>) {
        let mut glass = self.glass.data_ref();
        match picture {
            // Its colour as well as its glow: black where the terminal is black.
            Some(picture) => {
                glass.bind("diffuseTexture", picture.clone());
                glass.bind("emissionTexture", picture);
                glass.set_property("diffuseColor", Color::WHITE);
                glass.set_property("emissionStrength", Vector3::repeat(PICTURE_GLOW));
            }
            None => {
                glass.unbind("diffuseTexture");
                glass.unbind("emissionTexture");
                glass.set_property("diffuseColor", SCREEN_COLOUR);
                glass.set_property("emissionStrength", Vector3::repeat(SCREEN_GLOW));
            }
        }
    }

    /// What its terminal shows now, at which stage of the hack, and whether a key has just gone
    /// wrong.
    pub fn terminal(&self) -> (Screen, View, bool) {
        (
            self.hack.screen(self.blink < BLINK),
            self.hack.view(),
            self.hack.flash > 0.0,
        )
    }

    /// Runs the hack on for another `dt`, and shows how it stands on the frame.
    pub fn update(&mut self, dt: f32) {
        self.hack.update(dt);
        self.blink = (self.blink + dt) % (2.0 * BLINK);
        let stage = self.hack.stage();
        let cleared = stage == Stage::Cleared;
        if self.lit.map(|lit| lit == Stage::Cleared) != Some(cleared) {
            let mut frame = self.frame.data_ref();
            let colour = if cleared {
                CLEARED_COLOUR
            } else {
                LOCKED_COLOUR
            };
            frame.set_property("diffuseColor", colour);
        }
        self.lit = Some(stage);
    }
}

/// A cell, and which way along the grid something in it faces, as a step across it.
type Spot = ((usize, usize), (i64, i64));

/// Which way the computer can face in cell `i` of `grid`, if any: with room for it across, a
/// wall right behind it, and floor in front.
fn facing_at(grid: &WalkGrid, i: usize) -> Option<(i64, i64)> {
    let walkable = |x: i64, z: i64| {
        x >= 0
            && z >= 0
            && (x as usize) < grid.width
            && (z as usize) < grid.depth
            && grid.is_walkable(x as usize, z as usize)
    };
    let (x, z) = ((i % grid.width) as i64, (i / grid.width) as i64);
    [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().find(|&facing| {
        let across = (-facing.1, facing.0);
        let wide = (-1..=1).all(|k| walkable(x + across.0 * k, z + across.1 * k));
        let wall = !walkable(x - facing.0, z - facing.1);
        let room =
            (1..=ROOM_IN_FRONT as i64).all(|k| walkable(x + facing.0 * k, z + facing.1 * k));
        wide && wall && room
    })
}

/// Where the first computer goes, from `start` in `grid`: the cell it stands on, and which way it
/// faces along the grid, as a step across it. The nearest open floor from [`FROM_START`] cells of
/// walking on that it fits against a wall at.
pub fn spot(grid: &WalkGrid, start: (usize, usize)) -> Option<Spot> {
    let distances = grid.distances_from(start);
    let mut best: Option<(u32, Spot)> = None;
    for (i, distance) in distances.iter().enumerate() {
        let Some(d) = *distance else { continue };
        if d < FROM_START.0 || d > FROM_START.1 || best.is_some_and(|(b, _)| d >= b) {
            continue;
        }
        if let Some(facing) = facing_at(grid, i) {
            best = Some((d, ((i % grid.width, i / grid.width), facing)));
        }
    }
    best.map(|(_, spot)| spot)
}

/// Where another computer goes, from `start` in `grid`, picked by `rng`: anywhere it fits against
/// a wall at least [`AWAY_FROM_START`] cells of walking on, and [`APART`] cells from each of the
/// cells `taken` already - or, with nowhere that far, as far as there is room for.
pub fn spot_away(
    grid: &WalkGrid,
    start: (usize, usize),
    taken: &[(usize, usize)],
    rng: &mut Rng,
) -> Option<Spot> {
    let distances = grid.distances_from(start);
    let fits: Vec<Spot> = distances
        .iter()
        .enumerate()
        .filter(|(_, d)| d.is_some_and(|d| d >= AWAY_FROM_START))
        .filter_map(|(i, _)| Some(((i % grid.width, i / grid.width), facing_at(grid, i)?)))
        .collect();
    let mut apart = APART;
    loop {
        let far: Vec<&Spot> = fits
            .iter()
            .filter(|((x, z), _)| {
                taken.iter().all(|&(tx, tz)| {
                    let (dx, dz) = (*x as f32 - tx as f32, *z as f32 - tz as f32);
                    dx.hypot(dz) >= apart
                })
            })
            .collect();
        if !far.is_empty() {
            return Some(*far[rng.below(far.len())]);
        }
        if apart < 1.0 {
            return None;
        }
        apart /= 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_says_what_it_did_for_its_beep() {
        let mut hack = Hack::new(7);
        assert_eq!(hack.type_char('a'), Keystroke::Ignored, "nothing under way");
        assert!(hack.enter());
        assert!(!hack.enter(), "already under way");
        let line = hack.lines[0];
        let (last, rest) = (line.chars().last().unwrap(), &line[..line.len() - 1]);
        for c in rest.chars() {
            assert_eq!(hack.type_char(c), Keystroke::Right, "{c:?} of {line:?}");
        }
        assert_eq!(hack.type_char('\u{7f}'), Keystroke::Wrong);
        assert_eq!(hack.type_char(last), Keystroke::Line);
    }

    #[test]
    fn the_computers_beeps_are_made() {
        let sounds = Sounds::load(COMPUTER_SOUNDS).unwrap();
        for name in ["key", "wrong", "line", "enter"] {
            let sound = &sounds.sounds[name];
            let samples = formants::synth::make(sound, sounds.sample_rate);
            let peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
            assert!(samples.iter().all(|s| s.is_finite()), "{name}");
            assert!((peak - sound.volume).abs() < 1e-4, "{name}: {peak}");
            assert!(!sound.looped && sound.length <= 0.2, "{name} is a light beep");
        }
    }

    #[test]
    fn caps_lock_makes_no_difference_to_what_is_typed() {
        // Caps Lock on: small letters come as capitals, and with Shift as small letters.
        assert_eq!(without_caps_lock('S', false), 's');
        assert_eq!(without_caps_lock('f', true), 'F');
        // Caps Lock off, as typed.
        assert_eq!(without_caps_lock('s', false), 's');
        assert_eq!(without_caps_lock('F', true), 'F');
        // Only letters.
        for c in ['7', '@', '-', ' ', '/'] {
            assert_eq!(without_caps_lock(c, false), c);
            assert_eq!(without_caps_lock(c, true), c);
        }
        let mut hack = Hack::new(1);
        hack.lines = vec!["unlock --node=7F3A"];
        hack.stage = Stage::Breaching;
        hack.next_line();
        // Typed with Caps Lock on: Shift only for the capitals, as the player would.
        for (c, shift) in "UNLOCK --NODE=7f3a".chars().zip("unlock --node=7F3A".chars().map(char::is_uppercase)) {
            assert_ne!(hack.type_char(without_caps_lock(c, shift)), Keystroke::Wrong, "{c:?}");
        }
        assert_eq!(hack.stage(), Stage::Cleared);
    }

    fn cleared(files: Vec<Entry>) -> Hack {
        let mut hack = Hack::new(3);
        hack.enter();
        for _ in 0..BREACH_LINES {
            let line = hack.lines[hack.at];
            typed(&mut hack, line);
        }
        hack.set_files(files);
        hack
    }

    fn note(title: &str, text: &str) -> Entry {
        Entry {
            title: title.into(),
            kind: "note".into(),
            date: None,
            text: text.into(),
            group: None,
        }
    }

    #[test]
    fn once_cleared_its_files_are_picked_opened_scrolled_and_closed() {
        let mut locked = Hack::new(3);
        locked.set_files(vec![note("a", "x")]);
        assert!(!locked.step(1) && !locked.back(), "not before it is cleared");

        let long: Vec<String> = (0..200).map(|n| format!("word{n}")).collect();
        let long = long.join(" ");
        let mut hack = cleared(vec![note("a", "short"), note("b", &long)]);
        assert_eq!(hack.view(), (Stage::Cleared, None));
        assert!(!hack.step(-1), "already at the top");
        assert!(hack.step(1));
        assert!(!hack.step(1), "already at the bottom");
        assert!(hack.enter());
        assert_eq!(hack.view(), (Stage::Cleared, Some(1)));
        assert!(!hack.enter(), "already open");
        let text = |hack: &Hack| hack.screen(true).output[3..].to_vec();
        let top = text(&hack);
        assert!(hack.step(1));
        assert_ne!(text(&hack), top, "scrolled");
        while hack.step(1) {}
        assert!(hack.screen(true).keys[0].0.ends_with("SCROLL  BKSP BACK  TAB LEAVE"));
        assert!(hack.back());
        assert!(!hack.back(), "already closed");
        assert_eq!(hack.view(), (Stage::Cleared, None));
        assert_eq!(hack.picked, 1, "still picked");
    }

    fn typed(hack: &mut Hack, text: &str) {
        for c in text.chars() {
            hack.type_char(c);
        }
    }

    #[test]
    fn a_breach_is_cleared_by_typing_every_command_before_the_trace_completes() {
        let mut hack = Hack::new(7);
        assert_eq!(hack.stage(), Stage::Locked);
        hack.enter();
        assert_eq!(hack.stage(), Stage::Breaching);
        for _ in 0..BREACH_LINES {
            let line = hack.lines[hack.at];
            hack.update(0.5);
            typed(&mut hack, line);
        }
        assert_eq!(hack.stage(), Stage::Cleared);
        hack.enter();
        assert_eq!(hack.stage(), Stage::Cleared, "cleared stays cleared");
    }

    #[test]
    fn a_wrong_key_does_not_go_in_and_costs_time_and_the_trace_denies_access() {
        let mut hack = Hack::new(3);
        hack.enter();
        let left = hack.left;
        hack.type_char('\u{7f}');
        assert_eq!(hack.typed, 0);
        assert!((left - hack.left - WRONG_KEY).abs() < 1.0e-5);
        hack.update(100.0);
        assert_eq!(hack.stage(), Stage::Denied);
        hack.enter();
        assert_eq!(hack.stage(), Stage::Breaching, "retried at once");
        assert_eq!((hack.at, hack.typed), (0, 0));
    }

    #[test]
    fn a_breach_takes_different_commands() {
        let mut hack = Hack::new(11);
        hack.enter();
        let mut lines = hack.lines.clone();
        lines.sort();
        lines.dedup();
        assert_eq!(lines.len(), BREACH_LINES);
        assert!(COMMANDS
            .iter()
            .all(|c| !c.contains('[') && !c.contains(']')));
    }

    #[test]
    fn the_terminal_colours_each_piece_and_the_cursor_blinks() {
        let mut hack = Hack::new(5);
        hack.enter();
        let screen = hack.screen(true);
        let text = bbcode(&screen.output);
        assert!(text.starts_with("[c=#3cff6e]MAZE-NET"), "{text}");
        assert!(text.contains("TRACE"), "{text}");
        assert_eq!(
            screen
                .input
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<String>(),
            "$ █",
            "nothing typed yet"
        );
        let start = hack.lines[0][..3].to_string();
        typed(&mut hack, &start);
        let screen = hack.screen(false);
        let input: String = screen.input.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            input,
            format!("$ {} ", &hack.lines[0][..3]),
            "typed, in the box, the cursor off"
        );
    }

    /// How many lines of output the panel has room for, over the box and the keys.
    const OUTPUT_LINES: usize = 13;

    #[test]
    fn every_screen_of_the_terminal_fits_in_the_panel() {
        let mut screens = Vec::new();
        let mut hack = Hack::new(9);
        screens.push(hack.screen(true));
        hack.enter();
        for _ in 0..BREACH_LINES - 1 {
            let line = hack.lines[hack.at];
            typed(&mut hack, line);
        }
        hack.type_char('\u{7f}');
        screens.push(hack.screen(true));
        hack.update(100.0);
        screens.push(hack.screen(true));
        hack.enter();
        for _ in 0..BREACH_LINES {
            let line = hack.lines[hack.at];
            typed(&mut hack, line);
        }
        screens.push(hack.screen(true));
        // Cleared with every note there is, listed from the top and the bottom, and each read
        // from the top and scrolled to the bottom.
        let files = crate::notes::Notes::load(crate::notes::NOTES).unwrap().entries;
        hack.set_files(files.clone());
        screens.push(hack.screen(true));
        for _ in 0..files.len() {
            assert!(hack.enter(), "opened");
            screens.push(hack.screen(true));
            while hack.step(1) {}
            screens.push(hack.screen(true));
            assert!(hack.back());
            hack.step(1);
        }
        screens.push(hack.screen(true));
        hack.set_files(Vec::new());
        screens.push(hack.screen(true));
        for screen in &screens {
            assert!(
                screen.output.len() <= OUTPUT_LINES,
                "{} lines",
                screen.output.len()
            );
            // With the box and the keys, and the box's room round what is typed.
            assert!(OUTPUT_LINES as f32 + 1.6 + 1.3 <= PANEL_LINES);
            let every = screen.output.iter().chain([&screen.input, &screen.keys]);
            for line in every {
                let width: usize = line.iter().map(|(text, _)| text.chars().count()).sum();
                assert!(
                    width as f32 <= PANEL_COLUMNS,
                    "{width} characters: {line:?}"
                );
            }
        }
        // The screen is about 1.54 times as wide as it is high; either way round, it fits.
        for (width, height) in [(800.0, 520.0), (1200.0, 520.0), (500.0, 520.0)] {
            let (margin, size) = fitted(width, height);
            assert!(2.0 * margin + PANEL_COLUMNS * CHAR_WIDTH * size <= width + 0.01);
            assert!(2.0 * margin + PANEL_LINES * LINE_SPACING * size <= height + 0.01);
        }
    }

    #[test]
    fn a_screen_types_itself_out_a_character_at_a_time() {
        let lines: Vec<Line> = vec![
            vec![("AB".into(), GREEN), ("CD".into(), DIM)],
            Vec::new(),
            vec![("EF".into(), GREEN)],
        ];
        assert_eq!(
            typed_out(&lines, 3),
            vec![vec![("AB".into(), GREEN), ("C".into(), DIM)]]
        );
        assert_eq!(typed_out(&lines, 100), lines);
        assert!(typed_out(&lines, 0)
            .iter()
            .all(|line| line.iter().all(|(p, _)| p.is_empty())));
    }

    #[test]
    fn it_goes_against_a_wall_facing_open_floor_away_from_the_start() {
        // A room 12 cells wide and deep, walled all round.
        let mut grid = WalkGrid::new(14, 14);
        for x in 1..13 {
            for z in 1..13 {
                grid.set(x, z, true);
            }
        }
        let start = (6, 6);
        let ((x, z), facing) = spot(&grid, start).expect("somewhere");
        assert!(
            !grid.is_walkable(
                (x as i64 - facing.0) as usize,
                (z as i64 - facing.1) as usize
            ),
            "wall behind"
        );
        let walked = grid.distances_from(start)[z * grid.width + x].unwrap();
        assert!(walked >= FROM_START.0, "{walked}");
    }
}
