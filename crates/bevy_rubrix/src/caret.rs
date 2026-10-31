//! Where an editable view's rows put each byte, and the carets and
//! selection rects drawn from it.

/// A point resolved to the text: its line, its byte offset in the
/// buffer, and its x in logical pixels from the start of its visual
/// line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub line: u32,
    pub offset: usize,
    pub x: f32,
}
