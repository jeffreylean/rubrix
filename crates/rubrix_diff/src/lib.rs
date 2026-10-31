//! Line diffs, hunks and diff rows for rubrix.
//!
//! [`diff_texts`] diffs two texts into a [`FileDiff`]: both sides'
//! [`Lines`], and [`Hunk`]s of [`Block`]s over them. [`rows()`] turns
//! a diff, a [`Layout`] and an [`Expansion`] into [`Rows`], which a
//! view reads a window at a time. [`Expansion::reveal`] shows more of
//! the unchanged lines between hunks. [`FileDiff::edit_new`] edits
//! the new side and re-diffs only the hunks near the edit, and
//! [`Rows::patch`] moves rows through it.
//!
//! ```
//! use rubrix_diff::{Expansion, Layout, Side, diff_texts, rows};
//!
//! let old = "a\nb\nc\n";
//! let new = "a\nB\nc\n";
//! let diff = diff_texts(old, new, 3);
//! let mut expansion = Expansion::new();
//! expansion.reveal(1, Side::Down, Expansion::STEP);
//! let rows = rows(&diff, Layout::Split, &expansion);
//! for row in rows.range(0..40) {
//!     if let Some(line) = row.new_line() {
//!         let text = diff.new_lines().line(line.index);
//!         println!("{} {text}", line.number);
//!     }
//! }
//! ```
//!
//! # Lines, indexes and numbers
//!
//! A side's lines keep their terminators, and a full diff's side is
//! its text byte for byte, so line i of a side is line i of
//! `rubrix_highlight::highlight` over the same text. A row names each
//! of its lines twice: by [`LineRef::index`] into the side's
//! [`Lines`], for text and highlight runs, and by [`LineNo`], for the
//! gutter. The two agree as `number == index + 1` only in a full
//! diff; a diff from a patch holds its hunk lines alone.

#![no_std]

extern crate alloc;

mod edit;
mod expand;
mod model;
mod patch;
mod realign;
mod rows;
mod words;

use alloc::vec::Vec;

pub use edit::Rediff;
pub use expand::{Expansion, Side};
pub use model::{
    Block, Edit, FileChange, FileDiff, Hunk, Invariant, Lines, Meta,
    PatchBuilder, header_start,
};
pub use patch::{PatchError, PatchErrorKind, parse_patch};
pub use realign::realign;
pub use rows::{
    Anchor, Layout, LineNo, LineRef, Row, RowSplice, Rows, rows,
};
use similar::DiffOp;
pub use words::{WordRanges, word_changes};

/// A line-diff algorithm for `similar` to run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Algorithm {
    /// `similar`'s Myers with its cost cap.
    #[default]
    Myers,
    /// `similar`'s Histogram. It slows to hundreds of milliseconds,
    /// and recurses deep enough to overflow a 2 MiB thread stack, on
    /// 10,000 lines where every other line changed.
    Histogram,
}

impl Algorithm {
    fn similar(self) -> similar::Algorithm {
        match self {
            Algorithm::Myers => similar::Algorithm::Myers,
            Algorithm::Histogram => similar::Algorithm::Histogram,
        }
    }
}

/// Diffs two texts with the default [`Algorithm`], keeping `context`
/// unchanged lines around each change, as `git diff -U<context>`.
/// Hunks at most `2 * context` lines apart merge, as in git.
///
/// # Panics
///
/// Panics if either text is 4 GiB or longer.
pub fn diff_texts(old: &str, new: &str, context: u32) -> FileDiff {
    diff_texts_with(old, new, context, Algorithm::default())
}

/// [`diff_texts`] with a chosen algorithm.
///
/// # Panics
///
/// Panics if either text is 4 GiB or longer.
pub fn diff_texts_with(
    old: &str,
    new: &str,
    context: u32,
    algorithm: Algorithm,
) -> FileDiff {
    let (old, new) = (Lines::split(old), Lines::split(new));
    let ops = similar::capture_diff_slices(
        algorithm.similar(),
        &raw_lines(&old),
        &raw_lines(&new),
    );
    let context = context.min(old.len().max(new.len())) as usize;
    let hunks = similar::group_diff_ops(ops, context)
        .iter()
        .map(|group| {
            Hunk::from_blocks(
                group.iter().filter_map(block).collect(),
            )
        })
        .collect();
    FileDiff::full(old, new, hunks)
}

fn raw_lines(lines: &Lines) -> Vec<&str> {
    (0..lines.len()).map(|i| lines.raw(i)).collect()
}

/// The block a `similar` op becomes, or `None` for the zero-length
/// `Equal` that `group_diff_ops` leaves at context 0.
fn block(op: &DiffOp) -> Option<Block> {
    let (old, new) = (op.old_range(), op.new_range());
    let (old_start, new_start) = (old.start as u32, new.start as u32);
    match *op {
        DiffOp::Equal { len: 0, .. } => None,
        DiffOp::Equal { len, .. } => Some(Block::Context {
            len: len as u32,
            old_start,
            new_start,
        }),
        _ => Some(Block::Change {
            old_start,
            old_len: old.len() as u32,
            new_start,
            new_len: new.len() as u32,
        }),
    }
}
