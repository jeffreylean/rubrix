use core::marker::PhantomData;
use std::collections::VecDeque;

use bevy::app::{App, Plugin, PreUpdate};
use bevy::color::Color;
use bevy::ecs::component::{Component, Mutable};
use bevy::ecs::lifecycle::{Add, Remove};
use bevy::ecs::message::MessageReader;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, ResMut};
use bevy_fynix::views::VirtualList;
use rubrix_edit::{Change, EditState, LineAnchors, Ranges};

use crate::damage::BUILT_MAX;
use crate::editor::{EditDoc, EditSystems, EditView, EditorChanged};
use crate::frame;
use crate::keymap::content;
use crate::marks::{Mark, MarkKind, MarkLayer, Marks};

/// A component on an [`EditDoc`]'s entity that holds places in the
/// doc's text and moves them through each change the doc takes, undo
/// and redo included, in the order they were made.
///
/// Changes reach it in `PreUpdate`, inside [`EditSystems::Apply`]
/// after the edits of the frame, so a system that reads
/// [`EditorChanged`] after that set sees it moved, and a value it
/// writes then is in the offsets of the text after the change and
/// stays as written.
pub trait Follows: Component<Mutability = Mutable> {
    fn follow(&mut self, change: &Change);
}

/// Moves every `C` through its doc's changes.
/// [`RubrixPlugin`](crate::RubrixPlugin) adds it for
/// [`RemoteCarets`](crate::RemoteCarets) and [`Markers`]. Add it for
/// each [`LineAnnotations`] payload an app uses, or a `Follows` of
/// its own.
pub struct FollowPlugin<C>(PhantomData<fn() -> C>);

impl<C> Default for FollowPlugin<C> {
    fn default() -> FollowPlugin<C> {
        FollowPlugin(PhantomData)
    }
}

impl<C: Follows> Plugin for FollowPlugin<C> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            follow::<C>
                .in_set(EditSystems::Apply)
                .after(frame::apply),
        );
    }
}

fn follow<C: Follows>(
    mut changed: MessageReader<EditorChanged>,
    mut docs: Query<&mut C>,
) {
    for changed in changed.read() {
        if let Ok(mut held) = docs.get_mut(changed.doc) {
            held.follow(&changed.change);
        }
    }
}

/// How bad a [`Markers`] range is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    /// The colour of its underline.
    pub fn color(self) -> Color {
        match self {
            Severity::Error => Color::srgb_u8(0xf4, 0x47, 0x47),
            Severity::Warning => Color::srgb_u8(0xe5, 0xb5, 0x2a),
            Severity::Info => Color::srgb_u8(0x3a, 0x9b, 0xf0),
            Severity::Hint => Color::srgb_u8(0x8a, 0x8a, 0x8a),
        }
    }
}

/// Diagnostics on an [`EditDoc`]'s entity, as byte ranges of its text
/// with a severity each. Every view of the doc underlines them in the
/// severity's colour, in the rows near its viewport only. They follow
/// the text, and a change that deletes all of a range's bytes drops
/// it.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Markers(pub Ranges<Severity>);

impl Follows for Markers {
    fn follow(&mut self, change: &Change) {
        self.0.map(change);
    }
}

const MARKERS: MarkLayer = MarkLayer("markers");

const OVERSCAN_ABOVE: u32 = 64;

pub(crate) fn draw_markers(
    mut views: Query<(&EditView, &mut Marks)>,
    docs: Query<(&EditDoc, Option<&Markers>)>,
    lists: Query<&VirtualList>,
) {
    for (edit, mut marks) in &mut views {
        let Ok((doc, markers)) = docs.get(edit.doc()) else {
            continue;
        };
        let first = edit
            .list()
            .and_then(|list| lists.get(list).ok())
            .map_or(0, |list| list.first_visible() as u32);
        let wanted = markers.map_or_else(Vec::new, |markers| {
            near(doc, markers, first.saturating_sub(OVERSCAN_ABOVE))
        });
        if marks.layer(MARKERS) != wanted.as_slice() {
            marks.set(MARKERS, wanted);
        }
    }
}

fn near(doc: &EditDoc, markers: &Markers, from: u32) -> Vec<Mark> {
    let buffer = doc.editor().buffer();
    let last = buffer.line_count() - 1;
    let from = from.min(last);
    let to = from.saturating_add(BUILT_MAX).min(last);
    let (start, end) = (buffer.line_start(from), buffer.line_end(to));
    let mut marks = Vec::new();
    for (range, severity) in markers.0.iter() {
        if range.is_empty()
            || range.end <= start
            || range.start >= end
        {
            continue;
        }
        let (range_start, range_end) =
            (range.start.max(start), range.end.min(end));
        for line in
            buffer.line_of(range_start)..=buffer.line_of(range_end)
        {
            let at = buffer.line_start(line);
            let length = content(buffer, line).len();
            let bytes = range_start.max(at) - at
                ..(range_end - at).min(length);
            if bytes.start < bytes.end {
                marks.push(Mark {
                    line,
                    kind: MarkKind::Underline {
                        bytes,
                        color: severity.color(),
                    },
                });
            }
        }
    }
    marks.sort_by_key(|mark| mark.line);
    marks
}

/// A payload per line of an [`EditDoc`]'s text, on its entity, such
/// as review comments. Each moves with its line's text, a change that
/// deletes the line drops it, and the undo of that change brings it
/// back, as [`LineAnchors`] says. Add [`FollowPlugin`] for it.
#[derive(Component, Clone, Debug)]
pub struct LineAnnotations<T: Send + Sync + 'static>(
    pub LineAnchors<T>,
);

impl<T: Send + Sync + 'static> Follows for LineAnnotations<T> {
    fn follow(&mut self, change: &Change) {
        self.0.map(change);
    }
}

/// An app's name for the text an [`EditDoc`] holds, such as its path.
/// Despawning a doc that has one saves its text, selections and undo
/// history in [`EditStates`] under it, and spawning a doc with the
/// same key, `EditDoc` and `EditKey` together, takes them back, in
/// place of the text it was spawned with.
#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EditKey(pub String);

/// The edit states of keyed docs that were despawned, at most
/// [`EditStates::MAX`], the oldest dropped first.
#[derive(Resource, Default)]
pub struct EditStates {
    saved: VecDeque<(EditKey, EditState)>,
}

impl EditStates {
    pub const MAX: usize = 100;

    pub fn len(&self) -> usize {
        self.saved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.saved.is_empty()
    }

    pub fn get(&self, key: &EditKey) -> Option<&EditState> {
        self.saved
            .iter()
            .find(|(saved, _)| saved == key)
            .map(|(_, state)| state)
    }

    /// Drops the state under `key`, so the next doc with that key
    /// keeps the text it was spawned with.
    pub fn forget(&mut self, key: &EditKey) -> Option<EditState> {
        let at =
            self.saved.iter().position(|(saved, _)| saved == key)?;
        self.saved.remove(at).map(|(_, state)| state)
    }

    fn save(&mut self, key: EditKey, state: EditState) {
        self.forget(&key);
        self.saved.push_back((key, state));
        if self.saved.len() > Self::MAX {
            self.saved.pop_front();
        }
    }
}

pub(crate) fn save_state(
    removed: On<Remove<EditDoc>>,
    docs: Query<(&EditDoc, &EditKey)>,
    mut states: ResMut<EditStates>,
) {
    if let Ok((doc, key)) = docs.get(removed.entity) {
        states.save(key.clone(), EditState::save(doc.editor()));
    }
}

pub(crate) fn restore_state(
    added: On<Add<EditDoc>>,
    mut docs: Query<(&mut EditDoc, &EditKey)>,
    mut states: ResMut<EditStates>,
) {
    let Ok((mut doc, key)) = docs.get_mut(added.entity) else {
        return;
    };
    if let Some(state) = states.forget(key) {
        let lang = doc.lang();
        *doc = EditDoc::from_editor(lang, state.restore());
    }
}
