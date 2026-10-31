use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::buffer::Buffer;
use crate::change::{Change, Replacement};
use crate::selection::Selections;

/// How a recorded edit may join a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Typing with no line break in the text. Joins an insert run.
    Insert,
    /// Backspace and delete word back. Joins a Backspace run.
    Backspace,
    /// Delete and delete word forward. Joins a Delete run.
    Delete,
    /// Paste and cut. Joins nothing, and nothing joins it.
    Boundary,
    /// Enter, typed text with a line break, and anything else. Never
    /// joins.
    Other,
}

/// One undo step: the change from the text before its first edit to
/// the text after its last, and the selections at both ends.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) change: Change,
    pub(crate) kind: Kind,
    /// Restored by undo.
    pub(crate) before: Selections,
    /// Restored by redo.
    pub(crate) after: Selections,
}

/// Undo and redo, grouped by Pierre's rule, with no clock.
///
/// An edit joins the previous entry only when all of these hold,
/// which is Pierre's rule:
///
/// - no undo or redo has happened since the previous edit was
///   recorded;
/// - neither is a paste or a cut, and both have the same `Kind`:
///   typing, Backspace, or Delete;
/// - the edit leaves the line count unchanged;
/// - the caret count is unchanged: the edit started from as many
///   selections as the entry ended with, and it has as many
///   replacements as the entry;
/// - for every index `i`, replacement `i` of the edit continues
///   replacement `i` of the entry, in the offsets of the text between
///   the two. Typing removes nothing, holds no line break, and starts
///   exactly where the entry's new text ends, so a caret that jumped
///   to the left edge of the run does not join; the entry's own old
///   and new texts hold no line break. Backspace removes text that
///   ends where the entry's replacement now starts, and Delete text
///   that starts there; both insert nothing.
///
/// Unlike Pierre, the command names the run, so a Backspace after a
/// Delete of a selection starts a new step.
#[derive(Clone, Debug)]
pub struct History {
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    max_entries: usize,
    /// False after an undo or a redo, until the next edit is
    /// recorded.
    open: bool,
}

impl History {
    pub const DEFAULT_MAX_ENTRIES: usize = 100;

    /// Keeps at most [`History::DEFAULT_MAX_ENTRIES`] entries.
    pub fn new() -> History {
        History::with_max_entries(History::DEFAULT_MAX_ENTRIES)
    }

    /// Keeps at most `max_entries` entries and drops the oldest past
    /// that.
    ///
    /// # Panics
    ///
    /// Panics if `max_entries` is 0.
    pub fn with_max_entries(max_entries: usize) -> History {
        assert!(max_entries > 0, "a history of no entries");
        History {
            undo: VecDeque::new(),
            redo: Vec::new(),
            max_entries,
            open: true,
        }
    }

    /// The number of steps undo can take.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// The number of steps redo can take.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Records an edit that turned the selections `before` into
    /// `after`. Joins the last entry when the rule holds, else pushes
    /// a new one. Clears redo.
    pub(crate) fn record(
        &mut self,
        change: Change,
        kind: Kind,
        before: Selections,
        after: Selections,
    ) {
        self.redo.clear();
        match self.undo.back_mut() {
            Some(last)
                if joins(self.open, last, &change, kind, &before) =>
            {
                last.change.join(&change);
                last.after = after;
            }
            _ => {
                self.undo.push_back(Entry {
                    change,
                    kind,
                    before,
                    after,
                });
                if self.undo.len() > self.max_entries {
                    self.undo.pop_front();
                }
            }
        }
        self.open = true;
    }

    /// Undoes the last entry on `buffer`: applies the inverse of its
    /// change and returns the change that made, with the selections
    /// from before the entry. Closes the group.
    pub(crate) fn undo(
        &mut self,
        buffer: &mut Buffer,
    ) -> Option<(Change, Selections)> {
        let entry = self.undo.pop_back()?;
        let change = entry.change.undo(buffer);
        let before = entry.before.clone();
        self.redo.push(entry);
        self.open = false;
        Some((change, before))
    }

    /// Redoes the last undone entry on `buffer`, and returns the
    /// change that made, with the selections from after the entry.
    /// Closes the group.
    pub(crate) fn redo(
        &mut self,
        buffer: &mut Buffer,
    ) -> Option<(Change, Selections)> {
        let entry = self.redo.pop()?;
        let change = entry.change.redo(buffer);
        let after = entry.after.clone();
        self.undo.push_back(entry);
        self.open = false;
        Some((change, after))
    }
}

/// Whether `next`, of kind `kind`, joins `last` under the rule in
/// [`History`]'s doc. `open` is false after an undo or a redo.
fn joins(
    open: bool,
    last: &Entry,
    next: &Change,
    kind: Kind,
    before: &Selections,
) -> bool {
    let entry = &last.change;
    let continues = |(i, (own, next)): (
        usize,
        (Replacement<'_>, Replacement<'_>),
    )| {
        match kind {
            Kind::Insert => {
                next.old.is_empty()
                    && !next.new.contains('\n')
                    && next.range.start == entry.end_after(i)
                    && !own.old.contains('\n')
                    && !own.new.contains('\n')
            }
            Kind::Backspace => {
                next.new.is_empty()
                    && next.range.end == entry.start_after(i)
            }
            Kind::Delete => {
                next.new.is_empty()
                    && next.range.start == entry.start_after(i)
            }
            Kind::Boundary | Kind::Other => false,
        }
    };
    open && kind == last.kind
        && next.line_delta() == 0
        && before.len() == last.after.len()
        && next.len() == entry.len()
        && entry
            .replacements()
            .zip(next.replacements())
            .enumerate()
            .all(continues)
}

impl Default for History {
    fn default() -> History {
        History::new()
    }
}
