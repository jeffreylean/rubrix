use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::ops::Range;

use crate::buffer::Buffer;
use crate::change::Change;
use crate::history::{History, Kind};
use crate::motion::{
    Layout, Motion, indent, next_grapheme, prev_grapheme, snap,
    word_stop,
};
use crate::selection::{Selection, Selections};

/// One editing action, run at every selection by [`Editor::apply`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    /// Replaces each selection with the text, its line breaks written
    /// as the buffer's [`Eol`](crate::Eol). Text with no line break
    /// joins a typing run.
    Type(&'a str),
    /// The same as `Type`, but always its own undo step. The whole
    /// text goes in at every caret.
    Paste(&'a str),
    /// Deletes every selection, as its own undo step. The deleted
    /// text is in the returned [`Change`].
    Cut,
    /// Replaces each selection with a line break and the leading
    /// whitespace of the caret's line, up to the caret.
    Enter,
    /// Deletes each selection, or the grapheme cluster before each
    /// caret. At a line's start it joins the line to the one above.
    Backspace,
    /// Deletes each selection, or the grapheme cluster after each
    /// caret.
    Delete,
    /// Deletes each selection, or back to where [`Motion::WordLeft`]
    /// would stop.
    DeleteWordBack,
    /// Deletes each selection, or forward to where
    /// [`Motion::WordRight`] would stop.
    DeleteWordForward,
    /// Moves every head. Without `extend`, each selection collapses
    /// to a caret, and Left or Right on a range stops at its start or
    /// end. With `extend`, each anchor stays.
    Move {
        motion: Motion,
        extend: bool,
    },
    /// One selection over the whole text.
    SelectAll,
    /// Keeps only the primary selection.
    CollapseToPrimary,
    Undo,
    Redo,
}

/// A buffer with its selections and its history. One per editable
/// view.
///
/// The editor owns all three, so no change reaches the text without
/// moving the selections and being recorded.
#[derive(Debug)]
pub struct Editor {
    buffer: Buffer,
    selections: Selections,
    history: History,
}

impl Editor {
    /// One caret at offset 0 and an empty [`History::new`].
    pub fn new(buffer: Buffer) -> Editor {
        Editor {
            buffer,
            selections: Selections::new(Selection::caret(0)),
            history: History::new(),
        }
    }

    /// An editor from saved parts, such as a buffer snapshot with the
    /// selections and history saved beside it. The selections go
    /// through [`Editor::set_selections`].
    pub fn from_parts(
        buffer: Buffer,
        selections: Selections,
        history: History,
    ) -> Editor {
        let mut editor = Editor {
            history,
            ..Editor::new(buffer)
        };
        editor.set_selections(selections);
        editor
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    /// Replaces the selections. Clamps each end to the text and moves
    /// it back to a grapheme boundary, so no caret sits inside a
    /// cluster or between a `\r` and its `\n`, then merges selections
    /// that came to overlap. Clears every goal x.
    pub fn set_selections(&mut self, selections: Selections) {
        let buffer = &self.buffer;
        self.selections = selections;
        self.selections.update(|selection, goal| {
            *goal = None;
            Selection {
                anchor: snap(buffer, selection.anchor),
                head: snap(buffer, selection.head),
            }
        });
    }

    /// Runs `command` at every selection.
    ///
    /// Returns the [`Change`] it made to the text, or `None` when the
    /// text did not change: a motion, a selection command, or an
    /// edit, undo or redo with nothing to do. Undo and redo return
    /// the change they made, like any edit, so a view and remote
    /// carets follow them the same way.
    ///
    /// An edit makes one replacement per selection, in buffer order.
    /// When the ranges of two selections overlap, as word deletes at
    /// two near carets can, they become one replacement, so a change
    /// can have fewer replacements than there were carets.
    #[must_use = "the change keeps highlights, rows and carets in \
                  step"]
    pub fn apply(
        &mut self,
        command: Command<'_>,
        layout: &impl Layout,
    ) -> Option<Change> {
        use Motion::{Down, PageDown, PageUp, Up};
        if !matches!(
            command,
            Command::Move {
                motion: Up | Down | PageUp | PageDown,
                ..
            }
        ) {
            self.selections.clear_goals();
        }
        match command {
            Command::Type(text) | Command::Paste(text) => {
                let text = self.buffer.eol().normalize(text);
                let kind = match command {
                    Command::Paste(_) => Kind::Boundary,
                    _ if text.contains('\n') => Kind::Other,
                    _ => Kind::Insert,
                };
                self.edit(kind, |_, selection| {
                    (selection.range(), Cow::Borrowed(&*text))
                })
            }
            Command::Cut => self
                .edit(Kind::Boundary, |_, selection| {
                    (selection.range(), Cow::Borrowed(""))
                }),
            Command::Enter => {
                let eol = self.buffer.eol().as_str();
                self.edit(Kind::Other, |buffer, selection| {
                    let spot = buffer.spot(selection.start());
                    let indent = indent(&spot.content[..spot.rel]);
                    (
                        selection.range(),
                        Cow::Owned([eol, indent].concat()),
                    )
                })
            }
            Command::Backspace => {
                self.delete(Kind::Backspace, prev_grapheme)
            }
            Command::Delete => {
                self.delete(Kind::Delete, next_grapheme)
            }
            Command::DeleteWordBack => self
                .delete(Kind::Backspace, |buffer, at| {
                    word_stop(buffer, at, false)
                }),
            Command::DeleteWordForward => self
                .delete(Kind::Delete, |buffer, at| {
                    word_stop(buffer, at, true)
                }),
            Command::Move { motion, extend } => {
                self.move_heads(motion, extend, layout);
                None
            }
            Command::SelectAll => {
                self.selections = Selections::new(Selection {
                    anchor: 0,
                    head: self.buffer.len(),
                });
                None
            }
            Command::CollapseToPrimary => {
                self.selections.keep_primary();
                None
            }
            Command::Undo => self.restore(History::undo),
            Command::Redo => self.restore(History::redo),
        }
    }

    /// Replaces the range `edit` gives for each selection, in buffer
    /// order, with its text, joining ranges that overlap. Moves the
    /// selections through the change and records it as `kind`.
    fn edit<'t>(
        &mut self,
        kind: Kind,
        mut edit: impl FnMut(
            &Buffer,
            Selection,
        ) -> (Range<usize>, Cow<'t, str>),
    ) -> Option<Change> {
        let mut edits: Vec<(Range<usize>, Cow<'t, str>)> =
            Vec::with_capacity(self.selections.len());
        for selection in self.selections.iter() {
            let (range, text) = edit(&self.buffer, selection);
            match edits.last_mut() {
                Some((last, _)) if range.start < last.end => {
                    last.end = last.end.max(range.end);
                }
                _ => edits.push((range, text)),
            }
        }
        let change = self.buffer.replace(edits);
        if change.is_empty() {
            return None;
        }
        let before = self.selections.clone();
        self.selections.map_through(&change);
        self.history.record(
            change.clone(),
            kind,
            before,
            self.selections.clone(),
        );
        Some(change)
    }

    /// Deletes each selection, or the text from each caret to where
    /// `to` takes it.
    fn delete(
        &mut self,
        kind: Kind,
        to: impl Fn(&Buffer, usize) -> usize,
    ) -> Option<Change> {
        self.edit(kind, |buffer, selection| {
            let range = if selection.is_caret() {
                let (at, to) =
                    (selection.head, to(buffer, selection.head));
                at.min(to)..at.max(to)
            } else {
                selection.range()
            };
            (range, Cow::Borrowed(""))
        })
    }

    fn move_heads(
        &mut self,
        motion: Motion,
        extend: bool,
        layout: &impl Layout,
    ) {
        let buffer = &self.buffer;
        self.selections.update(|selection, goal| {
            let from = match motion {
                _ if extend => selection.head,
                Motion::Left | Motion::WordLeft
                    if !selection.is_caret() =>
                {
                    return Selection::caret(selection.start());
                }
                Motion::Right | Motion::WordRight
                    if !selection.is_caret() =>
                {
                    return Selection::caret(selection.end());
                }
                Motion::Up => selection.start(),
                Motion::Down => selection.end(),
                _ => selection.head,
            };
            let head = motion.target(buffer, layout, from, goal);
            Selection {
                anchor: if extend { selection.anchor } else { head },
                head,
            }
        });
    }

    fn restore(
        &mut self,
        step: fn(
            &mut History,
            &mut Buffer,
        ) -> Option<(Change, Selections)>,
    ) -> Option<Change> {
        let (change, selections) =
            step(&mut self.history, &mut self.buffer)?;
        self.selections = selections;
        Some(change)
    }
}
