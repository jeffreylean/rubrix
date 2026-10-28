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
//! - `move <x> <y>`, `click <x> <y> [double|triple|shift]`, `drag
//!   <x0> <y0> <x1> <y1>`, `press <x> <y>`, `release`, `scroll <dx>
//!   <dy>`, `wheel <dx> <dy>`
//! - `key <combo>` such as `ctrl+a`, and `type <text>`
//! - `screenshot <path>`, `record start <dir>`, `record stop`
//! - `frames start`, `frames stop <csv>`
//! - `log <topic>`, `expect <topic> <text>`, `clipboard`
//! - `resize <width> <height>`, `open <path>...`
//!
//! A topic is a word, but `runs <line>` takes a line number too, so
//! `expect runs 75 comment` checks topic `runs 75` for `comment`.
//! `open` reads a relative path from the script's directory.
//!
//! Positions are logical pixels in the window. A scroll `dy` below
//! zero moves the content up, so the page scrolls down. A `scroll`
//! turns in pixels, as a trackpad does, and a `wheel` in lines, the
//! notches of a mouse wheel, which ease. Every step
//! prints `drive: <step> ok` when it is done.
//!
//! A scroll goes to what is under the pointer, so a `click` on the
//! page list leaves the pointer there until a `move` takes it back.
//! `resize` asks Hyprland for the size and is done when the window
//! reports it, or exits with code 2 after two seconds. `exit` stops
//! a `record` still running.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::{fmt, fs, mem};

use bevy::app::{App, AppExit, First, Plugin};
use bevy::color::Srgba;
use bevy::ecs::change_detection::Mut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::MessageUpdateSystems;
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
use bevy::window::{CursorMoved, PrimaryWindow, Window, WindowEvent};
use bevy_fynix::views::VirtualList;
use bevy_rubrix::{CodeDoc, CodeLine, CodeSelection};
use rubrix_highlight::spans;

use crate::pages::rich::{Hit, HitLineRoot, LongLineRoot};
use crate::pages::{self, PAGES};
use crate::{
    Copied, Mode, PAGE_CENTER, Ready, Selected, page_areas, scrollers,
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

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Page(usize),
    Wait(u32),
    Move(Vec2),
    Click { at: Vec2, count: u8 },
    ShiftClick(Vec2),
    Press(Vec2),
    Release,
    Drag { from: Vec2, to: Vec2 },
    Scroll(Vec2),
    Wheel(Vec2),
    Key(Vec<Press>),
    Type(String),
    Screenshot(PathBuf),
    Log(String),
    FramesStart,
    FramesStop(PathBuf),
    RecordStart(PathBuf),
    RecordStop,
    Resize { width: u32, height: u32 },
    Open(Vec<PathBuf>),
    Clipboard,
    Expect { topic: String, text: String },
    Exit,
}

impl Step {
    fn name(&self) -> &'static str {
        match self {
            Self::Page(_) => "page",
            Self::Wait(_) => "wait",
            Self::Move(_) => "move",
            Self::Click { .. } | Self::ShiftClick(_) => "click",
            Self::Press(_) => "press",
            Self::Release => "release",
            Self::Drag { .. } => "drag",
            Self::Scroll(_) => "scroll",
            Self::Wheel(_) => "wheel",
            Self::Key(_) => "key",
            Self::Type(_) => "type",
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
    let mut lines = Vec::new();
    for (index, text) in script.lines().enumerate() {
        let number = index + 1;
        let step = parse_line(text).map_err(|reason| ParseError {
            line: number,
            reason,
        })?;
        lines.extend(step.map(|step| Line { number, step }));
    }
    Ok(lines)
}

fn parse_line(text: &str) -> Result<Option<Step>, String> {
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
            let usage = "click <x> <y> [double|triple|shift]";
            let (x, y, count) = match words[..] {
                [x, y] => (x, y, 1),
                [x, y, "double"] => (x, y, 2),
                [x, y, "triple"] => (x, y, 3),
                [x, y, "shift"] => {
                    let at = Vec2::new(number(x)?, number(y)?);
                    return Ok(Some(Step::ShiftClick(at)));
                }
                _ => return Err(format!("usage: {usage}")),
            };
            Step::Click {
                at: Vec2::new(number(x)?, number(y)?),
                count,
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
            let [x0, y0, x1, y1] =
                exact(&words, "drag <x0> <y0> <x1> <y1>")?;
            Step::Drag {
                from: Vec2::new(number(x0)?, number(y0)?),
                to: Vec2::new(number(x1)?, number(y1)?),
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
            Step::Key(parse_combo(combo)?)
        }
        "type" => {
            if rest.is_empty() {
                return Err("usage: type <text>".into());
            }
            Step::Type(rest.into())
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
        "runs" => 1,
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

fn parse_combo(combo: &str) -> Result<Vec<Press>, String> {
    let mut parts = combo.split('+').collect::<Vec<_>>();
    let name = parts
        .pop()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("`{combo}` names no key"))?;
    let mut presses = Vec::new();
    let (mut shift, mut command) = (false, false);
    for part in parts {
        let (code, key) = match part {
            "ctrl" | "control" => {
                command = true;
                (KeyCode::ControlLeft, Key::Control)
            }
            "alt" => {
                command = true;
                (KeyCode::AltLeft, Key::Alt)
            }
            "super" | "meta" => {
                command = true;
                (KeyCode::SuperLeft, Key::Super)
            }
            "shift" => {
                shift = true;
                (KeyCode::ShiftLeft, Key::Shift)
            }
            other => {
                return Err(format!("`{other}` is not a modifier"));
            }
        };
        presses.push(Press {
            code,
            key,
            text: None,
        });
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

#[derive(Clone, Debug)]
enum Input {
    Move(Vec2),
    Button(ButtonState),
    Wheel(Vec2, MouseScrollUnit),
    Key(Press, ButtonState),
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
}

pub struct DrivePlugin(Vec<Line>);

impl Plugin for DrivePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Drive::new(self.0.clone())).add_systems(
            First,
            advance
                .after(MessageUpdateSystems)
                .before(PickingSystems::Input)
                .run_if(resource_exists::<Ready>),
        );
    }
}

impl DrivePlugin {
    pub fn new(script: &str) -> Result<Self, ParseError> {
        parse(script).map(Self)
    }

    fn opening_from(mut self, dir: &Path) -> Self {
        for line in &mut self.0 {
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
    DrivePlugin::new(&script)
        .map(|drive| Some(drive.opening_from(dir)))
        .map_err(|error| error.to_string())
}

fn advance(world: &mut World) {
    world.resource_scope(|world, mut drive: Mut<Drive>| {
        drive.advance(world);
    });
}

impl Drive {
    fn new(lines: Vec<Line>) -> Self {
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
                self.done("exit");
                world.write_message(AppExit::Success);
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
            Step::Click { at, count } => {
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
                self.put_in(world, frames, name);
            }
            Step::ShiftClick(at) => {
                let shift = Press {
                    code: KeyCode::ShiftLeft,
                    key: Key::Shift,
                    text: None,
                };
                let frames = VecDeque::from([
                    vec![
                        Input::Key(
                            shift.clone(),
                            ButtonState::Pressed,
                        ),
                        Input::Move(at),
                    ],
                    vec![Input::Button(ButtonState::Pressed)],
                    vec![Input::Button(ButtonState::Released)],
                    vec![Input::Key(shift, ButtonState::Released)],
                ]);
                self.put_in(world, frames, name);
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
            Step::Drag { from, to } => {
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
                self.put_in(world, frames, name);
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
                let down = presses
                    .iter()
                    .map(|press| {
                        Input::Key(
                            press.clone(),
                            ButtonState::Pressed,
                        )
                    })
                    .collect();
                let up = presses
                    .into_iter()
                    .rev()
                    .map(|press| {
                        Input::Key(press, ButtonState::Released)
                    })
                    .collect();
                self.put_in(world, [down, up].into(), name);
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
                        let verdict = if line.contains(&text) {
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
    Some(match topic {
        "entities" => {
            format!("entities {}", world.entities().count_spawned())
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
        "stream" => pages::stream::log(world),
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

/// `selection anchor <a>..<b> focus <c>..<d> range <s>..<e> length
/// <n>` for the first code view, or `selection none` with none.
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
    use bevy::ecs::system::{Commands, Local, Query};
    use bevy::image::Image;
    use bevy::input::InputPlugin;
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
                    count: 1
                },
                Step::Click {
                    at: Vec2::new(10.0, 20.0),
                    count: 2
                },
                Step::Click {
                    at: Vec2::new(10.0, 20.0),
                    count: 3
                },
                Step::ShiftClick(Vec2::new(10.0, 20.0)),
                Step::Press(Vec2::new(5.0, 6.0)),
                Step::Release,
                Step::Drag {
                    from: Vec2::new(1.0, 2.0),
                    to: Vec2::new(3.0, 4.0)
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
            drive.0[0].step,
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
            if path.file_name() == Some("bad-script.drive".as_ref()) {
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
                "drive: exit ok",
            ]
        );
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
