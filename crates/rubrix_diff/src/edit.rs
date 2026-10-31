use alloc::vec::Vec;
use core::ops::{Index, Range};

use similar::DiffOp;

use crate::expand::Expansion;
use crate::model::{Block, FileDiff, Hunk, Lines};
use crate::realign::realign_block;
use crate::{Algorithm, block};

/// What one new-side edit did to a full diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rediff {
    /// The hunks the edit replaced, by index before the edit.
    pub hunks: Range<u32>,
    /// How many hunks took their place, from `hunks.start`.
    pub new_hunks: u32,
    /// The new-side lines the edit replaced, by index before the
    /// edit.
    pub lines: Range<u32>,
    /// How many lines took their place, from `lines.start`.
    pub inserted: u32,
}

impl FileDiff {
    /// Replaces new lines `lines` with the lines of `text`
    /// (terminators included; `text` may be empty, and its last line
    /// may lack `\n` only when `lines.end` is the side's line count),
    /// re-diffs only the hunks within reach, realigns the hunks it
    /// re-diffed (as [`realign`](crate::realign()) would), and remaps
    /// `expansion`. A line that showed in a gap still shows, a line
    /// that leaves a hunk for a gap shows unless the hunk took it
    /// from a hidden gap, and the last line `text` wrote, where a
    /// caret that typed it lands, always shows. An undo that writes
    /// back lines a gap hid before the edit hides them again.
    ///
    /// The re-diff covers the edit widened by `2 * context + 1` lines
    /// each way and every hunk that touches it, and grows while a
    /// hunk it gives comes within `2 * context` unchanged lines of a
    /// hunk outside it, so hunks merge and split as
    /// [`diff_texts`](crate::diff_texts) would group them.
    ///
    /// # Panics
    ///
    /// Panics on a partial diff, when `lines` passes the side's end,
    /// when `text` breaks the newline rule above or would follow a
    /// last line that lacks its `\n`, or when the side would reach
    /// 4 GiB.
    pub fn edit_new(
        &mut self,
        lines: Range<u32>,
        text: &str,
        context: u32,
        expansion: &mut Expansion,
    ) -> Rediff {
        assert!(!self.is_partial(), "edit_new needs a full diff");
        let len_before = self.new.len();
        assert!(
            lines.start <= lines.end && lines.end <= len_before,
            "lines {lines:?} pass the new side's {len_before} lines"
        );
        assert!(
            text.is_empty()
                || text.ends_with('\n')
                || lines.end == len_before,
            "only the side's last line may lack its newline"
        );
        assert!(
            text.is_empty()
                || lines.start < len_before
                || !self.new.missing_newline(),
            "text cannot follow a last line without a newline"
        );
        let inserted = self.new.splice(lines.clone(), text);
        let delta = i64::from(inserted) - lines.len() as i64;
        let context =
            context.min(self.old_lines().len().max(self.new.len()));
        let (touched, groups) =
            self.rediff_window(&lines, len_before, delta, context);
        let (region, region_new) = {
            let all = &self.hunks;
            let above = touched.start.checked_sub(1).map(|h| &all[h]);
            let (start, new_start) =
                above.map_or((0, 0), |h| (h.old_end(), h.new_end()));
            let end = all
                .get(touched.end)
                .map_or(self.old_lines().len(), Hunk::old_offset);
            (start..end, new_start)
        };
        let new: Vec<Hunk> = groups
            .iter()
            .map(|group| {
                Hunk::from_blocks(
                    group.iter().filter_map(block).collect(),
                )
            })
            .collect();
        let count = new.len();
        let replaced: Vec<Hunk> =
            self.hunks.splice(touched.clone(), new).collect();
        let first = touched.start;
        if delta != 0 {
            for hunk in &mut self.hunks[first + count..] {
                hunk.shift_new(delta);
            }
        }
        self.split_changes_in(first..first + count, realign_block)
            .expect("realign's parts cover their block");
        expansion.remap(
            first as u32,
            region,
            region_new,
            &replaced,
            &self.hunks[first..first + count],
            (lines.start + inserted).saturating_sub(1)
                ..lines.start + inserted,
        );
        debug_assert_eq!(self.check(), Ok(()));
        Rediff {
            hunks: first as u32..touched.end as u32,
            new_hunks: count as u32,
            lines,
            inserted,
        }
    }

    fn rediff_window(
        &self,
        lines: &Range<u32>,
        len_before: u32,
        delta: i64,
        context: u32,
    ) -> (Range<usize>, Vec<Vec<DiffOp>>) {
        let hunks = &self.hunks;
        let reach = context.saturating_mul(2).saturating_add(1);
        let mut window = lines.start.saturating_sub(reach)
            ..lines.end.saturating_add(reach).min(len_before);
        loop {
            let touched = touching(hunks, &window);
            if !touched.is_empty() {
                let first = &hunks[touched.start];
                let last = &hunks[touched.end - 1];
                window.start = window.start.min(first.new_offset());
                window.end = window.end.max(last.new_end());
            }
            let old = to_old_above(hunks, touched.start, window.start)
                ..to_old_above(hunks, touched.end, window.end);
            let new =
                window.start..(i64::from(window.end) + delta) as u32;
            let ops = similar::capture_diff(
                Algorithm::default().similar(),
                &Raw(self.old_lines()),
                old.start as usize..old.end as usize,
                &Raw(&self.new),
                new.start as usize..new.end as usize,
            );
            let groups =
                similar::group_diff_ops(ops, context as usize);
            let mut changes = groups
                .iter()
                .flatten()
                .filter(|op| !matches!(op, DiffOp::Equal { .. }));
            let Some(head) = changes.next() else {
                return (touched, groups);
            };
            let tail = changes.next_back().unwrap_or(head);
            let first = head.old_range().start as u32;
            let last = tail.old_range().end as u32;
            let near = |run: u32| run <= context.saturating_mul(2);
            let above =
                touched.start.checked_sub(1).map(|h| &hunks[h]);
            let below = hunks.get(touched.end);
            let grow_up = window.start > 0
                && (first - old.start < context
                    || above.is_some_and(|h| {
                        near(first - change_end(h))
                    }));
            let grow_down = window.end < len_before
                && (old.end - last < context
                    || below.is_some_and(|h| {
                        near(change_start(h) - last)
                    }));
            if !grow_up && !grow_down {
                return (touched, groups);
            }
            if grow_up {
                window.start = window.start.saturating_sub(reach);
            }
            if grow_down {
                window.end =
                    window.end.saturating_add(reach).min(len_before);
            }
        }
    }
}

fn touching(hunks: &[Hunk], window: &Range<u32>) -> Range<usize> {
    let start =
        hunks.partition_point(|hunk| hunk.new_end() < window.start);
    let end =
        hunks.partition_point(|hunk| hunk.new_offset() <= window.end);
    start..end
}

fn to_old_above(hunks: &[Hunk], hunk: usize, at: u32) -> u32 {
    let offset = hunk.checked_sub(1).map_or(0, |h| {
        let above = &hunks[h];
        i64::from(above.new_end()) - i64::from(above.old_end())
    });
    (i64::from(at) - offset) as u32
}

fn change_start(hunk: &Hunk) -> u32 {
    let mut changes = hunk.blocks().iter().filter(|b| is_change(b));
    changes.next().map_or(0, |block| block.old_range().start)
}

fn change_end(hunk: &Hunk) -> u32 {
    let mut changes =
        hunk.blocks().iter().rev().filter(|b| is_change(b));
    changes.next().map_or(0, |block| block.old_range().end)
}

fn is_change(block: &Block) -> bool {
    matches!(block, Block::Change { .. })
}

struct Raw<'a>(&'a Lines);

impl Index<usize> for Raw<'_> {
    type Output = str;

    fn index(&self, index: usize) -> &str {
        self.0.raw(index as u32)
    }
}
