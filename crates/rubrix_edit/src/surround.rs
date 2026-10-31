//! Typing an opening bracket or quote over a selection wraps it.

use alloc::string::String;
use alloc::vec::Vec;

use crate::buffer::Buffer;
use crate::command::{After, Plan};
use crate::selection::Selections;

/// What typing each opener over a selection puts around it. Pierre
/// also wraps in `<` and `>`, which the plan leaves out.
const PAIRS: [(&str, &str); 6] = [
    ("(", ")"),
    ("[", "]"),
    ("{", "}"),
    ("\"", "\""),
    ("'", "'"),
    ("`", "`"),
];

/// When `text` is an opening bracket or a quote and every selection
/// is a range, wraps each range in the pair and keeps it selected
/// inside it. `None` leaves `text` to be typed.
pub(crate) fn wrap(
    _buffer: &Buffer,
    selections: &Selections,
    text: &str,
) -> Option<Plan> {
    let (open, close) =
        PAIRS.into_iter().find(|(open, _)| *open == text)?;
    if selections.iter().any(|selection| selection.is_caret()) {
        return None;
    }
    let mut edits = Vec::with_capacity(2 * selections.len());
    for selection in selections.iter() {
        let (start, end) = (selection.start(), selection.end());
        edits.push((start..start, String::from(open)));
        edits.push((end..end, String::from(close)));
    }
    Some(Plan {
        edits,
        after: After::Map,
    })
}
