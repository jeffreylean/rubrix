use alloc::collections::BTreeMap;

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
/// diff, are ignored. Keys are hunk indexes, so after a re-diff the
/// counts land on whichever gaps now have those indexes; start a new
/// expansion for a new diff. A gap of one line always shows whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expansion {
    gaps: BTreeMap<u32, Reveal>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Reveal {
    top: u32,
    bottom: u32,
}

const ALWAYS_SHOWN: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shown {
    Whole,
    Ends { top: u32, bottom: u32 },
}

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

    pub(crate) fn shown(&self, hunk: u32, size: u32) -> Shown {
        let reveal =
            self.gaps.get(&hunk).copied().unwrap_or_default();
        let (top, bottom) =
            (reveal.top.min(size), reveal.bottom.min(size));
        if size <= ALWAYS_SHOWN || top.saturating_add(bottom) >= size
        {
            Shown::Whole
        } else {
            Shown::Ends { top, bottom }
        }
    }
}
