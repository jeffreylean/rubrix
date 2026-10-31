//! Commands that work on whole lines: indent, move, copy, cut and
//! paste of lines, and the per-line edits Tab, Ctrl+T and Ctrl+K
//! make.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use crate::buffer::Buffer;
use crate::command::Plan;
use crate::selection::Selections;

/// The lines the selections cover, as sorted half-open line ranges,
/// as Pierre's `getSelectedLineBlocks` finds them.
///
/// A range that ends at column 0 of a later line leaves that line
/// out. Blocks that overlap or touch merge into one.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the line commands use it")
)]
pub(crate) fn blocks(
    buffer: &Buffer,
    selections: &Selections,
) -> Vec<Range<u32>> {
    let mut blocks: Vec<Range<u32>> = Vec::new();
    for selection in selections.iter() {
        let start = buffer.line_of(selection.start());
        let mut end = buffer.line_of(selection.end());
        if !selection.is_caret()
            && end > start
            && buffer.line_start(end) == selection.end()
        {
            end -= 1;
        }
        let block = start..end + 1;
        match blocks.last_mut() {
            Some(last) if block.start <= last.end => {
                last.end = last.end.max(block.end);
            }
            _ => blocks.push(block),
        }
    }
    blocks
}

/// Tab: indents every line of a selection that spans lines, and puts
/// an indent at each caret or single-line range.
pub(crate) fn indent(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// Shift+Tab: removes one indent from the start of every selected
/// line.
pub(crate) fn outdent(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// Adds one indent to the start of every selected line, a caret's
/// line included.
pub(crate) fn indent_lines(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// Moves each block one line down, or up, past the line beside it.
/// `None` when a block is already at that end of the text.
pub(crate) fn move_lines(
    _buffer: &Buffer,
    _selections: &Selections,
    _down: bool,
) -> Option<Plan> {
    None
}

/// Copies each block below itself, or above, and selects the copy.
pub(crate) fn copy_lines(
    _buffer: &Buffer,
    _selections: &Selections,
    _down: bool,
) -> Option<Plan> {
    None
}

/// Opens a blank line below each caret's line, with the line's
/// indent, and moves the caret to its end.
pub(crate) fn blank_line_below(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// Swaps the grapheme clusters on either side of each caret.
pub(crate) fn transpose(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// Deletes from each caret to its line's end, or the line break when
/// the caret is already there.
pub(crate) fn delete_to_line_end(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// A paste of whole lines: when the selections are bare carets and
/// `text` is what [`line_copy`] copied, each copy goes in above its
/// caret's line. `None` leaves the paste to the plain path.
pub(crate) fn paste_lines(
    _buffer: &Buffer,
    _selections: &Selections,
    _text: &str,
) -> Option<Plan> {
    None
}

/// A cut of whole lines: when the selections are bare carets, the
/// cut removes each caret's line. `None` leaves the cut to the plain
/// path.
pub(crate) fn cut_lines(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Plan> {
    None
}

/// What a copy at bare carets copies: each caret's line with its line
/// break. `None` when some selection is a range.
pub(crate) fn line_copy(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<String> {
    None
}

/// Where Backspace at `at` deletes back to when `at` sits in leading
/// whitespace made of spaces: the previous indent stop. `None` leaves
/// it to delete one grapheme cluster.
pub(crate) fn soft_tab_start(
    _buffer: &Buffer,
    _at: usize,
) -> Option<usize> {
    None
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
