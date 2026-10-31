//! Chords and pointer gestures to editor actions.

use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use rubrix_edit::{Buffer, Command, Layout, Selections};

use crate::caret::Hit;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordError {
    Empty,
    UnknownModifier(String),
    UnknownKey(String),
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
    /// A feature's handler for a chord that is no editor command. Runs
    /// in key order with the focused view.
    Run(fn(&mut World, Entity)),
}

impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Action::Command(a), Action::Command(b)) => a == b,
            (Action::Copy, Action::Copy)
            | (Action::Cut, Action::Cut)
            | (Action::Paste, Action::Paste) => true,
            (Action::Run(a), Action::Run(b)) => core::ptr::fn_addr_eq(*a, *b),
            _ => false,
        }
    }
}

/// What a press with some modifiers and click count starts: a pure
/// function from the stroke so far to the selections, run at the press
/// and at every frame of its drag.
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

/// Chords and gestures. Replace it whole with
/// `app.insert_resource(Keymap::macos())`, or change one entry with
/// [`Keymap::bind`].
#[derive(Resource, Clone, Debug)]
pub struct Keymap {
    platform: Platform,
}

impl Default for Keymap {
    fn default() -> Keymap {
        Keymap::for_platform(Platform::host())
    }
}

impl Keymap {
    pub fn linux() -> Keymap {
        Keymap::for_platform(Platform::Linux)
    }

    pub fn macos() -> Keymap {
        Keymap::for_platform(Platform::MacOs)
    }

    pub fn for_platform(platform: Platform) -> Keymap {
        let _ = platform;
        todo!()
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Binds `chord`, such as `"mod+shift+z"`, to `action`, and returns
    /// what it replaced. `mod` is Ctrl on Linux and Cmd on macOS.
    pub fn bind(
        &mut self,
        chord: &str,
        action: Action,
    ) -> Result<Option<Action>, ChordError> {
        let _ = (chord, action);
        todo!()
    }

    pub fn unbind(
        &mut self,
        chord: &str,
    ) -> Result<Option<Action>, ChordError> {
        let _ = chord;
        todo!()
    }

    /// What `chord` does.
    pub fn get(&self, chord: &str) -> Option<&Action> {
        let _ = chord;
        todo!()
    }

    /// Binds a press with `mods` held (`""` for none) and `clicks`
    /// clicks to `gesture`, and returns what it replaced.
    pub fn bind_press(
        &mut self,
        mods: &str,
        clicks: u8,
        gesture: Gesture,
    ) -> Result<Option<Gesture>, ChordError> {
        let _ = (mods, clicks, gesture);
        todo!()
    }
}
