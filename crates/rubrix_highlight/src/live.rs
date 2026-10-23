use alloc::vec::Vec;
use core::mem;
use core::ops::Range;

use crate::table::Line;
use crate::{Highlights, Lines, Run, State, lex_line};

const MIN_GARBAGE: usize = 1 << 12;

/// A change to the lines of a text: `removed` lines starting at line
/// `first` became `inserted` new lines.
///
/// An edit inside one line is `removed: 1, inserted: 1`. Typing a
/// newline at the end of a line is `removed: 1, inserted: 2`,
/// because the line splits in two. Joining a line with the next is
/// `removed: 2, inserted: 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineEdit {
    pub first: u32,
    pub removed: u32,
    pub inserted: u32,
}

impl Highlights {
    /// Brings the highlights up to date with an edit that has
    /// already changed `lines`, and returns a range that covers
    /// every line it lexed inside `window`.
    ///
    /// `lines` holds the text after the edit. The edit's own lines
    /// are lexed at once, whatever `window` is. Lexing then goes on
    /// to the lines below, which a changed end state can change,
    /// and stops at the first line that ends in the state it ended
    /// in before, because every line after it is then unchanged.
    /// Lexing also stops at the end of `window`, or before it if
    /// `window` starts below the edit, and leaves the lines from
    /// there on stale. [`Highlights::runs`] lexes them when it reads
    /// them, and [`Highlights::stale_from`] says where they start.
    ///
    /// The window is the lines the caller draws. Give `0..u32::MAX`
    /// to lex until the states converge.
    ///
    /// An edit below stale lines first lexes the stale lines above
    /// it, so a caller that edits far below its window pays for
    /// the work it put off. An edit above stale lines lexes at least
    /// as far as the first stale line, unless the states converge.
    ///
    /// # Panics
    ///
    /// Panics if the edit lies past the last line or leaves the text
    /// with no line, or if the text grows to 4 GiB or more.
    pub fn edit(
        &mut self,
        edit: LineEdit,
        lines: &(impl Lines + ?Sized),
        window: Range<u32>,
    ) -> Range<u32> {
        let LineEdit {
            first,
            removed,
            inserted,
        } = edit;
        let old_count = self.line_count();
        assert!(
            first <= old_count && removed <= old_count - first,
            "edit past the end of the lines"
        );
        let new_count = (old_count - removed)
            .checked_add(inserted)
            .filter(|&count| count > 0)
            .expect("edit leaves no lines");

        let lowest = self.exact.min(first);
        self.catch_up(first, lines);

        let end = first + removed;
        let boundary = self.lines.start_state(end).clone();
        let resume = if self.exact >= end {
            self.exact - removed + inserted
        } else {
            new_count
        };
        let force_to = if resume < new_count { resume } else { 0 };

        self.splice(edit, lines);

        let region_end = first + inserted;
        let mut expected = boundary;
        let mut line = first;
        let converged = loop {
            if line >= region_end {
                if *self.lines.start_state(line) == expected {
                    break true;
                }
                if line >= new_count
                    || (line >= force_to && !window.contains(&line))
                {
                    break false;
                }
            }
            let before = self.relex(line, lines);
            if line >= region_end {
                expected = before;
            }
            line += 1;
        };
        self.exact = match converged {
            true if resume >= line => resume,
            true => new_count,
            false => line,
        };

        let start = lowest.max(window.start);
        start..line.min(window.end).max(start)
    }

    /// The runs of a line, lexing first any stale lines up to it, in
    /// order, as far as is needed. `lines` holds the current text.
    ///
    /// # Panics
    ///
    /// Panics if `line` is not a line of the text.
    pub fn runs(
        &mut self,
        line: u32,
        lines: &(impl Lines + ?Sized),
    ) -> &[Run] {
        assert!(line < self.line_count(), "line past the end");
        self.catch_up(line + 1, lines);
        self.stored_runs(line)
    }

    /// The first line whose saved runs and states may be out of date,
    /// or `None` when every line is current. Every line before it is
    /// current.
    pub fn stale_from(&self) -> Option<u32> {
        (self.exact < self.line_count()).then_some(self.exact)
    }

    fn splice(
        &mut self,
        edit: LineEdit,
        lines: &(impl Lines + ?Sized),
    ) {
        let LineEdit {
            first,
            removed,
            inserted,
        } = edit;
        self.lines.move_gap(first);
        let first_start = self.lines.start(first);
        let removed_span =
            self.lines.start(first + removed) - first_start;
        for _ in 0..removed {
            self.garbage += self.lines.take_after_gap().runs.len();
        }
        let mut start = first_start;
        for line in first..first + inserted {
            self.lines.insert_at_gap(
                start,
                Line::new(0, 0..0, State::default()),
            );
            start = start
                .checked_add(lines.line(line).len() as u32 + 1)
                .expect("text of 4 GiB or more");
        }
        let end = (self.lines.end() - removed_span)
            .checked_add(start - first_start)
            .expect("text of 4 GiB or more");
        self.lines.set_end(end);
    }

    fn catch_up(&mut self, upto: u32, lines: &(impl Lines + ?Sized)) {
        while self.exact < upto {
            let line = self.exact;
            let before = self.relex(line, lines);
            self.exact = if self.lines.get(line).end == before {
                self.line_count()
            } else {
                line + 1
            };
        }
    }

    fn relex(
        &mut self,
        line: u32,
        lines: &(impl Lines + ?Sized),
    ) -> State {
        let raw = lines.line(line);
        let content = raw.strip_suffix('\r').unwrap_or(raw);
        let first = self.runs.len() as u32;
        let state = self.lines.start_state(line);
        let end = lex_line(self.lang, content, state, &mut self.runs);
        let saved = self.lines.get_mut(line);
        self.garbage += saved.runs.len();
        saved.runs = first..self.runs.len() as u32;
        saved.len = content.len() as u32;
        let before = mem::replace(&mut saved.end, end);
        if self.garbage > MIN_GARBAGE
            && self.garbage > self.runs.len() - self.garbage
        {
            self.compact();
        }
        before
    }

    fn compact(&mut self) {
        let live = self.runs.len() - self.garbage;
        let mut runs =
            Vec::with_capacity(live + live.max(MIN_GARBAGE));
        for line in self.lines.iter_mut() {
            let saved =
                line.runs.start as usize..line.runs.end as usize;
            let start = runs.len() as u32;
            runs.extend_from_slice(&self.runs[saved]);
            line.runs = start..runs.len() as u32;
        }
        self.runs = runs;
        self.garbage = 0;
    }
}
