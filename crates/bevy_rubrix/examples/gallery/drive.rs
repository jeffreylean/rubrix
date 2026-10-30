//! The scripted driver. `RUBRIX_DRIVE=<script>` runs a script, one
//! step per line, and every step puts its input into the app as Bevy
//! messages, never through the OS. Hyprland gives the gallery window
//! no focus, so an OS pointer warp or a synthetic key press never
//! arrives.
//!
//! A script is lines of `<step> <arguments>`. A blank line and a line
//! that starts with `#` do nothing, and a bad line makes the gallery
//! exit with code 2 and print its number. Steps:
//!
//! - `page <name>`, `wait <frames>`, `exit`
//! - `move <x> <y>`, `click <x> <y> [double|triple] [shift] [mod]
//!   [alt]`, `drag <x0> <y0> <x1> <y1> [shift] [mod] [alt]`, `press
//!   <x> <y>`, `release`, `scroll <dx> <dy>`, `wheel <dx> <dy>`
//! - `key <combo>` such as `mod+a`, and `type <text>`
//! - `ime enable`, `ime disable`, `ime preedit [<text> [<start>
//!   <end>]]`, `ime commit <text>`, `paste <text>`, `wtype <text>`
//! - `screenshot <path>`, `record start <dir>`, `record stop`
//! - `frames start`, `frames stop <csv>`
//! - `log <topic>`, `expect <topic> <text>`, `clipboard`; the `diff`
//!   topic names the diff view's first visible row
//! - `resize <width> <height>`, `open <path>...`
//!
//! `mod` is the command key, Ctrl on Linux and Cmd on macOS. A topic
//! is a word, but `runs <line>` and `text <line>` take a line number
//! too, so `expect runs 75 comment` checks topic `runs 75` for
//! `comment`. `open` reads a relative path from the script's
//! directory. `paste` reads `\n`, `\t` and `\\` in its text as a
//! newline, a tab and a backslash. `wtype` types through the OS, so
//! it runs only under `launch.sh --ime-live`, which sets
//! `RUBRIX_IME_LIVE=1` and gives the window focus.
//!
//! Positions are logical pixels in the window. A scroll `dy` below
//! zero moves the content up, so the page scrolls down. A `scroll`
//! turns in pixels, as a trackpad does, and a `wheel` in lines, the
//! notches of a mouse wheel, which ease. Every step prints
//! `drive: <step> ok` when it is done.
//!
//! A scroll goes to what is under the pointer, so a `click` on the
//! page list leaves the pointer there until a `move` takes it back.
//! `resize` asks Hyprland for the size and is done when the window
//! reports it, or exits with code 2 after two seconds. `exit` stops
//! a `record` still running, prints `drive: <passed> of <checks>
//! checks pass` for the `expect` steps, and exits with code 1 when
//! one failed.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::{fmt, fs, mem};

use bevy::app::{App, AppExit, First, Plugin, Update};
use bevy::color::Srgba;
use bevy::ecs::change_detection::Mut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::{MessageReader, MessageUpdateSystems};
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::system::ResMut;
use bevy::ecs::world::World;
use bevy::input::ButtonState;
use bevy::input::keyboard::{
    Key, KeyCode, KeyboardInput, NativeKeyCode,
};
use bevy::input::mouse::{
    MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel,
};
use bevy::input::touch::TouchPhase;
use bevy::math::{UVec2, Vec2};
use bevy::picking::PickingSystems;
use bevy::render::view::screenshot::{
    Screenshot, ScreenshotCaptured, save_to_disk,
};
use bevy::text::{TextColor, TextSpan};
use bevy::ui::ScrollPosition;
use bevy::window::{
    CursorMoved, Ime, PrimaryWindow, Window, WindowEvent,
};
use bevy_fynix::views::VirtualList;
use bevy_rubrix::{
    CodeDoc, CodeLine, CodeSelection, DiffDoc, DiffRow, Layout, Row,
};
use rubrix_highlight::spans;

use crate::pages::rich::{Hit, HitLineRoot, LongLineRoot};
use crate::pages::{self, Ask, PAGES};
use crate::{
    Copied, Mode, PAGE_CENTER, Paste, Ready, Selected, page_areas,
    scrollers,
};

const DRAG_FRAMES: usize = 4;
const RESIZE_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.reason)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Press {
    code: KeyCode,
    key: Key,
    text: Option<String>,
}

/// The platform a script runs on, which picks the key `mod` holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    Macos,
}

impl Os {
    fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Page(usize),
    Wait(u32),
    Move(Vec2),
    Click {
        at: Vec2,
        count: u8,
        held: Vec<Press>,
    },
    Press(Vec2),
    Release,
    Drag {
        from: Vec2,
        to: Vec2,
        held: Vec<Press>,
    },
    Scroll(Vec2),
    Wheel(Vec2),
    Key(Vec<Press>),
    Type(String),
    /// The window in the message is a placeholder until the step
    /// runs.
    Ime(Ime),
    Paste {
        text: String,
        chord: Vec<Press>,
    },
    Wtype(String),
    Screenshot(PathBuf),
    Log(String),
    FramesStart,
    FramesStop(PathBuf),
    RecordStart(PathBuf),
    RecordStop,
    Resize {
        width: u32,
        height: u32,
    },
    Open(Vec<PathBuf>),
    Clipboard,
    Expect {
        topic: String,
        text: String,
    },
    Exit,
}

impl Step {
    fn name(&self) -> &'static str {
        match self {
            Self::Page(_) => "page",
            Self::Wait(_) => "wait",
            Self::Move(_) => "move",
            Self::Click { .. } => "click",
            Self::Press(_) => "press",
            Self::Release => "release",
            Self::Drag { .. } => "drag",
            Self::Scroll(_) => "scroll",
            Self::Wheel(_) => "wheel",
            Self::Key(_) => "key",
            Self::Type(_) => "type",
            Self::Ime(_) => "ime",
            Self::Paste { .. } => "paste",
            Self::Wtype(_) => "wtype",
            Self::Screenshot(_) => "screenshot",
            Self::Log(_) => "log",
            Self::FramesStart => "frames start",
            Self::FramesStop(_) => "frames stop",
            Self::RecordStart(_) => "record start",
            Self::RecordStop => "record stop",
            Self::Resize { .. } => "resize",
            Self::Open(_) => "open",
            Self::Clipboard => "clipboard",
            Self::Expect { .. } => "expect",
            Self::Exit => "exit",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    number: usize,
    step: Step,
}

pub fn parse(script: &str) -> Result<Vec<Line>, ParseError> {
    parse_for(script, Os::host())
}

fn parse_for(script: &str, os: Os) -> Result<Vec<Line>, ParseError> {
    let mut lines = Vec::new();
    for (index, text) in script.lines().enumerate() {
        let number = index + 1;
        let step =
            parse_line(text, os).map_err(|reason| ParseError {
                line: number,
                reason,
            })?;
        lines.extend(step.map(|step| Line { number, step }));
    }
    Ok(lines)
}

fn parse_line(text: &str, os: Os) -> Result<Option<Step>, String> {
    let text = text.trim();
    if text.is_empty() || text.starts_with('#') {
        return Ok(None);
    }
    let (verb, rest) =
        text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let rest = rest.trim();
    let words = rest.split_whitespace().collect::<Vec<_>>();
    let step = match verb {
        "page" => {
            let [name] = exact(&words, "page <name>")?;
            let index = PAGES
                .iter()
                .position(|page| page.name == name)
                .ok_or_else(|| format!("no page named `{name}`"))?;
            Step::Page(index)
        }
        "wait" => {
            let [frames] = exact(&words, "wait <frames>")?;
            Step::Wait(number(frames)?)
        }
        "move" => {
            let [x, y] = exact(&words, "move <x> <y>")?;
            Step::Move(Vec2::new(number(x)?, number(y)?))
        }
        "click" => {
            let usage =
                "click <x> <y> [double|triple] [shift] [mod] [alt]";
            let [x, y, ref after @ ..] = words[..] else {
                return Err(format!("usage: {usage}"));
            };
            let (count, named) = match after {
                ["double", named @ ..] => (2, named),
                ["triple", named @ ..] => (3, named),
                named => (1, named),
            };
            Step::Click {
                at: Vec2::new(number(x)?, number(y)?),
                count,
                held: held(named, os, usage)?,
            }
        }
        "press" => {
            let [x, y] = exact(&words, "press <x> <y>")?;
            Step::Press(Vec2::new(number(x)?, number(y)?))
        }
        "release" => {
            exact::<0>(&words, "release")?;
            Step::Release
        }
        "drag" => {
            let usage =
                "drag <x0> <y0> <x1> <y1> [shift] [mod] [alt]";
            let [x0, y0, x1, y1, ref named @ ..] = words[..] else {
                return Err(format!("usage: {usage}"));
            };
            Step::Drag {
                from: Vec2::new(number(x0)?, number(y0)?),
                to: Vec2::new(number(x1)?, number(y1)?),
                held: held(named, os, usage)?,
            }
        }
        "scroll" => {
            let [dx, dy] = exact(&words, "scroll <dx> <dy>")?;
            Step::Scroll(Vec2::new(number(dx)?, number(dy)?))
        }
        "wheel" => {
            let [dx, dy] = exact(&words, "wheel <dx> <dy>")?;
            Step::Wheel(Vec2::new(number(dx)?, number(dy)?))
        }
        "key" => {
            let [combo] = exact(&words, "key <combo>")?;
            Step::Key(parse_combo(combo, os)?)
        }
        "type" => {
            if rest.is_empty() {
                return Err("usage: type <text>".into());
            }
            Step::Type(rest.into())
        }
        "ime" => Step::Ime(parse_ime(rest)?),
        "paste" => {
            if rest.is_empty() {
                return Err("usage: paste <text>".into());
            }
            Step::Paste {
                text: unescape(rest)?,
                chord: parse_combo("mod+v", os)?,
            }
        }
        "wtype" => {
            if rest.is_empty() {
                return Err("usage: wtype <text>".into());
            }
            Step::Wtype(rest.into())
        }
        "screenshot" => {
            let [path] = exact(&words, "screenshot <path>")?;
            Step::Screenshot(path.into())
        }
        "log" => {
            let (topic, rest) = split_topic(rest)?;
            if topic.is_empty() || !rest.is_empty() {
                return Err("usage: log <topic>".into());
            }
            Step::Log(topic)
        }
        "frames" => match words[..] {
            ["start"] => Step::FramesStart,
            ["stop", csv] => Step::FramesStop(csv.into()),
            _ => {
                return Err("usage: frames start | frames stop \
                            <csv>"
                    .into());
            }
        },
        "record" => match words[..] {
            ["start", dir] => Step::RecordStart(dir.into()),
            ["stop"] => Step::RecordStop,
            _ => {
                return Err("usage: record start <dir> | record \
                            stop"
                    .into());
            }
        },
        "resize" => {
            let [width, height] = exact(&words, "resize <w> <h>")?;
            Step::Resize {
                width: number(width)?,
                height: number(height)?,
            }
        }
        "open" => {
            if words.is_empty() {
                return Err("usage: open <path>...".into());
            }
            Step::Open(words.iter().map(PathBuf::from).collect())
        }
        "clipboard" => {
            exact::<0>(&words, "clipboard")?;
            Step::Clipboard
        }
        "expect" => {
            let (topic, text) = split_topic(rest)?;
            if text.is_empty() {
                return Err("usage: expect <topic> <text>".into());
            }
            Step::Expect {
                topic,
                text: text.into(),
            }
        }
        "exit" => {
            exact::<0>(&words, "exit")?;
            Step::Exit
        }
        other => return Err(format!("unknown step `{other}`")),
    };
    Ok(Some(step))
}

fn topic_args(name: &str) -> usize {
    match name {
        "runs" | "text" => 1,
        _ => 0,
    }
}

fn split_topic(text: &str) -> Result<(String, &str), String> {
    let (name, mut rest) = first_word(text);
    let mut topic = name.to_owned();
    for _ in 0..topic_args(name) {
        let (word, after) = first_word(rest);
        if word.is_empty() {
            return Err(format!(
                "topic `{name}` takes a line number"
            ));
        }
        number::<usize>(word)?;
        topic = format!("{topic} {word}");
        rest = after;
    }
    Ok((topic, rest.trim()))
}

fn first_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    text.split_once(char::is_whitespace).unwrap_or((text, ""))
}

fn exact<'a, const N: usize>(
    words: &[&'a str],
    usage: &str,
) -> Result<[&'a str; N], String> {
    <[&str; N]>::try_from(words)
        .map_err(|_| format!("usage: {usage}"))
}

fn number<T: FromStr>(word: &str) -> Result<T, String> {
    word.parse()
        .map_err(|_| format!("`{word}` is not a number"))
}

const LETTERS: [KeyCode; 26] = [
    KeyCode::KeyA,
    KeyCode::KeyB,
    KeyCode::KeyC,
    KeyCode::KeyD,
    KeyCode::KeyE,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyM,
    KeyCode::KeyN,
    KeyCode::KeyO,
    KeyCode::KeyP,
    KeyCode::KeyQ,
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
    KeyCode::KeyV,
    KeyCode::KeyW,
    KeyCode::KeyX,
    KeyCode::KeyY,
    KeyCode::KeyZ,
];

const DIGITS: [KeyCode; 10] = [
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

fn character(c: char) -> Press {
    let code = match c {
        'a'..='z' => LETTERS[c as usize - 'a' as usize],
        'A'..='Z' => LETTERS[c as usize - 'A' as usize],
        '0'..='9' => DIGITS[c as usize - '0' as usize],
        ' ' => KeyCode::Space,
        _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
    };
    let key = if c == ' ' {
        Key::Space
    } else {
        Key::Character(c.to_string().into())
    };
    Press {
        code,
        key,
        text: Some(c.to_string()),
    }
}

/// The key a modifier name holds. `mod` is the command key, Ctrl on
/// Linux and Cmd, which Bevy calls Super, on macOS.
fn modifier(name: &str, os: Os) -> Option<Press> {
    let (code, key) = match (name, os) {
        ("ctrl" | "control", _) | ("mod", Os::Linux) => {
            (KeyCode::ControlLeft, Key::Control)
        }
        ("super" | "meta", _) | ("mod", Os::Macos) => {
            (KeyCode::SuperLeft, Key::Super)
        }
        ("alt", _) => (KeyCode::AltLeft, Key::Alt),
        ("shift", _) => (KeyCode::ShiftLeft, Key::Shift),
        _ => return None,
    };
    Some(Press {
        code,
        key,
        text: None,
    })
}

/// The modifiers a `click` or a `drag` holds, each named once.
fn held(
    names: &[&str],
    os: Os,
    usage: &str,
) -> Result<Vec<Press>, String> {
    let mut held = Vec::new();
    for &name in names {
        match modifier(name, os) {
            Some(press)
                if matches!(name, "shift" | "mod" | "alt")
                    && !held.contains(&press) =>
            {
                held.push(press);
            }
            _ => return Err(format!("usage: {usage}")),
        }
    }
    Ok(held)
}

fn parse_combo(combo: &str, os: Os) -> Result<Vec<Press>, String> {
    let mut parts = combo.split('+').collect::<Vec<_>>();
    let name = parts
        .pop()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("`{combo}` names no key"))?;
    let mut presses = Vec::new();
    let (mut shift, mut command) = (false, false);
    for part in parts {
        let press = modifier(part, os)
            .ok_or_else(|| format!("`{part}` is not a modifier"))?;
        if press.key == Key::Shift {
            shift = true;
        } else {
            command = true;
        }
        presses.push(press);
    }
    let named = |code, key| Press {
        code,
        key,
        text: None,
    };
    let mut press = match name {
        "enter" | "return" => named(KeyCode::Enter, Key::Enter),
        "esc" | "escape" => named(KeyCode::Escape, Key::Escape),
        "tab" => named(KeyCode::Tab, Key::Tab),
        "backspace" => named(KeyCode::Backspace, Key::Backspace),
        "delete" => named(KeyCode::Delete, Key::Delete),
        "up" => named(KeyCode::ArrowUp, Key::ArrowUp),
        "down" => named(KeyCode::ArrowDown, Key::ArrowDown),
        "left" => named(KeyCode::ArrowLeft, Key::ArrowLeft),
        "right" => named(KeyCode::ArrowRight, Key::ArrowRight),
        "home" => named(KeyCode::Home, Key::Home),
        "end" => named(KeyCode::End, Key::End),
        "pageup" => named(KeyCode::PageUp, Key::PageUp),
        "pagedown" => named(KeyCode::PageDown, Key::PageDown),
        "space" => character(' '),
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => {
                    character(if shift {
                        c.to_ascii_uppercase()
                    } else {
                        c.to_ascii_lowercase()
                    })
                }
                _ => return Err(format!("`{name}` is not a key")),
            }
        }
    };
    if command {
        press.text = None;
    }
    presses.push(press);
    Ok(presses)
}

fn parse_ime(rest: &str) -> Result<Ime, String> {
    let window = Entity::PLACEHOLDER;
    let (form, text) = first_word(rest);
    let text = text.trim();
    Ok(match form {
        "enable" if text.is_empty() => Ime::Enabled { window },
        "disable" if text.is_empty() => Ime::Disabled { window },
        "commit" if !text.is_empty() => Ime::Commit {
            window,
            value: text.into(),
        },
        "preedit" => {
            let (value, cursor) = preedit(text)?;
            Ime::Preedit {
                window,
                value: value.into(),
                cursor,
            }
        }
        _ => {
            return Err("usage: ime enable | ime disable | ime \
                        preedit [<text> [<start> <end>]] | ime \
                        commit <text>"
                .into());
        }
    })
}

/// A preedit cursor's start and end, in bytes of its text.
type Cursor = (usize, usize);

/// Splits `ime preedit` words into the text and its cursor. The last
/// two words are the cursor, in bytes of the text, when at least one
/// word comes before them and both are numbers.
fn preedit(text: &str) -> Result<(&str, Option<Cursor>), String> {
    let words = text.split_whitespace().collect::<Vec<_>>();
    let [_, .., start, end] = words[..] else {
        return Ok((text, None));
    };
    let (Ok(from), Ok(to)) = (start.parse(), end.parse()) else {
        return Ok((text, None));
    };
    let value = text
        .strip_suffix(end)
        .and_then(|text| text.trim_end().strip_suffix(start))
        .map(str::trim_end)
        .expect("the cursor words end the text");
    for byte in [from, to] {
        if byte > value.len() {
            return Err(format!(
                "preedit cursor {from}..{to} is past the {} bytes \
                 of {value:?}",
                value.len()
            ));
        }
        if !value.is_char_boundary(byte) {
            return Err(format!(
                "preedit cursor byte {byte} is inside a character \
                 of {value:?}"
            ));
        }
    }
    if from > to {
        return Err(format!(
            "preedit cursor {from}..{to} runs backwards"
        ));
    }
    Ok((value, Some((from, to))))
}

fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next() {
            Some('n') => '\n',
            Some('t') => '\t',
            Some('\\') => '\\',
            other => {
                let shown = other.map_or(String::new(), String::from);
                return Err(format!(
                    "`\\{shown}` is not an escape, use \\n, \\t or \
                     \\\\"
                ));
            }
        });
    }
    Ok(out)
}

#[derive(Clone, Debug)]
enum Input {
    Move(Vec2),
    Button(ButtonState),
    Wheel(Vec2, MouseScrollUnit),
    Key(Press, ButtonState),
    Ime(Ime),
}

enum Busy {
    Idle,
    Input(VecDeque<Vec<Input>>, &'static str),
    Wait(u32),
    Screenshot,
    Resize {
        want: UVec2,
        line: usize,
        deadline: Instant,
    },
    Exit,
    Done,
}

#[derive(Clone, Copy)]
enum Shot {
    Step,
    Frame,
}

struct Frames {
    last: Instant,
    millis: Vec<f64>,
}

struct Recording {
    dir: PathBuf,
    next: usize,
}

#[derive(Resource)]
pub struct Drive {
    steps: VecDeque<Line>,
    busy: Busy,
    pointer: Option<Vec2>,
    frames: Option<Frames>,
    recording: Option<Recording>,
    pending_shots: usize,
    pending_frames: usize,
    shot_this_frame: bool,
    said: Vec<String>,
    checks: usize,
    passed: usize,
    ime_live: bool,
}

/// The last `Ime` message the gallery read, from the driver or from
/// an input method.
#[derive(Resource, Default)]
struct LastIme(Option<Ime>);

fn read_ime(mut read: MessageReader<Ime>, mut last: ResMut<LastIme>) {
    if let Some(ime) = read.read().last() {
        last.0 = Some(ime.clone());
    }
}

pub struct DrivePlugin {
    lines: Vec<Line>,
    ime_live: bool,
}

impl Plugin for DrivePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Drive::new(
            self.lines.clone(),
            self.ime_live,
        ))
        .init_resource::<LastIme>()
        .add_systems(
            First,
            advance
                .after(MessageUpdateSystems)
                .before(PickingSystems::Input)
                .run_if(resource_exists::<Ready>),
        )
        .add_systems(Update, read_ime);
    }
}

impl DrivePlugin {
    pub fn new(script: &str) -> Result<Self, ParseError> {
        parse(script).map(|lines| Self {
            lines,
            ime_live: false,
        })
    }

    fn opening_from(mut self, dir: &Path) -> Self {
        for line in &mut self.lines {
            if let Step::Open(paths) = &mut line.step {
                for path in paths {
                    *path = dir.join(&*path);
                }
            }
        }
        self
    }
}

pub fn from_env() -> Result<Option<DrivePlugin>, String> {
    let Ok(path) = std::env::var("RUBRIX_DRIVE") else {
        return Ok(None);
    };
    let script = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {path}: {error}"))?;
    let dir = Path::new(&path).parent().unwrap_or(Path::new(""));
    let ime_live = std::env::var_os("RUBRIX_IME_LIVE")
        .is_some_and(|live| live == "1");
    DrivePlugin::new(&script)
        .map(|drive| {
            Some(DrivePlugin {
                ime_live,
                ..drive.opening_from(dir)
            })
        })
        .map_err(|error| error.to_string())
}

fn advance(world: &mut World) {
    world.resource_scope(|world, mut drive: Mut<Drive>| {
        drive.advance(world);
    });
}

impl Drive {
    fn new(lines: Vec<Line>, ime_live: bool) -> Self {
        Self {
            steps: lines.into(),
            busy: Busy::Idle,
            pointer: None,
            frames: None,
            recording: None,
            pending_shots: 0,
            pending_frames: 0,
            shot_this_frame: false,
            said: Vec::new(),
            checks: 0,
            passed: 0,
            ime_live,
        }
    }

    fn say(&mut self, line: String) {
        println!("{line}");
        self.said.push(line);
    }

    fn done(&mut self, name: &str) {
        self.say(format!("drive: {name} ok"));
    }

    fn fail(&mut self, world: &mut World, line: usize, reason: &str) {
        self.say(format!("drive: line {line}: {reason}"));
        world.write_message(AppExit::from_code(2));
        self.busy = Busy::Done;
    }

    fn advance(&mut self, world: &mut World) {
        let now = Instant::now();
        if let Some(frames) = &mut self.frames {
            let gap = now.duration_since(frames.last);
            frames.millis.push(gap.as_secs_f64() * 1e3);
            frames.last = now;
        }
        match mem::replace(&mut self.busy, Busy::Idle) {
            Busy::Idle => self.start(world),
            Busy::Input(queue, name) => {
                self.put_in(world, queue, name);
            }
            Busy::Wait(left) if left > 1 => {
                self.busy = Busy::Wait(left - 1);
            }
            Busy::Wait(_) => self.done("wait"),
            Busy::Screenshot if self.pending_shots > 0 => {
                self.busy = Busy::Screenshot;
            }
            Busy::Screenshot => self.done("screenshot"),
            Busy::Resize {
                want,
                line,
                deadline,
            } => {
                let size = window_size(world);
                if size == Some(want) {
                    self.done("resize");
                } else if Instant::now() >= deadline {
                    let now = size.map_or("missing".into(), |size| {
                        format!("{}x{}", size.x, size.y)
                    });
                    self.fail(
                        world,
                        line,
                        &format!(
                            "resize {}x{} did not take effect, the \
                             window is {now}",
                            want.x, want.y
                        ),
                    );
                } else {
                    self.busy = Busy::Resize {
                        want,
                        line,
                        deadline,
                    };
                }
            }
            Busy::Exit
                if self.pending_shots + self.pending_frames > 0 =>
            {
                self.busy = Busy::Exit
            }
            Busy::Exit => {
                self.say(format!(
                    "drive: {} of {} checks pass",
                    self.passed, self.checks
                ));
                if self.passed == self.checks {
                    self.done("exit");
                    world.write_message(AppExit::Success);
                } else {
                    self.say("drive: exit failed".into());
                    world.write_message(AppExit::from_code(1));
                }
                self.busy = Busy::Done;
            }
            Busy::Done => self.busy = Busy::Done,
        }
        self.record(world);
    }

    fn start(&mut self, world: &mut World) {
        let Some(Line { number, step }) = self.steps.pop_front()
        else {
            return;
        };
        let name = step.name();
        match step {
            Step::Page(index) => {
                world.insert_resource(Selected(index));
                self.done(name);
            }
            Step::Wait(0) => self.done(name),
            Step::Wait(frames) => self.busy = Busy::Wait(frames),
            Step::Move(at) => self.put_in(
                world,
                [vec![Input::Move(at)]].into(),
                name,
            ),
            Step::Click { at, count, held } => {
                let mut frames =
                    VecDeque::from([vec![Input::Move(at)]]);
                for _ in 0..count {
                    frames.push_back(vec![Input::Button(
                        ButtonState::Pressed,
                    )]);
                    frames.push_back(vec![Input::Button(
                        ButtonState::Released,
                    )]);
                }
                self.put_in(world, holding(held, frames), name);
            }
            Step::Press(at) => self.put_in(
                world,
                [
                    vec![Input::Move(at)],
                    vec![Input::Button(ButtonState::Pressed)],
                ]
                .into(),
                name,
            ),
            Step::Release => self.put_in(
                world,
                [vec![Input::Button(ButtonState::Released)]].into(),
                name,
            ),
            Step::Drag { from, to, held } => {
                let mut frames = VecDeque::from([
                    vec![Input::Move(from)],
                    vec![Input::Button(ButtonState::Pressed)],
                ]);
                for stage in 1..=DRAG_FRAMES {
                    let along = stage as f32 / DRAG_FRAMES as f32;
                    frames.push_back(vec![Input::Move(
                        from.lerp(to, along),
                    )]);
                }
                frames.push_back(vec![Input::Button(
                    ButtonState::Released,
                )]);
                self.put_in(world, holding(held, frames), name);
            }
            Step::Scroll(by) | Step::Wheel(by) => {
                let unit = match step {
                    Step::Wheel(_) => MouseScrollUnit::Line,
                    _ => MouseScrollUnit::Pixel,
                };
                let mut frames = VecDeque::new();
                if self.pointer.is_none() {
                    frames.push_back(vec![Input::Move(PAGE_CENTER)]);
                }
                frames.push_back(vec![Input::Wheel(by, unit)]);
                self.put_in(world, frames, name);
            }
            Step::Key(presses) => {
                self.put_in(world, chord(presses), name);
            }
            Step::Type(text) => {
                let typed = text
                    .chars()
                    .flat_map(|c| {
                        let press = character(c);
                        [
                            Input::Key(
                                press.clone(),
                                ButtonState::Pressed,
                            ),
                            Input::Key(press, ButtonState::Released),
                        ]
                    })
                    .collect();
                self.put_in(world, [typed].into(), name);
            }
            Step::Ime(ime) => {
                self.put_in(
                    world,
                    [vec![Input::Ime(ime)]].into(),
                    name,
                );
            }
            Step::Paste { text, chord: keys } => {
                world.resource_mut::<Paste>().0 = Some(text);
                self.put_in(world, chord(keys), name);
            }
            Step::Wtype(_) if !self.ime_live => self.fail(
                world,
                number,
                "wtype needs launch.sh --ime-live",
            ),
            Step::Wtype(text) => match wtype(&text) {
                Ok(()) => self.done(name),
                Err(error) => self.fail(world, number, &error),
            },
            Step::Screenshot(path) => match make_parent(&path) {
                Ok(()) => {
                    let mut paths = vec![path];
                    paths.extend(self.next_frame());
                    self.capture(world, paths, Shot::Step);
                    self.shot_this_frame = true;
                    self.busy = Busy::Screenshot;
                }
                Err(error) => self.fail(world, number, &error),
            },
            Step::Log(topic) => match topic_line(world, &topic) {
                Some(line) => {
                    self.say(line);
                    self.done(name);
                }
                None => self.fail(
                    world,
                    number,
                    &format!("no log topic `{topic}`"),
                ),
            },
            Step::Expect { topic, text } => {
                match topic_line(world, &topic) {
                    Some(line) => {
                        self.checks += 1;
                        let verdict = if line.contains(&text) {
                            self.passed += 1;
                            "pass"
                        } else {
                            "fail"
                        };
                        self.say(format!(
                            "accept: {topic} {verdict}"
                        ));
                        self.done(name);
                    }
                    None => self.fail(
                        world,
                        number,
                        &format!("no log topic `{topic}`"),
                    ),
                }
            }
            Step::FramesStart => {
                self.frames = Some(Frames {
                    last: Instant::now(),
                    millis: Vec::new(),
                });
                self.done(name);
            }
            Step::FramesStop(path) => match self.frames.take() {
                Some(frames) => {
                    let mut csv = String::from("frame,ms\n");
                    for (index, millis) in
                        frames.millis.iter().enumerate()
                    {
                        csv.push_str(&format!(
                            "{index},{millis:.4}\n"
                        ));
                    }
                    match write_file(&path, csv) {
                        Ok(()) => self.done(name),
                        Err(error) => {
                            self.fail(world, number, &error);
                        }
                    }
                }
                None => self.fail(
                    world,
                    number,
                    "frames stop without frames start",
                ),
            },
            Step::RecordStart(dir) => {
                match fs::create_dir_all(&dir) {
                    Ok(()) => {
                        self.recording =
                            Some(Recording { dir, next: 0 });
                        self.done(name);
                    }
                    Err(error) => self.fail(
                        world,
                        number,
                        &format!(
                            "cannot create {}: {error}",
                            dir.display()
                        ),
                    ),
                }
            }
            Step::RecordStop => {
                self.recording = None;
                self.done(name);
            }
            Step::Resize { width, height } => {
                match hyprland_resize(width, height) {
                    Ok(()) => {
                        self.busy = Busy::Resize {
                            want: UVec2::new(width, height),
                            line: number,
                            deadline: Instant::now() + RESIZE_WAIT,
                        };
                    }
                    Err(error) => self.fail(world, number, &error),
                }
            }
            Step::Open(paths) => {
                let page = &PAGES[world.resource::<Selected>().0];
                match page.open.map(|open| open(world, &paths)) {
                    Some(Ok(())) => self.done(name),
                    Some(Err(error)) => {
                        self.fail(world, number, &error)
                    }
                    None => self.fail(
                        world,
                        number,
                        &format!("page {} takes no files", page.name),
                    ),
                }
            }
            Step::Clipboard => {
                let copied = world.resource::<Copied>();
                let line = match &copied.0 {
                    Some(text) => format!("clipboard: {text}"),
                    None => "clipboard: (nothing copied)".into(),
                };
                self.say(line);
                self.done(name);
            }
            Step::Exit => {
                self.recording = None;
                self.busy = Busy::Exit;
            }
        }
    }

    fn put_in(
        &mut self,
        world: &mut World,
        mut queue: VecDeque<Vec<Input>>,
        name: &'static str,
    ) {
        for input in queue.pop_front().unwrap_or_default() {
            self.inject(world, input);
        }
        if queue.is_empty() {
            self.done(name);
        } else {
            self.busy = Busy::Input(queue, name);
        }
    }

    fn inject(&mut self, world: &mut World, input: Input) {
        let Some(window) = world
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .iter(world)
            .next()
        else {
            return;
        };
        match input {
            Input::Move(position) => {
                let delta = self.pointer.map(|last| position - last);
                self.pointer = Some(position);
                send(
                    world,
                    CursorMoved {
                        window,
                        position,
                        delta,
                    },
                );
            }
            Input::Button(state) => send(
                world,
                MouseButtonInput {
                    button: MouseButton::Left,
                    state,
                    window,
                },
            ),
            Input::Wheel(by, unit) => send(
                world,
                MouseWheel {
                    unit,
                    x: by.x,
                    y: by.y,
                    window,
                    phase: TouchPhase::Moved,
                },
            ),
            Input::Key(press, state) => send(
                world,
                KeyboardInput {
                    key_code: press.code,
                    logical_key: press.key,
                    state,
                    text: press.text.map(Into::into),
                    repeat: false,
                    window,
                },
            ),
            Input::Ime(mut ime) => {
                let (Ime::Preedit { window: to, .. }
                | Ime::Commit { window: to, .. }
                | Ime::Enabled { window: to }
                | Ime::Disabled { window: to }) = &mut ime;
                *to = window;
                send(world, ime);
            }
        }
    }

    fn pending(&mut self, shot: Shot) -> &mut usize {
        match shot {
            Shot::Step => &mut self.pending_shots,
            Shot::Frame => &mut self.pending_frames,
        }
    }

    fn capture(
        &mut self,
        world: &mut World,
        paths: Vec<PathBuf>,
        shot: Shot,
    ) {
        let mut capture = world.spawn(Screenshot::primary_window());
        for path in paths {
            capture.observe(save_to_disk(path));
        }
        capture.observe(
            move |_: On<ScreenshotCaptured>,
                  mut drive: ResMut<Drive>| {
                let pending = drive.pending(shot);
                *pending = pending.saturating_sub(1);
            },
        );
        *self.pending(shot) += 1;
    }

    fn next_frame(&mut self) -> Option<PathBuf> {
        let recording = self.recording.as_mut()?;
        let path = recording
            .dir
            .join(format!("frame-{:05}.png", recording.next));
        recording.next += 1;
        Some(path)
    }

    /// Bevy drops a second capture of one window in a frame without
    /// firing `ScreenshotCaptured`.
    fn record(&mut self, world: &mut World) {
        if mem::take(&mut self.shot_this_frame) {
            return;
        }
        if let Some(path) = self.next_frame() {
            self.capture(world, vec![path], Shot::Frame);
        }
    }
}

/// Presses every key in order, then lets them go in reverse.
fn chord(presses: Vec<Press>) -> VecDeque<Vec<Input>> {
    let down = presses
        .iter()
        .map(|press| Input::Key(press.clone(), ButtonState::Pressed))
        .collect();
    let up = presses
        .into_iter()
        .rev()
        .map(|press| Input::Key(press, ButtonState::Released))
        .collect();
    [down, up].into()
}

/// Holds `held` down from the first frame of a gesture to a frame
/// after its last.
fn holding(
    held: Vec<Press>,
    mut frames: VecDeque<Vec<Input>>,
) -> VecDeque<Vec<Input>> {
    if held.is_empty() {
        return frames;
    }
    let first = frames.front_mut().expect("a gesture has a frame");
    first.splice(
        0..0,
        held.iter().map(|press| {
            Input::Key(press.clone(), ButtonState::Pressed)
        }),
    );
    frames.push_back(
        held.into_iter()
            .rev()
            .map(|press| Input::Key(press, ButtonState::Released))
            .collect(),
    );
    frames
}

/// Types `text` through the OS into the focused window, which must be
/// this gallery, so a lost focus never types into another app.
fn wtype(text: &str) -> Result<(), String> {
    let active = Command::new("hyprctl")
        .arg("activewindow")
        .output()
        .map_err(|error| format!("cannot run hyprctl: {error}"))?;
    let pid =
        String::from_utf8_lossy(&active.stdout).lines().find_map(
            |line| line.trim().strip_prefix("pid: ")?.parse().ok(),
        );
    if pid != Some(std::process::id()) {
        return Err(format!(
            "wtype needs the gallery focused, the focused window is \
             pid {}",
            pid.map_or("none".into(), |pid: u32| pid.to_string())
        ));
    }
    let status = Command::new("wtype")
        .arg(text)
        .status()
        .map_err(|error| format!("cannot run wtype: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("wtype failed with {status}"))
    }
}

/// Writes `message` where bevy reads it from, and again as the
/// window event that picking reads.
fn send<M>(world: &mut World, message: M)
where
    M: bevy::ecs::message::Message + Clone + Into<WindowEvent>,
{
    world.write_message(message.clone().into());
    world.write_message(message);
}

fn window_size(world: &mut World) -> Option<UVec2> {
    let window = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()?;
    Some(UVec2::new(
        window.width().round() as u32,
        window.height().round() as u32,
    ))
}

fn hyprland_resize(width: u32, height: u32) -> Result<(), String> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return Err(
            "resize needs Hyprland, which sizes the window".into()
        );
    }
    let dispatch = format!(
        "hl.dsp.window.resize({{ window = \"pid:{}\", x = {width}, \
         y = {height} }})",
        std::process::id()
    );
    let output = Command::new("hyprctl")
        .args(["dispatch", &dispatch])
        .output()
        .map_err(|error| format!("cannot run hyprctl: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "hyprctl failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn scroll_offset(world: &mut World) -> f32 {
    let Some(&area) = page_areas(world).first() else {
        return 0.0;
    };
    scrollers(world, area)
        .first()
        .and_then(|&node| world.get::<ScrollPosition>(node))
        .map_or(0.0, |position| position.y)
}

fn make_parent(path: &Path) -> Result<(), String> {
    match path.parent() {
        Some(parent) => fs::create_dir_all(parent).map_err(|error| {
            format!("cannot create {}: {error}", parent.display())
        }),
        None => Ok(()),
    }
}

fn write_file(path: &Path, contents: String) -> Result<(), String> {
    make_parent(path)?;
    fs::write(path, contents).map_err(|error| {
        format!("cannot write {}: {error}", path.display())
    })
}

fn topic_line(world: &mut World, topic: &str) -> Option<String> {
    if let Some(ask) = Ask::of(topic) {
        return Some(format!("{topic} {}", answer(world, ask)));
    }
    Some(match topic {
        "entities" => {
            let count = world.entities().count_spawned();
            let sample = pages::hamlet_rich::sample_ids(world);
            if sample.is_empty() {
                format!("entities {count}")
            } else {
                format!(
                    "entities {count} sample {}",
                    sample.join(" ")
                )
            }
        }
        "page" => {
            format!(
                "page {}",
                PAGES[world.resource::<Selected>().0].name
            )
        }
        "present_mode" => {
            let mode = world
                .query_filtered::<&Window, With<PrimaryWindow>>()
                .iter(world)
                .next()?
                .present_mode;
            format!("present_mode requested {mode:?}")
        }
        "rows" => {
            match world
                .query::<(&VirtualList, &ScrollPosition)>()
                .iter(world)
                .next()
            {
                Some((list, position)) => match list.spawned() {
                    Some(spawned) => format!(
                        "rows first_visible {} spawned {}..={} rows \
                         {} total {} following {} x {}",
                        list.first_visible(),
                        spawned.start(),
                        spawned.end(),
                        list.rows(),
                        list.count(),
                        list.following(),
                        position.x
                    ),
                    None => "rows empty total 0".into(),
                },
                None => "rows none".into(),
            }
        }
        "scroll" => format!("scroll {}", scroll_offset(world)),
        "diff" => diff(world),
        "stream" => pages::stream::log(world),
        "rich-styles" => {
            let palette = world.resource::<Mode>().0;
            pages::hamlet_rich::styles(world, &palette)
        }
        "rich-stream" => {
            let palette = world.resource::<Mode>().0;
            pages::hamlet_rich::streamed(world, &palette)
        }
        "code-plate" => pages::hamlet_code::plate_line(world),
        "code-grow" => pages::hamlet_code::growth_line(world),
        "code-blank" => pages::hamlet_code::blank_line(world),
        "copied" => match &world.resource::<Copied>().0 {
            Some(text) => format!("copied {text:?}"),
            None => "copied none".into(),
        },
        "paste" => match &world.resource::<Paste>().0 {
            Some(text) => format!("paste {text:?}"),
            None => "paste none".into(),
        },
        "ime" => ime(world),
        "diff-colours" => pages::hamlet_diff::colours_line(world),
        "diff-more" => pages::hamlet_diff::more_line(world),
        "stress" => pages::stress::log(world),
        "stress-blank" => pages::stress::blank_line(world),
        "selection" => selection(world),
        "theme" => {
            format!("theme {}", world.resource::<Mode>().0.name())
        }
        "spans" => match spans_under::<LongLineRoot>(world) {
            Some(spans) => {
                let listed = spans
                    .iter()
                    .map(|(span, text, color)| {
                        format!("{span} {text:?} {}", color.to_hex())
                    })
                    .collect::<Vec<_>>();
                format!(
                    "spans {}: {}",
                    spans.len(),
                    listed.join(", ")
                )
            }
            None => "spans none".into(),
        },
        "hit" => {
            let text = spans_under::<HitLineRoot>(world)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, text, _)| text)
                .collect::<String>();
            let offset = world.resource::<Hit>().byte;
            let hit = offset.and_then(|offset| {
                let at = text.get(offset..)?.chars().next()?;
                Some(format!("hit {offset} {at:?}"))
            });
            hit.unwrap_or_else(|| "hit none".into())
        }
        _ => {
            let line = topic.strip_prefix("runs ")?.parse().ok()?;
            runs(world, line)
        }
    })
}

/// The page on show answers `ask`, or `n/a` when it has no answer.
fn answer(world: &mut World, ask: Ask) -> String {
    let page = &PAGES[world.resource::<Selected>().0];
    page.answer
        .and_then(|answer| answer(world, ask))
        .unwrap_or_else(|| "n/a".into())
}

fn ime(world: &mut World) -> String {
    let (enabled, position) = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map_or((false, Vec2::ZERO), |window| {
            (window.ime_enabled, window.ime_position)
        });
    let last = match &world.resource::<LastIme>().0 {
        None => "none".into(),
        Some(Ime::Enabled { .. }) => "enabled".into(),
        Some(Ime::Disabled { .. }) => "disabled".into(),
        Some(Ime::Commit { value, .. }) => {
            format!("commit {value:?}")
        }
        Some(Ime::Preedit {
            value,
            cursor: Some((from, to)),
            ..
        }) => format!("preedit {value:?} {from}..{to}"),
        Some(Ime::Preedit { value, .. }) => {
            format!("preedit {value:?}")
        }
    };
    format!(
        "ime enabled {enabled} position {} {} last {last} page {}",
        position.x,
        position.y,
        answer(world, Ask::Ime)
    )
}

fn diff(world: &mut World) -> String {
    let Some((view, doc)) =
        world.query::<(Entity, &DiffDoc)>().iter(world).next()
    else {
        return "diff none".into();
    };
    let first = pages::find_under::<VirtualList>(world, view)
        .and_then(|list| world.get::<VirtualList>(list))
        .map_or(0, VirtualList::first_visible);
    let shape = match (doc.is_compact(), doc.layout()) {
        (true, _) => "compact",
        (false, Layout::Stacked) => "stacked",
        (false, Layout::Split) => "split",
    };
    let number = |line: Option<rubrix_diff::LineRef>| {
        line.map_or("-".into(), |line| line.number.to_string())
    };
    let row = match doc.row(first) {
        Some(DiffRow::Header { file }) => {
            let diff =
                doc.source().files().nth(file).expect("a file");
            let name = diff
                .new_name()
                .or(diff.old_name())
                .unwrap_or_default();
            format!("header {file} {name}")
        }
        Some(DiffRow::Body {
            file,
            row: Row::Gap { hidden, .. },
        }) => {
            format!("gap {file} hidden {hidden}")
        }
        Some(DiffRow::Body { file, row }) => format!(
            "line {file} old {} new {}",
            number(row.old_line()),
            number(row.new_line())
        ),
        Some(DiffRow::More { lines }) => format!("more {lines}"),
        None => "none".into(),
    };
    format!(
        "diff files {} rows {} {shape} first {first} {row}",
        doc.source().files().len(),
        doc.len()
    )
}

fn selection(world: &mut World) -> String {
    let Some(selection) =
        world.query::<&CodeSelection>().iter(world).next()
    else {
        return "selection none".into();
    };
    let (anchor, focus, range) =
        (selection.anchor(), selection.focus(), selection.range());
    format!(
        "selection anchor {}..{} focus {}..{} range {}..{} length {}",
        anchor.start,
        anchor.end,
        focus.start,
        focus.end,
        range.start,
        range.end,
        range.len()
    )
}

fn runs(world: &mut World, line_number: usize) -> String {
    let Some((view, row, runs)) = world
        .query::<(Entity, &CodeDoc)>()
        .iter(world)
        .next()
        .and_then(|(view, doc)| {
            let row = line_number
                .checked_sub(1)
                .filter(|&row| row < doc.rows())?;
            let runs = spans(doc.runs(row))
                .map(|(bytes, kind)| {
                    format!("{}..{} {kind}", bytes.start, bytes.end)
                })
                .collect::<Vec<_>>();
            Some((view, row, runs))
        })
    else {
        return format!("runs {line_number} none");
    };
    let runs = if runs.is_empty() {
        "empty".to_owned()
    } else {
        runs.join(", ")
    };
    let root = world
        .query::<(Entity, &CodeLine)>()
        .iter(world)
        .find(|(_, shown)| **shown == CodeLine { view, row })
        .map(|(root, _)| root);
    let shown = match root {
        Some(root) => {
            let spans = spans_of(world, root)
                .into_iter()
                .map(|(_, text, color)| {
                    format!("{text:?} {}", color.to_hex())
                })
                .collect::<Vec<_>>();
            format!("shown {}", spans.join(", "))
        }
        None => "not shown".to_owned(),
    };
    format!("runs {line_number}: {runs} | {shown}")
}

fn spans_under<M: Component>(
    world: &mut World,
) -> Option<Vec<(Entity, String, Srgba)>> {
    let root = world
        .query_filtered::<Entity, With<M>>()
        .iter(world)
        .next()?;
    world
        .get::<Children>(root)
        .is_some()
        .then(|| spans_of(world, root))
}

fn spans_of(
    world: &World,
    root: Entity,
) -> Vec<(Entity, String, Srgba)> {
    world
        .get::<Children>(root)
        .into_iter()
        .flat_map(|kids| kids.iter().copied())
        .filter_map(|span| {
            let text = world.get::<TextSpan>(span)?;
            let color = world.get::<TextColor>(span)?;
            Some((span, text.0.clone(), color.0.to_srgba()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use bevy::app::{Last, PostUpdate};
    use bevy::asset::{AssetApp, AssetId, Assets};
    use bevy::camera::visibility::{
        InheritedVisibility, VisibilitySystems,
    };
    use bevy::camera::{
        Camera, Camera2d, ComputedCameraValues, RenderTargetInfo,
    };
    use bevy::color::{Color, LinearRgba, Luminance};
    use bevy::ecs::message::Messages;
    use bevy::ecs::query::Added;
    use bevy::ecs::system::{Commands, Local, Query, Res};
    use bevy::image::Image;
    use bevy::input::{ButtonInput, InputPlugin};
    use bevy::math::UVec2;
    use bevy::picking::DefaultPickingPlugins;
    use bevy::text::Font;
    use bevy::time::TimeUpdateStrategy;
    use bevy::transform::TransformPlugin;
    use bevy::ui::widget::Text;
    use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
    use bevy::window::WindowPlugin;
    use bevy_fynix::scroll::{ScrollGoal, ScrollbarStyle};
    use bevy_fynix::tokens::SurfaceTokens;
    use bevy_fynix::{FynixPlugin, mount};
    use bevy_rubrix::RubrixPlugin;

    use super::*;
    use crate::{FONT, Palette, ShellPlugin, shell, toggle_theme};

    fn steps(script: &str) -> Vec<Step> {
        parse(script)
            .expect("a valid script")
            .into_iter()
            .map(|line| line.step)
            .collect()
    }

    fn key(code: KeyCode, key: Key, text: Option<&str>) -> Press {
        Press {
            code,
            key,
            text: text.map(Into::into),
        }
    }

    fn shift() -> Press {
        key(KeyCode::ShiftLeft, Key::Shift, None)
    }

    #[test]
    fn every_step_parses() {
        let script = "\
page about
wait 12
move 1.5 20
click 10 20
click 10 20 double
click 10 20 triple
click 10 20 shift
press 5 6
release
drag 1 2 3 4
scroll 0 -400
wheel 0 -3
key ctrl+a
key enter
type hello  world
screenshot out/shot.png
log entities
frames start
frames stop out/frames.csv
record start out/video
record stop
resize 800 600
open a.txt b.txt
clipboard
expect page about
exit
";
        assert_eq!(
            steps(script),
            vec![
                Step::Page(0),
                Step::Wait(12),
                Step::Move(Vec2::new(1.5, 20.0)),
                Step::Click {
                    at: Vec2::new(10.0, 20.0),
                    count: 1,
                    held: vec![]
                },
                Step::Click {
                    at: Vec2::new(10.0, 20.0),
                    count: 2,
                    held: vec![]
                },
                Step::Click {
                    at: Vec2::new(10.0, 20.0),
                    count: 3,
                    held: vec![]
                },
                Step::Click {
                    at: Vec2::new(10.0, 20.0),
                    count: 1,
                    held: vec![shift()]
                },
                Step::Press(Vec2::new(5.0, 6.0)),
                Step::Release,
                Step::Drag {
                    from: Vec2::new(1.0, 2.0),
                    to: Vec2::new(3.0, 4.0),
                    held: vec![]
                },
                Step::Scroll(Vec2::new(0.0, -400.0)),
                Step::Wheel(Vec2::new(0.0, -3.0)),
                Step::Key(vec![
                    key(KeyCode::ControlLeft, Key::Control, None),
                    key(
                        KeyCode::KeyA,
                        Key::Character("a".into()),
                        None
                    ),
                ]),
                Step::Key(vec![key(
                    KeyCode::Enter,
                    Key::Enter,
                    None
                )]),
                Step::Type("hello  world".into()),
                Step::Screenshot("out/shot.png".into()),
                Step::Log("entities".into()),
                Step::FramesStart,
                Step::FramesStop("out/frames.csv".into()),
                Step::RecordStart("out/video".into()),
                Step::RecordStop,
                Step::Resize {
                    width: 800,
                    height: 600
                },
                Step::Open(vec!["a.txt".into(), "b.txt".into()]),
                Step::Clipboard,
                Step::Expect {
                    topic: "page".into(),
                    text: "about".into()
                },
                Step::Exit,
            ]
        );
    }

    fn steps_on(os: Os, script: &str) -> Vec<Step> {
        parse_for(script, os)
            .expect("a valid script")
            .into_iter()
            .map(|line| line.step)
            .collect()
    }

    fn ctrl() -> Press {
        key(KeyCode::ControlLeft, Key::Control, None)
    }

    fn cmd() -> Press {
        key(KeyCode::SuperLeft, Key::Super, None)
    }

    #[test]
    fn mod_holds_ctrl_on_linux_and_cmd_on_macos() {
        let a = key(KeyCode::KeyA, Key::Character("a".into()), None);
        for (os, command) in [(Os::Linux, ctrl()), (Os::Macos, cmd())]
        {
            assert_eq!(
                steps_on(os, "key mod+a"),
                [Step::Key(vec![command.clone(), a.clone()])],
                "{os:?}"
            );
            assert_eq!(
                steps_on(os, "click 10 10 mod\ndrag 0 0 9 9 alt"),
                [
                    Step::Click {
                        at: Vec2::new(10.0, 10.0),
                        count: 1,
                        held: vec![command.clone()]
                    },
                    Step::Drag {
                        from: Vec2::ZERO,
                        to: Vec2::new(9.0, 9.0),
                        held: vec![key(
                            KeyCode::AltLeft,
                            Key::Alt,
                            None
                        )]
                    },
                ]
            );
        }
    }

    #[test]
    fn a_mod_chord_parses_in_every_combo_form() {
        let forms = [
            "mod+shift+z",
            "shift+mod+z",
            "mod+alt+up",
            "mod+enter",
            "mod+backspace",
            "mod+space",
            "mod+1",
            "mod+home",
            "mod+pagedown",
        ];
        for os in [Os::Linux, Os::Macos] {
            let command = modifier("mod", os).expect("mod");
            for form in forms {
                let [Step::Key(presses)] =
                    &steps_on(os, &format!("key {form}"))[..]
                else {
                    panic!("{form} is one key step");
                };
                assert!(
                    presses.contains(&command),
                    "{form} on {os:?}"
                );
                let last = presses.last().expect("a key");
                assert_eq!(last.text, None, "{form} types no text");
            }
        }
    }

    #[test]
    fn click_and_drag_take_each_modifier_once() {
        assert_eq!(
            steps_on(Os::Linux, "click 1 2 triple alt mod shift"),
            [Step::Click {
                at: Vec2::new(1.0, 2.0),
                count: 3,
                held: vec![
                    key(KeyCode::AltLeft, Key::Alt, None),
                    ctrl(),
                    shift()
                ]
            }]
        );
        for bad in [
            "click 1 2 mod mod",
            "click 1 2 ctrl",
            "click 1 2 shift double",
            "drag 1 2 3 4 super",
            "drag 1 2 3",
        ] {
            let error = parse(bad).expect_err(bad);
            assert!(
                error.reason.starts_with("usage: "),
                "{bad}: {error}"
            );
        }
    }

    #[test]
    fn each_ime_form_parses_to_the_message_it_writes() {
        let window = Entity::PLACEHOLDER;
        let preedit = |value: &str, cursor| {
            Step::Ime(Ime::Preedit {
                window,
                value: value.into(),
                cursor,
            })
        };
        assert_eq!(
            steps(
                "ime enable\nime disable\nime preedit 你好 0 6\nime \
                 preedit ni hao 2 2\nime preedit ni hao\nime \
                 preedit\nime commit 你好 吗"
            ),
            [
                Step::Ime(Ime::Enabled { window }),
                Step::Ime(Ime::Disabled { window }),
                preedit("你好", Some((0, 6))),
                preedit("ni hao", Some((2, 2))),
                preedit("ni hao", None),
                preedit("", None),
                Step::Ime(Ime::Commit {
                    window,
                    value: "你好 吗".into()
                }),
            ]
        );
    }

    #[test]
    fn a_preedit_cursor_off_its_text_is_refused_with_its_line() {
        for (script, reason) in [
            ("page about\nime preedit abc 2 9", "past the 3 bytes"),
            (
                "page about\nime preedit 你好 1 3",
                "inside a character",
            ),
            ("page about\nime preedit abc 2 1", "runs backwards"),
            ("page about\nime commit", "usage: ime"),
            ("page about\nime enable now", "usage: ime"),
        ] {
            let error = parse(script).expect_err(script);
            assert_eq!(error.line, 2, "{script}");
            assert!(error.reason.contains(reason), "{error}");
        }
    }

    #[test]
    fn a_paste_decodes_its_escapes_and_sends_mod_v() {
        let v = key(KeyCode::KeyV, Key::Character("v".into()), None);
        assert_eq!(
            steps_on(Os::Macos, "paste a\\nb\\t\\\\c"),
            [Step::Paste {
                text: "a\nb\t\\c".into(),
                chord: vec![cmd(), v]
            }]
        );
        let error = parse("paste a\\qb").expect_err("a bad escape");
        assert!(
            error.reason.contains("`\\q` is not an escape"),
            "{error}"
        );
    }

    #[test]
    fn the_page_topics_parse_with_their_words() {
        assert_eq!(
            steps(
                "log text 3\nexpect undo 2\nlog caret\nlog \
                 paste\nlog ime"
            ),
            [
                Step::Log("text 3".into()),
                Step::Expect {
                    topic: "undo".into(),
                    text: "2".into()
                },
                Step::Log("caret".into()),
                Step::Log("paste".into()),
                Step::Log("ime".into()),
            ]
        );
        for (script, reason) in [
            ("log text", "takes a line number"),
            ("log text three", "`three` is not a number"),
            ("expect text 3", "usage: expect"),
        ] {
            let error = parse(script).expect_err(script);
            assert!(
                error.reason.contains(reason),
                "{script}: {error}"
            );
        }
    }

    #[test]
    fn a_shifted_key_types_its_capital() {
        assert_eq!(
            steps("key shift+a"),
            vec![Step::Key(vec![
                key(KeyCode::ShiftLeft, Key::Shift, None),
                key(
                    KeyCode::KeyA,
                    Key::Character("A".into()),
                    Some("A")
                ),
            ])]
        );
    }

    #[test]
    fn a_bad_line_reports_its_number() {
        let bad = [
            ("page about\nwait 1\nbogus 1 2\nexit", 3, "bogus"),
            ("\n# a note\n\nbogus", 4, "bogus"),
            ("page nowhere", 1, "nowhere"),
            ("wait soon", 1, "soon"),
            ("move 1", 1, "usage: move"),
            ("click 1 2 quadruple", 1, "usage: click"),
            ("frames", 1, "usage: frames"),
            ("key ctrl+", 1, "names no key"),
            ("key hyper+a", 1, "hyper"),
            ("open", 1, "usage: open"),
            ("exit now", 1, "usage: exit"),
        ];
        for (script, line, reason) in bad {
            let error = parse(script).expect_err(script);
            assert_eq!(error.line, line, "{script:?}");
            assert!(error.reason.contains(reason), "{error}");
        }
        assert_eq!(
            parse("a\nb").unwrap_err().to_string(),
            "line 1: unknown step `a`"
        );
    }

    #[test]
    fn log_runs_parses_its_line() {
        assert_eq!(
            steps("log runs 75"),
            [Step::Log("runs 75".into())]
        );
    }

    #[test]
    fn expect_runs_takes_the_text_after_the_line() {
        assert_eq!(
            steps(
                "expect runs 75 comment\nexpect rows total 1 \
                 following"
            ),
            [
                Step::Expect {
                    topic: "runs 75".into(),
                    text: "comment".into()
                },
                Step::Expect {
                    topic: "rows".into(),
                    text: "total 1 following".into()
                },
            ]
        );
    }

    #[test]
    fn log_runs_without_a_number_is_refused() {
        for (script, reason) in [
            ("log runs", "takes a line number"),
            ("log runs one", "`one` is not a number"),
            ("log runs 1 2", "usage: log"),
            ("expect runs 1", "usage: expect"),
        ] {
            let error = parse(script).expect_err(script);
            assert!(
                error.reason.contains(reason),
                "{script}: {error}"
            );
        }
    }

    #[test]
    fn an_open_path_is_read_from_the_script_directory() {
        let drive = DrivePlugin::new("open ../fixtures/a.rs /b.rs")
            .expect("a script")
            .opening_from(Path::new("/repo/drive"));
        assert_eq!(
            drive.lines[0].step,
            Step::Open(vec![
                "/repo/drive/../fixtures/a.rs".into(),
                "/b.rs".into()
            ])
        );
    }

    /// Stands in for Bevy's visibility propagation, whose plugin
    /// needs more than a headless app has. Picking skips what is not
    /// shown.
    fn show_everything(mut nodes: Query<&mut InheritedVisibility>) {
        for mut visibility in &mut nodes {
            *visibility = InheritedVisibility::VISIBLE;
        }
    }

    fn headless(script: &str) -> App {
        let mut app = App::new();
        app.add_plugins((
            bevy::time::TimePlugin,
            bevy::asset::AssetPlugin::default(),
            WindowPlugin::default(),
            TransformPlugin,
            InputPlugin,
            DefaultPickingPlugins,
            FynixPlugin::<Palette>::default(),
            RubrixPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(10),
        ))
        .add_plugins(ShellPlugin)
        .init_asset::<bevy::image::Image>()
        .init_asset::<bevy::image::TextureAtlasLayout>()
        .add_plugins((
            bevy::text::TextPlugin,
            UiPlugin,
            bevy::ui_widgets::ButtonPlugin,
        ))
        .add_systems(
            PostUpdate,
            show_everything
                .in_set(VisibilitySystems::VisibilityPropagate),
        );
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new(1280, 800),
                        scale_factor: 1.0,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .insert(
                AssetId::default(),
                Font::from_bytes(FONT.to_vec()),
            )
            .expect("the default font handle accepts a font");
        mount::<Palette>(app.world_mut(), shell());
        for _ in 0..3 {
            app.update();
        }
        app.add_plugins(DrivePlugin::new(script).expect("a script"))
            .insert_resource(Ready);
        app
    }

    fn run(app: &mut App) -> Vec<AppExit> {
        let mut exits = Vec::new();
        for _ in 0..40 {
            app.update();
            exits.extend(
                app.world_mut()
                    .resource_mut::<Messages<AppExit>>()
                    .drain(),
            );
        }
        exits
    }

    fn scrolled(app: &mut App) -> f32 {
        scroll_offset(app.world_mut())
    }

    #[test]
    fn a_scroll_step_scrolls_the_about_page() {
        let mut app = headless("scroll 0 -400\nwait 3\nexit");
        assert_eq!(scrolled(&mut app), 0.0);
        let exits = run(&mut app);
        assert_eq!(exits, vec![AppExit::Success]);
        assert_eq!(scrolled(&mut app), 400.0);
    }

    #[test]
    fn a_scroll_up_at_the_top_stays_there() {
        let mut app = headless("scroll 0 400\nexit");
        run(&mut app);
        assert_eq!(scrolled(&mut app), 0.0);
    }

    #[test]
    fn an_open_on_a_page_without_files_exits_with_code_2() {
        let mut app = headless("wait 1\nopen notes.txt\nexit");
        let exits = run(&mut app);
        assert_eq!(exits, vec![AppExit::from_code(2)]);
        let said = &app.world().resource::<Drive>().said;
        assert_eq!(
            said.last().map(String::as_str),
            Some("drive: line 2: page about takes no files")
        );
    }

    #[test]
    fn expect_passes_when_the_log_line_holds_the_text() {
        let mut app = headless(
            "expect page about\nexpect page other\nlog theme\nexit",
        );
        run(&mut app);
        let said = &app.world().resource::<Drive>().said;
        assert!(said.contains(&"accept: page pass".to_string()));
        assert!(said.contains(&"accept: page fail".to_string()));
        assert!(said.contains(&"theme dark".to_string()));
    }

    fn scroll_areas(app: &mut App) -> Vec<Entity> {
        app.world_mut()
            .query_filtered::<Entity, With<ScrollPosition>>()
            .iter(app.world())
            .collect()
    }

    #[test]
    fn a_theme_change_keeps_the_page_scrolled() {
        let mut app = headless("scroll 0 -400\nexit");
        run(&mut app);
        assert_eq!(scrolled(&mut app), 400.0);
        let before = scroll_areas(&mut app);
        toggle_theme(app.world_mut());
        for _ in 0..5 {
            app.update();
        }
        assert!(!app.world().resource::<Mode>().0.dark);
        assert_ne!(scroll_areas(&mut app), before, "built again");
        assert_eq!(scrolled(&mut app), 400.0);
    }

    #[test]
    fn a_wheel_turns_in_lines_and_eases_there() {
        let mut app = headless("wheel 0 -2\nwait 30\nexit");
        run(&mut app);
        let per_line = *app
            .world()
            .resource::<bevy::input::mouse::MouseScrollPixelsPerLine>(
        );
        assert_eq!(scrolled(&mut app), 2.0 * per_line);
    }

    #[test]
    fn the_scroll_log_reads_the_page_and_no_other_area() {
        let mut app = headless("scroll 0 -400\nexit");
        run(&mut app);
        app.world_mut().spawn((
            ScrollGoal::default(),
            ScrollPosition(Vec2::new(0.0, 9000.0)),
        ));
        assert_eq!(scrolled(&mut app), 400.0);
    }

    /// Bevy blends a translucent node over what is under it in
    /// linear light.
    fn contrast(thumb: Color, panel: Color) -> f32 {
        let (thumb, panel) = (thumb.to_linear(), panel.to_linear());
        let shown = LinearRgba::rgb(
            panel.red + (thumb.red - panel.red) * thumb.alpha,
            panel.green + (thumb.green - panel.green) * thumb.alpha,
            panel.blue + (thumb.blue - panel.blue) * thumb.alpha,
        );
        let (a, b) = (shown.luminance(), panel.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn the_light_scrollbar_stands_out_from_the_panel() {
        let mut app = headless("exit");
        toggle_theme(app.world_mut());
        let light = app.world().resource::<Mode>().0;
        assert!(!light.dark);
        let thumb = app.world().resource::<ScrollbarStyle>().rest;
        assert!(contrast(thumb, light.panel()) >= 3.0);
    }

    const CAPTURE_LATENCY: u32 = 5;

    /// Bevy captures one image of a window a frame, drops a second
    /// one asked for in the same frame without firing
    /// `ScreenshotCaptured`, and hands the image back a few frames
    /// later.
    fn capture_late(
        mut commands: Commands,
        shots: Query<Entity, Added<Screenshot>>,
        mut waiting: Local<Vec<(Entity, u32)>>,
    ) {
        let mut shots = shots.iter();
        if let Some(entity) = shots.next() {
            waiting.push((entity, 0));
        }
        for entity in shots {
            commands.entity(entity).despawn();
        }
        waiting.retain_mut(|(entity, age)| {
            *age += 1;
            if *age < CAPTURE_LATENCY {
                return true;
            }
            commands.trigger(ScreenshotCaptured {
                entity: *entity,
                image: Image::default(),
            });
            commands.entity(*entity).despawn();
            false
        });
    }

    #[test]
    fn a_screenshot_and_an_exit_finish_while_a_recording_runs() {
        let dir = std::env::current_exe()
            .expect("a test binary")
            .with_file_name(format!(
                "drive-test-{}",
                std::process::id()
            ));
        let shown = dir.display();
        let mut app = headless(&format!(
            "record start {shown}/one\nscroll 0 -20\nscreenshot \
             {shown}/shot.png\nrecord stop\nrecord start \
             {shown}/two\nexit"
        ));
        app.add_systems(Last, capture_late);
        let exits = run(&mut app);
        let said = app.world().resource::<Drive>().said.clone();
        let shot = dir.join("shot.png").exists();
        fs::remove_dir_all(&dir).ok();
        assert_eq!(exits, vec![AppExit::Success], "{said:?}");
        assert!(shot, "the screenshot step saved its file");
    }

    fn recorded(name: &str, steps: &str) -> (usize, usize) {
        let dir = std::env::current_exe()
            .expect("a test binary")
            .with_file_name(format!(
                "drive-test-{}-{name}",
                std::process::id()
            ));
        let shown = dir.display();
        let steps = steps.replace("{dir}", &shown.to_string());
        let mut app = headless(&format!(
            "record start {shown}/rec\n{steps}record stop\nexit"
        ));
        app.add_systems(Last, capture_late);
        let mut elapsed = 0;
        let mut exits = Vec::new();
        for _ in 0..80 {
            app.update();
            if app.world().resource::<Drive>().recording.is_some() {
                elapsed += 1;
            }
            exits.extend(
                app.world_mut()
                    .resource_mut::<Messages<AppExit>>()
                    .drain(),
            );
        }
        assert_eq!(exits, vec![AppExit::Success]);
        let frames = fs::read_dir(dir.join("rec"))
            .expect("the record directory")
            .count();
        fs::remove_dir_all(&dir).ok();
        (frames, elapsed)
    }

    #[test]
    fn a_recording_has_a_frame_for_every_frame_it_ran() {
        let (frames, elapsed) = recorded("plain", "wait 20\n");
        assert_eq!(frames, elapsed);
    }

    #[test]
    fn a_screenshot_step_costs_the_recording_no_frame() {
        let (frames, elapsed) = recorded(
            "one",
            "wait 8\nscreenshot {dir}/one.png\nwait 8\n",
        );
        assert_eq!(frames, elapsed);
        let (frames, elapsed) = recorded(
            "two",
            "wait 3\nscreenshot {dir}/a.png\nscreenshot \
             {dir}/b.png\nwait 3\n",
        );
        assert_eq!(frames, elapsed);
    }

    const FIRST_SHOT_WAIT: u32 = 30;

    #[test]
    fn a_shipped_script_waits_before_its_first_screenshot() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/gallery/drive");
        let mut seen = 0;
        for entry in fs::read_dir(&dir).expect("the drive directory")
        {
            let path = entry.expect("a directory entry").path();
            let text = fs::read_to_string(&path).expect("a script");
            let name =
                path.file_name().and_then(|name| name.to_str());
            if matches!(
                name,
                Some("bad-script.drive" | "driver-ime-bad.drive")
            ) {
                assert!(parse(&text).is_err());
                continue;
            }

            seen += 1;
            let steps = steps(&text);
            let first = steps
                .iter()
                .position(|step| matches!(step, Step::Screenshot(_)));
            if let Some(first) = first {
                let waited = steps[..first].iter().any(
                    |step| matches!(step, Step::Wait(frames) if *frames >= FIRST_SHOT_WAIT),
                );
                assert!(
                    waited,
                    "{} takes a screenshot before it waits",
                    path.display()
                );
            }
        }
        assert!(seen >= 10, "found {seen} shipped scripts");
    }

    fn logged<'a>(app: &'a App, topic: &str) -> Vec<&'a str> {
        app.world()
            .resource::<Drive>()
            .said
            .iter()
            .filter_map(|line| line.strip_prefix(topic))
            .collect()
    }

    fn listed(line: &str) -> Vec<(&str, &str)> {
        let (_, spans) = line.split_once(": ").expect("a span list");
        spans
            .split(", ")
            .map(|span| {
                let (id, rest) = span.split_once(' ').expect("an id");
                let (_, color) =
                    rest.rsplit_once(' ').expect("a colour");
                (id, color)
            })
            .collect()
    }

    #[test]
    fn a_recolour_keeps_every_span_of_the_long_line() {
        let mut app = headless(
            "click 100 96\nwait 5\nlog spans\nclick 290 68\nwait \
             5\nlog spans\nexit",
        );
        run(&mut app);
        let lines = logged(&app, "spans ");
        assert_eq!(lines.len(), 2, "{lines:?}");
        let (before, after) = (listed(lines[0]), listed(lines[1]));
        assert_eq!(before.len(), 200);
        let ids = |spans: &[(&str, &str)]| {
            spans
                .iter()
                .map(|span| span.0.to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&before), ids(&after));
        let recoloured = before
            .iter()
            .zip(&after)
            .filter(|(b, a)| b.1 != a.1)
            .count();
        assert_eq!(recoloured, 200);
    }

    #[test]
    fn the_long_line_keeps_one_empty_span_at_0_and_grows_back() {
        let mut app = headless(
            "click 100 96\nwait 5\nlog spans\nclick 586 68\nwait \
             5\nlog spans\nclick 734 68\nwait 5\nlog spans\nexit",
        );
        run(&mut app);
        let lines = logged(&app, "spans ");
        assert_eq!(lines.len(), 3, "{lines:?}");
        let first = listed(lines[0])[0].0.to_owned();
        assert!(lines[1].starts_with("1: "), "{}", lines[1]);
        assert!(lines[1].contains(" \"\" "), "one empty span");
        let five = listed(lines[2]);
        assert_eq!(five.len(), 5);
        assert_eq!(five[0].0, first, "the first span kept");
    }

    #[test]
    fn a_press_on_the_hit_line_logs_the_byte_of_its_character() {
        let mut app = headless(
            "click 100 96\nwait 5\nlog hit\nclick 342 233\nwait \
             3\nlog hit\nclick 387 233\nwait 3\nlog hit\nexit",
        );
        run(&mut app);
        assert_eq!(
            logged(&app, "hit "),
            ["none", "15 'ö'", "21 '日'"],
            "the bytes of ö in wörld and of 日"
        );
    }

    #[test]
    fn a_step_prints_ok_when_it_is_done() {
        let mut app = headless("page about\nclipboard\nexit");
        run(&mut app);
        let said = &app.world().resource::<Drive>().said;
        assert_eq!(
            said,
            &[
                "drive: page ok",
                "clipboard: (nothing copied)",
                "drive: clipboard ok",
                "drive: 0 of 0 checks pass",
                "drive: exit ok",
            ]
        );
    }

    fn exits_and_said(script: &str) -> (Vec<AppExit>, Vec<String>) {
        let mut app = headless(script);
        let exits = run(&mut app);
        (exits, app.world().resource::<Drive>().said.clone())
    }

    #[test]
    fn a_failed_check_ends_the_drive_with_code_1() {
        let (exits, said) = exits_and_said(
            "expect page about\nexpect theme dark\nexpect page \
             other\nexit",
        );
        assert_eq!(exits, [AppExit::from_code(1)]);
        assert_eq!(
            said[said.len() - 2..],
            ["drive: 2 of 3 checks pass", "drive: exit failed"]
        );
    }

    #[test]
    fn every_check_passing_ends_the_drive_with_code_0() {
        let (exits, said) = exits_and_said(
            "expect page about\nexpect theme dark\nexit",
        );
        assert_eq!(exits, [AppExit::Success]);
        assert_eq!(
            said[said.len() - 2..],
            ["drive: 2 of 2 checks pass", "drive: exit ok"]
        );
    }

    #[test]
    fn a_wtype_step_without_ime_live_exits_with_code_2() {
        let (exits, said) =
            exits_and_said("wait 1\nwtype hello\nexit");
        assert_eq!(exits, [AppExit::from_code(2)]);
        assert_eq!(
            said.last().map(String::as_str),
            Some("drive: line 2: wtype needs launch.sh --ime-live")
        );
    }

    #[test]
    fn a_page_without_an_answer_prints_n_a() {
        let (_, said) = exits_and_said(
            "log caret\nlog text 1\nlog undo\npage code\nwait \
             3\nlog caret\nexpect text 7 n/a\nexit",
        );
        for line in ["caret n/a", "text 1 n/a", "undo n/a"] {
            assert!(said.iter().any(|said| said == line), "{said:?}");
        }
        assert_eq!(
            said.iter().filter(|s| *s == "caret n/a").count(),
            2
        );
        assert!(said.contains(&"accept: text 7 pass".to_owned()));
    }

    #[test]
    fn a_paste_step_hands_its_text_to_the_gallery() {
        let mut app =
            headless("log paste\npaste one\\ttwo\nlog paste\nexit");
        run(&mut app);
        assert_eq!(logged(&app, "paste "), ["none", "\"one\\ttwo\""]);
        assert_eq!(
            app.world().resource::<Paste>().0.as_deref(),
            Some("one\ttwo")
        );
    }

    #[test]
    fn the_ime_log_names_the_last_message_the_gallery_read() {
        let mut app = headless(
            "log ime\nime enable\nlog ime\nime preedit 你好 0 \
             6\nlog ime\nime commit 你好\nlog ime\nime preedit\nlog \
             ime\nime disable\nlog ime\nexit",
        );
        run(&mut app);
        let window = "enabled false position 0 0 last";
        assert_eq!(
            logged(&app, "ime "),
            [
                format!("{window} none page n/a"),
                format!("{window} enabled page n/a"),
                format!("{window} preedit \"你好\" 0..6 page n/a"),
                format!("{window} commit \"你好\" page n/a"),
                format!("{window} preedit \"\" page n/a"),
                format!("{window} disabled page n/a"),
            ]
        );
    }

    #[derive(Resource, Default)]
    struct HeldAtPress(Vec<Vec<KeyCode>>);

    fn note_held(
        mut buttons: MessageReader<MouseButtonInput>,
        keys: Res<ButtonInput<KeyCode>>,
        mut held: ResMut<HeldAtPress>,
    ) {
        for button in buttons.read() {
            if button.state == ButtonState::Pressed {
                held.0.push(keys.get_pressed().copied().collect());
            }
        }
    }

    #[test]
    fn a_click_and_a_drag_hold_their_modifiers_for_the_gesture() {
        let mut app = headless(
            "click 10 10 mod\ndrag 0 0 9 9 alt\nclick 10 10 double \
             shift\nclick 10 10\nexit",
        );
        app.init_resource::<HeldAtPress>()
            .add_systems(Update, note_held);
        run(&mut app);
        let command = modifier("mod", Os::host()).expect("mod").code;
        let shift = KeyCode::ShiftLeft;
        assert_eq!(
            app.world().resource::<HeldAtPress>().0,
            [
                vec![command],
                vec![KeyCode::AltLeft],
                vec![shift],
                vec![shift],
                vec![]
            ]
        );
        let keys = app.world().resource::<ButtonInput<KeyCode>>();
        assert_eq!(keys.get_pressed().count(), 0, "every key let go");
    }

    fn fixture(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/gallery/fixtures")
            .join(name)
            .display()
            .to_string()
    }

    #[test]
    fn the_code_page_logs_a_line_s_runs_and_the_spans_it_shows() {
        let mut app = headless(
            "page code\nwait 5\nlog runs 1\nlog runs 10373\nexit",
        );
        run(&mut app);
        assert_eq!(
            logged(&app, "runs "),
            [
                "1: 0..33 comment.doc | shown \"//! The theme the \
                 examples share.\" #737AA2",
                "10373 none",
            ]
        );
    }

    #[test]
    fn a_code_row_is_17_px_so_a_scroll_of_rows_lands_on_its_line() {
        let mut app = headless(
            "page code\nwait 5\nscroll 0 -169983\nwait 5\nlog \
             rows\nexit",
        );
        run(&mut app);
        let rows = logged(&app, "rows ");
        assert!(
            rows[0].starts_with("first_visible 9999 "),
            "{rows:?}"
        );
    }

    #[test]
    fn the_gutter_shows_in_the_bundled_monospace_font() {
        let mut app = headless("page code\nwait 5\nexit");
        run(&mut app);
        let world = app.world_mut();
        let roots = world
            .query_filtered::<Entity, With<Text>>()
            .iter(world)
            .collect::<Vec<_>>();
        let first = roots
            .into_iter()
            .find(|&root| {
                spans_of(world, root)
                    .iter()
                    .map(|(_, text, _)| text.as_str())
                    .eq(["    1"])
            })
            .expect("line number 1");
        let node =
            world.get::<ComputedNode>(first).expect("laid out");
        let width = node.size().x * node.inverse_scale_factor();
        let digits = 5.0;
        let advance = 600.0 / 1000.0;
        let font_size = 14.0;
        let laid_out = digits * advance * font_size;
        assert!((width - laid_out).abs() <= 1.0, "{width} px");
    }

    fn left_of(world: &mut World, text: &str) -> f32 {
        let roots = world
            .query_filtered::<Entity, With<Text>>()
            .iter(world)
            .collect::<Vec<_>>();
        let root = roots
            .into_iter()
            .find(|&root| {
                let spans = spans_of(world, root);
                spans
                    .iter()
                    .map(|(_, text, _)| text.as_str())
                    .collect::<String>()
                    == text
            })
            .unwrap_or_else(|| panic!("a text {text:?}"));
        let node = world.get::<ComputedNode>(root).expect("laid out");
        let at =
            world.get::<UiGlobalTransform>(root).expect("placed");
        (at.translation.x - node.size().x / 2.0)
            * node.inverse_scale_factor()
    }

    #[test]
    fn a_sideways_scroll_moves_the_text_and_keeps_the_gutter() {
        let script = format!(
            "page code\nwait 1\nopen {}\nwait 5\nmove 740 \
             400\nscroll -6000 0\nwait 5\nlog rows\nexit",
            fixture("long-line.rs")
        );
        let mut app = headless(&script);
        run(&mut app);
        let rows = logged(&app, "rows ");
        let x = rows[0]
            .rsplit_once(" x ")
            .and_then(|(_, x)| x.parse::<f32>().ok())
            .expect("an x");
        assert!(x > 2000.0, "{rows:?}");
        let world = app.world_mut();
        assert_eq!(left_of(world, "1"), 220.0, "the gutter stays");
        let line =
            fs::read_to_string(fixture("long-line.rs")).unwrap();
        let long = line.lines().nth(2).unwrap();
        assert!(
            left_of(world, long) < 220.0 - 2000.0,
            "the text moves"
        );
    }

    #[test]
    fn an_open_on_the_code_page_shows_the_file() {
        let script = format!(
            "page code\nwait 1\nopen {}\nwait 3\nlog runs 1\nexpect \
             rows total 1 following\nexit",
            fixture("one-line.rs")
        );
        let mut app = headless(&script);
        assert_eq!(run(&mut app), [AppExit::Success]);
        let said = &app.world().resource::<Drive>().said;
        assert!(
            said.contains(&"accept: rows pass".to_owned()),
            "{said:?}"
        );
        let runs = logged(&app, "runs 1: ");
        assert!(runs[0].contains("| shown \"fn \" "), "{runs:?}");
    }

    #[test]
    fn an_open_of_a_missing_file_exits_with_code_2() {
        let script = format!(
            "page code\nwait 1\nopen {}\nexit",
            fixture("missing.rs")
        );
        let mut app = headless(&script);
        assert_eq!(run(&mut app), [AppExit::from_code(2)]);
        let said = &app.world().resource::<Drive>().said;
        let last = said.last().expect("a line");
        assert!(
            last.starts_with("drive: line 3: cannot read"),
            "{last}"
        );
    }
}
