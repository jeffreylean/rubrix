use alloc::vec::Vec;
use core::ops::Range;

use similar::{Algorithm, DiffTag, capture_diff_slices};

const MAX_LEN: usize = 1000;

/// Changed byte ranges on the old line, then on the new line.
pub type WordRanges = (Vec<Range<u32>>, Vec<Range<u32>>);

/// The changed byte ranges of a removed line and the added line it
/// pairs with: `(old, new)`, each sorted and disjoint, each range
/// starting and ending on a character boundary of its line.
///
/// A run of word characters (ASCII letters and digits, `_`, and most
/// Latin letters with diacritics) is one token, and so is a run of
/// whitespace. Every other character is a token of its own, so
/// `tally.entry` is `tally`, `.` and `entry`, and `don't` is `don`,
/// `'` and `t`. Two ranges on one side with a single space between
/// them merge into one. Pass line contents without their
/// terminators, as [`Lines::line`] gives them.
///
/// `None` when either line is longer than 1,000 bytes.
///
/// [`Lines::line`]: crate::Lines::line
pub fn word_changes(old: &str, new: &str) -> Option<WordRanges> {
    if old.len() > MAX_LEN || new.len() > MAX_LEN {
        return None;
    }
    let (old_tokens, new_tokens) = (tokens(old), tokens(new));
    let old_bounds = token_bounds(&old_tokens);
    let new_bounds = token_bounds(&new_tokens);
    let (mut old_ranges, mut new_ranges) = (Vec::new(), Vec::new());
    for op in capture_diff_slices(
        Algorithm::Myers,
        &old_tokens,
        &new_tokens,
    ) {
        if op.tag() == DiffTag::Equal {
            continue;
        }
        push(&mut old_ranges, old, &old_bounds, op.old_range());
        push(&mut new_ranges, new, &new_bounds, op.new_range());
    }
    Some((old_ranges, new_ranges))
}

#[derive(Clone, Copy, PartialEq)]
enum Run {
    Word,
    Space,
    Lone,
}

fn run(c: char) -> Run {
    if is_word(c) {
        Run::Word
    } else if is_space(c) {
        Run::Space
    } else {
        Run::Lone
    }
}

/// jsdiff's word characters, so marks match Pierre's.
fn is_word(c: char) -> bool {
    matches!(
        c,
        'a'..='z'
            | 'A'..='Z'
            | '0'..='9'
            | '_'
            | '\u{AD}'
            | '\u{C0}'..='\u{D6}'
            | '\u{D8}'..='\u{F6}'
            | '\u{F8}'..='\u{2C6}'
            | '\u{2C8}'..='\u{2D7}'
            | '\u{2DE}'..='\u{2FF}'
            | '\u{1E00}'..='\u{1EFF}'
    )
}

/// JavaScript's `\s` without `\n` and `\r`, as jsdiff splits.
fn is_space(c: char) -> bool {
    match c {
        '\n' | '\r' | '\u{85}' => false,
        '\u{FEFF}' => true,
        _ => c.is_whitespace(),
    }
}

fn tokens(line: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut rest = line;
    while let Some(first) = rest.chars().next() {
        let len = match run(first) {
            Run::Lone => first.len_utf8(),
            kind => {
                rest.find(|c| run(c) != kind).unwrap_or(rest.len())
            }
        };
        let (token, tail) = rest.split_at(len);
        tokens.push(token);
        rest = tail;
    }
    tokens
}

fn token_bounds(tokens: &[&str]) -> Vec<u32> {
    let mut at = 0;
    let mut bounds = Vec::new();
    for token in tokens {
        bounds.push(at);
        at += token.len() as u32;
    }
    bounds.push(at);
    bounds
}

fn push(
    ranges: &mut Vec<Range<u32>>,
    line: &str,
    bounds: &[u32],
    tokens: Range<usize>,
) {
    if tokens.is_empty() {
        return;
    }
    let range = bounds[tokens.start]..bounds[tokens.end];
    if let Some(last) = ranges.last_mut() {
        let between = &line[last.end as usize..range.start as usize];
        if between.is_empty() || between == " " {
            last.end = range.end;
            return;
        }
    }
    ranges.push(range);
}
