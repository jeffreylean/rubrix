use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use rubrix_highlight::LineEdit;

/// What one [`Buffer::replace`](crate::Buffer::replace) did to a
/// text: replacements sorted by offset, none overlapping, in the byte
/// offsets of the text before it.
///
/// Only the buffer builds a change, so every change agrees with the
/// text it came from: each range lay on char boundaries there, each
/// removed text is the text that was there, and each line number is
/// right. No replacement is empty on both sides. Several replacements
/// that remove nothing may share an offset; their texts follow each
/// other in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    reps: Vec<Rep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rep {
    /// The first byte replaced, before the change.
    pub(crate) start: usize,
    pub(crate) old: String,
    pub(crate) new: String,
    /// The line of `start`, before the change.
    pub(crate) line: u32,
    /// Where `start` lands after the change: `start` plus the byte
    /// delta of the replacements before this one. `Change::new` and
    /// `Change::join` recompute it for every replacement.
    pub(crate) after: usize,
}

impl Rep {
    fn end(&self) -> usize {
        self.start + self.old.len()
    }
}

/// The number of line breaks in `text`.
fn breaks(text: &str) -> u32 {
    text.bytes().filter(|&byte| byte == b'\n').count() as u32
}

/// One replacement of a [`Change`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement<'a> {
    /// The bytes replaced, in the text before the change.
    pub range: Range<usize>,
    pub old: &'a str,
    pub new: &'a str,
}

/// Which side of the new text an offset lands on when the change
/// inserts text exactly at it or replaces the text around it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Assoc {
    Before,
    #[default]
    After,
}

impl Change {
    pub(crate) fn new(reps: Vec<Rep>) -> Change {
        let mut change = Change { reps };
        change.place();
        change
    }

    /// The number of replacements. A typing keystroke has one per
    /// caret.
    pub fn len(&self) -> usize {
        self.reps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reps.is_empty()
    }

    pub fn replacements(
        &self,
    ) -> impl ExactSizeIterator<Item = Replacement<'_>> + '_ {
        self.reps.iter().map(|rep| Replacement {
            range: rep.start..rep.end(),
            old: &rep.old,
            new: &rep.new,
        })
    }

    /// Lines after the change minus lines before it.
    pub fn line_delta(&self) -> i64 {
        self.reps
            .iter()
            .map(|rep| {
                i64::from(breaks(&rep.new))
                    - i64::from(breaks(&rep.old))
            })
            .sum()
    }

    /// Moves `offset`, a byte of the text before the change, to the
    /// same place in the text after it.
    ///
    /// The replacements are taken in order, as Pierre's
    /// `remapOffsetThroughEdits` does. An offset before a replacement
    /// stops the walk. An offset at or past a replacement's end
    /// shifts by its change in length, so the end of a replaced range
    /// lands at the end of the new text, and text inserted exactly at
    /// the offset pushes it right. With [`Assoc::Before`] the walk
    /// instead stops at the first replacement that inserts at the
    /// offset or starts a replaced range there, and the offset lands
    /// at the start of its new text. An offset inside a replaced
    /// range lands at the end of the new text with [`Assoc::After`]
    /// and at its start with `Before`. O(log n) in the replacements.
    pub fn map(&self, offset: usize, assoc: Assoc) -> usize {
        // The walk stops at the first replacement past `offset`, or
        // holding it, and the ends of sorted, non-overlapping
        // replacements ascend, so the stop is a partition point.
        let stops = |rep: &Rep| match assoc {
            Assoc::After => offset < rep.end(),
            Assoc::Before => offset < rep.end().max(rep.start + 1),
        };
        let i = self.reps.partition_point(|rep| !stops(rep));
        match self.reps.get(i) {
            Some(rep) if rep.start <= offset => match assoc {
                Assoc::After => self.end_after(i),
                Assoc::Before => rep.after,
            },
            _ if i == 0 => offset,
            _ => {
                offset - self.reps[i - 1].end()
                    + self.end_after(i - 1)
            }
        }
    }

    /// The change as `LineEdit`s for
    /// `rubrix_highlight::Highlights::edit_many` and a row list,
    /// ascending and in sequential coordinates: each edit's `first`
    /// counts lines in the text with every earlier edit applied and
    /// no later one. That is the line of the text after the change
    /// where the edit's new lines begin, so a consumer applies the
    /// edits top to bottom with no arithmetic of its own.
    ///
    /// Replacements that touch one line before the change share an
    /// edit, so no two edits overlap. Every edit removes at least one
    /// line and inserts at least one, and
    /// `edits[i].first + edits[i].inserted <= edits[i + 1].first`.
    pub fn line_edits(&self) -> impl Iterator<Item = LineEdit> + '_ {
        let mut reps = self.reps.iter().peekable();
        let (mut added, mut gone) = (0, 0);
        core::iter::from_fn(move || {
            let rep = reps.next()?;
            let first = rep.line;
            let mut last = rep.line + breaks(&rep.old);
            let (mut old, mut new) =
                (breaks(&rep.old), breaks(&rep.new));
            while let Some(rep) = reps.next_if(|rep| rep.line <= last)
            {
                last = rep.line + breaks(&rep.old);
                old += breaks(&rep.old);
                new += breaks(&rep.new);
            }
            let removed = last - first + 1;
            let inserted = removed - old + new;
            let edit = LineEdit {
                first: first + added - gone,
                removed,
                inserted,
            };
            added += inserted;
            gone += removed;
            Some(edit)
        })
    }

    /// The replacements that undo this change, in the offsets of the
    /// text after it, ready for `Buffer::replace`.
    pub(crate) fn inverse(
        &self,
    ) -> impl Iterator<Item = (Range<usize>, &str)> + '_ {
        self.reps.iter().map(|rep| {
            (rep.after..rep.after + rep.new.len(), rep.old.as_str())
        })
    }

    /// The replacements that redo this change, in the offsets of the
    /// text before it.
    pub(crate) fn forward(
        &self,
    ) -> impl Iterator<Item = (Range<usize>, &str)> + '_ {
        self.reps
            .iter()
            .map(|rep| (rep.start..rep.end(), rep.new.as_str()))
    }

    /// Where replacement `i` starts in the text after the change.
    pub(crate) fn start_after(&self, i: usize) -> usize {
        self.reps[i].after
    }

    /// Where replacement `i`'s new text ends in the text after the
    /// change.
    pub(crate) fn end_after(&self, i: usize) -> usize {
        self.reps[i].after + self.reps[i].new.len()
    }

    /// Folds `next`, made right after `self`, into `self`, so that
    /// `self` turns the text before itself into the text after
    /// `next`. History calls it only when its join rule holds, so
    /// both changes have as many replacements, and each replacement
    /// of `next` extends the same-index replacement of `self`: an
    /// insert appends to `new`, a Delete appends to `old`, and a
    /// Backspace moves `start` back over the text it removed and puts
    /// that text in front of `old`.
    pub(crate) fn join(&mut self, next: &Change) {
        for (rep, next) in self.reps.iter_mut().zip(&next.reps) {
            if next.old.is_empty() {
                rep.new.push_str(&next.new);
            } else if next.start == rep.after {
                rep.old.push_str(&next.old);
            } else {
                rep.start -= next.old.len();
                rep.old.insert_str(0, &next.old);
            }
        }
        self.place();
    }

    /// Sets every replacement's `after` from the replacements before
    /// it.
    fn place(&mut self) {
        let (mut added, mut removed) = (0, 0);
        for rep in &mut self.reps {
            rep.after = rep.start + added - removed;
            added += rep.new.len();
            removed += rep.old.len();
        }
    }
}
