use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::buffer::{Buffer, Spot};

/// Where a motion moves each selection's head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// One grapheme cluster. A `\r\n` is one cluster, so the caret
    /// steps over both bytes. At a line's start Left goes to the end
    /// of the line above, and at a line's end Right goes to the start
    /// of the line below.
    Left,
    Right,
    /// Past any whitespace, then past one run of word characters
    /// (alphanumeric or `_`) or one run of other characters, within
    /// the line. At a line's edge it steps to the next line, as Left
    /// and Right do.
    WordLeft,
    WordRight,
    /// The first non-blank character of the line, or column 0 when
    /// the head is already there.
    Home,
    /// The end of the line's content, before any `\r\n`.
    End,
    /// One row, through [`Layout`], aiming for the goal x. Up on the
    /// first row goes to the start of the text, and Down on the last
    /// row to its end.
    Up,
    Down,
    /// [`Layout::page_rows`] rows, through [`Layout`].
    PageUp,
    PageDown,
    DocStart,
    DocEnd,
}

impl Motion {
    /// Where the motion takes a head at `from`. A vertical motion
    /// aims for `goal`, and sets it from [`Layout::x`] when unset.
    pub(crate) fn target(
        self,
        buffer: &Buffer,
        layout: &impl Layout,
        from: usize,
        goal: &mut Option<f32>,
    ) -> usize {
        match self {
            Motion::Left => prev_grapheme(buffer, from),
            Motion::Right => next_grapheme(buffer, from),
            Motion::WordLeft => word_stop(buffer, from, false),
            Motion::WordRight => word_stop(buffer, from, true),
            Motion::Home => {
                let Spot {
                    start,
                    content,
                    rel,
                    ..
                } = buffer.spot(from);
                let indent = indent(content).len();
                start + if rel == indent { 0 } else { indent }
            }
            Motion::End => {
                let Spot { start, content, .. } = buffer.spot(from);
                start + content.len()
            }
            Motion::DocStart => 0,
            Motion::DocEnd => buffer.len(),
            Motion::Up
            | Motion::Down
            | Motion::PageUp
            | Motion::PageDown => {
                let page = i32::try_from(layout.page_rows())
                    .unwrap_or(i32::MAX);
                let rows = match self {
                    Motion::Up => -1,
                    Motion::Down => 1,
                    Motion::PageUp => -page,
                    _ => page,
                };
                let x = *goal
                    .get_or_insert_with(|| layout.x(buffer, from));
                let past = if rows < 0 { 0 } else { buffer.len() };
                layout.vertical(buffer, from, rows, x).unwrap_or(past)
            }
        }
    }
}

/// What vertical motion needs from a view. The view knows its rows,
/// wrapping and glyph widths. The editor keeps each selection's goal
/// x and decides what happens past the first and last rows.
pub trait Layout {
    /// The x of a caret at `offset`, in the layout's own units, from
    /// the start of its row.
    fn x(&self, buffer: &Buffer, offset: usize) -> f32;

    /// The grapheme boundary nearest `x` on the row `rows` rows below
    /// the row of `from`, or above it when `rows` is negative. An x
    /// past that row's end gives the row's end. `None` when the row
    /// would be above the first row or below the last.
    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        rows: i32,
        x: f32,
    ) -> Option<usize>;

    /// The rows Page Up and Page Down move.
    fn page_rows(&self) -> u32;
}

/// A fixed-width layout for headless tests, the replay and the bench.
/// One row per line, one column per grapheme cluster, and a tab to
/// the next multiple of `tab` columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monospace {
    pub tab: u32,
    pub page_rows: u32,
}

impl Default for Monospace {
    fn default() -> Monospace {
        Monospace {
            tab: 4,
            page_rows: 40,
        }
    }
}

impl Monospace {
    /// The column after `grapheme`, which starts at `column`.
    fn advance(&self, column: u32, grapheme: &str) -> u32 {
        if grapheme == "\t" {
            let tab = self.tab.max(1);
            (column / tab + 1) * tab
        } else {
            column + 1
        }
    }
}

impl Layout for Monospace {
    fn x(&self, buffer: &Buffer, offset: usize) -> f32 {
        let spot = buffer.spot(offset);
        let before = &spot.content[..spot.rel];
        before.graphemes(true).fold(0, |column, grapheme| {
            self.advance(column, grapheme)
        }) as f32
    }

    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        rows: i32,
        x: f32,
    ) -> Option<usize> {
        let line = i64::from(buffer.line_of(from)) + i64::from(rows);
        let line = u32::try_from(line)
            .ok()
            .filter(|&line| line < buffer.line_count())?;
        let (mut best, mut nearest) = (0, x.abs());
        let mut column = 0;
        for (i, grapheme) in
            buffer.content(line).grapheme_indices(true)
        {
            column = self.advance(column, grapheme);
            let distance = (column as f32 - x).abs();
            if distance < nearest {
                (best, nearest) = (i + grapheme.len(), distance);
            } else if column as f32 > x {
                break;
            }
        }
        Some(buffer.line_start(line) + best)
    }

    fn page_rows(&self) -> u32 {
        self.page_rows
    }
}

/// The next grapheme boundary after `at`, or `at` at the end of the
/// text.
pub(crate) fn next_grapheme(buffer: &Buffer, at: usize) -> usize {
    let Spot {
        line,
        start,
        content,
        rel,
    } = buffer.spot(at);
    match boundary(content, rel, true) {
        Some(rel) => start + rel,
        None if line + 1 < buffer.line_count() => {
            buffer.line_start(line + 1)
        }
        None => start + rel,
    }
}

/// The grapheme boundary before `at`, or 0 at the start of the text.
pub(crate) fn prev_grapheme(buffer: &Buffer, at: usize) -> usize {
    let Spot {
        line,
        start,
        content,
        rel,
    } = buffer.spot(at);
    match boundary(content, rel, false) {
        Some(rel) => start + rel,
        None if line > 0 => buffer.line_end(line - 1),
        None => 0,
    }
}

/// Where a word motion from `at` stops, forward or back.
pub(crate) fn word_stop(
    buffer: &Buffer,
    at: usize,
    forward: bool,
) -> usize {
    let Spot {
        start,
        content,
        mut rel,
        ..
    } = buffer.spot(at);
    if rel == if forward { content.len() } else { 0 } {
        return if forward {
            next_grapheme(buffer, at)
        } else {
            prev_grapheme(buffer, at)
        };
    }
    let mut run = Class::Space;
    while let Some(next) = boundary(content, rel, forward) {
        let class = Class::of(&content[rel.min(next)..rel.max(next)]);
        if run != Class::Space && class != run {
            break;
        }
        (run, rel) = (class, next);
    }
    start + rel
}

/// `at` clamped to the text and moved back to a grapheme boundary.
pub(crate) fn snap(buffer: &Buffer, at: usize) -> usize {
    let Spot {
        start,
        content,
        rel,
        ..
    } = buffer.spot(at.min(buffer.len()));
    let rel = content.floor_char_boundary(rel);
    let mut cursor = GraphemeCursor::new(rel, content.len(), true);
    match cursor.is_boundary(content, 0) {
        Ok(true) => start + rel,
        _ => start + boundary(content, rel, false).unwrap_or(0),
    }
}

/// The spaces and tabs that start `text`.
pub(crate) fn indent(text: &str) -> &str {
    let rest = text.trim_start_matches([' ', '\t']);
    &text[..text.len() - rest.len()]
}

/// The grapheme boundary of `content` after or before `at`, or
/// `None` at its end or start.
fn boundary(
    content: &str,
    at: usize,
    forward: bool,
) -> Option<usize> {
    let mut cursor = GraphemeCursor::new(at, content.len(), true);
    let found = if forward {
        cursor.next_boundary(content, 0)
    } else {
        cursor.prev_boundary(content, 0)
    };
    found.ok().flatten()
}

/// What a word motion steps over: whitespace, word characters, or
/// anything else.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Other,
}

impl Class {
    fn of(grapheme: &str) -> Class {
        if grapheme.chars().all(char::is_whitespace) {
            Class::Space
        } else if grapheme
            .chars()
            .any(|c| c.is_alphanumeric() || c == '_')
        {
            Class::Word
        } else {
            Class::Other
        }
    }
}
