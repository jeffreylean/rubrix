use alloc::vec::Vec;
use core::fmt;
use core::num::NonZeroU32;
use core::ops::{Bound, Range, RangeBounds};

use crate::expand::{Expansion, Shown};
use crate::model::{Block, FileDiff, Hunk};
use crate::words::{WordRanges, word_changes};

/// How change blocks become rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Layout {
    /// One column: a change block's deletions, then its additions,
    /// `old_len + new_len` rows.
    #[default]
    Stacked,
    /// Two columns: old line i beside new line i of each change
    /// block, `max(old_len, new_len)` rows.
    Split,
}

/// A 1-based file line number, for the gutter.
///
/// Never an index: a partial diff's line `n` is rarely at index
/// `n - 1`. Index a side's [`Lines`](crate::Lines) with
/// [`LineRef::index`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct LineNo(NonZeroU32);

impl LineNo {
    pub fn get(self) -> u32 {
        self.0.get()
    }

    fn after(offset: u32) -> LineNo {
        LineNo(NonZeroU32::MIN.saturating_add(offset))
    }
}

impl fmt::Display for LineNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One line on one side of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LineRef {
    /// Into that side's [`Lines`](crate::Lines), and line `index` of
    /// the side's highlights.
    pub index: u32,
    /// The line's number in its file.
    pub number: LineNo,
}

/// One row of a diff view.
///
/// `Context`, `Expanded`, `Deletion`, `Addition` and `Gap` occur in
/// both layouts; `Paired` only in split. A change block's old line i
/// and new line i are a pair while `i < min(old_len, new_len)`:
/// split shows them as one `Paired` row, and stacked shows them as a
/// `Deletion` and, after the block's other deletions, an `Addition`.
///
/// Stacked rows treat a run of adjacent change blocks as one: all its
/// deletions, then all its additions, as git prints them. So stacked
/// rows do not depend on how [`FileDiff::split_changes`] split a run,
/// while split rows pair block by block.
///
/// [`FileDiff::split_changes`]: crate::FileDiff::split_changes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    /// An unchanged line inside a hunk.
    Context { old: LineRef, new: LineRef },
    /// An unchanged line revealed from a gap.
    Expanded { old: LineRef, new: LineRef },
    /// A removed line.
    Deletion { old: LineRef },
    /// An added line.
    Addition { new: LineRef },
    /// Split only: a removed line beside the added line it pairs
    /// with.
    Paired { old: LineRef, new: LineRef },
    /// `hidden` unchanged lines not shown, above hunk `hunk`, or
    /// after the last hunk when `hunk` is the hunk count. Pass
    /// `hunk` to [`Expansion::reveal`]. `up` and `down` tell which
    /// of [`Side::Up`](crate::Side::Up) and
    /// [`Side::Down`](crate::Side::Down) a view should offer: `up`
    /// needs a hunk below, `down` a hunk above, and both are false
    /// on a partial diff, whose gaps cannot expand.
    Gap {
        hunk: u32,
        hidden: u32,
        up: bool,
        down: bool,
    },
}

impl Row {
    /// The old line this row shows, if any.
    pub fn old_line(&self) -> Option<LineRef> {
        match *self {
            Row::Context { old, .. }
            | Row::Expanded { old, .. }
            | Row::Deletion { old }
            | Row::Paired { old, .. } => Some(old),
            Row::Addition { .. } | Row::Gap { .. } => None,
        }
    }

    /// The new line this row shows, if any.
    pub fn new_line(&self) -> Option<LineRef> {
        match *self {
            Row::Context { new, .. }
            | Row::Expanded { new, .. }
            | Row::Addition { new }
            | Row::Paired { new, .. } => Some(new),
            Row::Deletion { .. } | Row::Gap { .. } => None,
        }
    }

    /// Where this row sits in the source, for [`Rows::find`] in
    /// another layout: its old line if it has one, else its new
    /// line, else its gap. A split `Paired` row anchors on its old
    /// line.
    pub fn anchor(&self) -> Anchor {
        match *self {
            Row::Gap { hunk, .. } => Anchor::Gap(hunk),
            Row::Addition { new } => Anchor::New(new.index),
            Row::Context { old, .. }
            | Row::Expanded { old, .. }
            | Row::Deletion { old }
            | Row::Paired { old, .. } => Anchor::Old(old.index),
        }
    }

    /// The changed byte ranges of the line pair this row shows, as
    /// [`word_changes`] gives them for its old and new line: for a
    /// split `Paired` row, and for a stacked `Deletion` or
    /// `Addition` whose line pairs with one on the other side of its
    /// change block. A view marks the side it shows.
    ///
    /// `None` for every other row, unpaired changes included, and
    /// for a pair with a line over 1,000 bytes. `diff` must be the
    /// diff the row came from. Run [`realign`](crate::realign()) on
    /// it before deriving rows, so that similar lines pair.
    pub fn words(&self, diff: &FileDiff) -> Option<WordRanges> {
        let (old, new) = match *self {
            Row::Paired { old, new } => (old.index, new.index),
            Row::Deletion { old } => {
                partner(diff, old.index, Lane::Old)?
            }
            Row::Addition { new } => {
                partner(diff, new.index, Lane::New)?
            }
            Row::Context { .. }
            | Row::Expanded { .. }
            | Row::Gap { .. } => {
                return None;
            }
        };
        word_changes(
            diff.old_lines().line(old),
            diff.new_lines().line(new),
        )
    }
}

fn partner(
    diff: &FileDiff,
    index: u32,
    lane: Lane,
) -> Option<(u32, u32)> {
    let side = |block: &Block| match lane {
        Lane::Old => block.old_range(),
        Lane::New => block.new_range(),
    };
    let hunks = diff.hunks();
    let first = |hunk: &Hunk| {
        hunk.blocks().first().map_or(0, |b| side(b).start)
    };
    let hunk = hunks
        .partition_point(|h| first(h) <= index)
        .checked_sub(1)?;
    let blocks = hunks[hunk].blocks();
    let block = blocks
        .partition_point(|b| side(b).start <= index)
        .checked_sub(1)?;
    let Block::Change {
        old_start,
        old_len,
        new_start,
        new_len,
    } = blocks[block]
    else {
        return None;
    };
    let at = index - side(&blocks[block]).start;
    (at < old_len.min(new_len))
        .then_some((old_start + at, new_start + at))
}

/// A source position that survives a layout switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Anchor {
    /// An old line, by index.
    Old(u32),
    /// A new line, by index.
    New(u32),
    /// The gap above hunk `n`, or after the last when `n` is the
    /// hunk count.
    Gap(u32),
}

/// A diff's rows for one layout and expansion, ready for windowed
/// reads.
///
/// Built by [`rows()`]. Holds no borrow of the diff it came from;
/// read line text with each row's [`LineRef::index`] from that
/// diff's [`Lines`](crate::Lines). Rebuild it whenever the diff, the
/// layout or the expansion changes: rows from an older diff name
/// lines that the new one may not have, and reading them panics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rows {
    layout: Layout,
    segments: Vec<Segment>,
    len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Segment {
    row: u32,
    hunk: u32,
    old: Start,
    new: Start,
    piece: Piece,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Start {
    index: u32,
    offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece {
    Context { len: u32 },
    Expanded { len: u32 },
    Change { old_len: u32, new_len: u32 },
    Gap { hidden: u32, up: bool, down: bool },
}

/// Derives the rows of `diff` for `layout`, showing what
/// `expansion` reveals. O(hunks + blocks), never O(rows).
///
/// A diff with no hunks has no rows, as identical texts do. A full
/// diff's gaps, the trailing one included, follow `expansion`; a
/// partial diff's gaps before each hunk show as `Gap` rows that
/// cannot expand, and its trailing gap, of unknown size, not at all.
pub fn rows(
    diff: &FileDiff,
    layout: Layout,
    expansion: &Expansion,
) -> Rows {
    let hunks = diff.hunks();
    let mut builder = Builder {
        layout,
        expansion,
        hunk_count: hunks.len() as u32,
        segments: Vec::new(),
        next_row: 0,
    };
    let (mut old_end, mut new_end) = (0, 0);
    for (h, hunk) in (0..).zip(hunks) {
        if diff.is_partial() {
            builder.partial_gap(h, hunk, hunk.new_offset() - new_end);
        } else {
            builder.gap(
                h,
                hunk.old_offset() - old_end,
                old_end,
                new_end,
            );
        }
        builder.blocks(h, hunk);
        old_end = hunk.old_offset() + hunk.old_len();
        new_end = hunk.new_offset() + hunk.new_len();
    }
    if !hunks.is_empty() && !diff.is_partial() {
        let size = diff.old_lines().len() - old_end;
        builder.gap(builder.hunk_count, size, old_end, new_end);
    }
    Rows {
        layout,
        segments: builder.segments,
        len: builder.next_row,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    Old,
    New,
}

impl Piece {
    fn is_block(self) -> bool {
        matches!(self, Piece::Context { .. } | Piece::Change { .. })
    }

    fn rows(self, layout: Layout) -> u32 {
        match (self, layout) {
            (Piece::Context { len } | Piece::Expanded { len }, _) => {
                len
            }
            (Piece::Gap { .. }, _) => 1,
            (Piece::Change { old_len, new_len }, Layout::Stacked) => {
                old_len + new_len
            }
            (Piece::Change { old_len, new_len }, Layout::Split) => {
                old_len.max(new_len)
            }
        }
    }
}

struct Builder<'a> {
    layout: Layout,
    expansion: &'a Expansion,
    hunk_count: u32,
    segments: Vec<Segment>,
    next_row: u32,
}

impl Builder<'_> {
    fn push(
        &mut self,
        hunk: u32,
        old: Start,
        new: Start,
        piece: Piece,
    ) {
        self.segments.push(Segment {
            row: self.next_row,
            hunk,
            old,
            new,
            piece,
        });
        self.next_row += piece.rows(self.layout);
    }

    fn push_file(
        &mut self,
        hunk: u32,
        old: u32,
        new: u32,
        piece: Piece,
    ) {
        let file = |at| Start {
            index: at,
            offset: at,
        };
        self.push(hunk, file(old), file(new), piece);
    }

    fn gap(&mut self, hunk: u32, size: u32, old: u32, new: u32) {
        if size == 0 {
            return;
        }
        let (top, bottom) = match self.expansion.shown(hunk, size) {
            Shown::Whole => {
                let whole = Piece::Expanded { len: size };
                return self.push_file(hunk, old, new, whole);
            }
            Shown::Ends { top, bottom } => (top, bottom),
        };
        if top > 0 {
            let expanded = Piece::Expanded { len: top };
            self.push_file(hunk, old, new, expanded);
        }
        let gap = Piece::Gap {
            hidden: size - top - bottom,
            up: hunk < self.hunk_count,
            down: hunk > 0,
        };
        self.push_file(hunk, old + top, new + top, gap);
        if bottom > 0 {
            let at = size - bottom;
            let expanded = Piece::Expanded { len: bottom };
            self.push_file(hunk, old + at, new + at, expanded);
        }
    }

    fn partial_gap(&mut self, hunk: u32, next: &Hunk, size: u32) {
        if size == 0 {
            return;
        }
        let gap = Piece::Gap {
            hidden: size,
            up: false,
            down: false,
        };
        let old = Start {
            index: next.old_index(),
            offset: next.old_offset(),
        };
        let new = Start {
            index: next.new_index(),
            offset: next.new_offset(),
        };
        self.push(hunk, old, new, gap);
    }

    fn blocks(&mut self, h: u32, hunk: &Hunk) {
        let (old_index, new_index) =
            (hunk.old_index(), hunk.new_index());
        let stacked = self.layout == Layout::Stacked;
        let joins = |a: &Block, b: &Block| {
            stacked
                && matches!(
                    (a, b),
                    (Block::Change { .. }, Block::Change { .. })
                )
        };
        for run in hunk.blocks().chunk_by(joins) {
            let (first, last) = (run[0], run[run.len() - 1]);
            let old = first.old_range().start..last.old_range().end;
            let new = first.new_range().start..last.new_range().end;
            let piece = match first {
                Block::Context { len, .. } => Piece::Context { len },
                Block::Change { .. } => Piece::Change {
                    old_len: old.end - old.start,
                    new_len: new.end - new.start,
                },
            };
            let old = Start {
                index: old.start,
                offset: hunk.old_offset() + (old.start - old_index),
            };
            let new = Start {
                index: new.start,
                offset: hunk.new_offset() + (new.start - new_index),
            };
            self.push(h, old, new, piece);
        }
    }
}

impl Rows {
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Every row the view lays out, gap rows included.
    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Row `row`, or `None` past the end. O(log segments).
    pub fn get(&self, row: u32) -> Option<Row> {
        self.range(row..row.saturating_add(1)).next()
    }

    /// Rows `rows`, clamped to `0..len`: `range(..)` for all of
    /// them, `range(a..b)` for a window. O(log segments) to seek,
    /// then O(1) a row.
    pub fn range(
        &self,
        rows: impl RangeBounds<u32>,
    ) -> impl ExactSizeIterator<Item = Row> + '_ {
        let end = match rows.end_bound() {
            Bound::Included(&end) => end.saturating_add(1),
            Bound::Excluded(&end) => end,
            Bound::Unbounded => self.len,
        }
        .min(self.len);
        let start = match rows.start_bound() {
            Bound::Included(&start) => start,
            Bound::Excluded(&start) => start.saturating_add(1),
            Bound::Unbounded => 0,
        }
        .min(end);
        let seg = if start < end {
            self.segments.partition_point(|s| s.row <= start) - 1
        } else {
            0
        };
        Window {
            rows: self,
            seg,
            at: self.segments.get(seg).map_or(0, |s| start - s.row),
            left: end - start,
        }
    }

    /// The row showing `anchor`'s line, or the gap row hiding it.
    /// `Gap(g)` gives gap g's separator row, or its first row when
    /// it shows whole. `None` when the line is outside every hunk
    /// and gap, as past a partial diff's last hunk. O(log segments).
    pub fn find(&self, anchor: Anchor) -> Option<u32> {
        match anchor {
            Anchor::Old(index) => self.find_line(index, Lane::Old),
            Anchor::New(index) => self.find_line(index, Lane::New),
            Anchor::Gap(gap) => {
                let k =
                    self.segments.partition_point(|s| s.hunk < gap);
                let own = self.segments[k..]
                    .iter()
                    .take_while(|s| s.hunk == gap);
                let first = own.clone().next()?;
                let separator = own
                    .take_while(|s| !s.piece.is_block())
                    .find(|s| matches!(s.piece, Piece::Gap { .. }));
                Some(separator.unwrap_or(first).row)
            }
        }
    }

    fn find_line(&self, index: u32, lane: Lane) -> Option<u32> {
        let start = |s: &Segment| match lane {
            Lane::Old => s.old,
            Lane::New => s.new,
        };
        let k = self
            .segments
            .partition_point(|s| start(s).index <= index);
        let segment = self.segments.get(k.checked_sub(1)?)?;
        let at = index - start(segment).index;
        let (lines, row) = match segment.piece {
            Piece::Gap { hidden, .. } => (hidden, 0),
            Piece::Context { len } | Piece::Expanded { len } => {
                (len, at)
            }
            Piece::Change { old_len, .. } if lane == Lane::Old => {
                (old_len, at)
            }
            Piece::Change { old_len, new_len } => {
                let after_deletions = match self.layout {
                    Layout::Stacked => old_len,
                    Layout::Split => 0,
                };
                (new_len, after_deletions + at)
            }
        };
        (at < lines).then_some(segment.row + row)
    }

    /// The rows of hunk `hunk`'s blocks, without the gap above it.
    /// Empty past the last hunk.
    pub fn hunk_rows(&self, hunk: u32) -> Range<u32> {
        let first = self.segments.partition_point(|s| {
            (s.hunk, s.piece.is_block()) < (hunk, true)
        });
        let end = self.segments.partition_point(|s| s.hunk <= hunk);
        let row = |k: usize| {
            self.segments.get(k).map_or(self.len, |s| s.row)
        };
        row(first)..row(end)
    }
}

fn row_at(segment: &Segment, at: u32, layout: Layout) -> Row {
    let line = |start: Start, i: u32| LineRef {
        index: start.index + i,
        number: LineNo::after(start.offset + i),
    };
    let (old, new) = (segment.old, segment.new);
    match segment.piece {
        Piece::Context { .. } => Row::Context {
            old: line(old, at),
            new: line(new, at),
        },
        Piece::Expanded { .. } => Row::Expanded {
            old: line(old, at),
            new: line(new, at),
        },
        Piece::Gap { hidden, up, down } => Row::Gap {
            hunk: segment.hunk,
            hidden,
            up,
            down,
        },
        Piece::Change { old_len, new_len } => match layout {
            Layout::Stacked if at < old_len => {
                Row::Deletion { old: line(old, at) }
            }
            Layout::Stacked => Row::Addition {
                new: line(new, at - old_len),
            },
            Layout::Split if at < old_len.min(new_len) => {
                Row::Paired {
                    old: line(old, at),
                    new: line(new, at),
                }
            }
            Layout::Split if old_len > new_len => {
                Row::Deletion { old: line(old, at) }
            }
            Layout::Split => Row::Addition { new: line(new, at) },
        },
    }
}

struct Window<'a> {
    rows: &'a Rows,
    seg: usize,
    at: u32,
    left: u32,
}

impl Iterator for Window<'_> {
    type Item = Row;

    fn next(&mut self) -> Option<Row> {
        if self.left == 0 {
            return None;
        }
        let rows = self.rows;
        let segment = &rows.segments[self.seg];
        let row = row_at(segment, self.at, rows.layout);
        self.left -= 1;
        self.at += 1;
        let end = rows
            .segments
            .get(self.seg + 1)
            .map_or(rows.len, |next| next.row);
        if segment.row + self.at == end {
            self.seg += 1;
            self.at = 0;
        }
        Some(row)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left as usize, Some(self.left as usize))
    }
}

impl ExactSizeIterator for Window<'_> {}
