//! Chords and pointer gestures to editor actions.
//!
//! One [`Keymap`] resource maps chords to [`Action`]s and presses
//! (modifiers and a click count) to [`Gesture`]s. Two platform tables
//! ship, each constructible on any platform; the default follows
//! `cfg(target_os)`. A feature adds its bindings to the tables here,
//! so an app that replaces the keymap with `Keymap::linux()` keeps
//! them.

use std::collections::HashMap;

use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageReader;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::input::keyboard::{
    Key, KeyCode, KeyboardFocusLost, KeyboardInput,
};
use bevy::input::{ButtonInput, ButtonState};
use bevy::input_focus::InputFocus;
use rubrix_edit::{
    Buffer, Command, Layout, Motion, Selection, Selections, word_at,
};
use rubrix_highlight::Lines;

use crate::caret::Hit;
use crate::frame::Tx;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Platform {
    #[default]
    Linux,
    MacOs,
}

impl Platform {
    /// The platform this build runs on.
    pub fn host() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Linux
        }
    }
}

/// Modifier keys held, left and right alike.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS, the Windows or Super key elsewhere.
    pub super_key: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
    };

    /// What `mod` means in a chord: Ctrl, or Cmd on macOS.
    pub fn primary(platform: Platform) -> Mods {
        match platform {
            Platform::Linux => Mods {
                ctrl: true,
                ..Mods::NONE
            },
            Platform::MacOs => Mods {
                super_key: true,
                ..Mods::NONE
            },
        }
    }

    fn with(self, other: Mods) -> Mods {
        Mods {
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
            shift: self.shift || other.shift,
            super_key: self.super_key || other.super_key,
        }
    }

    /// The modifier a key is, as a `Mods` with only it held.
    fn of(key: KeyCode) -> Option<Mods> {
        let only = |set: fn(&mut Mods)| {
            let mut mods = Mods::NONE;
            set(&mut mods);
            Some(mods)
        };
        match key {
            KeyCode::ControlLeft | KeyCode::ControlRight => {
                only(|m| m.ctrl = true)
            }
            KeyCode::AltLeft | KeyCode::AltRight => {
                only(|m| m.alt = true)
            }
            KeyCode::ShiftLeft | KeyCode::ShiftRight => {
                only(|m| m.shift = true)
            }
            KeyCode::SuperLeft | KeyCode::SuperRight => {
                only(|m| m.super_key = true)
            }
            _ => None,
        }
    }

    pub(crate) fn from_held(
        keys: Option<&ButtonInput<KeyCode>>,
    ) -> Mods {
        keys.map_or(Mods::NONE, |keys| {
            keys.get_pressed()
                .filter_map(|&key| Mods::of(key))
                .fold(Mods::NONE, Mods::with)
        })
    }

    /// Applies one key message to the held set. Returns the modifier
    /// it pressed, if it pressed one.
    fn track(&mut self, input: &KeyboardInput) -> Option<Mods> {
        let key = Mods::of(input.key_code)?;
        let held = input.state == ButtonState::Pressed;
        let set = |on: &mut bool, of: bool| {
            if of {
                *on = held;
            }
        };
        set(&mut self.ctrl, key.ctrl);
        set(&mut self.alt, key.alt);
        set(&mut self.shift, key.shift);
        set(&mut self.super_key, key.super_key);
        held.then_some(key)
    }
}

/// A key with modifiers, as a binding names it.
///
/// The key is a logical key: the character a letter, digit or symbol
/// types without Shift, lower-cased (`z`, `/`, `[`), or a named key
/// (`left`, `enter`, `backspace`). Matching follows Bevy's text
/// input: an ASCII logical key matches case-blind, so AZERTY and
/// Dvorak keep their shortcut letters, and a non-ASCII one falls back
/// to the physical key, so `Ctrl+Я` is `ctrl+z`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Chord {
    key: ChordKey,
    mods: Mods,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ChordKey {
    Char(char),
    Named(Key),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    UnknownModifier(String),
    UnknownKey(String),
}

const NAMED: &[(&str, Key)] = &[
    ("left", Key::ArrowLeft),
    ("right", Key::ArrowRight),
    ("up", Key::ArrowUp),
    ("down", Key::ArrowDown),
    ("home", Key::Home),
    ("end", Key::End),
    ("pageup", Key::PageUp),
    ("pagedown", Key::PageDown),
    ("enter", Key::Enter),
    ("backspace", Key::Backspace),
    ("delete", Key::Delete),
    ("tab", Key::Tab),
    ("escape", Key::Escape),
    ("space", Key::Space),
];

/// The character a physical key types on a US layout, for a logical
/// key that is not ASCII.
fn us_char(key: KeyCode) -> Option<char> {
    use KeyCode::*;
    let letters = [
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ,
        KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT,
        KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    ];
    let digits = [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6,
        Digit7, Digit8, Digit9,
    ];
    if let Some(i) = letters.iter().position(|&k| k == key) {
        return Some((b'a' + i as u8) as char);
    }
    if let Some(i) = digits.iter().position(|&k| k == key) {
        return Some((b'0' + i as u8) as char);
    }
    Some(match key {
        Slash => '/',
        Backslash => '\\',
        BracketLeft => '[',
        BracketRight => ']',
        Minus => '-',
        Equal => '=',
        Comma => ',',
        Period => '.',
        Semicolon => ';',
        Quote => '\'',
        Backquote => '`',
        _ => return None,
    })
}

impl Chord {
    /// `"mod+shift+z"`, `"alt+left"`, `"ctrl+/"`. `mod` takes its
    /// meaning from `platform`.
    fn parse(
        text: &str,
        platform: Platform,
    ) -> Result<Chord, ChordError> {
        let (mods, key) = match text.rsplit_once('+') {
            Some((mods, "")) => {
                (mods.strip_suffix('+').unwrap_or(mods), "+")
            }
            Some((mods, key)) => (mods, key),
            None => ("", text),
        };
        if key.is_empty() {
            return Err(ChordError::Empty);
        }
        let mods = parse_mods(mods, platform)?;
        let lower = key.to_ascii_lowercase();
        let key = match NAMED.iter().find(|(name, _)| *name == lower)
        {
            Some((_, named)) => ChordKey::Named(named.clone()),
            None => {
                let mut chars = lower.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => ChordKey::Char(c),
                    _ => {
                        return Err(ChordError::UnknownKey(
                            key.to_owned(),
                        ));
                    }
                }
            }
        };
        Ok(Chord { key, mods })
    }

    /// The chord a key message is with `mods` held, `None` for a key
    /// no chord names.
    fn of(input: &KeyboardInput, mods: Mods) -> Option<Chord> {
        let key = match &input.logical_key {
            Key::Character(c) if c.is_ascii() => {
                let mut chars = c.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => {
                        ChordKey::Char(c.to_ascii_lowercase())
                    }
                    _ => return None,
                }
            }
            Key::Character(_) => {
                ChordKey::Char(us_char(input.key_code)?)
            }
            named => ChordKey::Named(
                NAMED.iter().find(|(_, key)| key == named)?.1.clone(),
            ),
        };
        Some(Chord { key, mods })
    }
}

fn parse_mods(
    text: &str,
    platform: Platform,
) -> Result<Mods, ChordError> {
    text.split('+').filter(|part| !part.is_empty()).try_fold(
        Mods::NONE,
        |mods, part| {
            let one = match part.to_ascii_lowercase().as_str() {
                "mod" => Some(Mods::primary(platform)),
                "ctrl" | "control" => Mods::of(KeyCode::ControlLeft),
                "alt" | "option" => Mods::of(KeyCode::AltLeft),
                "shift" => Mods::of(KeyCode::ShiftLeft),
                "super" | "cmd" | "meta" => {
                    Mods::of(KeyCode::SuperLeft)
                }
                _ => None,
            }
            .ok_or_else(|| {
                ChordError::UnknownModifier(part.to_owned())
            })?;
            Ok(mods.with(one))
        },
    )
}

/// What a chord does to the focused editor.
#[derive(Clone, Copy, Debug)]
pub enum Action {
    /// A `rubrix_edit` command at every selection.
    Command(Command<'static>),
    /// The selections' text to the clipboard.
    Copy,
    /// Copy, then `Command::Cut`.
    Cut,
    /// A clipboard read, then `Command::Paste` when it resolves.
    Paste,
    /// A feature's handler for a chord that is no editor command. It
    /// runs in key order, through the transaction of the view the key
    /// went to.
    Run(fn(&mut Tx<'_>)),
}

impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Action::Command(a), Action::Command(b)) => a == b,
            (Action::Copy, Action::Copy)
            | (Action::Cut, Action::Cut)
            | (Action::Paste, Action::Paste) => true,
            (Action::Run(a), Action::Run(b)) => {
                core::ptr::fn_addr_eq(*a, *b)
            }
            _ => false,
        }
    }
}

/// What one key press resolved to.
pub(crate) enum Resolved<'a> {
    Action(Action),
    /// Text to type: the key's text, printable, with neither Ctrl nor
    /// Cmd held. Alt may be, since Option and AltGr type characters.
    Text(&'a str),
}

/// What a press with some modifiers and click count starts: a pure
/// function from the stroke so far to the selections, run at the
/// press and at every frame of its drag.
#[derive(Clone, Copy, Debug)]
pub struct Gesture(pub fn(&Stroke<'_>) -> Selections);

/// A press and where it has been dragged to.
pub struct Stroke<'a> {
    pub buffer: &'a Buffer,
    /// The view's row geometry.
    pub layout: &'a dyn Layout,
    /// The selections before the press.
    pub base: &'a Selections,
    pub from: Hit,
    pub to: Hit,
}

/// One caret at the press, extended to the pointer.
const PLACE: Gesture = Gesture(|stroke| {
    Selections::new(Selection {
        anchor: stroke.from.offset,
        head: stroke.to.offset,
    })
});

/// The primary's anchor kept, its head at the pointer.
const EXTEND: Gesture = Gesture(|stroke| {
    Selections::new(Selection {
        anchor: stroke.base.primary().anchor,
        head: stroke.to.offset,
    })
});

/// The word at the press through the word at the pointer.
const WORDS: Gesture = Gesture(|stroke| {
    let word = |hit: &Hit| {
        let start = stroke.buffer.line_start(hit.line);
        let line = content(stroke.buffer, hit.line);
        let word =
            word_at(line, (hit.offset - start).min(line.len()));
        start + word.start..start + word.end
    };
    spanning(word(&stroke.from), word(&stroke.to))
});

/// The line at the press through the line at the pointer, each with
/// its line break.
const LINES: Gesture = Gesture(|stroke| {
    let buffer = stroke.buffer;
    let line = |hit: &Hit| {
        let end = if hit.line + 1 < buffer.line_count() {
            buffer.line_start(hit.line + 1)
        } else {
            buffer.len()
        };
        buffer.line_start(hit.line)..end
    };
    spanning(line(&stroke.from), line(&stroke.to))
});

const ADD_WORDS: Gesture =
    Gesture(|stroke| added(stroke.base, (WORDS.0)(stroke).primary()));

const ADD_LINES: Gesture =
    Gesture(|stroke| added(stroke.base, (LINES.0)(stroke).primary()));

pub(crate) fn added(
    base: &Selections,
    selection: Selection,
) -> Selections {
    let mut selections = base.clone();
    selections.add(selection);
    selections
}

/// `from` through `to`, the head at the side `to` lies on.
fn spanning(
    from: core::ops::Range<usize>,
    to: core::ops::Range<usize>,
) -> Selections {
    Selections::new(if to.start < from.start {
        Selection {
            anchor: from.end,
            head: to.start,
        }
    } else {
        Selection {
            anchor: from.start,
            head: to.end.max(from.end),
        }
    })
}

/// Line `line` of `buffer` without its `\r`.
pub(crate) fn content(buffer: &Buffer, line: u32) -> &str {
    let text = Lines::line(buffer, line);
    text.strip_suffix('\r').unwrap_or(text)
}

/// Chords and gestures. Replace it whole with
/// `app.insert_resource(Keymap::macos())`, or change one entry with
/// [`Keymap::bind`].
#[derive(Resource, Clone, Debug)]
pub struct Keymap {
    platform: Platform,
    keys: HashMap<Chord, Action>,
    gestures: HashMap<(Mods, u8), Gesture>,
}

impl Default for Keymap {
    fn default() -> Keymap {
        Keymap::for_platform(Platform::host())
    }
}

/// One binding of a table.
type Entry = (&'static str, Action);

const fn go(motion: Motion) -> Action {
    Action::Command(Command::Move {
        motion,
        extend: false,
    })
}

const fn reach_to(motion: Motion) -> Action {
    Action::Command(Command::Move {
        motion,
        extend: true,
    })
}

/// Bindings both platforms share, written with `mod`.
const SHARED: &[Entry] = &[
    ("left", go(Motion::Left)),
    ("shift+left", reach_to(Motion::Left)),
    ("right", go(Motion::Right)),
    ("shift+right", reach_to(Motion::Right)),
    ("up", go(Motion::Up)),
    ("shift+up", reach_to(Motion::Up)),
    ("down", go(Motion::Down)),
    ("shift+down", reach_to(Motion::Down)),
    ("home", go(Motion::Home)),
    ("shift+home", reach_to(Motion::Home)),
    ("end", go(Motion::End)),
    ("shift+end", reach_to(Motion::End)),
    ("pageup", go(Motion::PageUp)),
    ("shift+pageup", reach_to(Motion::PageUp)),
    ("pagedown", go(Motion::PageDown)),
    ("shift+pagedown", reach_to(Motion::PageDown)),
    ("enter", Action::Command(Command::Enter)),
    ("shift+enter", Action::Command(Command::Enter)),
    ("backspace", Action::Command(Command::Backspace)),
    ("shift+backspace", Action::Command(Command::Backspace)),
    ("delete", Action::Command(Command::Delete)),
    ("alt+backspace", Action::Command(Command::DeleteWordBack)),
    ("mod+z", Action::Command(Command::Undo)),
    ("mod+shift+z", Action::Command(Command::Redo)),
    ("mod+a", Action::Command(Command::SelectAll)),
    ("mod+c", Action::Copy),
    ("mod+x", Action::Cut),
    ("mod+v", Action::Paste),
    ("escape", Action::Command(Command::CollapseToPrimary)),
    ("tab", Action::Command(Command::Indent)),
    ("shift+tab", Action::Command(Command::Outdent)),
    ("mod+]", Action::Command(Command::IndentLines)),
    ("mod+[", Action::Command(Command::Outdent)),
    (
        "alt+up",
        Action::Command(Command::MoveLines { down: false }),
    ),
    (
        "alt+down",
        Action::Command(Command::MoveLines { down: true }),
    ),
    (
        "shift+alt+up",
        Action::Command(Command::CopyLines { down: false }),
    ),
    (
        "shift+alt+down",
        Action::Command(Command::CopyLines { down: true }),
    ),
    ("mod+enter", Action::Command(Command::BlankLineBelow)),
    ("ctrl+t", Action::Command(Command::Transpose)),
    ("mod+d", Action::Command(Command::AddNextOccurrence)),
    ("mod+/", Action::Run(crate::commands::toggle_line_comment)),
    (
        "shift+alt+a",
        Action::Run(crate::commands::toggle_block_comment),
    ),
    ("mod+f", Action::Run(crate::find::open_find)),
    ("mod+alt+f", Action::Run(crate::find::open_replace)),
    ("mod+g", Action::Run(crate::find::next)),
    ("mod+shift+g", Action::Run(crate::find::previous)),
];

/// Linux and Windows conventions.
const LINUX: &[Entry] = &[
    ("ctrl+left", go(Motion::WordLeft)),
    ("ctrl+shift+left", reach_to(Motion::WordLeft)),
    ("ctrl+right", go(Motion::WordRight)),
    ("ctrl+shift+right", reach_to(Motion::WordRight)),
    ("ctrl+home", go(Motion::DocStart)),
    ("ctrl+shift+home", reach_to(Motion::DocStart)),
    ("ctrl+end", go(Motion::DocEnd)),
    ("ctrl+shift+end", reach_to(Motion::DocEnd)),
    ("ctrl+backspace", Action::Command(Command::DeleteWordBack)),
    ("ctrl+delete", Action::Command(Command::DeleteWordForward)),
    ("ctrl+y", Action::Command(Command::Redo)),
];

/// macOS conventions.
const MACOS: &[Entry] = &[
    ("alt+left", go(Motion::WordLeft)),
    ("alt+shift+left", reach_to(Motion::WordLeft)),
    ("alt+right", go(Motion::WordRight)),
    ("alt+shift+right", reach_to(Motion::WordRight)),
    ("super+left", go(Motion::Home)),
    ("super+shift+left", reach_to(Motion::Home)),
    ("super+right", go(Motion::End)),
    ("super+shift+right", reach_to(Motion::End)),
    ("super+up", go(Motion::DocStart)),
    ("super+shift+up", reach_to(Motion::DocStart)),
    ("super+down", go(Motion::DocEnd)),
    ("super+shift+down", reach_to(Motion::DocEnd)),
    ("super+home", go(Motion::DocStart)),
    ("super+shift+home", reach_to(Motion::DocStart)),
    ("super+end", go(Motion::DocEnd)),
    ("super+shift+end", reach_to(Motion::DocEnd)),
    ("alt+delete", Action::Command(Command::DeleteWordForward)),
    ("ctrl+k", Action::Command(Command::DeleteToLineEnd)),
    ("ctrl+a", go(Motion::Home)),
    ("ctrl+e", go(Motion::End)),
    ("ctrl+p", go(Motion::Up)),
    ("ctrl+n", go(Motion::Down)),
    ("ctrl+f", go(Motion::Right)),
    ("ctrl+b", go(Motion::Left)),
];

const GESTURES: &[(&str, u8, Gesture)] = &[
    ("", 1, PLACE),
    ("shift", 1, EXTEND),
    ("shift", 2, EXTEND),
    ("shift", 3, EXTEND),
    ("", 2, WORDS),
    ("", 3, LINES),
    ("mod", 1, crate::commands::ADD_CARET),
    ("mod", 2, ADD_WORDS),
    ("mod", 3, ADD_LINES),
    ("alt", 1, crate::commands::COLUMN),
    ("alt", 2, crate::commands::COLUMN),
    ("alt", 3, crate::commands::COLUMN),
];

impl Keymap {
    pub fn linux() -> Keymap {
        Keymap::for_platform(Platform::Linux)
    }

    pub fn macos() -> Keymap {
        Keymap::for_platform(Platform::MacOs)
    }

    /// The shared table, then the platform's own, which wins on a
    /// clash.
    ///
    /// # Panics
    ///
    /// Never for the tables that ship: a bad chord in them fails
    /// every test that builds a keymap.
    pub fn for_platform(platform: Platform) -> Keymap {
        let own = match platform {
            Platform::Linux => LINUX,
            Platform::MacOs => MACOS,
        };
        let mut keymap = Keymap {
            platform,
            keys: HashMap::new(),
            gestures: HashMap::new(),
        };
        for &(chord, action) in SHARED.iter().chain(own) {
            keymap
                .bind(chord, action)
                .expect("a chord the table names");
        }
        for &(mods, clicks, gesture) in GESTURES {
            keymap
                .bind_press(mods, clicks, gesture)
                .expect("modifiers the table names");
        }
        keymap
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Binds `chord`, such as `"mod+shift+z"`, to `action`, and
    /// returns what it replaced. `mod` is Ctrl on Linux and Cmd on
    /// macOS.
    pub fn bind(
        &mut self,
        chord: &str,
        action: Action,
    ) -> Result<Option<Action>, ChordError> {
        let chord = Chord::parse(chord, self.platform)?;
        Ok(self.keys.insert(chord, action))
    }

    pub fn unbind(
        &mut self,
        chord: &str,
    ) -> Result<Option<Action>, ChordError> {
        let chord = Chord::parse(chord, self.platform)?;
        Ok(self.keys.remove(&chord))
    }

    /// What `chord` does.
    pub fn get(&self, chord: &str) -> Option<&Action> {
        self.keys.get(&Chord::parse(chord, self.platform).ok()?)
    }

    /// Binds a press with `mods` held (`""` for none) and `clicks`
    /// clicks, 1 to 3, to `gesture`, and returns what it replaced.
    pub fn bind_press(
        &mut self,
        mods: &str,
        clicks: u8,
        gesture: Gesture,
    ) -> Result<Option<Gesture>, ChordError> {
        let mods = parse_mods(mods, self.platform)?;
        Ok(self.gestures.insert((mods, clicks.clamp(1, 3)), gesture))
    }

    /// The gesture a press starts. Modifiers no gesture names extend
    /// with Shift held and place a caret otherwise.
    pub(crate) fn gesture(&self, mods: Mods, clicks: u8) -> Gesture {
        let clicks = clicks.clamp(1, 3);
        self.gestures
            .get(&(mods, clicks))
            .copied()
            .unwrap_or(if mods.shift { EXTEND } else { PLACE })
    }

    /// One key press through the table, then as text. Releases and
    /// modifier keys resolve to nothing.
    pub(crate) fn resolve<'a>(
        &self,
        press: &'a KeyPress,
    ) -> Option<Resolved<'a>> {
        if press.input.state != ButtonState::Pressed {
            return None;
        }
        if let Some(action) = Chord::of(&press.input, press.mods)
            .and_then(|chord| self.keys.get(&chord))
        {
            return Some(Resolved::Action(*action));
        }
        let text = press.input.text.as_deref()?;
        let printable =
            !text.is_empty() && !text.chars().any(char::is_control);
        (printable && !press.mods.ctrl && !press.mods.super_key)
            .then_some(Resolved::Text(text))
    }
}

/// One key message, the modifiers held when it arrived, and the
/// entity that had the input focus then.
pub(crate) struct KeyPress {
    pub(crate) input: KeyboardInput,
    pub(crate) mods: Mods,
    pub(crate) focus: Option<Entity>,
}

/// This frame's key messages, decoded once, in order, for the editor
/// and for the read-only views. Modifiers are tracked message by
/// message, so "Ctrl down, z down, z up, Ctrl up" inside one frame
/// still reads as `ctrl+z`.
#[derive(Resource, Default)]
pub(crate) struct Keys {
    /// Held after the last message read, across frames.
    held: Mods,
    /// Held when this frame began.
    at_start: Mods,
    /// The modifiers pressed this frame.
    pressed: Mods,
    presses: Vec<KeyPress>,
}

impl Keys {
    pub(crate) fn presses(&self) -> &[KeyPress] {
        &self.presses
    }

    /// The modifiers a pointer press this frame holds: those held as
    /// the frame began and those pressed in it.
    pub(crate) fn for_press(&self) -> Mods {
        self.at_start.with(self.pressed)
    }
}

/// Decodes this frame's key messages into [`Keys`], each stamped with
/// the input focus before Bevy dispatches it.
pub(crate) fn read_keys(
    mut keys: ResMut<Keys>,
    mut input: MessageReader<KeyboardInput>,
    mut lost: MessageReader<KeyboardFocusLost>,
    focus: Option<Res<InputFocus>>,
) {
    let keys = &mut *keys;
    if lost.read().count() > 0 {
        keys.held = Mods::NONE;
    }
    let focus = focus.and_then(|focus| focus.get());
    keys.at_start = keys.held;
    keys.pressed = Mods::NONE;
    keys.presses.clear();
    for message in input.read() {
        let mods = keys.held;
        if let Some(pressed) = keys.held.track(message) {
            keys.pressed = keys.pressed.with(pressed);
        }
        keys.presses.push(KeyPress {
            input: message.clone(),
            mods,
            focus,
        });
    }
}

#[cfg(test)]
mod tests {
    use bevy::input::keyboard::NativeKeyCode;

    use super::*;

    fn press(
        key_code: KeyCode,
        logical_key: Key,
        text: Option<&str>,
        mods: Mods,
    ) -> KeyPress {
        KeyPress {
            input: KeyboardInput {
                key_code,
                logical_key,
                state: ButtonState::Pressed,
                text: text.map(Into::into),
                repeat: false,
                window: Entity::PLACEHOLDER,
            },
            mods,
            focus: None,
        }
    }

    const CTRL: Mods = Mods {
        ctrl: true,
        ..Mods::NONE
    };

    fn is(
        resolved: Option<Resolved<'_>>,
        command: Command<'_>,
    ) -> bool {
        matches!(resolved, Some(Resolved::Action(Action::Command(c))) if c == command)
    }

    #[test]
    fn a_chord_names_its_modifiers_and_one_key() {
        let chord = Chord::parse("mod+Shift+Z", Platform::MacOs)
            .expect("a chord");
        assert_eq!(chord.key, ChordKey::Char('z'));
        assert!(
            chord.mods.super_key
                && chord.mods.shift
                && !chord.mods.ctrl
        );
        let plus = Chord::parse("ctrl++", Platform::Linux)
            .map(|chord| chord.key);
        assert_eq!(plus, Ok(ChordKey::Char('+')));
        assert_eq!(
            Chord::parse("hyper+a", Platform::Linux),
            Err(ChordError::UnknownModifier("hyper".into()))
        );
        assert_eq!(
            Chord::parse("ctrl+left2", Platform::Linux),
            Err(ChordError::UnknownKey("left2".into()))
        );
        assert_eq!(
            Chord::parse("", Platform::Linux),
            Err(ChordError::Empty)
        );
    }

    #[test]
    fn shortcuts_match_like_bevy_text_input() {
        let keymap = Keymap::linux();
        let azerty = press(
            KeyCode::KeyW,
            Key::Character("z".into()),
            Some("z"),
            CTRL,
        );
        assert!(
            is(keymap.resolve(&azerty), Command::Undo),
            "the logical letter wins"
        );
        let cyrillic = press(
            KeyCode::KeyZ,
            Key::Character("я".into()),
            Some("я"),
            CTRL,
        );
        assert!(
            is(keymap.resolve(&cyrillic), Command::Undo),
            "the physical key backs it"
        );
        let shifted = Mods {
            shift: true,
            ..CTRL
        };
        let redo = press(
            KeyCode::KeyZ,
            Key::Character("Z".into()),
            Some("Z"),
            shifted,
        );
        assert!(is(keymap.resolve(&redo), Command::Redo));
    }

    #[test]
    fn unbound_text_types_unless_ctrl_or_cmd_is_held() {
        let keymap = Keymap::linux();
        let unknown =
            KeyCode::Unidentified(NativeKeyCode::Unidentified);
        let dot = press(
            unknown,
            Key::Character(".".into()),
            Some("."),
            Mods::NONE,
        );
        assert!(matches!(
            keymap.resolve(&dot),
            Some(Resolved::Text("."))
        ));
        let option = Mods {
            alt: true,
            ..Mods::NONE
        };
        let accent = press(
            KeyCode::KeyE,
            Key::Character("é".into()),
            Some("é"),
            option,
        );
        assert!(matches!(
            keymap.resolve(&accent),
            Some(Resolved::Text("é"))
        ));
        let ctrl = press(
            KeyCode::KeyQ,
            Key::Character("q".into()),
            Some("q"),
            CTRL,
        );
        assert!(keymap.resolve(&ctrl).is_none());
        let tab =
            press(KeyCode::Tab, Key::Tab, Some("\t"), Mods::NONE);
        assert!(
            is(keymap.resolve(&tab), Command::Indent),
            "Tab indents rather than types"
        );
    }

    fn same(a: Gesture, b: Gesture) -> bool {
        core::ptr::fn_addr_eq(a.0, b.0)
    }

    #[test]
    fn a_press_with_unbound_modifiers_falls_back_to_place_or_extend()
    {
        let keymap = Keymap::linux();
        let alt_shift = Mods {
            alt: true,
            shift: true,
            ..Mods::NONE
        };
        let ctrl_alt = Mods { alt: true, ..CTRL };
        assert!(same(keymap.gesture(alt_shift, 1), EXTEND));
        assert!(same(keymap.gesture(ctrl_alt, 2), PLACE));
        assert!(same(keymap.gesture(Mods::NONE, 7), LINES));
    }

    #[test]
    fn mod_and_alt_keep_their_gestures_at_every_click_count() {
        let alt = Mods {
            alt: true,
            ..Mods::NONE
        };
        for keymap in [Keymap::linux(), Keymap::macos()] {
            let primary = match keymap.platform() {
                Platform::Linux => CTRL,
                Platform::MacOs => Mods {
                    super_key: true,
                    ..Mods::NONE
                },
            };
            let gesture = |mods, clicks| keymap.gesture(mods, clicks);
            assert!(same(gesture(primary, 2), ADD_WORDS));
            assert!(same(gesture(primary, 3), ADD_LINES));
            for clicks in 1..=3 {
                assert!(same(
                    gesture(alt, clicks),
                    crate::commands::COLUMN
                ));
            }
        }
    }

    #[test]
    fn modifiers_are_tracked_message_by_message() {
        let mut held = Mods::NONE;
        let ctrl = press(
            KeyCode::ControlLeft,
            Key::Control,
            None,
            Mods::NONE,
        )
        .input;
        assert_eq!(held.track(&ctrl), Some(CTRL));
        assert!(held.ctrl);
        let released = KeyboardInput {
            state: ButtonState::Released,
            ..ctrl
        };
        assert_eq!(held.track(&released), None);
        assert_eq!(held, Mods::NONE);
    }
}
