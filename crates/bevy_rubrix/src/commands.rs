//! The bindings that are more than one command: the comment toggles,
//! which read the doc's language, the add-caret and column gestures,
//! and the bracket pair at the primary caret.

use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::{DetectChangesMut, Mut};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::Without;
use bevy::ecs::system::{Commands, Query};
use rubrix_edit::{
    Brackets, Buffer, Command, SCAN_LINES, Selection, bracket_skips,
    column, matching_bracket, toggle_caret,
};

use crate::caret::CaretLayer;
use crate::diff_edit::LiveFile;
use crate::editor::{EditDoc, EditView};
use crate::frame::Tx;
use crate::keymap::{Gesture, added};
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
    let (from, to) = (stroke.from, stroke.to);
    if from.offset == to.offset {
        return toggle_caret(stroke.base, to.offset, to.affinity);
    }
    added(
        stroke.base,
        Selection {
            anchor: from.offset,
            head: to.offset,
        },
        to.affinity,
    )
});

/// Alt+drag: one selection per line from the press to the pointer,
/// at the columns under the press and the pointer even where their
/// own lines are shorter.
pub(crate) const COLUMN: Gesture = Gesture(|stroke| {
    let (from, to) = (stroke.from, stroke.to);
    column(
        stroke.buffer,
        stroke.layout,
        (from.offset, from.affinity, from.pointer),
        (to.offset, to.affinity, to.pointer),
    )
});

/// The bracket at or just before a doc's primary caret and the one
/// that matches it, on the doc's entity. Brackets inside strings and
/// comments neither match nor count. Each editable view of the doc
/// tints both in its `bracket` [`MarkLayer`].
#[derive(Component, Clone, Copy, Debug)]
pub struct BracketMatch {
    brackets: Option<Brackets>,
    /// The doc's highlight generation when the match was found.
    lexed: u64,
    /// The primary head the match was found at.
    head: usize,
}

impl BracketMatch {
    pub fn brackets(&self) -> Option<Brackets> {
        self.brackets
    }
}

/// The layer of [`Marks`] that holds the bracket pair.
pub const BRACKET: MarkLayer = MarkLayer("bracket");

pub(crate) const BRACKET_ALPHA: f32 = 0.3;

/// The bracket match of every doc but a diff's: a diff lexes and
/// matches its own docs in one pass with its rows' window.
pub(crate) fn match_brackets(
    mut docs: Query<
        (Entity, &mut EditDoc, Option<&mut BracketMatch>),
        Without<LiveFile>,
    >,
    mut views: Query<(&EditView, &mut Marks)>,
    layers: Query<&CaretLayer>,
    mut commands: Commands,
) {
    for (entity, mut doc, held) in &mut docs {
        let line = doc
            .editor()
            .buffer()
            .line_of(doc.editor().selections().primary().head);
        doc.bypass_change_detection()
            .catch_up(line.saturating_add(SCAN_LINES));
        let fresh = refreshed(&doc, held.as_deref());
        let brackets = match (held, fresh) {
            (Some(held), None) => held.brackets,
            (Some(held), Some(fresh)) => {
                keep(held, fresh);
                fresh.brackets
            }
            (None, fresh) => {
                if let Some(fresh) = fresh {
                    commands.entity(entity).insert(fresh);
                }
                fresh.and_then(|fresh| fresh.brackets)
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

/// The bracket pair at `doc`'s primary caret, or `None` while
/// `held` was found at the same caret and lexing.
pub(crate) fn refreshed(
    doc: &EditDoc,
    held: Option<&BracketMatch>,
) -> Option<BracketMatch> {
    let head = doc.editor().selections().primary().head;
    let lexed = doc.highlight_seq();
    if held
        .is_some_and(|held| (held.lexed, held.head) == (lexed, head))
    {
        return None;
    }
    let brackets =
        matching_bracket(doc.editor().buffer(), head, |line| {
            doc.current_runs(line).map(bracket_skips)
        });
    Some(BracketMatch {
        brackets,
        lexed,
        head,
    })
}

/// Writes `fresh` over `held`, marking it changed only when the pair
/// moved.
pub(crate) fn keep(mut held: Mut<BracketMatch>, fresh: BracketMatch) {
    if held.brackets == fresh.brackets {
        *held.bypass_change_detection() = fresh;
    } else {
        *held = fresh;
    }
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

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::ecs::world::World;
    use rubrix_edit::{Monospace, Selections};
    use rubrix_highlight::Lang;

    use super::*;
    use crate::editor::CATCH_UP;

    const LEXED_LINES: u32 = 10;

    fn stale_comment_above(open: u32) -> (World, Entity) {
        let pad = "x\n".repeat(open as usize - 1);
        let text = format!("a\n{pad}{{\n{}}}\n", "x\n".repeat(4));
        let mut doc = EditDoc::new(Lang::Rust, &text);
        let opened = doc.run(
            |editor| {
                let change = editor.apply(
                    Command::Type("/*"),
                    &Monospace::default(),
                );
                let after_brace =
                    editor.buffer().line_start(open) + 1;
                editor.set_selections(Selections::new(
                    Selection::caret(after_brace),
                ));
                change
            },
            |_, _| LEXED_LINES,
        );
        assert!(opened.is_some());
        let mut world = World::new();
        let entity = world.spawn(doc).id();
        (world, entity)
    }

    fn found(world: &mut World, entity: Entity) -> Option<Brackets> {
        world
            .run_system_once(match_brackets)
            .expect("the system runs");
        world
            .get::<BracketMatch>(entity)
            .and_then(BracketMatch::brackets)
    }

    #[test]
    fn the_match_lexes_the_lines_it_scans_first() {
        let (mut world, entity) = stale_comment_above(600);
        assert_eq!(
            found(&mut world, entity),
            None,
            "the closing brace is inside the comment"
        );
    }

    #[test]
    fn the_match_trusts_no_line_still_stale_after_the_catch_up() {
        let past_one_catch_up = CATCH_UP + SCAN_LINES;
        let (mut world, entity) =
            stale_comment_above(past_one_catch_up);
        assert_eq!(found(&mut world, entity), None);
    }
}
