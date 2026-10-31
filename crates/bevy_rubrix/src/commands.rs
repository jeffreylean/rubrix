//! The bindings that are more than one command: the comment toggles,
//! which read the doc's language, the add-caret and column gestures,
//! and the bracket pair at the primary caret.

use core::ops::Range;

use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::{Commands, Query};
use rubrix_edit::{
    Brackets, Buffer, Command, Selection, column, matching_bracket,
    toggle_caret,
};
use rubrix_highlight::{Kind, spans};

use crate::caret::CaretLayer;
use crate::editor::{EditDoc, EditView};
use crate::frame::Tx;
use crate::keymap::Gesture;
use crate::marks::{Mark, MarkKind, MarkLayer, Marks};

/// `mod+/`: comments or uncomments the selected lines with the doc's
/// line marker, or its block pair when its language has none.
pub fn toggle_line_comment(tx: &mut Tx<'_>) {
    toggle_comment(tx, false);
}

/// Shift+Alt+A: wraps each selection in, or unwraps it from, the
/// doc's block comment pair.
pub fn toggle_block_comment(tx: &mut Tx<'_>) {
    toggle_comment(tx, true);
}

fn toggle_comment(tx: &mut Tx<'_>, block: bool) {
    let doc = tx.doc();
    let Some(lang) =
        tx.world().get::<EditDoc>(doc).map(EditDoc::lang)
    else {
        return;
    };
    tx.apply(Command::ToggleComment { lang, block });
}

/// `mod+click` adds a caret, or removes the one it lands on. A drag
/// after the press adds the dragged range instead, as in Pierre.
pub(crate) const ADD_CARET: Gesture = Gesture(|stroke| {
    let (from, to) = (stroke.from.offset, stroke.to.offset);
    if from == to {
        return toggle_caret(stroke.base, to);
    }
    let mut selections = stroke.base.clone();
    selections.add(Selection {
        anchor: from,
        head: to,
    });
    selections
});

/// Alt+drag: one selection per line from the press to the pointer,
/// each ending at the pointer's column even when the pointer's own
/// line is shorter.
pub(crate) const COLUMN: Gesture = Gesture(|stroke| {
    let (from, to) = (stroke.from, stroke.to);
    column(
        stroke.buffer,
        stroke.layout,
        (from.line, from.x),
        (to.line, stroke.pointer),
    )
});

/// The bracket at or just before a doc's primary caret and the one
/// that matches it, on the doc's entity. Brackets inside strings and
/// comments, as last lexed, neither match nor count. Each editable
/// view of the doc tints both in its `bracket` [`MarkLayer`].
#[derive(Component, Clone, Copy, Debug)]
pub struct BracketMatch {
    brackets: Option<Brackets>,
    /// The doc's seq and primary head the match was found at.
    read: (u64, usize),
}

impl BracketMatch {
    pub fn brackets(&self) -> Option<Brackets> {
        self.brackets
    }
}

/// The layer of [`Marks`] that holds the bracket pair.
pub const BRACKET: MarkLayer = MarkLayer("bracket");

/// Stronger than a selection's tint, so a pair inside a selection
/// still shows.
const BRACKET_ALPHA: f32 = 0.3;

/// Finds each doc's bracket pair again when its text or its primary
/// head moved, and brings every view's `bracket` layer to it. A view
/// built since, such as after a theme toggle, takes the pair too.
pub(crate) fn match_brackets(
    mut docs: Query<(Entity, &EditDoc, Option<&mut BracketMatch>)>,
    mut views: Query<(&EditView, &mut Marks)>,
    layers: Query<&CaretLayer>,
    mut commands: Commands,
) {
    for (entity, doc, held) in &mut docs {
        let read =
            (doc.seq(), doc.editor().selections().primary().head);
        let find = || {
            matching_bracket(doc.editor().buffer(), read.1, |line| {
                skipped(doc, line)
            })
        };
        let brackets = match held {
            Some(held) if held.read == read => held.brackets,
            Some(mut held) => {
                let brackets = find();
                let held = if held.brackets == brackets {
                    held.bypass_change_detection()
                } else {
                    &mut *held
                };
                *held = BracketMatch { brackets, read };
                brackets
            }
            None => {
                let brackets = find();
                commands
                    .entity(entity)
                    .insert(BracketMatch { brackets, read });
                brackets
            }
        };
        for (view, mut marks) in &mut views {
            if view.doc() != entity {
                continue;
            }
            let Ok(layer) = layers.get(view.carets()) else {
                continue;
            };
            let tint = layer.color.with_alpha(BRACKET_ALPHA);
            let want =
                pair_marks(doc.editor().buffer(), brackets, tint);
            if marks.layer(BRACKET) != want.as_slice() {
                marks.set(BRACKET, want);
            }
        }
    }
}

/// The byte ranges of line `line`'s string and comment runs.
fn skipped(doc: &EditDoc, line: u32) -> Vec<Range<usize>> {
    spans(doc.runs(line as usize))
        .filter(|(_, kind)| {
            matches!(kind.root(), Kind::String | Kind::Comment)
        })
        .map(|(bytes, _)| bytes)
        .collect()
}

/// A tint on each bracket, in the order [`Marks::set`] keeps them.
fn pair_marks(
    buffer: &Buffer,
    brackets: Option<Brackets>,
    color: Color,
) -> Vec<Mark> {
    let Some(Brackets { near, far }) = brackets else {
        return Vec::new();
    };
    let mut ends = [near, far];
    ends.sort_unstable();
    ends.into_iter()
        .map(|at| {
            let line = buffer.line_of(at);
            let byte = at - buffer.line_start(line);
            Mark {
                line,
                kind: MarkKind::Tint {
                    bytes: byte..byte + 1,
                    color,
                },
            }
        })
        .collect()
}
