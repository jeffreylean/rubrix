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

const PAIRS: [(u8, u8); 3] =
    [(b'(', b')'), (b'[', b']'), (b'{', b'}')];

/// The bracket at or just before `at` and the one that matches it,
/// scanning at most [`SCAN_LINES`] lines and [`SCAN_CHARS`] bytes.
///
/// `ignored(line)` gives the byte ranges of that line's content to
/// skip, its strings and comments, so a bracket in them neither
/// matches nor counts. `None` when no bracket is beside `at` or its
/// match is not within reach.
pub fn matching(
    buffer: &Buffer,
    at: usize,
    mut ignored: impl FnMut(u32) -> Vec<Range<usize>>,
) -> Option<Brackets> {
    let line = buffer.line_of(at);
    let start = buffer.line_start(line);
    let content = buffer.content(line).as_bytes();
    let skipped = ignored(line);
    let bracket = |rel: usize| {
        let byte = *content.get(rel)?;
        let pair = PAIRS
            .into_iter()
            .find(|&(open, close)| byte == open || byte == close)?;
        let hidden = skipped.iter().any(|range| range.contains(&rel));
        (!hidden).then_some((rel, pair, byte == pair.0))
    };
    let rel = at - start;
    let (rel, pair, forward) = rel
        .checked_sub(1)
        .and_then(bracket)
        .or_else(|| bracket(rel))?;
    let far = scan(buffer, (line, rel), pair, forward, ignored)?;
    Some(Brackets {
        near: start + rel,
        far,
    })
}

/// Pierre's `findClosingBracket` and `findOpeningBracket`: walks from
/// the bracket at byte `rel` of `line`, forward from an opener or
/// back from a closer, counting that bracket up and its partner down,
/// and gives where the count returns to zero.
fn scan(
    buffer: &Buffer,
    (line, rel): (u32, usize),
    (open, close): (u8, u8),
    forward: bool,
    mut ignored: impl FnMut(u32) -> Vec<Range<usize>>,
) -> Option<usize> {
    let (same, other) = if forward {
        (open, close)
    } else {
        (close, open)
    };
    let mut depth = 0u32;
    let mut scanned = 0;
    for step in 0..SCAN_LINES {
        let current = if forward {
            line.checked_add(step)
                .filter(|&next| next < buffer.line_count())
        } else {
            line.checked_sub(step)
        }?;
        let content = buffer.content(current).as_bytes();
        let skipped = ignored(current);
        let span = match (step, forward) {
            (0, true) => rel..content.len(),
            (0, false) => 0..rel + 1,
            _ => 0..content.len(),
        };
        for k in 0..span.len() {
            if scanned >= SCAN_CHARS {
                return None;
            }
            scanned += 1;
            let i = if forward {
                span.start + k
            } else {
                span.end - 1 - k
            };
            if skipped.iter().any(|range| range.contains(&i)) {
                continue;
            }
            if content[i] == same {
                depth += 1;
            } else if content[i] == other {
                depth -= 1;
                if depth == 0 {
                    return Some(buffer.line_start(current) + i);
                }
            }
        }
    }
    None
}
