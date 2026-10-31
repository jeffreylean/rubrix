//! The bindings that are more than one command: the comment toggles,
//! which read the doc's language, the add-caret and column gestures,
//! and the bracket pair at the primary caret.

use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::{Commands, Query};
use rubrix_edit::{
    Brackets, Buffer, Command, Selection, bracket_skips, column,
    matching_bracket, toggle_caret,
};

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
/// at the columns under the press and the pointer even where their
/// own lines are shorter.
pub(crate) const COLUMN: Gesture = Gesture(|stroke| {
    let (from, to) = (stroke.from, stroke.to);
    column(
        stroke.buffer,
        stroke.layout,
        (from.line, from.pointer),
        (to.line, to.pointer),
    )
});

/// The bracket at or just before a doc's primary caret and the one
/// that matches it, on the doc's entity. Brackets inside strings and
/// comments, as last lexed, neither match nor count. Each editable
/// view of the doc tints both in its `bracket` [`MarkLayer`].
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
        let (lexed, head) = (
            doc.highlight_seq(),
            doc.editor().selections().primary().head,
        );
        let find = || {
            matching_bracket(doc.editor().buffer(), head, |line| {
                bracket_skips(doc.runs(line as usize))
            })
        };
        let brackets = match held {
            Some(held)
                if (held.lexed, held.head) == (lexed, head) =>
            {
                held.brackets
            }
            Some(mut held) => {
                let brackets = find();
                let held = if held.brackets == brackets {
                    held.bypass_change_detection()
                } else {
                    &mut *held
                };
                *held = BracketMatch {
                    brackets,
                    lexed,
                    head,
                };
                brackets
            }
            None => {
                let brackets = find();
                commands.entity(entity).insert(BracketMatch {
                    brackets,
                    lexed,
                    head,
                });
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
    use rubrix_edit::{Editor, Monospace, Selections};
    use rubrix_highlight::Lang;

    use super::*;

    #[test]
    fn a_catch_up_finds_the_bracket_match_again() {
        let text = format!("{{\n{}}}\n", "x\n".repeat(600));
        let after_brace = Selections::new(Selection::caret(1));
        let mut editor = Editor::new(Buffer::new(&text));
        editor.set_selections(after_brace.clone());
        let mut doc = EditDoc::from_editor(Lang::Rust, editor);
        let opened = doc.run(
            |editor| {
                editor.set_selections(Selections::new(
                    Selection::caret(2),
                ));
                let change = editor.apply(
                    Command::Type("/*"),
                    &Monospace::default(),
                );
                editor.set_selections(after_brace);
                change
            },
            |_, _| 10,
        );
        assert!(opened.is_some());
        let mut world = World::new();
        let entity = world.spawn(doc).id();
        let found = |world: &mut World| {
            world
                .run_system_once(match_brackets)
                .expect("the system runs");
            world
                .get::<BracketMatch>(entity)
                .and_then(BracketMatch::brackets)
        };
        assert!(
            found(&mut world).is_some(),
            "the runs past the reach are still stale"
        );
        world
            .get_mut::<EditDoc>(entity)
            .expect("a doc")
            .catch_up(u32::MAX);
        assert_eq!(
            found(&mut world),
            None,
            "the closing brace is inside the comment"
        );
    }
}
