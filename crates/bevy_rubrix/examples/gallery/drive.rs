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
//! - `move <x> <y>`, `click <x> <y> [double|triple]`, `drag <x0> <y0>
//!   <x1> <y1>`, `scroll <dx> <dy>`
//! - `key <combo>` such as `ctrl+a`, and `type <text>`
//! - `screenshot <path>`, `record start <dir>`, `record stop`
//! - `frames start`, `frames stop <csv>`
//! - `log <topic>`, `expect <topic> <text>`, `clipboard`
//! - `resize <width> <height>`, `open <path>...`
//!
//! Positions are logical pixels in the window. A scroll `dy` below
//! zero moves the content up, so the page scrolls down. Every step
//! prints `drive: <step> ok` when it is done.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;
use std::{fmt, fs, mem};

use bevy::app::{App, AppExit, First, Plugin};
use bevy::ecs::change_detection::Mut;
use bevy::ecs::entity::Entity;
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
use bevy::math::Vec2;
use bevy::picking::PickingSystems;
use bevy::render::view::screenshot::{
    Screenshot, ScreenshotCaptured, save_to_disk,
};
use bevy::window::{CursorMoved, PrimaryWindow, Window, WindowEvent};

use crate::pages::PAGES;
use crate::{Copied, Mode, PAGE_CENTER, Ready, Selected};

const DRAG_FRAMES: usize = 4;

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
    Drag { from: Vec2, to: Vec2 },
    Scroll(Vec2),
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
            Self::Click { .. } => "click",
            Self::Drag { .. } => "drag",
            Self::Scroll(_) => "scroll",
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
            let usage = "click <x> <y> [double|triple]";
            let (x, y, count) = match words[..] {
                [x, y] => (x, y, 1),
                [x, y, "double"] => (x, y, 2),
                [x, y, "triple"] => (x, y, 3),
                _ => return Err(format!("usage: {usage}")),
            };
            Step::Click {
                at: Vec2::new(number(x)?, number(y)?),
                count,
            }
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
            let [topic] = exact(&words, "log <topic>")?;
            Step::Log(topic.into())
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
            let (topic, text) = rest
                .split_once(char::is_whitespace)
                .ok_or("usage: expect <topic> <text>")?;
            Step::Expect {
                topic: topic.into(),
                text: text.trim().into(),
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
    Wheel(Vec2),
    Key(Press, ButtonState),
}

enum Busy {
    Idle,
    Input(VecDeque<Vec<Input>>, &'static str),
    Wait(u32),
    Screenshot,
    Exit,
    Done,
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
}

pub fn from_env() -> Result<Option<DrivePlugin>, String> {
    let Ok(path) = std::env::var("RUBRIX_DRIVE") else {
        return Ok(None);
    };
    let script = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {path}: {error}"))?;
    DrivePlugin::new(&script)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn advance(world: &mut World) {
    world.resource_scope(|world, mut drive: Mut<Drive>| {
        drive.advance(world);
    });
}

fn landed(_: On<ScreenshotCaptured>, mut drive: ResMut<Drive>) {
    drive.pending_shots = drive.pending_shots.saturating_sub(1);
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
            Busy::Exit if self.pending_shots > 0 => {
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
            Step::Scroll(by) => {
                let mut frames = VecDeque::new();
                if self.pointer.is_none() {
                    frames.push_back(vec![Input::Move(PAGE_CENTER)]);
                }
                frames.push_back(vec![Input::Wheel(by)]);
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
                    self.capture(world, path);
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
                for mut window in world
                    .query_filtered::<&mut Window, With<PrimaryWindow>>()
                    .iter_mut(world)
                {
                    window.resolution.set(width as f32, height as f32);
                }
                self.done(name);
            }
            Step::Open(paths) => {
                let page = &PAGES[world.resource::<Selected>().0];
                match page.open {
                    Some(open) => {
                        open(world, &paths);
                        self.done(name);
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
            Step::Exit => self.busy = Busy::Exit,
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
            Input::Wheel(by) => send(
                world,
                MouseWheel {
                    unit: MouseScrollUnit::Pixel,
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

    fn capture(&mut self, world: &mut World, path: PathBuf) {
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(landed);
        self.pending_shots += 1;
    }

    fn record(&mut self, world: &mut World) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        let path = recording
            .dir
            .join(format!("frame-{:05}.png", recording.next));
        recording.next += 1;
        self.capture(world, path);
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
            format!("present_mode {mode:?}")
        }
        "theme" => {
            format!("theme {}", world.resource::<Mode>().0.name())
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use bevy::app::PostUpdate;
    use bevy::asset::AssetApp;
    use bevy::camera::visibility::{
        InheritedVisibility, VisibilitySystems,
    };
    use bevy::camera::{
        Camera, Camera2d, ComputedCameraValues, RenderTargetInfo,
    };
    use bevy::ecs::message::Messages;
    use bevy::ecs::system::Query;
    use bevy::input::InputPlugin;
    use bevy::math::UVec2;
    use bevy::picking::DefaultPickingPlugins;
    use bevy::time::TimeUpdateStrategy;
    use bevy::transform::TransformPlugin;
    use bevy::ui::{ScrollPosition, UiPlugin, percent, px};
    use bevy::window::WindowPlugin;
    use bevy_fynix::views::{FrameProps, column};
    use bevy_fynix::{FynixPlugin, Theme, mount};

    use super::*;
    use crate::{DARK, Palette};

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
drag 1 2 3 4
scroll 0 -400
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
                Step::Drag {
                    from: Vec2::new(1.0, 2.0),
                    to: Vec2::new(3.0, 4.0)
                },
                Step::Scroll(Vec2::new(0.0, -400.0)),
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
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(
            Duration::from_millis(10),
        ))
        .insert_resource(Theme(DARK))
        .insert_resource(Mode(DARK))
        .insert_resource(Selected(0))
        .init_resource::<Copied>()
        .init_asset::<bevy::image::Image>()
        .init_resource::<bevy::text::TextPipeline>()
        .init_resource::<bevy::text::FontCx>()
        .init_resource::<bevy::text::ScaleCx>()
        .init_resource::<bevy::text::RemSize>()
        .add_plugins(UiPlugin)
        .add_systems(
            PostUpdate,
            show_everything
                .in_set(VisibilitySystems::VisibilityPropagate),
        )
        .set_error_handler(bevy::ecs::error::ignore);
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
        mount::<Palette>(
            app.world_mut(),
            column(((PAGES[0].build)(),))
                .width(percent(100.0))
                .height(percent(100.0))
                .min_height(px(0.0)),
        );
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
        app.world_mut()
            .query::<&ScrollPosition>()
            .iter(app.world())
            .map(|position| position.y)
            .fold(0.0, f32::max)
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
}
