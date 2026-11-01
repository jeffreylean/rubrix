use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::iter;
use core::ops::Range;

use crate::model::Hunk;
use crate::rows::Rows;

/// Which end of a gap [`Expansion::reveal`] grows, or of a gap row's
/// hidden lines [`Expansion::reveal_at`] grows, named by what the
/// revealed lines touch.
///
/// | rubrix | reveals |
/// |---|---|
/// | `Up` | right above the hunk below |
/// | `Down` | right below the hunk above |
/// | `All` | the whole gap |
///
/// The gap before the first hunk has no hunk above it, and the gap
/// after the last has none below, so a view offers only `Up` on the
/// first and only `Down` on the last, until lines revealed at the
/// file's top or end sit next to the gap row. Both still mean the
/// same thing there: `Down` on the first gap reveals from the top of
/// the file, and `Up` on the last from its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Up,
    Down,
    All,
}

/// Lines revealed per gap, keyed by the hunk below the gap.
///
/// Key `k` names the gap above hunk `k`, and the hunk count names
/// the gap after the last hunk. Counts are stored as clicked and
/// clamped to the gap only when rows read them, so rows never fail
/// on an expansion: keys past its gaps, and every key on a partial
/// diff, are ignored. Keys are hunk indexes, so after a new diff the
/// counts land on whichever gaps now have those indexes; start a new
/// expansion for a new diff. [`FileDiff::edit_new`] moves the
/// expansion along with the hunks it re-diffs. A line that leaves a
/// hunk for a gap shows unless the hunk took it from a hidden gap,
/// and the last line the edit wrote always shows, so a gap may then
/// also show runs in its middle. A gap of one line always shows
/// whole.
///
/// [`FileDiff::edit_new`]: crate::FileDiff::edit_new
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expansion {
    gaps: BTreeMap<u32, Reveal>,
    taken: Vec<Range<u32>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Reveal {
    top: u32,
    bottom: u32,
    middle: Vec<Range<u32>>,
}

const ALWAYS_SHOWN: u32 = 1;

impl Expansion {
    /// Lines one expand click reveals.
    pub const STEP: u32 = 100;

    /// Every gap collapsed.
    pub fn new() -> Expansion {
        Expansion::default()
    }

    /// Reveals `lines` more lines of the gap above hunk `hunk` from
    /// the `side` end, or the whole gap for [`Side::All`], which
    /// ignores `lines`. Pass the hunk count for the gap after the
    /// last hunk; a gap row carries the right key as its `hunk`.
    ///
    /// Counts saturate, so revealing past a gap's size is harmless
    /// and `All` is idempotent.
    pub fn reveal(&mut self, hunk: u32, side: Side, lines: u32) {
        let reveal = self.gaps.entry(hunk).or_default();
        match side {
            Side::Up => {
                reveal.bottom = reveal.bottom.saturating_add(lines)
            }
            Side::Down => {
                reveal.top = reveal.top.saturating_add(lines)
            }
            Side::All => reveal.bottom = u32::MAX,
        }
    }

    /// Reveals `lines` more of the lines that gap row `row` of `rows`
    /// hides: with [`Side::Down`] the ones right below what shows
    /// above the row, with [`Side::Up`] the ones right above what
    /// shows below it, and with [`Side::All`], which ignores `lines`,
    /// all of them. On a gap that shows no run in its middle this is
    /// [`reveal`](Expansion::reveal) with the row's `hunk`.
    ///
    /// `rows` must come from this expansion. Does nothing when `row`
    /// is not a gap row with a button, as on a partial diff.
    pub fn reveal_at(
        &mut self,
        rows: &Rows,
        row: u32,
        side: Side,
        lines: u32,
    ) {
        let Some((hunk, size, hidden)) = rows.hidden_run(row) else {
            return;
        };
        let run = match side {
            Side::Down => {
                let end = hidden.start.saturating_add(lines);
                hidden.start..end.min(hidden.end)
            }
            Side::Up => {
                let start = hidden.end.saturating_sub(lines);
                start.max(hidden.start)..hidden.end
            }
            Side::All => hidden,
        };
        if run.is_empty() {
            return;
        }
        let mut runs: Vec<Range<u32>> =
            self.revealed(hunk, size).collect();
        runs.push(run);
        self.gaps.insert(hunk, Reveal::from_runs(runs, size));
    }

    /// The runs of gap `hunk`, of `size` lines, that rows show: the
    /// revealed runs, or the whole gap when it is too small to hide.
    pub(crate) fn shown(
        &self,
        hunk: u32,
        size: u32,
    ) -> impl Iterator<Item = Range<u32>> + '_ {
        let whole = size <= ALWAYS_SHOWN;
        let revealed =
            self.revealed(hunk, if whole { 0 } else { size });
        iter::once(0..size).filter(move |_| whole).chain(revealed)
    }

    fn revealed(
        &self,
        hunk: u32,
        size: u32,
    ) -> impl Iterator<Item = Range<u32>> + '_ {
        let reveal = self.gaps.get(&hunk);
        let (top, bottom) = reveal.map_or((0, 0), |reveal| {
            (reveal.top.min(size), reveal.bottom.min(size))
        });
        let floor = (size - bottom).max(top);
        let middle = reveal
            .into_iter()
            .flat_map(|reveal| &reveal.middle)
            .map(move |run| run.start.max(top)..run.end.min(floor));
        iter::once(0..top)
            .chain(middle)
            .chain(iter::once(floor..size))
            .filter(|run| !run.is_empty())
    }

    pub(crate) fn remap(
        &mut self,
        first: u32,
        region: Range<u32>,
        region_new: u32,
        replaced: &[Hunk],
        new: &[Hunk],
        written: Range<u32>,
    ) {
        let mut shown: Vec<Range<u32>> = Vec::new();
        for ((key, gap), hunk) in (first..)
            .zip(gaps(&region, replaced))
            .zip(old_ranges(replaced))
        {
            let size = gap.end - gap.start;
            for run in self.revealed(key, size) {
                push_run(
                    &mut shown,
                    gap.start + run.start..gap.start + run.end,
                );
            }
            for run in
                hunk.into_iter().flat_map(|h| minus(h, &self.taken))
            {
                push_run(&mut shown, run);
            }
        }
        let taken: Vec<Range<u32>> = old_ranges(new)
            .flatten()
            .flat_map(|hunk| minus(hunk, &shown))
            .collect();
        let low =
            self.taken.partition_point(|r| r.end <= region.start);
        let high =
            self.taken.partition_point(|r| r.start < region.end);
        self.taken.splice(low..high, taken);
        let count = replaced.len() as u32;
        let stale: Vec<u32> = self
            .gaps
            .range(first..=first + count)
            .map(|(&key, _)| key)
            .collect();
        for key in stale {
            self.gaps.remove(&key);
        }
        let moved = new.len() as i64 - i64::from(count);
        if moved != 0 {
            let later = self.gaps.split_off(&(first + count + 1));
            self.gaps.extend(later.into_iter().map(
                |(key, reveal)| {
                    ((i64::from(key) + moved) as u32, reveal)
                },
            ));
        }
        let new_starts = iter::once(region_new)
            .chain(new.iter().map(Hunk::new_end));
        for ((key, gap), new_start) in
            (first..).zip(gaps(&region, new)).zip(new_starts)
        {
            let size = gap.end - gap.start;
            let mut runs: Vec<Range<u32>> = shown
                .iter()
                .map(|run| clip(run, gap.start, size))
                .collect();
            runs.push(clip(&written, new_start, size));
            let reveal = Reveal::from_runs(runs, size);
            if reveal != Reveal::default() {
                self.gaps.insert(key, reveal);
            }
        }
    }
}

impl Reveal {
    /// The reveal that shows `runs`, relative to a gap of `size`
    /// lines.
    fn from_runs(mut runs: Vec<Range<u32>>, size: u32) -> Reveal {
        runs.sort_unstable_by_key(|run| run.start);
        let mut middle = Vec::new();
        for run in runs {
            push_run(&mut middle, run);
        }
        let top = if middle.first().is_some_and(|run| run.start == 0)
        {
            middle.remove(0).end
        } else {
            0
        };
        let bottom = middle
            .pop_if(|run| run.end == size)
            .map_or(0, |run| size - run.start);
        Reveal {
            top,
            bottom,
            middle,
        }
    }
}

/// Appends `run` to sorted disjoint `runs`, merging it into the last
/// run when the two touch. `run` must not start before the last run.
fn push_run(runs: &mut Vec<Range<u32>>, run: Range<u32>) {
    if run.is_empty() {
        return;
    }
    match runs.last_mut() {
        Some(last) if last.end >= run.start => {
            last.end = last.end.max(run.end);
        }
        _ => runs.push(run),
    }
}

/// The parts of `range` outside the sorted disjoint `runs`.
fn minus(
    range: Range<u32>,
    runs: &[Range<u32>],
) -> impl Iterator<Item = Range<u32>> + '_ {
    let from = runs.partition_point(|run| run.end <= range.start);
    let end = range.end;
    let mut at = range.start;
    runs[from..]
        .iter()
        .take_while(move |run| run.start < end)
        .cloned()
        .chain(iter::once(end..end))
        .map(move |run| {
            let part = at..run.start.max(at);
            at = at.max(run.end);
            part
        })
        .filter(|part| !part.is_empty())
}

/// `run`'s part within `from..from + size`, relative to `from`.
fn clip(run: &Range<u32>, from: u32, size: u32) -> Range<u32> {
    let to = from + size;
    run.start.clamp(from, to) - from..run.end.clamp(from, to) - from
}

fn gaps<'a>(
    region: &Range<u32>,
    hunks: &'a [Hunk],
) -> impl Iterator<Item = Range<u32>> + 'a {
    let mut start = region.start;
    let end = region.end;
    old_ranges(hunks).map(move |hunk| {
        let hunk = hunk.unwrap_or(end..end);
        let gap = start..hunk.start;
        start = hunk.end;
        gap
    })
}

fn old_ranges(
    hunks: &[Hunk],
) -> impl Iterator<Item = Option<Range<u32>>> + '_ {
    let range = |hunk: &Hunk| Some(hunk.old_offset()..hunk.old_end());
    hunks.iter().map(range).chain(iter::once(None))
}
