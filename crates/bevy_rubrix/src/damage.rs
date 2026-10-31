//! Which rows must read again after an edit, and how line numbers
//! move through one.

use core::ops::Range;

use bevy::log::error;
use bevy_fynix::views::VirtualList;
use rubrix_highlight::LineEdit;

/// The most rows fynix's virtual list builds at once, its private
/// `MAX_SLOTS`
/// (`vendor/fynix/crates/bevy_fynix/src/views/virtual_list.rs`).
/// A test fails when fynix changes it.
pub(crate) const BUILT_MAX: u32 = 199;

/// The rows one change's line edits replace, in order, in the same
/// sequential coordinates. An edit's first line keeps its row: it
/// still starts the text that replaced it, as a typed letter's line
/// does, so its row keeps its height and width and reads again. A
/// join or an Enter replaces only the rows past it. A 1:1 edit, a
/// typed letter, replaces none.
pub(crate) fn row_edits(edits: &[LineEdit]) -> Vec<LineEdit> {
    edits
        .iter()
        .filter(|edit| edit.removed > 1 || edit.inserted > 1)
        .map(|edit| LineEdit {
            first: edit.first + 1,
            removed: edit.removed - 1,
            inserted: edit.inserted - 1,
        })
        .collect()
}

/// The list splices one change's line edits make, in order, as
/// `(at, removed, inserted)` for `VirtualList::splice`. A row edit
/// that replaces one row with one changes no count and no index, and
/// a splice would reset its row to the estimated height for a frame,
/// a jump under `Overflow::Wrap`, so it makes none.
fn row_splices(edits: &[LineEdit]) -> Vec<(usize, usize, usize)> {
    row_edits(edits)
        .into_iter()
        .filter(|edit| edit.removed != 1 || edit.inserted != 1)
        .map(|edit| {
            (
                edit.first as usize,
                edit.removed as usize,
                edit.inserted as usize,
            )
        })
        .collect()
}

/// Splices `list` for one change of a doc that has `rows` rows after
/// it. A list whose count is not the doc's row count before the
/// change shows some other text; it is left as it is, with an error.
pub(crate) fn splice_rows(
    list: &mut VirtualList,
    edits: &[LineEdit],
    rows: usize,
) {
    let before = edits.iter().fold(rows, |rows, edit| {
        rows + edit.removed as usize - edit.inserted as usize
    });
    if list.count() != before {
        error!(
            "rubrix: a list of {} rows shows a doc of {before} \
             rows; its rows are not spliced",
            list.count()
        );
        return;
    }
    for (at, removed, inserted) in row_splices(edits) {
        list.splice(at, removed, inserted);
    }
}

/// The end of the `edit_many` window for one list, in the line
/// numbers after the change: every row `fill` can build this frame
/// lies before it.
///
/// `fill` builds at most [`BUILT_MAX`] rows from at or before the
/// first visible row, which a splice moves by the rule
/// [`shift_line`] follows. A reveal of the caret this frame can move
/// the view: from above, the caret's row becomes the first row, from
/// below the last, so `max(first, caret) + BUILT_MAX` covers both.
pub(crate) fn reach(
    first_visible: usize,
    edits: &[LineEdit],
    caret_line: u32,
    lines: u32,
) -> u32 {
    let first = shift_line(
        first_visible as u32,
        &row_edits(edits),
        End::Start,
    );
    first.max(caret_line).saturating_add(BUILT_MAX).min(lines)
}

/// Which end of a line range a line number is, for [`shift_line`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum End {
    Start,
    /// An exclusive end.
    Stop,
}

/// Moves `line`, a line number before `edits`, to the same place
/// after them, by the rule `VirtualList::splice` moves row indices.
/// A start inside an edit goes to the edit's first line, and an end
/// inside one past its inserted lines, so a moved range covers what
/// replaced it.
pub(crate) fn shift_line(
    line: u32,
    edits: &[LineEdit],
    end: End,
) -> u32 {
    Shift::new(edits).line(line, end)
}

/// [`shift_line`] for ascending lines, in one walk over the edits.
pub(crate) struct Shift<'a> {
    edits: &'a [LineEdit],
    next: usize,
    /// Lines inserted less lines removed by the edits before `next`.
    delta: i64,
}

impl<'a> Shift<'a> {
    pub(crate) fn new(edits: &'a [LineEdit]) -> Shift<'a> {
        Shift {
            edits,
            next: 0,
            delta: 0,
        }
    }

    /// `line` is no less than the last line this moved.
    pub(crate) fn line(&mut self, line: u32, end: End) -> u32 {
        let at = i64::from(line);
        while let Some(edit) = self.edits.get(self.next) {
            let first = i64::from(edit.first) - self.delta;
            if at >= first + i64::from(edit.removed) {
                self.delta += i64::from(edit.inserted)
                    - i64::from(edit.removed);
                self.next += 1;
                continue;
            }
            match end {
                End::Start if at >= first => return edit.first,
                End::Stop if at > first => {
                    return edit.first + edit.inserted;
                }
                _ => break,
            }
        }
        (at + self.delta) as u32
    }

    /// The edit whose removed lines hold `line`, which is no less
    /// than the last line this moved.
    pub(crate) fn inside(&mut self, line: u32) -> Option<LineEdit> {
        self.line(line, End::Start);
        self.edits.get(self.next).copied().filter(|edit| {
            i64::from(line) >= i64::from(edit.first) - self.delta
        })
    }

    fn range(&mut self, range: &Range<u32>) -> Range<u32> {
        let start = self.line(range.start, End::Start);
        start..self.line(range.end, End::Stop)
    }
}

/// Which lines' spans may differ from what a row read, by generation.
///
/// Each note is an entry of lines, in the line numbers of the current
/// text: a later note moves every kept entry through its line edits.
/// Entries of this frame and the last are kept, so a row that reads
/// once a frame, in fynix's Mount, sees every entry noted after its
/// read, whichever schedule noted it. A row that read before the last
/// frame began skipped a Mount, and reads again.
#[derive(Default)]
pub(crate) struct Damage {
    seq: u64,
    /// `seq` when this frame began.
    began: u64,
    /// `seq` when the last frame began.
    since: u64,
    /// In `seq` order.
    log: Vec<Entry>,
}

struct Entry {
    seq: u64,
    /// Sorted, disjoint and apart.
    lines: Vec<Range<u32>>,
}

impl Damage {
    /// The generation of the newest entry.
    pub(crate) fn seq(&self) -> u64 {
        self.seq
    }

    /// Starts a frame: drops the entries noted before the last one.
    pub(crate) fn settle(&mut self) {
        self.since = self.began;
        self.began = self.seq;
        let old =
            self.log.partition_point(|entry| entry.seq <= self.since);
        self.log.drain(..old);
    }

    /// Records `lines`, in the numbers after `edits`, and moves the
    /// kept entries through `edits`, sequential and ascending. A
    /// change notes its line edits and the lines it lexed; each
    /// edit's inserted lines are dirty too.
    pub(crate) fn note(
        &mut self,
        edits: &[LineEdit],
        lines: impl IntoIterator<Item = Range<u32>>,
    ) {
        if !edits.is_empty() {
            for entry in &mut self.log {
                let mut shift = Shift::new(edits);
                entry.lines = merged(
                    entry
                        .lines
                        .iter()
                        .map(|lines| shift.range(lines)),
                );
            }
        }
        let mut dirty: Vec<Range<u32>> = edits
            .iter()
            .map(|edit| edit.first..edit.first + edit.inserted)
            .chain(lines)
            .collect();
        dirty.sort_unstable_by_key(|lines| lines.start);
        self.seq += 1;
        self.log.push(Entry {
            seq: self.seq,
            lines: merged(dirty.into_iter()),
        });
    }

    /// Whether a row that showed `line` after reading at generation
    /// `read` must read again.
    pub(crate) fn fires(&self, line: u32, read: u64) -> bool {
        read < self.since
            || self
                .log
                .iter()
                .rev()
                .take_while(|entry| entry.seq > read)
                .any(|entry| {
                    let i = entry
                        .lines
                        .partition_point(|lines| lines.end <= line);
                    entry
                        .lines
                        .get(i)
                        .is_some_and(|lines| lines.start <= line)
                })
    }
}

/// Sorted ranges with the empty ones dropped and the ones that
/// overlap or touch joined.
fn merged(
    ranges: impl Iterator<Item = Range<u32>>,
) -> Vec<Range<u32>> {
    let mut out: Vec<Range<u32>> = Vec::new();
    for range in ranges.filter(|range| !range.is_empty()) {
        match out.last_mut() {
            Some(last) if range.start <= last.end => {
                last.end = last.end.max(range.end);
            }
            _ => out.push(range),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn edit(
        first: u32,
        removed: u32,
        inserted: u32,
    ) -> LineEdit {
        LineEdit {
            first,
            removed,
            inserted,
        }
    }

    #[test]
    fn a_line_above_an_edit_stays_and_one_below_moves_by_its_net() {
        let enter = [edit(5, 1, 2)];
        assert_eq!(shift_line(4, &enter, End::Start), 4);
        assert_eq!(shift_line(6, &enter, End::Start), 7);
        let join = [edit(5, 2, 1)];
        assert_eq!(shift_line(9, &join, End::Start), 8);
    }

    #[test]
    fn a_line_inside_an_edit_goes_to_its_first_line_or_past_its_insert()
     {
        let paste = [edit(3, 2, 5)];
        assert_eq!(shift_line(3, &paste, End::Start), 3);
        assert_eq!(shift_line(4, &paste, End::Start), 3);
        assert_eq!(shift_line(4, &paste, End::Stop), 8);
        assert_eq!(shift_line(5, &paste, End::Stop), 8);
        assert_eq!(
            shift_line(3, &paste, End::Stop),
            3,
            "an end at the edit's first line excludes the edit"
        );
    }

    #[test]
    fn sequential_edits_shift_one_after_another() {
        let edits = [edit(2, 1, 2), edit(11, 1, 2)];
        let moved = [1, 5, 10, 20]
            .map(|line| shift_line(line, &edits, End::Start));
        assert_eq!(moved, [1, 6, 11, 22]);
        let mut shift = Shift::new(&edits);
        let walked =
            [1, 5, 10, 20].map(|line| shift.line(line, End::Start));
        assert_eq!(walked, moved, "one walk agrees with each call");
    }

    #[test]
    fn inside_names_the_edit_that_removed_a_line() {
        let edits = [edit(2, 2, 1), edit(6, 1, 1)];
        let mut shift = Shift::new(&edits);
        let firsts = [1, 2, 3, 4, 7, 8]
            .map(|line| shift.inside(line).map(|edit| edit.first));
        assert_eq!(
            firsts,
            [None, Some(2), Some(2), None, Some(6), None]
        );
    }

    /// fynix's `VirtualList::splice` for the first visible row,
    /// copied from `virtual_list.rs` as the oracle.
    fn spliced_first(
        first: usize,
        count: usize,
        at: usize,
        removed: usize,
        inserted: usize,
    ) -> usize {
        let end = at + removed;
        let moved = |index: usize| {
            if index >= end {
                index - removed + inserted
            } else {
                index.min(at)
            }
        };
        let count_after = count - removed + inserted;
        if count > 0 && end <= first {
            moved(first)
        } else if at <= first {
            at.min(count_after.saturating_sub(1))
        } else {
            first
        }
    }

    #[test]
    fn shift_line_moves_the_first_row_as_the_row_splices_do() {
        let mut seed: u64 = 7;
        let mut next = |below: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % below
        };
        for round in 0..2_000 {
            let count = 2 + next(300) as usize;
            let first = next(count as u64) as usize;
            let (mut rows, mut spliced) = (count, first);
            let mut edits = Vec::new();
            let mut floor = 0;
            while floor < rows && edits.len() < 4 {
                let at = floor + next((rows - floor) as u64) as usize;
                let removed =
                    1 + next((rows - at).min(5) as u64) as usize;
                let inserted = 1 + next(5) as usize;
                if removed != inserted || removed > 2 {
                    spliced = spliced_first(
                        spliced,
                        rows,
                        at + 1,
                        removed - 1,
                        inserted - 1,
                    );
                }
                rows = rows - removed + inserted;
                edits.push(edit(
                    at as u32,
                    removed as u32,
                    inserted as u32,
                ));
                floor = at + inserted;
                if next(2) == 0 {
                    break;
                }
            }
            assert_eq!(
                (shift_line(
                    first as u32,
                    &row_edits(&edits),
                    End::Start
                ) as usize)
                    .min(rows - 1),
                spliced.min(rows - 1),
                "round {round}: first {first} of {count}, edits \
                 {edits:?}"
            );
        }
    }

    #[test]
    fn reach_covers_the_moved_first_row_or_the_caret() {
        let enter = [edit(5, 1, 2)];
        assert_eq!(reach(10, &enter, 0, 10_000), 11 + BUILT_MAX);
        assert_eq!(reach(10, &enter, 500, 10_000), 500 + BUILT_MAX);
        assert_eq!(reach(10, &[], 0, 50), 50);
    }

    #[test]
    fn an_edit_keeps_its_first_row_and_a_typed_letter_splices_none() {
        let edits = [
            edit(2, 1, 1),
            edit(7, 1, 2),
            edit(9, 3, 1),
            edit(20, 2, 2),
        ];
        assert_eq!(row_splices(&edits), [(8, 0, 1), (10, 2, 0)]);
    }

    #[test]
    fn a_change_dirties_its_hull_and_inserted_lines_only() {
        let mut damage = Damage::default();
        damage.settle();
        damage.note(&[edit(5, 1, 2)], Some(5..6));
        assert!(damage.fires(5, 0) && damage.fires(6, 0));
        assert!(!damage.fires(4, 0) && !damage.fires(7, 0));
        assert!(!damage.fires(5, damage.seq()), "already read");
    }

    #[test]
    fn a_later_change_moves_the_lines_an_earlier_one_dirtied() {
        let mut damage = Damage::default();
        damage.settle();
        damage.note(&[edit(8, 1, 1)], Some(8..9));
        let typed = damage.seq();
        damage.note(&[edit(2, 1, 3)], Some(2..5));
        assert!(damage.fires(10, typed - 1), "line 8 is now line 10");
        assert!(!damage.fires(8, typed - 1));
        assert!(
            !damage.fires(10, typed),
            "only the first change hit it"
        );
    }

    #[test]
    fn a_range_inside_a_removed_edit_moves_onto_its_replacement() {
        let mut damage = Damage::default();
        damage.settle();
        damage.note(&[edit(4, 1, 1)], Some(4..5));
        damage.note(&[edit(3, 3, 1)], Some(3..4));
        assert!(damage.fires(3, 0), "line 4 joined into line 3");
        assert!(
            !damage.fires(4, 0),
            "old line 6 is line 4, untouched"
        );
    }

    #[test]
    fn an_entry_noted_after_mount_outlives_the_next_settle() {
        let mut damage = Damage::default();
        damage.settle();
        let read = damage.seq();
        damage.note(&[], Some(7..8));
        damage.settle();
        assert!(damage.fires(7, read), "noted after the row read");
        assert!(!damage.fires(6, read));
        damage.settle();
        assert!(!damage.fires(7, damage.seq()));
        assert!(
            damage.fires(0, read),
            "a row that skipped reads again"
        );
    }
}
