use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

/// One side's lines, held in one buffer with their terminators.
///
/// Lines split after each `\n`, as `str::split_inclusive('\n')`
/// does: a text with n `\n` bytes has n lines, plus one when it does
/// not end in `\n`, and the empty text has none. Lines are compared
/// with their terminators, so `"a\r\n"` differs from `"a\n"` and a
/// last line without `\n` differs from the same line with one.
///
/// Line i of a full diff's side is line i of
/// `rubrix_highlight::highlight` over [`Lines::text`]. The
/// highlighter counts one more line when the text ends in `\n`, an
/// empty one with no diff counterpart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lines {
    text: String,
    ends: Vec<u32>,
}

impl Lines {
    pub fn len(&self) -> u32 {
        self.ends.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Every line, terminators included. For a full diff this is the
    /// side's text exactly, so highlighting it gives runs whose line
    /// i matches index i here.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The line's content: its text without one `\n` and then one
    /// `\r`, the same bytes `rubrix_highlight` lexes for that line.
    ///
    /// # Panics
    ///
    /// Panics if `index >= self.len()`.
    pub fn line(&self, index: u32) -> &str {
        let raw = self.raw(index);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        raw.strip_suffix('\r').unwrap_or(raw)
    }

    /// The line with its terminator, for rebuilding text exactly.
    /// It lacks a `\n` only on the side's last line, and only when
    /// that file ends without one.
    ///
    /// # Panics
    ///
    /// Panics if `index >= self.len()`.
    pub fn raw(&self, index: u32) -> &str {
        let index = index as usize;
        let end = self.ends[index] as usize;
        let before = index.checked_sub(1);
        let start = before.map_or(0, |i| self.ends[i] as usize);
        &self.text[start..end]
    }

    /// The side's file ends without a newline: its last line has no
    /// `\n`.
    pub fn missing_newline(&self) -> bool {
        !self.text.is_empty() && !self.text.ends_with('\n')
    }

    pub(crate) fn split(text: &str) -> Lines {
        assert!(
            u32::try_from(text.len()).is_ok(),
            "text is 4 GiB or longer"
        );
        let mut end = 0;
        let ends = text
            .split_inclusive('\n')
            .map(|line| {
                end += line.len() as u32;
                end
            })
            .collect();
        Lines {
            text: text.into(),
            ends,
        }
    }

    fn is_well_formed(&self) -> bool {
        let bytes = self.text.as_bytes();
        let mut start = 0;
        for (i, &end) in self.ends.iter().enumerate() {
            let last = i + 1 == self.ends.len();
            let ended = bytes.get(end as usize - 1) == Some(&b'\n');
            if end <= start || !(last || ended) {
                return false;
            }
            start = end;
        }
        start as usize == bytes.len()
    }

    pub(crate) fn has_room(&self, content: &str) -> bool {
        self.text.len() + content.len() < u32::MAX as usize
    }

    pub(crate) fn push(&mut self, content: &str) {
        debug_assert!(
            !content.contains('\n') && self.has_room(content)
        );
        self.text.push_str(content);
        self.text.push('\n');
        self.ends.push(self.text.len() as u32);
    }

    pub(crate) fn strip_last_newline(&mut self) {
        debug_assert!(self.raw(self.len() - 1).len() > 1);
        debug_assert!(self.text.ends_with('\n'));
        self.text.pop();
        if let Some(end) = self.ends.last_mut() {
            *end -= 1;
        }
    }
}

const MAX_POSITION: u32 = u32::MAX - 1;

/// What happened to the file as a whole.
///
/// [`diff_texts`](crate::diff_texts) always gives `Modified`: two
/// texts carry no names or modes.
/// [`FileDiff::check`] holds each kind to its shape, listed per
/// variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FileChange {
    /// Content or mode changed in place, or nothing changed, as
    /// [`diff_texts`](crate::diff_texts) gives for equal texts. A
    /// mode-only change has no hunks.
    #[default]
    Modified,
    /// Moved without a content change: no hunks.
    Renamed,
    /// Moved and edited: at least one hunk.
    RenamedModified,
    /// Created: the old side has no lines.
    Added,
    /// Removed: the new side has no lines.
    Deleted,
    /// `copy from` / `copy to`, kept apart from renames. Hunks may
    /// follow, as for a modified copy.
    Copied,
    /// `Binary files a and b differ` or `GIT binary patch`: no hunks
    /// and no lines. The names and modes still tell a renamed or
    /// added binary file.
    Binary,
}

/// A run of unchanged or changed lines inside a hunk, positioned by
/// line-list index on each side.
///
/// A hunk's blocks are contiguous on both sides: each starts where
/// the one before it ends. Any order of kinds can occur, two change
/// blocks in a row included. No block is empty on both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Block {
    /// `len` lines, the same on both sides. `len > 0`.
    Context {
        len: u32,
        old_start: u32,
        new_start: u32,
    },
    /// Old lines `old_start..old_start + old_len` became new lines
    /// `new_start..new_start + new_len`. One side may be empty, and
    /// its start then sits where its lines would go. In the split
    /// layout, old line `old_start + i` and new line `new_start + i`
    /// share a row.
    Change {
        old_start: u32,
        old_len: u32,
        new_start: u32,
        new_len: u32,
    },
}

impl Block {
    /// The block's old lines, as indexes.
    pub fn old_range(&self) -> Range<u32> {
        match *self {
            Block::Context { len, old_start, .. } => {
                old_start..old_start + len
            }
            Block::Change {
                old_start, old_len, ..
            } => old_start..old_start + old_len,
        }
    }

    /// The block's new lines, as indexes.
    pub fn new_range(&self) -> Range<u32> {
        match *self {
            Block::Context { len, new_start, .. } => {
                new_start..new_start + len
            }
            Block::Change {
                new_start, new_len, ..
            } => new_start..new_start + new_len,
        }
    }
}

/// One `@@` hunk: a run of blocks with context around its changes.
///
/// A hunk holds at least one block. Its file positions stay private:
/// read them as text through [`Hunk::header`] and as line numbers on
/// rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    old_offset: u32,
    new_offset: u32,
    heading: String,
    blocks: Vec<Block>,
}

impl Hunk {
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The section heading after the second `@@`, or `""`.
    pub fn heading(&self) -> &str {
        &self.heading
    }

    pub(crate) fn from_blocks(blocks: Vec<Block>) -> Hunk {
        let first = blocks.first().expect("a hunk holds a block");
        Hunk {
            old_offset: first.old_range().start,
            new_offset: first.new_range().start,
            heading: String::new(),
            blocks,
        }
    }

    /// Old lines in the hunk, context included. O(1).
    pub fn old_len(&self) -> u32 {
        let end = |last: &Block| last.old_range().end;
        self.blocks.last().map_or(0, end) - self.old_index()
    }

    pub fn new_len(&self) -> u32 {
        let end = |last: &Block| last.new_range().end;
        self.blocks.last().map_or(0, end) - self.new_index()
    }

    /// `@@ -a,b +c,d @@` as git writes it: 1-based starts, a count
    /// of 1 left out, an empty side's start naming the line before
    /// it, and the heading, if any, after a space.
    pub fn header(&self) -> impl fmt::Display + '_ {
        Header(self)
    }

    pub(crate) fn old_offset(&self) -> u32 {
        self.old_offset
    }

    pub(crate) fn new_offset(&self) -> u32 {
        self.new_offset
    }

    pub(crate) fn old_index(&self) -> u32 {
        self.blocks
            .first()
            .map_or(0, |block| block.old_range().start)
    }

    pub(crate) fn new_index(&self) -> u32 {
        self.blocks
            .first()
            .map_or(0, |block| block.new_range().start)
    }
}

struct Header<'a>(&'a Hunk);

impl fmt::Display for Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hunk = self.0;
        f.write_str("@@ -")?;
        write_side(f, hunk.old_offset, hunk.old_len())?;
        f.write_str(" +")?;
        write_side(f, hunk.new_offset, hunk.new_len())?;
        f.write_str(" @@")?;
        if !hunk.heading.is_empty() {
            write!(f, " {}", hunk.heading)?;
        }
        Ok(())
    }
}

fn write_side(
    f: &mut fmt::Formatter<'_>,
    start: u32,
    len: u32,
) -> fmt::Result {
    match len {
        0 => write!(f, "{start},0"),
        1 => write!(f, "{}", start + 1),
        _ => write!(f, "{},{len}", start + 1),
    }
}

/// Lines above a hunk side, from git's `start,count` header pair:
/// `start - 1`, or `start` when `count == 0`. The inverse of the
/// rule [`Hunk::header`] writes. `None` for
/// `start == 0` with lines, which no header can mean.
pub fn header_start(start: u32, count: u32) -> Option<u32> {
    match count {
        0 => Some(start),
        _ => start.checked_sub(1),
    }
}

/// One file's diff: both sides' lines, the hunks over them, and
/// what git says about the file.
///
/// Stores no rows: [`rows`](crate::rows()) derives them for a layout
/// and an [`Expansion`](crate::Expansion).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    old: Lines,
    new: Lines,
    hunks: Vec<Hunk>,
    change: FileChange,
    old_name: Option<String>,
    new_name: Option<String>,
    old_mode: Option<u32>,
    new_mode: Option<u32>,
    partial: bool,
}

impl FileDiff {
    pub fn old_lines(&self) -> &Lines {
        &self.old
    }

    pub fn new_lines(&self) -> &Lines {
        &self.new
    }

    pub fn hunks(&self) -> &[Hunk] {
        &self.hunks
    }

    pub fn change(&self) -> FileChange {
        self.change
    }

    pub fn old_name(&self) -> Option<&str> {
        self.old_name.as_deref()
    }

    pub fn new_name(&self) -> Option<&str> {
        self.new_name.as_deref()
    }

    /// Git's octal mode, such as `0o100644`.
    pub fn old_mode(&self) -> Option<u32> {
        self.old_mode
    }

    /// Git's octal mode, such as `0o100755`.
    pub fn new_mode(&self) -> Option<u32> {
        self.new_mode
    }

    pub fn is_partial(&self) -> bool {
        self.partial
    }

    /// Added lines: the new sides of all change blocks, git's
    /// `--numstat` first column.
    pub fn added(&self) -> u32 {
        self.change_lens().map(|(_, new_len)| new_len).sum()
    }

    /// Removed lines: the old sides of all change blocks.
    pub fn removed(&self) -> u32 {
        self.change_lens().map(|(old_len, _)| old_len).sum()
    }

    fn change_lens(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let blocks = self.hunks.iter().flat_map(|hunk| &hunk.blocks);
        blocks.filter_map(|block| match *block {
            Block::Change {
                old_len, new_len, ..
            } => Some((old_len, new_len)),
            Block::Context { .. } => None,
        })
    }

    /// Tests the rules every constructor here keeps, and returns the
    /// first one broken. Debug builds run it at the end of each
    /// constructor.
    pub fn check(&self) -> Result<(), Invariant> {
        let limit = u64::from(MAX_POSITION);
        let full = !self.partial;
        let (mut old_end, mut new_end) = (0, 0);
        let (mut old_next, mut new_next) = (0, 0);
        for (h, hunk) in (0..).zip(&self.hunks) {
            if hunk.blocks.is_empty() {
                return Err(Invariant::EmptyHunk { hunk: h });
            }
            let empty = |block: &Block| {
                block.old_range().is_empty()
                    && block.new_range().is_empty()
            };
            if let Some(block) = hunk.blocks.iter().position(empty) {
                let block = block as u32;
                return Err(Invariant::EmptyBlock { hunk: h, block });
            }
            let apart = |pair: &[Block]| {
                pair[0].old_range().end != pair[1].old_range().start
                    || pair[0].new_range().end
                        != pair[1].new_range().start
            };
            if let Some(block) =
                hunk.blocks.windows(2).position(apart)
            {
                let block = block as u32 + 1;
                return Err(Invariant::Discontiguous {
                    hunk: h,
                    block,
                });
            }
            let (old_start, new_start) = (
                u64::from(hunk.old_offset),
                u64::from(hunk.new_offset),
            );
            if old_start < old_end || new_start < new_end {
                return Err(Invariant::Overlap { hunk: h });
            }
            let old_gap = old_start - old_end;
            if full && old_gap != new_start - new_end {
                return Err(Invariant::UnevenGap { hunk: h });
            }
            let (old_index, new_index) =
                (hunk.old_index(), hunk.new_index());
            let indexed = if full {
                u64::from(old_index) == old_start
                    && u64::from(new_index) == new_start
            } else {
                (old_index, new_index) == (old_next, new_next)
            };
            if !indexed {
                return Err(Invariant::Index { hunk: h });
            }
            old_end = old_start + u64::from(hunk.old_len());
            new_end = new_start + u64::from(hunk.new_len());
            if full
                && (old_end > u64::from(self.old.len())
                    || new_end > u64::from(self.new.len()))
            {
                return Err(Invariant::PastEnd { hunk: h });
            }
            if old_end > limit || new_end > limit {
                return Err(Invariant::Range { hunk: h });
            }
            old_next = old_index + hunk.old_len();
            new_next = new_index + hunk.new_len();
        }
        let count = self.hunks.len() as u32;
        if full
            && u64::from(self.old.len()) - old_end
                != u64::from(self.new.len()) - new_end
        {
            return Err(Invariant::UnevenGap { hunk: count });
        }
        if u64::from(self.old.len()) > limit
            || u64::from(self.new.len()) > limit
        {
            return Err(Invariant::Range { hunk: count });
        }
        if !self.old.is_well_formed() || !self.new.is_well_formed() {
            return Err(Invariant::Newline);
        }
        if !full
            && (old_next, new_next)
                != (self.old.len(), self.new.len())
        {
            return Err(Invariant::Coverage);
        }
        self.check_kind()
    }

    fn check_kind(&self) -> Result<(), Invariant> {
        let agrees = match self.change {
            FileChange::Binary => {
                self.hunks.is_empty()
                    && self.old.is_empty()
                    && self.new.is_empty()
            }
            FileChange::Added => self.old.is_empty(),
            FileChange::Deleted => self.new.is_empty(),
            FileChange::Renamed => self.hunks.is_empty(),
            FileChange::RenamedModified => !self.hunks.is_empty(),
            FileChange::Modified | FileChange::Copied => true,
        };
        if agrees { Ok(()) } else { Err(Invariant::Kind) }
    }

    pub(crate) fn full(
        old: Lines,
        new: Lines,
        hunks: Vec<Hunk>,
    ) -> FileDiff {
        let diff = FileDiff::bare(old, new, hunks, false);
        debug_assert_eq!(diff.check(), Ok(()));
        diff
    }

    fn bare(
        old: Lines,
        new: Lines,
        hunks: Vec<Hunk>,
        partial: bool,
    ) -> FileDiff {
        FileDiff {
            old,
            new,
            hunks,
            change: FileChange::Modified,
            old_name: None,
            new_name: None,
            old_mode: None,
            new_mode: None,
            partial,
        }
    }

    /// Sets what git says about the file, such as the names a pair
    /// of texts came from. Fails with [`Invariant::Kind`] if the kind
    /// disagrees with the hunks or the lines, as
    /// [`PatchBuilder::finish`] does.
    pub fn with_meta(
        mut self,
        meta: Meta,
    ) -> Result<FileDiff, Invariant> {
        self.change = meta.change;
        self.old_name = meta.old_name;
        self.new_name = meta.new_name;
        self.old_mode = meta.old_mode;
        self.new_mode = meta.new_mode;
        self.check_kind()?;
        Ok(self)
    }

    /// Re-splits change blocks in one pass over every hunk. `split`
    /// sees each change block with both sides' lines and pushes the
    /// `(old_len, new_len)` lengths of the adjacent change blocks to
    /// put in its place, in order, or pushes nothing to keep it.
    ///
    /// Fails with [`Invariant::Split`], leaving the diff as it was,
    /// if a part is empty on both sides or the parts' lengths do not
    /// sum to the block's.
    pub fn split_changes(
        &mut self,
        mut split: impl FnMut(&Lines, &Lines, Block, &mut Vec<(u32, u32)>),
    ) -> Result<(), Invariant> {
        let mut parts = Vec::new();
        let mut lists = Vec::with_capacity(self.hunks.len());
        for (h, hunk) in (0..).zip(&self.hunks) {
            let mut list = Vec::with_capacity(hunk.blocks.len());
            for (b, &block) in (0..).zip(&hunk.blocks) {
                let Block::Change {
                    old_start,
                    old_len,
                    new_start,
                    new_len,
                } = block
                else {
                    list.push(block);
                    continue;
                };
                parts.clear();
                split(&self.old, &self.new, block, &mut parts);
                if parts.is_empty() {
                    list.push(block);
                    continue;
                }
                let olds =
                    parts.iter().map(|&(old, _)| u64::from(old));
                let news =
                    parts.iter().map(|&(_, new)| u64::from(new));
                let sums_up = olds.sum::<u64>() == u64::from(old_len)
                    && news.sum::<u64>() == u64::from(new_len);
                if !sums_up || parts.contains(&(0, 0)) {
                    return Err(Invariant::Split {
                        hunk: h,
                        block: b,
                    });
                }
                let (mut old_at, mut new_at) = (old_start, new_start);
                for &(old_len, new_len) in &parts {
                    list.push(Block::Change {
                        old_start: old_at,
                        old_len,
                        new_start: new_at,
                        new_len,
                    });
                    old_at += old_len;
                    new_at += new_len;
                }
            }
            lists.push(list);
        }
        for (hunk, list) in self.hunks.iter_mut().zip(lists) {
            hunk.blocks = list;
        }
        debug_assert_eq!(self.check(), Ok(()));
        Ok(())
    }
}

/// A rule of the model that a diff breaks, from [`FileDiff::check`]
/// or [`PatchBuilder`]. Hunk and block numbers count from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Invariant {
    EmptyHunk {
        hunk: u32,
    },
    EmptyBlock {
        hunk: u32,
        block: u32,
    },
    Discontiguous {
        hunk: u32,
        block: u32,
    },
    Overlap {
        hunk: u32,
    },
    /// The unchanged lines before `hunk` (or after the last, when
    /// `hunk` is the hunk count) differ in number between sides.
    UnevenGap {
        hunk: u32,
    },
    Index {
        hunk: u32,
    },
    PastEnd {
        hunk: u32,
    },
    /// A partial diff's line lists hold lines outside its hunks.
    Coverage,
    /// A hunk's file end, or a side's line count, would pass
    /// `u32::MAX - 1`.
    Range {
        hunk: u32,
    },
    /// A line other than a side's last lacks its `\n`, a line is
    /// empty without one, or a patch line holds a `\n` or lands on a
    /// side already ended by `\ No newline at end of file`.
    Newline,
    /// [`FileDiff::split_changes`] got parts that are empty or do not
    /// sum to the change block they replace.
    Split {
        hunk: u32,
        block: u32,
    },
    /// The [`FileChange`] disagrees with the hunks or the lines.
    Kind,
}

impl fmt::Display for Invariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::EmptyHunk { hunk } => {
                write!(f, "hunk {hunk} has no blocks")
            }
            Self::EmptyBlock { hunk, block } => {
                write!(f, "block {block} of hunk {hunk} is empty")
            }
            Self::Discontiguous { hunk, block } => write!(
                f,
                "block {block} of hunk {hunk} does not start where \
                 the block before it ends"
            ),
            Self::Overlap { hunk } => write!(
                f,
                "hunk {hunk} starts before the hunk before it ends"
            ),
            Self::UnevenGap { hunk } => write!(
                f,
                "the unchanged lines before hunk {hunk} differ \
                 between the sides"
            ),
            Self::Index { hunk } => write!(
                f,
                "hunk {hunk} starts at the wrong line-list index"
            ),
            Self::PastEnd { hunk } => {
                write!(f, "hunk {hunk} ends past the last line")
            }
            Self::Coverage => f.write_str(
                "a line list holds lines outside the hunks",
            ),
            Self::Range { hunk } => {
                write!(
                    f,
                    "hunk {hunk} passes the largest line number"
                )
            }
            Self::Newline => f.write_str(
                "a line lacks its newline, holds one, or follows \
                 the end of its side",
            ),
            Self::Split { hunk, block } => write!(
                f,
                "block {block} of hunk {hunk} split into empty \
                 parts or the wrong total"
            ),
            Self::Kind => f.write_str(
                "the file change disagrees with the hunks or lines",
            ),
        }
    }
}

impl core::error::Error for Invariant {}

/// A patch body line's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    /// ` `: in both sides.
    Context,
    /// `-`: in the old side.
    Delete,
    /// `+`: in the new side.
    Insert,
}

/// What a patch's file header says. [`FileDiff`] takes it whole in
/// [`PatchBuilder::finish`].
#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub change: FileChange,
    pub old_name: Option<String>,
    pub new_name: Option<String>,
    pub old_mode: Option<u32>,
    pub new_mode: Option<u32>,
}

/// Builds a partial [`FileDiff`] one patch line at a time.
///
/// A `-` or `+` line extends the change block before it, whichever
/// came first, as a patch's change run does.
#[derive(Debug)]
pub struct PatchBuilder {
    diff: FileDiff,
    last: Option<Edit>,
    old_ended: bool,
    new_ended: bool,
}

impl Default for PatchBuilder {
    fn default() -> PatchBuilder {
        PatchBuilder::new()
    }
}

impl PatchBuilder {
    pub fn new() -> PatchBuilder {
        let (old, new) = (Lines::default(), Lines::default());
        PatchBuilder {
            diff: FileDiff::bare(old, new, Vec::new(), true),
            last: None,
            old_ended: false,
            new_ended: false,
        }
    }

    /// Opens a hunk at file positions from [`header_start`], with the
    /// heading text after its second `@@`. Fails if the hunk before
    /// it is empty, this one overlaps it, or its start passes
    /// `u32::MAX - 1`.
    pub fn hunk(
        &mut self,
        old_start: u32,
        new_start: u32,
        heading: &str,
    ) -> Result<(), Invariant> {
        let hunks = &mut self.diff.hunks;
        let at = hunks.len() as u32;
        let (old_end, new_end) = match hunks.last() {
            Some(prev) if prev.blocks.is_empty() => {
                return Err(Invariant::EmptyHunk { hunk: at - 1 });
            }
            Some(prev) => (
                prev.old_offset + prev.old_len(),
                prev.new_offset + prev.new_len(),
            ),
            None => (0, 0),
        };
        if old_start.max(new_start) > MAX_POSITION {
            return Err(Invariant::Range { hunk: at });
        }
        if old_start < old_end || new_start < new_end {
            return Err(Invariant::Overlap { hunk: at });
        }
        hunks.push(Hunk {
            old_offset: old_start,
            new_offset: new_start,
            heading: heading.into(),
            blocks: Vec::new(),
        });
        self.last = None;
        Ok(())
    }

    /// Pushes one body line: its text without the ` `, `-` or `+`
    /// prefix and without its `\n`. Fails if the text holds a `\n`,
    /// a side it lands on was ended by [`PatchBuilder::no_newline`],
    /// or the hunk's file end would pass `u32::MAX - 1`.
    ///
    /// # Panics
    ///
    /// Panics if no hunk is open.
    pub fn line(
        &mut self,
        edit: Edit,
        text: &str,
    ) -> Result<(), Invariant> {
        let diff = &mut self.diff;
        let open =
            diff.hunks.len().checked_sub(1).expect("no hunk is open");
        let (at, hunk) = (open as u32, &mut diff.hunks[open]);
        let on_old = edit != Edit::Insert;
        let on_new = edit != Edit::Delete;
        if text.contains('\n')
            || (on_old && self.old_ended)
            || (on_new && self.new_ended)
        {
            return Err(Invariant::Newline);
        }
        let fits = |on: bool, lines: &Lines, start: u32, len: u32| {
            !on || (lines.has_room(text)
                && start + len < MAX_POSITION)
        };
        if !fits(on_old, &diff.old, hunk.old_offset, hunk.old_len())
            || !fits(
                on_new,
                &diff.new,
                hunk.new_offset,
                hunk.new_len(),
            )
        {
            return Err(Invariant::Range { hunk: at });
        }
        let (old_at, new_at) = (diff.old.len(), diff.new.len());
        if on_old {
            diff.old.push(text);
        }
        if on_new {
            diff.new.push(text);
        }
        let (old_add, new_add) =
            (u32::from(on_old), u32::from(on_new));
        match (hunk.blocks.last_mut(), edit) {
            (Some(Block::Context { len, .. }), Edit::Context) => {
                *len += 1;
            }
            (
                Some(Block::Change {
                    old_len, new_len, ..
                }),
                Edit::Delete | Edit::Insert,
            ) => {
                *old_len += old_add;
                *new_len += new_add;
            }
            (_, Edit::Context) => hunk.blocks.push(Block::Context {
                len: 1,
                old_start: old_at,
                new_start: new_at,
            }),
            (_, Edit::Delete | Edit::Insert) => {
                hunk.blocks.push(Block::Change {
                    old_start: old_at,
                    old_len: old_add,
                    new_start: new_at,
                    new_len: new_add,
                });
            }
        }
        self.last = Some(edit);
        Ok(())
    }

    /// `\ No newline at end of file`: strips the `\n` from the last
    /// line on the side or sides of the line before it, and ends
    /// those sides. Fails if no line came before it, a side was
    /// already ended, or the line is empty.
    pub fn no_newline(&mut self) -> Result<(), Invariant> {
        let edit = self.last.ok_or(Invariant::Newline)?;
        let on_old = edit != Edit::Insert;
        let on_new = edit != Edit::Delete;
        let diff = &mut self.diff;
        let has_content =
            |lines: &Lines| lines.raw(lines.len() - 1).len() > 1;
        if (on_old && (self.old_ended || !has_content(&diff.old)))
            || (on_new && (self.new_ended || !has_content(&diff.new)))
        {
            return Err(Invariant::Newline);
        }
        if on_old {
            diff.old.strip_last_newline();
            self.old_ended = true;
        }
        if on_new {
            diff.new.strip_last_newline();
            self.new_ended = true;
        }
        Ok(())
    }

    /// The open hunk's old and new line counts.
    pub fn hunk_lens(&self) -> (u32, u32) {
        self.diff
            .hunks
            .last()
            .map_or((0, 0), |hunk| (hunk.old_len(), hunk.new_len()))
    }

    /// Ends the file. Fails if the last hunk is empty or `meta`'s
    /// kind disagrees with the hunks.
    pub fn finish(self, meta: Meta) -> Result<FileDiff, Invariant> {
        let hunks = &self.diff.hunks;
        if hunks.last().is_some_and(|hunk| hunk.blocks.is_empty()) {
            return Err(Invariant::EmptyHunk {
                hunk: hunks.len() as u32 - 1,
            });
        }
        let diff = self.diff.with_meta(meta)?;
        debug_assert_eq!(diff.check(), Ok(()));
        Ok(diff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_texts;

    type Break = fn(&mut FileDiff);

    fn two_hunks() -> FileDiff {
        let old = "a\nb\nc\nd\ne\nf\ng\n";
        diff_texts(old, "a\nB\nc\nd\ne\nF\ng\n", 1)
    }

    fn one_line_patch() -> FileDiff {
        let mut builder = PatchBuilder::new();
        builder.hunk(0, 0, "").unwrap();
        builder.line(Edit::Context, "a").unwrap();
        builder.finish(Meta::default()).unwrap()
    }

    #[test]
    fn check_names_the_rule_a_broken_full_diff_breaks() {
        let cases: [(Break, Invariant); 9] = [
            (
                |d| d.hunks[1].blocks.clear(),
                Invariant::EmptyHunk { hunk: 1 },
            ),
            (
                |d| {
                    d.hunks[0].blocks[0] = Block::Context {
                        len: 0,
                        old_start: 0,
                        new_start: 0,
                    }
                },
                Invariant::EmptyBlock { hunk: 0, block: 0 },
            ),
            (
                |d| {
                    d.hunks[0].blocks[1] = Block::Change {
                        old_start: 2,
                        old_len: 1,
                        new_start: 1,
                        new_len: 1,
                    }
                },
                Invariant::Discontiguous { hunk: 0, block: 1 },
            ),
            (
                |d| d.hunks[1].old_offset = 2,
                Invariant::Overlap { hunk: 1 },
            ),
            (
                |d| d.hunks[1].new_offset += 1,
                Invariant::UnevenGap { hunk: 1 },
            ),
            (|d| d.new.push("z"), Invariant::UnevenGap { hunk: 2 }),
            (
                |d| {
                    d.hunks[1].old_offset += 1;
                    d.hunks[1].new_offset += 1;
                },
                Invariant::Index { hunk: 1 },
            ),
            (
                |d| d.old = Lines::split("a\nb\nc\nd\ne\nf\n"),
                Invariant::PastEnd { hunk: 1 },
            ),
            (
                |d| d.old.text.replace_range(1..2, "x"),
                Invariant::Newline,
            ),
        ];
        for (index, (break_it, want)) in cases.into_iter().enumerate()
        {
            let mut diff = two_hunks();
            break_it(&mut diff);
            assert_eq!(diff.check(), Err(want), "case {index}");
        }
    }

    #[test]
    fn check_names_the_rule_a_broken_partial_diff_breaks() {
        let cases: [(Break, Invariant); 3] = [
            (|d| d.old.push("x"), Invariant::Coverage),
            (
                |d| d.hunks[0].old_offset = u32::MAX - 1,
                Invariant::Range { hunk: 0 },
            ),
            (|d| d.change = FileChange::Binary, Invariant::Kind),
        ];
        for (index, (break_it, want)) in cases.into_iter().enumerate()
        {
            let mut diff = one_line_patch();
            break_it(&mut diff);
            assert_eq!(diff.check(), Err(want), "case {index}");
        }
    }

    #[test]
    fn check_allows_uneven_gaps_in_a_partial_diff() {
        let mut diff = one_line_patch();
        diff.hunks[0].old_offset = 5;
        assert_eq!(diff.check(), Ok(()));
    }
}
