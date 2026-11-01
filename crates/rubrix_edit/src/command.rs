use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use rubrix_highlight::Lang;

use crate::buffer::Buffer;
use crate::change::{Change, Move};
use crate::history::{History, Kind};
use crate::motion::{
    Layout, Motion, indent, next_grapheme, prev_grapheme, snap,
    word_stop,
};
use crate::selection::{Selection, Selections};
use crate::{comment, lines, occurrence, surround};

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
    /// Replaces selection `i`, in buffer order, with piece `i`, its
    /// line breaks written as the buffer's [`Eol`](crate::Eol), as
    /// one undo step. The pieces of [`Editor::copy_pieces`] at as
    /// many selections go back whole, each to its own. With another
    /// number of pieces it does nothing.
    PasteEach(&'a [String]),
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
    /// Keeps only the primary selection, as it is. A single range
    /// becomes a caret at its head.
    CollapseToPrimary,
    Undo,
    Redo,
    /// Indents the lines of a selection that spans lines, or puts an
    /// indent at a caret (Tab).
    Indent,
    /// Removes one indent from the start of each selected line
    /// (Shift+Tab, mod+\[).
    Outdent,
    /// Adds one indent to the start of each selected line, even at a
    /// caret, like Pierre's `indentMore` (mod+\]).
    IndentLines,
    /// Moves the selected lines one line down, or up, past the line
    /// next to them.
    MoveLines {
        down: bool,
    },
    /// Copies the selected lines below them, or above them, and
    /// selects the copy.
    CopyLines {
        down: bool,
    },
    /// Opens a blank line below each caret's line and moves the caret
    /// to it, keeping the line's indent (mod+Enter).
    BlankLineBelow,
    /// Swaps the grapheme clusters on either side of each caret
    /// (Ctrl+T).
    Transpose,
    /// Deletes from each caret to its line's end, or joins the next
    /// line at the end (emacs Ctrl+K).
    DeleteToLineEnd,
    /// Comments or uncomments the selected lines with `lang`'s
    /// markers. With `block` false it uses the line marker, falling
    /// back to the block pair when `lang` has none. With `block` true
    /// it wraps each selection in the block pair.
    ToggleComment {
        lang: Lang,
        block: bool,
    },
    /// Selects the word at a caret, or adds a selection at the next
    /// occurrence of the primary's text (mod+D).
    AddNextOccurrence,
}

/// Where the selections go after a [`Plan`] is applied.
pub(crate) enum After {
    /// Through the change, as [`Selections::map_through`] moves them.
    Map,
    /// To these selections, in the offsets of the text after the
    /// change. The planner keeps their ages.
    Set(Selections),
}

/// The replacements one command makes and where its selections go.
///
/// The edits are offsets of the text before the change, sorted by
/// start and not overlapping, as [`Buffer::replace`] requires.
pub(crate) struct Plan<S = String> {
    pub(crate) edits: Vec<(Range<usize>, S)>,
    pub(crate) after: After,
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

    /// The text of the selections in buffer order: each range's text
    /// and each bare caret's line with its line break, the text two
    /// selections share once. The buffer's line ending joins two
    /// pieces that do not touch, unless the first ends in one.
    pub fn copy_text(&self) -> String {
        lines::copy(&self.buffer, &self.selections)
    }

    /// The text each selection gives a copy, in buffer order: each
    /// range's text and each bare caret's line with its line break,
    /// one piece per selection, for [`Command::PasteEach`].
    pub fn copy_pieces(&self) -> Vec<String> {
        lines::pieces(&self.buffer, &self.selections)
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
            Command::Type(text) => {
                if let Some(plan) =
                    surround::wrap(&self.selections, text)
                {
                    return self.commit(Kind::Other, plan);
                }
                let text = self.buffer.eol().normalize(text);
                let kind = if text.contains('\n') {
                    Kind::Other
                } else {
                    Kind::Insert
                };
                self.replace_each(kind, &text)
            }
            Command::Paste(text) => {
                let text = self.buffer.eol().normalize(text);
                match lines::paste_lines(
                    &self.buffer,
                    &self.selections,
                    &text,
                ) {
                    Some(plan) => self.commit(Kind::Boundary, plan),
                    None => self.replace_each(Kind::Boundary, &text),
                }
            }
            Command::PasteEach(pieces) => {
                let eol = self.buffer.eol();
                let pieces: Vec<Cow<str>> = pieces
                    .iter()
                    .map(|piece| eol.normalize(piece))
                    .collect();
                let plan =
                    lines::paste_each(&self.selections, &pieces)?;
                self.commit(Kind::Boundary, plan)
            }
            Command::Cut => self.commit(
                Kind::Boundary,
                lines::cut(&self.buffer, &self.selections),
            ),
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
                self.delete(Kind::Backspace, |buffer, at| {
                    lines::soft_tab_start(buffer, at)
                        .unwrap_or_else(|| prev_grapheme(buffer, at))
                })
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
                if self.selections.len() > 1 {
                    self.selections.keep_primary();
                } else {
                    self.selections.update(|selection, _| {
                        Selection::caret(selection.head)
                    });
                }
                None
            }
            Command::Undo => self.restore(History::undo),
            Command::Redo => self.restore(History::redo),
            Command::Indent => self.planned(lines::indent),
            Command::Outdent => self.planned(lines::outdent),
            Command::IndentLines => self.planned(lines::indent_lines),
            Command::MoveLines { down } => {
                let (plan, moves) = lines::move_lines(
                    &self.buffer,
                    &self.selections,
                    down,
                )?;
                self.commit_moving(Kind::Other, plan, moves)
            }
            Command::CopyLines { down } => {
                self.planned(|buffer, selections| {
                    lines::copy_lines(buffer, selections, down)
                })
            }
            Command::BlankLineBelow => {
                self.planned(lines::blank_line_below)
            }
            Command::Transpose => self.planned(lines::transpose),
            Command::DeleteToLineEnd => {
                self.planned(lines::delete_to_line_end)
            }
            Command::ToggleComment { lang, block } => {
                self.planned(|buffer, selections| {
                    let markers = comment::markers(lang);
                    comment::toggle(
                        buffer, selections, markers, block,
                    )
                })
            }
            Command::AddNextOccurrence => {
                if let Some(selections) = occurrence::add_next(
                    &self.buffer,
                    &self.selections,
                ) {
                    self.selections = selections;
                }
                None
            }
        }
    }

    /// Commits the plan `make` makes from the buffer and selections,
    /// as an undo step that joins nothing.
    fn planned(
        &mut self,
        make: impl FnOnce(&Buffer, &Selections) -> Option<Plan>,
    ) -> Option<Change> {
        let plan = make(&self.buffer, &self.selections)?;
        self.commit(Kind::Other, plan)
    }

    /// Replaces every selection with `text`.
    fn replace_each(
        &mut self,
        kind: Kind,
        text: &str,
    ) -> Option<Change> {
        self.edit(kind, |_, selection| {
            (selection.range(), Cow::Borrowed(text))
        })
    }

    /// Replaces the range `edit` gives for each selection, in buffer
    /// order, with its text, joining ranges that overlap, and commits
    /// the result with [`After::Map`].
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
        self.commit(
            kind,
            Plan {
                edits,
                after: After::Map,
            },
        )
    }

    /// Makes the plan's replacements, moves the selections as it
    /// says, and records the change as `kind`. `None`, with the
    /// selections left as they were, when the change is empty.
    fn commit<S: AsRef<str>>(
        &mut self,
        kind: Kind,
        plan: Plan<S>,
    ) -> Option<Change> {
        self.commit_moving(kind, plan, Vec::new())
    }

    fn commit_moving<S: AsRef<str>>(
        &mut self,
        kind: Kind,
        plan: Plan<S>,
        moves: Vec<Move>,
    ) -> Option<Change> {
        let change = self.buffer.replace(plan.edits).moving(moves);
        if change.is_empty() {
            return None;
        }
        let before = self.selections.clone();
        match plan.after {
            After::Map => self.selections.map_through(&change),
            After::Set(selections) => self.selections = selections,
        }
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
