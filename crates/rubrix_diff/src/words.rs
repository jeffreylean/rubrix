use alloc::vec::Vec;
use core::ops::Range;

use similar::{DiffTag, TextDiff};

const MAX_LEN: usize = 1000;

/// The changed byte ranges of a removed line and the added line it
/// pairs with: `(old, new)`, each sorted and disjoint, each range
/// starting and ending on a character boundary of its line.
///
/// The lines split at the word boundaries of Unicode Standard Annex
/// 29, so `foo(bar)` is four words and punctuation changes alone.
/// Two ranges on one side with a single space between them merge
/// into one, as Pierre's `word-alt` mode does. Pass line contents
/// without their terminators, as [`Lines::line`] gives them.
///
/// `None` when either line is longer than 1,000 bytes, Pierre's
/// `maxLineDiffLength`.
///
/// [`Lines::line`]: crate::Lines::line
pub fn word_changes(
    old: &str,
    new: &str,
) -> Option<(Vec<Range<u32>>, Vec<Range<u32>>)> {
    if old.len() > MAX_LEN || new.len() > MAX_LEN {
        return None;
    }
    let diff = TextDiff::from_unicode_words(old, new);
    let old_starts = starts(diff.iter_old_slices());
    let new_starts = starts(diff.iter_new_slices());
    let (mut old_ranges, mut new_ranges) = (Vec::new(), Vec::new());
    for op in diff.ops() {
        if op.tag() == DiffTag::Equal {
            continue;
        }
        push(&mut old_ranges, old, &old_starts, op.old_range());
        push(&mut new_ranges, new, &new_starts, op.new_range());
    }
    Some((old_ranges, new_ranges))
}

/// Each token's byte offset, then the line's length.
fn starts<'a>(tokens: impl Iterator<Item = &'a str>) -> Vec<u32> {
    let mut at = 0;
    let mut starts = Vec::new();
    for token in tokens {
        starts.push(at);
        at += token.len() as u32;
    }
    starts.push(at);
    starts
}

fn push(
    ranges: &mut Vec<Range<u32>>,
    line: &str,
    starts: &[u32],
    tokens: Range<usize>,
) {
    if tokens.is_empty() {
        return;
    }
    let range = starts[tokens.start]..starts[tokens.end];
    if let Some(last) = ranges.last_mut() {
        let between = &line[last.end as usize..range.start as usize];
        if between.is_empty() || between == " " {
            last.end = range.end;
            return;
        }
    }
    ranges.push(range);
}
