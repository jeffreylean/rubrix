//! Commands that work on whole lines: indent, move, copy, cut and
//! paste of lines, and the per-line edits Tab, Ctrl+T and Ctrl+K
//! make.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use crate::buffer::Buffer;
use crate::change::Move;
use crate::command::{After, Plan};
use crate::motion::{
    indent as leading_blanks, next_grapheme, prev_grapheme,
};
use crate::selection::{Selection, Selections};

/// The columns between tab stops: where Tab and Backspace stop in
/// leading spaces, and where a view draws a tab to.
pub const TAB: usize = 4;

/// The lines the selections cover, as sorted half-open line ranges,
/// as Pierre's `getSelectedLineBlocks` finds them.
///
/// A range that ends at column 0 of a later line leaves that line
/// out. Blocks that overlap or touch merge into one.
pub(crate) fn blocks(
    buffer: &Buffer,
    selections: &Selections,
) -> Vec<Range<u32>> {
    let mut blocks: Vec<Range<u32>> = Vec::new();
    for selection in selections.iter() {
        let block = selected_lines(buffer, selection);
        match blocks.last_mut() {
            Some(last) if block.start <= last.end => {
                last.end = last.end.max(block.end);
            }
            _ => blocks.push(block),
        }
    }
    blocks
}

/// The lines one selection covers, under the rule in [`blocks`].
pub(crate) fn selected_lines(
    buffer: &Buffer,
    selection: Selection,
) -> Range<u32> {
    let start = buffer.line_of(selection.start());
    let mut end = buffer.line_of(selection.end());
    if !selection.is_caret()
        && end > start
        && buffer.line_start(end) == selection.end()
    {
        end -= 1;
    }
    start..end + 1
}

/// Tab: indents every line of a selection that spans lines, and puts
/// an indent at each caret or single-line range.
pub(crate) fn indent(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Plan> {
    let mut edits = Vec::new();
    let mut done = None;
    for selection in selections.iter() {
        let start = buffer.spot(selection.start());
        if start.line != buffer.line_of(selection.end()) {
            line_indents(
                buffer, selection, false, &mut done, &mut edits,
            );
            continue;
        }
        let text = if start.content.starts_with('\t') {
            String::from("\t")
        } else {
            let column = start.content[..start.rel].chars().fold(
                0,
                |column, c| {
                    if c == '\t' {
                        column + TAB - column % TAB
                    } else {
                        column + 1
                    }
                },
            );
            " ".repeat(TAB - column % TAB)
        };
        edits.push((selection.range(), text));
    }
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    Some(Plan {
        edits,
        after: After::Map,
    })
}

/// Shift+Tab: removes one indent from the start of every selected
/// line.
pub(crate) fn outdent(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Plan> {
    each_line_indent(buffer, selections, true)
}

/// Adds one indent to the start of every selected line, a caret's
/// line included.
pub(crate) fn indent_lines(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Plan> {
    each_line_indent(buffer, selections, false)
}

fn each_line_indent(
    buffer: &Buffer,
    selections: &Selections,
    outdent: bool,
) -> Option<Plan> {
    let mut edits = Vec::new();
    let mut done = None;
    for selection in selections.iter() {
        line_indents(
            buffer, selection, outdent, &mut done, &mut edits,
        );
    }
    Some(Plan {
        edits,
        after: After::Map,
    })
}

/// Pushes the edits that add or remove one indent at the start of
/// each line `selection` covers, as Pierre's `resolveIndentEdits`
/// does: a tab when the line starts with one, else four spaces. A
/// selection over several lines skips its blank lines. `done` is the
/// last line an earlier selection edited, so a line shared by two
/// selections changes once.
fn line_indents(
    buffer: &Buffer,
    selection: Selection,
    outdent: bool,
    done: &mut Option<u32>,
    edits: &mut Vec<(Range<usize>, String)>,
) {
    let several = buffer.line_of(selection.start())
        != buffer.line_of(selection.end());
    for line in selected_lines(buffer, selection) {
        if done.is_some_and(|done| line <= done) {
            continue;
        }
        *done = Some(line);
        let content = buffer.content(line);
        if several && content.trim().is_empty() {
            continue;
        }
        let at = buffer.line_start(line);
        let tab = content.starts_with('\t');
        if !outdent {
            let unit = if tab { "\t" } else { "    " };
            edits.push((at..at, String::from(unit)));
            continue;
        }
        let spaces =
            content.len() - content.trim_start_matches(' ').len();
        let removed = if tab { 1 } else { spaces.min(TAB) };
        if removed > 0 {
            edits.push((at..at + removed, String::new()));
        }
    }
}

/// Moves each block one line down, or up, past the line beside it.
/// `None` when a block is already at that end of the text.
///
/// Each block stays where it is and the line beside it moves to its
/// other side, so the edit is the size of one line however long the
/// block is. The moves name the text of the block and of the line
/// beside it, so places in both go with their lines.
pub(crate) fn move_lines(
    buffer: &Buffer,
    selections: &Selections,
    down: bool,
) -> Option<(Plan, Vec<Move>)> {
    let blocks = blocks(buffer, selections);
    let at_edge = if down {
        blocks.last()?.end == buffer.line_count()
    } else {
        blocks.first()?.start == 0
    };
    if at_edge {
        return None;
    }
    let mut edits = Vec::with_capacity(2 * blocks.len());
    let mut moves = Vec::with_capacity(2 * blocks.len());
    let mut shifts = Vec::with_capacity(blocks.len());
    for block in &blocks {
        let first = buffer.line_start(block.start);
        let last = buffer.line_end(block.end - 1);
        if down {
            let next = block.end;
            let (start, end) =
                (buffer.line_start(next), buffer.line_end(next));
            let moved = buffer.slice(last..end);
            let text = rotate(&moved, start - last);
            shifts.push(text.len() as isize);
            moves.push(Move {
                from: first..last,
                to: first + text.len(),
            });
            moves.push(Move {
                from: start..end,
                to: first,
            });
            edits.push((first..first, text));
            edits.push((last..end, String::new()));
        } else {
            let prev = block.start - 1;
            let (start, end) =
                (buffer.line_start(prev), buffer.line_end(prev));
            let moved = buffer.slice(start..first);
            let text = rotate(&moved, end - start);
            shifts.push(-(text.len() as isize));
            moves.push(Move {
                from: start..end,
                to: last - (end - start),
            });
            moves.push(Move {
                from: first..last,
                to: start,
            });
            edits.push((start..first, String::new()));
            edits.push((last..last, text));
        }
    }
    let after =
        shifted(buffer, selections, &blocks, &shifts, buffer.len());
    Some((
        Plan {
            edits,
            after: After::Set(after),
        },
        moves,
    ))
}

/// `text` with its first `mid` bytes moved to its end.
fn rotate(text: &str, mid: usize) -> String {
    [&text[mid..], &text[..mid]].concat()
}

/// Copies each block below itself, or above, and selects the copy.
///
/// Both put the copy in above the block, so the text is the same.
/// Down leaves the selections on the lower lines.
pub(crate) fn copy_lines(
    buffer: &Buffer,
    selections: &Selections,
    down: bool,
) -> Option<Plan> {
    let blocks = blocks(buffer, selections);
    let eol = buffer.eol().as_str();
    let mut edits = Vec::with_capacity(blocks.len());
    let mut shifts = Vec::with_capacity(blocks.len());
    let mut copied = 0;
    for block in &blocks {
        let first = buffer.line_start(block.start);
        let lines =
            buffer.slice(first..buffer.line_end(block.end - 1));
        let text = [&*lines, eol].concat();
        let own = if down { text.len() } else { 0 };
        shifts.push((copied + own) as isize);
        copied += text.len();
        edits.push((first..first, text));
    }
    let after = shifted(
        buffer,
        selections,
        &blocks,
        &shifts,
        buffer.len() + copied,
    );
    Some(Plan {
        edits,
        after: After::Set(after),
    })
}

/// The selections, each moved by the shift of the block that holds
/// it and clamped to `len`, the length of the text after the edit.
fn shifted(
    buffer: &Buffer,
    selections: &Selections,
    blocks: &[Range<u32>],
    shifts: &[isize],
    len: usize,
) -> Selections {
    let mut after = selections.clone();
    let mut block = 0;
    after.update(|selection, _| {
        let line = buffer.line_of(selection.start());
        while blocks[block].end <= line {
            block += 1;
        }
        let shift = shifts[block];
        let at = |offset: usize| {
            offset.saturating_add_signed(shift).min(len)
        };
        Selection {
            anchor: at(selection.anchor),
            head: at(selection.head),
        }
    });
    after
}

/// Opens a blank line below each caret's line, with the line's
/// indent, and moves the caret to its end.
pub(crate) fn blank_line_below(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Plan> {
    let eol = buffer.eol().as_str();
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut opened: Option<(u32, usize)> = None;
    let mut added = 0;
    let mut after = selections.clone();
    after.update(|selection, _| {
        let line = buffer.line_of(selection.head);
        if let Some((done, caret)) = opened
            && done == line
        {
            return Selection::caret(caret);
        }
        let end = buffer.line_end(line);
        let text =
            [eol, leading_blanks(buffer.content(line))].concat();
        added += text.len();
        let caret = end + added;
        edits.push((end..end, text));
        opened = Some((line, caret));
        Selection::caret(caret)
    });
    Some(Plan {
        edits,
        after: After::Set(after),
    })
}

/// Swaps the grapheme clusters on either side of each caret, as
/// Pierre's `applyTransposeToSelections` does. At a line's end it
/// swaps the two before the caret, and at a line's start the last of
/// the line above and the first of this one, across the break.
/// Ranges stay as they are, and so does a caret whose swap would
/// overlap the one before it.
pub(crate) fn transpose(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Plan> {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut after = selections.clone();
    after.update(|selection, _| {
        if !selection.is_caret() {
            return selection;
        }
        let Some([start, left, right, end]) =
            swapped(buffer, selection.head)
        else {
            return selection;
        };
        if edits.last().is_some_and(|(last, _)| start < last.end) {
            return selection;
        }
        let text = [
            buffer.slice(right..end),
            buffer.slice(left..right),
            buffer.slice(start..left),
        ]
        .concat();
        edits.push((start..end, text));
        Selection::caret(end)
    });
    Some(Plan {
        edits,
        after: After::Set(after),
    })
}

/// The offsets `[start, left, right, end]` a transpose at `at`
/// reverses: `start..left` and `right..end` trade places around
/// `left..right`, which is the line break when the swap crosses one
/// and empty otherwise. `None` when there is nothing to swap.
fn swapped(buffer: &Buffer, at: usize) -> Option<[usize; 4]> {
    let spot = buffer.spot(at);
    let len = spot.content.len();
    if spot.rel > 0 && spot.rel < len {
        Some([
            prev_grapheme(buffer, at),
            at,
            at,
            next_grapheme(buffer, at),
        ])
    } else if spot.rel == len && spot.rel > 0 {
        let mid = prev_grapheme(buffer, at);
        (mid > spot.start)
            .then(|| [prev_grapheme(buffer, mid), mid, mid, at])
    } else if spot.line > 0 && len > 0 {
        let above = spot.line - 1;
        let left = buffer.line_end(above);
        let start = if left > buffer.line_start(above) {
            prev_grapheme(buffer, left)
        } else {
            left
        };
        Some([start, left, at, next_grapheme(buffer, at)])
    } else {
        None
    }
}

/// Deletes from each caret to its line's end, or the line break when
/// the caret is already there.
pub(crate) fn delete_to_line_end(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Plan> {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for selection in selections.iter() {
        let range = if selection.is_caret() {
            let spot = buffer.spot(selection.head);
            let end = if spot.rel < spot.content.len() {
                spot.start + spot.content.len()
            } else if spot.line + 1 < buffer.line_count() {
                buffer.line_start(spot.line + 1)
            } else {
                selection.head
            };
            selection.head..end
        } else {
            selection.range()
        };
        match edits.last_mut() {
            Some((last, _)) if range.start < last.end => {
                last.end = last.end.max(range.end);
            }
            _ => edits.push((range, String::new())),
        }
    }
    Some(Plan {
        edits,
        after: After::Map,
    })
}

/// A paste of one line per selection: when there are several
/// selections and `text`, less one trailing line break, has as many
/// lines, line `i` replaces selection `i`. `None` leaves the paste to
/// the plain path, which puts the whole text at every selection.
pub(crate) fn paste_lines(
    buffer: &Buffer,
    selections: &Selections,
    text: &str,
) -> Option<Plan> {
    if selections.len() < 2 {
        return None;
    }
    let eol = buffer.eol().as_str();
    let text = text.strip_suffix(eol).unwrap_or(text);
    if text.split(eol).count() != selections.len() {
        return None;
    }
    let edits = selections
        .iter()
        .zip(text.split(eol))
        .map(|(selection, line)| {
            (selection.range(), String::from(line))
        })
        .collect();
    Some(Plan {
        edits,
        after: After::Map,
    })
}

fn line_with_break_after(buffer: &Buffer, line: u32) -> Range<usize> {
    let end = if line + 1 < buffer.line_count() {
        buffer.line_start(line + 1)
    } else {
        buffer.len()
    };
    buffer.line_start(line)..end
}

fn line_with_a_break_unless_alone(
    buffer: &Buffer,
    line: u32,
) -> Range<usize> {
    if line + 1 < buffer.line_count() {
        line_with_break_after(buffer, line)
    } else if line > 0 {
        buffer.line_end(line - 1)..buffer.len()
    } else {
        0..buffer.len()
    }
}

pub(crate) fn cut(buffer: &Buffer, selections: &Selections) -> Plan {
    let caret = line_with_a_break_unless_alone;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for region in regions(buffer, selections, caret) {
        match edits.last_mut() {
            _ if region.is_empty() => {}
            Some((last, _)) if region.start <= last.end => {
                last.end = last.end.max(region.end);
            }
            _ => edits.push((region, String::new())),
        }
    }
    Plan {
        edits,
        after: After::Map,
    }
}

pub(crate) fn copy(
    buffer: &Buffer,
    selections: &Selections,
) -> String {
    let mut text = String::new();
    let mut copied: Option<usize> = None;
    for region in regions(buffer, selections, line_with_break_after) {
        match copied {
            _ if region.is_empty() => {}
            Some(end) if region.start <= end => {
                if region.end > end {
                    text.push_str(&buffer.slice(end..region.end));
                    copied = Some(region.end);
                }
            }
            _ => {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push_str(buffer.eol().as_str());
                }
                text.push_str(&buffer.slice(region.clone()));
                copied = Some(region.end);
            }
        }
    }
    text
}

fn regions(
    buffer: &Buffer,
    selections: &Selections,
    caret: fn(&Buffer, u32) -> Range<usize>,
) -> Vec<Range<usize>> {
    let mut regions: Vec<Range<usize>> = selections
        .iter()
        .map(|selection| {
            if selection.is_caret() {
                caret(buffer, buffer.line_of(selection.head))
            } else {
                selection.range()
            }
        })
        .collect();
    regions.sort_unstable_by_key(|region| (region.start, region.end));
    regions
}

/// Where Backspace at `at` deletes back to when only spaces, at
/// least one, sit between the line's start and `at`: the indent stop
/// before `at`. `None` leaves it to delete one grapheme cluster.
pub(crate) fn soft_tab_start(
    buffer: &Buffer,
    at: usize,
) -> Option<usize> {
    let spot = buffer.spot(at);
    let before = &spot.content[..spot.rel];
    (!before.is_empty() && before.bytes().all(|b| b == b' '))
        .then(|| spot.start + (before.len() - 1) / TAB * TAB)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::blocks;
    use crate::buffer::Buffer;
    use crate::selection::{Selection, Selections};

    /// The blocks, as `(start, end)`, of selections given as
    /// `(anchor, head)` in a text whose lines are 3 bytes and a `\n`,
    /// so line `n` starts at `4 * n`.
    fn lines_of(selections: &[(usize, usize)]) -> Vec<(u32, u32)> {
        let buffer = Buffer::new("aaa\nbbb\nccc\nddd\neee\nfff");
        let mut all = Selections::new(Selection {
            anchor: selections[0].0,
            head: selections[0].1,
        });
        for &(anchor, head) in &selections[1..] {
            all.add(Selection { anchor, head });
        }
        blocks(&buffer, &all)
            .into_iter()
            .map(|block| (block.start, block.end))
            .collect()
    }

    #[test]
    fn a_caret_covers_its_line() {
        assert_eq!(lines_of(&[(5, 5)]), [(1, 2)]);
        assert_eq!(lines_of(&[(4, 4)]), [(1, 2)]);
    }

    #[test]
    fn a_range_ending_at_a_line_start_leaves_that_line_out() {
        assert_eq!(lines_of(&[(1, 8)]), [(0, 2)]);
        assert_eq!(lines_of(&[(8, 1)]), [(0, 2)]);
        assert_eq!(lines_of(&[(1, 9)]), [(0, 3)]);
        assert_eq!(lines_of(&[(4, 8)]), [(1, 2)]);
    }

    #[test]
    fn touching_and_overlapping_blocks_merge() {
        assert_eq!(lines_of(&[(1, 1), (5, 5)]), [(0, 2)]);
        assert_eq!(lines_of(&[(1, 1), (9, 9)]), [(0, 1), (2, 3)]);
        assert_eq!(lines_of(&[(1, 2), (6, 9)]), [(0, 3)]);
        assert_eq!(lines_of(&[(1, 5), (6, 9)]), [(0, 3)]);
        assert_eq!(
            lines_of(&[(20, 21), (1, 2), (13, 13)]),
            [(0, 1), (3, 4), (5, 6)]
        );
    }
}
