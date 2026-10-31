//! The bracket that matches the one at a caret.

use alloc::vec::Vec;
use core::ops::Range;

use crate::buffer::Buffer;

/// The most lines [`matching`] scans from the caret, as in Pierre.
pub const SCAN_LINES: u32 = 1_000;

/// The most bytes [`matching`] scans from the caret, as in Pierre.
pub const SCAN_CHARS: usize = 50_000;

/// A bracket and the one that matches it, as byte offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brackets {
    /// The bracket at or just before the caret.
    pub near: usize,
    /// The bracket that matches it.
    pub far: usize,
}

/// The bracket at or just before `at` and the one that matches it,
/// scanning at most [`SCAN_LINES`] lines and [`SCAN_CHARS`] bytes.
///
/// `ignored(line)` gives the byte ranges of that line's content to
/// skip, its strings and comments, so a bracket in them neither
/// matches nor counts. `None` when no bracket is beside `at` or its
/// match is not within reach.
pub fn matching(
    _buffer: &Buffer,
    _at: usize,
    _ignored: impl FnMut(u32) -> Vec<Range<usize>>,
) -> Option<Brackets> {
    None
}
