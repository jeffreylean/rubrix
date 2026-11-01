//! Selections added by pointer and by search: add next occurrence,
//! mod+click carets and column selection.

use alloc::vec::Vec;
use core::ops::Range;

use rubrix_highlight::Lines;

use crate::buffer::Buffer;
use crate::motion::{Affinity, Layout, word_at};
use crate::selection::{Selection, Selections};

/// Mod+D: selects the word at each caret when there is a caret, or
/// else adds the next occurrence of the primary's text after it that
/// overlaps no selection, wrapping at the end of the text, as the new
/// primary. Every selection must hold the same text. `None` when
/// there is nothing to select or add.
pub(crate) fn add_next(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Selections> {
    if selections.iter().any(|selection| selection.is_caret()) {
        return expand_carets(buffer, selections);
    }
    let primary = selections.primary();
    let needle = buffer.slice(primary.range());
    if selections
        .iter()
        .any(|selection| buffer.slice(selection.range()) != needle)
    {
        return None;
    }
    let taken: Vec<Range<usize>> = selections
        .iter()
        .map(|selection| selection.range())
        .collect();
    let free = |start: usize| {
        let next = taken.partition_point(|range| range.end <= start);
        taken
            .get(next)
            .is_none_or(|range| start + needle.len() <= range.start)
    };
    let found = next_match(buffer, &needle, primary.end(), free)?;
    let mut added = selections.clone();
    added.add(Selection {
        anchor: found,
        head: found + needle.len(),
    });
    Some(added)
}

/// Selects the word at or just before each caret, as Pierre's
/// `expandCollapsedSelectionToWord` does. `None` when no caret
/// touches a word.
fn expand_carets(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Selections> {
    let is_word = |text: &str| {
        text.chars().any(|c| c.is_alphanumeric() || c == '_')
    };
    let mut expanded = selections.clone();
    let mut changed = false;
    expanded.update(|selection, _, _| {
        if !selection.is_caret() {
            return selection;
        }
        let spot = buffer.spot(selection.head);
        let (content, rel) = (spot.content, spot.rel);
        let word =
            [word_at(content, rel), word_at(&content[..rel], rel)]
                .into_iter()
                .find(|word| is_word(&content[word.clone()]));
        match word {
            Some(word) => {
                changed = true;
                Selection {
                    anchor: spot.start + word.start,
                    head: spot.start + word.end,
                }
            }
            None => selection,
        }
    });
    changed.then_some(expanded)
}

/// The start of the first occurrence of `needle` at or after `from`
/// that `free` accepts, wrapping to the start of the text. Walks the
/// lines from `from`'s, so a press near the next occurrence reads
/// only the lines between.
fn next_match(
    buffer: &Buffer,
    needle: &str,
    from: usize,
    free: impl Fn(usize) -> bool,
) -> Option<usize> {
    let parts: Vec<&str> = needle.split('\n').collect();
    let first = buffer.line_of(from);
    let lines = (first..buffer.line_count())
        .map(|line| (line, from))
        .chain((0..=first).map(|line| (line, 0)));
    for (line, least) in lines {
        let start = buffer.line_start(line);
        let text = buffer.line(line);
        if let [only] = parts[..] {
            let mut rel = least.saturating_sub(start);
            while let Some(found) = text[rel..].find(only) {
                let at = start + rel + found;
                if free(at) {
                    return Some(at);
                }
                rel += found
                    + only.chars().next().map_or(1, char::len_utf8);
            }
        } else if spans(buffer, line, &parts) {
            let at = start + text.len() - parts[0].len();
            if at >= least && free(at) {
                return Some(at);
            }
        }
    }
    None
}

/// Whether the lines from `line` hold `parts`, the pieces of a
/// needle split at `\n`: the first ends `line`, the middle ones are
/// whole lines and the last starts the line after them.
fn spans(buffer: &Buffer, line: u32, parts: &[&str]) -> bool {
    let last = parts.len() - 1;
    line as usize + last < buffer.line_count() as usize
        && buffer.line(line).ends_with(parts[0])
        && (1..last).all(|i| buffer.line(line + i as u32) == parts[i])
        && buffer.line(line + last as u32).starts_with(parts[last])
}

/// mod+click at `at`: removes the selection that holds `at`, unless
/// it is the only one, or else adds a caret at `at` as the new
/// primary, on the row `affinity` names.
pub fn toggle_caret(
    selections: &Selections,
    at: usize,
    affinity: Affinity,
) -> Selections {
    let mut toggled = selections.clone();
    match selections
        .iter()
        .position(|s| s.start() <= at && at <= s.end())
    {
        Some(index) => toggled.remove(index),
        None => {
            toggled.add_with_affinity(Selection::caret(at), affinity)
        }
    }
    toggled
}

/// A column selection dragged from `from` to `to`, each an offset,
/// the affinity that names the visual row under it, and an x in
/// `layout`'s units: one selection per line between them, from the
/// boundary nearest `from`'s x to the one nearest `to`'s x, the line
/// of `to` holding the primary. The lines of `from` and `to` take
/// both x's on the row under that end, and the lines between take
/// them on their first row. Each head keeps the affinity of the row
/// it was found on.
///
/// # Panics
///
/// Panics if an offset is past the end of the text.
pub fn column(
    buffer: &Buffer,
    layout: &dyn Layout,
    from: (usize, Affinity, f32),
    to: (usize, Affinity, f32),
) -> Selections {
    let on = |line: u32| {
        let (row, affinity) = [from, to]
            .into_iter()
            .find(|&(at, ..)| buffer.line_of(at) == line)
            .map_or(
                (buffer.line_start(line), Affinity::Downstream),
                |(at, affinity, _)| (at, affinity),
            );
        let at = |x| {
            layout
                .vertical(buffer, row, affinity, 0, x)
                .unwrap_or((row, affinity))
        };
        let (anchor, _) = at(from.2);
        let (head, affinity) = at(to.2);
        (Selection { anchor, head }, affinity)
    };
    let (first, last) =
        (buffer.line_of(from.0), buffer.line_of(to.0));
    let step = if last < first { -1 } else { 1 };
    let (selection, affinity) = on(first);
    let mut selections =
        Selections::with_affinity(selection, affinity);
    let mut line = first;
    while line != last {
        line = line.wrapping_add_signed(step);
        let (selection, affinity) = on(line);
        selections.add_with_affinity(selection, affinity);
    }
    selections
}
