//! Selections added by pointer and by search: add next occurrence,
//! Alt+click carets and column selection.

use crate::buffer::Buffer;
use crate::motion::Layout;
use crate::selection::{Selection, Selections};

/// Mod+D: selects the word at the primary when it is a caret, or adds
/// the next occurrence of the primary's text after it, wrapping at
/// the end of the text, as the new primary. `None` when there is
/// nothing to add.
pub(crate) fn add_next(
    _buffer: &Buffer,
    _selections: &Selections,
) -> Option<Selections> {
    None
}

/// Alt+click at `at`: removes the selection that holds `at`, unless
/// it is the only one, or else adds a caret at `at` as the new
/// primary.
pub fn toggle_caret(
    selections: &Selections,
    _at: usize,
) -> Selections {
    selections.clone()
}

/// A column selection dragged from `from` to `to`, each a line and an
/// x in `layout`'s units: one selection per line between them, from
/// the boundary nearest `from`'s x to the one nearest `to`'s x, the
/// line of `to` holding the primary.
///
/// # Panics
///
/// Panics if a line is not a line of the text.
pub fn column(
    buffer: &Buffer,
    _layout: &dyn Layout,
    from: (u32, f32),
    _to: (u32, f32),
) -> Selections {
    Selections::new(Selection::caret(buffer.line_start(from.0)))
}
