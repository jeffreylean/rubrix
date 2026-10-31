use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::iter;
use core::ops::Range;

use crate::model::Hunk;

/// Which end of a gap [`Expansion::reveal`] grows, named by what the
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
/// first and only `Down` on the last. Both still mean the same
/// thing there: `Down` on the first gap reveals from the top of the
/// file, and `Up` on the last from its end.
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
/// expansion along with the hunks it re-diffs, keeping every line it
/// showed, so a gap may then also show runs in its middle. A gap of
/// one line always shows whole.
///
/// [`FileDiff::edit_new`]: crate::FileDiff::edit_new
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expansion {
    gaps: BTreeMap<u32, Reveal>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Reveal {
    top: u32,
    bottom: u32,
    /// Shown runs between the ends, as sorted, disjoint offsets
    /// into the gap.
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

    /// The shown runs of the gap above hunk `hunk`, `size` lines
    /// long, as offsets into it: in order, disjoint and non-empty,
    /// though one may end where the next starts.
    pub(crate) fn shown(
        &self,
        hunk: u32,
        size: u32,
    ) -> impl Iterator<Item = Range<u32>> + '_ {
        let reveal = self.gaps.get(&hunk);
        let (top, bottom) = reveal.map_or((0, 0), |reveal| {
            (reveal.top.min(size), reveal.bottom.min(size))
        });
        let top = if size <= ALWAYS_SHOWN { size } else { top };
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

    /// Moves the gaps around hunks `replaced`, which `edit_new` put
    /// `new` in place of, so that every line shown before stays
    /// shown. `region` is the old lines from the end of the hunk
    /// before them to the start of the hunk after them, which both
    /// lists tile with their gaps; the old side never moves.
    pub(crate) fn remap(
        &mut self,
        first: u32,
        region: Range<u32>,
        replaced: &[Hunk],
        new: &[Hunk],
    ) {
        let mut shown: Vec<Range<u32>> = Vec::new();
        let mut add = |run: Range<u32>| match shown.last_mut() {
            Some(last) if last.end >= run.start => {
                last.end = last.end.max(run.end);
            }
            _ => shown.push(run),
        };
        for ((key, gap), hunk) in (first..)
            .zip(gaps(&region, replaced))
            .zip(old_ranges(replaced))
        {
            let size = gap.end - gap.start;
            for run in self.shown(key, size) {
                add(gap.start + run.start..gap.start + run.end);
            }
            if let Some(hunk) = hunk {
                add(hunk);
            }
        }
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
        for (key, gap) in (first..).zip(gaps(&region, new)) {
            let size = gap.end - gap.start;
            let mut reveal = Reveal::default();
            for run in &shown {
                let (start, end) =
                    (run.start.max(gap.start), run.end.min(gap.end));
                if start >= end {
                    continue;
                }
                let run = start - gap.start..end - gap.start;
                if run.start == 0 {
                    reveal.top = run.end;
                } else if run.end == size {
                    reveal.bottom = size - run.start;
                } else {
                    reveal.middle.push(run);
                }
            }
            if reveal != Reveal::default() {
                self.gaps.insert(key, reveal);
            }
        }
    }
}

/// The old lines of the gaps that `hunks` leave in `region`: one
/// before each hunk and one after the last.
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

/// Each hunk's old lines, then `None` for the gap after the last.
fn old_ranges(
    hunks: &[Hunk],
) -> impl Iterator<Item = Option<Range<u32>>> + '_ {
    let range = |hunk: &Hunk| Some(hunk.old_offset()..hunk.old_end());
    hunks.iter().map(range).chain(iter::once(None))
}
