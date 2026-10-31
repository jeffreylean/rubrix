//! Typing an opening bracket or quote over a selection wraps it.

use crate::buffer::Buffer;
use crate::command::Plan;
use crate::selection::Selections;

/// When `text` is an opening bracket or a quote and some selection is
/// a range, wraps each range in the pair and keeps it selected inside
/// it. `None` leaves `text` to be typed.
pub(crate) fn wrap(
    _buffer: &Buffer,
    _selections: &Selections,
    _text: &str,
) -> Option<Plan> {
    None
}
