use alloc::vec::Vec;
use core::ops::Range;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::buffer::{Buffer, Spot};

/// Which row a head at a wrap point is on. Where a line wraps, one
/// offset ends a row and starts the next. `Upstream` keeps the head
/// at the end of the row before the break, as End leaves it, and
/// `Downstream` puts it at the start of the row after. Anywhere else
/// the two are the same place.
///
/// The row motions act on the row the affinity names, so Home on a
/// `Downstream` head at a wrap point stays on the row after it, and
/// End goes to that row's end. Pierre, which has no affinity, puts
/// such a head on the row before (`getSoftLineInfo`). rubrix does
/// not copy that.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affinity {
    #[default]
    Downstream,
    Upstream,
}

impl Affinity {
    /// The affinity of a head at `at` on `row`, a row of a line whose
    /// content ends at `line_end`. `Upstream` at the end of a row the
    /// line wraps after, and `Downstream` anywhere else.
    pub fn on_row(
        at: usize,
        row: &Range<usize>,
        line_end: usize,
    ) -> Affinity {
        if at == row.end && at < line_end {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        }
    }
}

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
    /// The first non-blank character of the head's row, or the row's
    /// start when the head is already there. Pierre's `textStart`,
    /// Cmd+Left on macOS.
    Home,
    /// The start of the head's row. Pierre's `start`, the Home key.
    LineStart,
    /// The end of the head's row, which is the wrap point with the
    /// head kept on that row, or the end of the line's content before
    /// any `\r\n`. Pierre's `end`.
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
    /// Where the motion takes a head at `from` with `affinity`, and
    /// the affinity it lands with. A vertical motion aims for `goal`,
    /// and sets it from [`Layout::x`] when unset.
    pub(crate) fn target(
        self,
        buffer: &Buffer,
        layout: &impl Layout,
        from: usize,
        affinity: Affinity,
        goal: &mut Option<f32>,
    ) -> (usize, Affinity) {
        use Affinity::Downstream;
        match self {
            Motion::Left => (prev_grapheme(buffer, from), Downstream),
            Motion::Right => {
                (next_grapheme(buffer, from), Downstream)
            }
            Motion::WordLeft => {
                (word_stop(buffer, from, false), Downstream)
            }
            Motion::WordRight => {
                (word_stop(buffer, from, true), Downstream)
            }
            Motion::Home => {
                let row = layout.row(buffer, from, affinity);
                let indent = row.start
                    + indent(&buffer.slice(row.clone())).len();
                let at =
                    if from == indent { row.start } else { indent };
                landing(buffer, &row, at)
            }
            Motion::LineStart => {
                (layout.row(buffer, from, affinity).start, Downstream)
            }
            Motion::End => {
                let row = layout.row(buffer, from, affinity);
                landing(buffer, &row, row.end)
            }
            Motion::DocStart => (0, Downstream),
            Motion::DocEnd => (buffer.len(), Downstream),
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
                let x = *goal.get_or_insert_with(|| {
                    layout.x(buffer, from, affinity)
                });
                let past = if rows < 0 { 0 } else { buffer.len() };
                layout
                    .vertical(buffer, from, affinity, rows, x)
                    .unwrap_or((past, Downstream))
            }
        }
    }
}

fn landing(
    buffer: &Buffer,
    row: &Range<usize>,
    at: usize,
) -> (usize, Affinity) {
    let line_end = buffer.line_end(buffer.line_of(at));
    (at, Affinity::on_row(at, row, line_end))
}

/// What the row motions need from a view. The view knows its rows,
/// wrapping and glyph widths. The editor keeps each selection's goal
/// x and affinity and decides what happens past the first and last
/// rows. A head is an offset with an [`Affinity`], which tells the
/// rows apart at a wrap point.
pub trait Layout {
    /// The x of a caret at `offset`, in the layout's own units, from
    /// the start of its row.
    fn x(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> f32;

    /// The grapheme boundary nearest `x` on the row `rows` rows below
    /// the row of `from`, or above it when `rows` is negative. An x
    /// past that row's end gives the row's end, `Upstream` when the
    /// line wraps there. `None` when the row would be above the first
    /// row or below the last.
    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        affinity: Affinity,
        rows: i32,
        x: f32,
    ) -> Option<(usize, Affinity)>;

    /// The offsets of the row that holds the head at `offset`, from
    /// its first byte to the wrap point after it, or to the end of
    /// the line's content on its last row. A line that does not wrap
    /// is one row.
    fn row(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> Range<usize>;

    /// The rows Page Up and Page Down move.
    fn page_rows(&self) -> u32;
}

/// A layout borrowed, so a `&dyn Layout` can go where
/// [`Editor::apply`](crate::Editor::apply) takes `&impl Layout`.
impl<L: Layout + ?Sized> Layout for &L {
    fn x(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> f32 {
        (**self).x(buffer, offset, affinity)
    }

    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        affinity: Affinity,
        rows: i32,
        x: f32,
    ) -> Option<(usize, Affinity)> {
        (**self).vertical(buffer, from, affinity, rows, x)
    }

    fn row(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> Range<usize> {
        (**self).row(buffer, offset, affinity)
    }

    fn page_rows(&self) -> u32 {
        (**self).page_rows()
    }
}

/// The unit a double click selects at `at` in `line`: the run of
/// word graphemes or of whitespace graphemes around the grapheme at
/// `at`, or that one grapheme when it is neither. At the end of the
/// line it takes the grapheme before. `line` is one line's content,
/// without its terminator, and `at` lies on a char boundary of it.
///
/// It classes graphemes as the word motions do, so a word a double
/// click selects is a word [`Motion::WordRight`] stops around. Unlike
/// the motions, a grapheme that is neither word nor whitespace stands
/// alone, so in `::` it selects one `:`.
pub fn word_at(line: &str, at: usize) -> Range<usize> {
    let grapheme = if at < line.len() {
        let mut cursor = GraphemeCursor::new(at, line.len(), true);
        let start = match cursor.is_boundary(line, 0) {
            Ok(true) => at,
            _ => boundary(line, at, false).unwrap_or(0),
        };
        boundary(line, start, true).map(|end| start..end)
    } else {
        boundary(line, at, false).map(|start| start..at)
    };
    let Some(grapheme) = grapheme else {
        return at..at;
    };
    let class = Class::of(&line[grapheme.clone()]);
    if class == Class::Other {
        return grapheme;
    }
    let (mut start, mut end) = (grapheme.start, grapheme.end);
    while let Some(before) = boundary(line, start, false)
        && Class::of(&line[before..start]) == class
    {
        start = before;
    }
    while let Some(after) = boundary(line, end, true)
        && Class::of(&line[end..after]) == class
    {
        end = after;
    }
    start..end
}

/// The grapheme clusters of `text`, each with its byte offset, as
/// the editor steps over them.
pub fn graphemes(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.grapheme_indices(true)
}

/// A fixed-width layout. One column per grapheme cluster, a tab to
/// the next multiple of `tab` columns, and one row per line unless
/// `columns` is set, when a line wraps as [`Wrapped::wrap`] breaks
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monospace {
    pub tab: u32,
    pub page_rows: u32,
    pub columns: Option<u32>,
}

impl Default for Monospace {
    fn default() -> Monospace {
        Monospace {
            tab: 4,
            page_rows: 40,
            columns: None,
        }
    }
}

impl Monospace {
    fn column(&self, content: &str, rel: usize) -> u32 {
        content[..rel].graphemes(true).fold(0, |column, grapheme| {
            advance(column, grapheme, self.tab)
        })
    }

    fn rows(&self, content: &str) -> Wrapped {
        Wrapped::wrap(
            content,
            self.columns.map(|columns| columns as usize),
            self.tab,
        )
    }
}

impl Layout for Monospace {
    fn x(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> f32 {
        let Spot { content, rel, .. } = buffer.spot(offset);
        let rows = self.rows(content);
        let row = rows.range(rows.index(rel, affinity));
        (self.column(content, rel) - self.column(content, row.start))
            as f32
    }

    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        affinity: Affinity,
        rows: i32,
        x: f32,
    ) -> Option<(usize, Affinity)> {
        let spot = buffer.spot(from);
        let mut line = spot.line;
        let mut wrapped = self.rows(spot.content);
        let mut index = wrapped.index(spot.rel, affinity) as i64
            + i64::from(rows);
        loop {
            if index < 0 {
                line = line.checked_sub(1)?;
                wrapped = self.rows(buffer.content(line));
                index += wrapped.count() as i64;
            } else if index >= wrapped.count() as i64 {
                index -= wrapped.count() as i64;
                line += 1;
                if line >= buffer.line_count() {
                    return None;
                }
                wrapped = self.rows(buffer.content(line));
            } else {
                break;
            }
        }
        let content = buffer.content(line);
        let row = wrapped.range(index as usize);
        let first = self.column(content, row.start);
        let width = self.column(content, row.end) - first;
        let x = x.clamp(0.0, width as f32);
        let (mut best, mut nearest) = (row.start, x);
        let mut column = first;
        for (i, grapheme) in
            content[row.clone()].grapheme_indices(true)
        {
            column = advance(column, grapheme, self.tab);
            let distance = ((column - first) as f32 - x).abs();
            if distance < nearest {
                (best, nearest) =
                    (row.start + i + grapheme.len(), distance);
            } else if (column - first) as f32 > x {
                break;
            }
        }
        let start = buffer.line_start(line);
        let row = start + row.start..start + row.end;
        Some(landing(buffer, &row, start + best))
    }

    fn row(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> Range<usize> {
        let Spot {
            start,
            content,
            rel,
            ..
        } = buffer.spot(offset);
        let rows = self.rows(content);
        let row = rows.range(rows.index(rel, affinity));
        start + row.start..start + row.end
    }

    fn page_rows(&self) -> u32 {
        self.page_rows
    }
}

fn advance(column: u32, grapheme: &str, tab: u32) -> u32 {
    if grapheme == "\t" {
        let tab = tab.max(1);
        (column / tab + 1) * tab
    } else {
        column + 1
    }
}

/// The visual rows one line's text breaks into, as the byte offset
/// each starts at. The first starts at 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wrapped {
    starts: Vec<usize>,
    len: usize,
}

impl Wrapped {
    /// `text` broken at `columns`, greedily before the word that
    /// would pass the edge, or inside a word wider than a row. One
    /// row with `None`. A tab advances to the next multiple of `tab`
    /// columns. Whitespace hangs past the edge, so a row after the
    /// first never starts with it.
    pub fn wrap(
        text: &str,
        columns: Option<usize>,
        tab: u32,
    ) -> Wrapped {
        let mut starts = Vec::from([0]);
        let Some(columns) = columns else {
            return Wrapped {
                starts,
                len: text.len(),
            };
        };
        let (mut column, mut first) = (0, 0);
        let mut word: Option<(usize, u32)> = None;
        for (byte, grapheme) in graphemes(text) {
            let space = grapheme.chars().all(char::is_whitespace);
            if !space && (column - first) as usize >= columns {
                let row = *starts.last().expect("a first row");
                let (at, at_column) = word
                    .filter(|&(at, _)| at > row)
                    .unwrap_or((byte, column));
                starts.push(at);
                first = at_column;
            }
            if space {
                word = None;
            } else if word.is_none() {
                word = Some((byte, column));
            }
            column = advance(column, grapheme, tab);
        }
        Wrapped {
            starts,
            len: text.len(),
        }
    }

    /// The number of rows, at least 1.
    pub fn count(&self) -> usize {
        self.starts.len()
    }

    /// The row that holds the head at `at`, which is at most the
    /// text's length.
    pub fn index(&self, at: usize, affinity: Affinity) -> usize {
        let after = match affinity {
            Affinity::Downstream => {
                self.starts.partition_point(|&start| start <= at)
            }
            Affinity::Upstream => {
                self.starts.partition_point(|&start| start < at)
            }
        };
        after.saturating_sub(1)
    }

    /// The bytes of row `index`, to the next row's start or the
    /// text's end.
    pub fn range(&self, index: usize) -> Range<usize> {
        let end =
            self.starts.get(index + 1).map_or(self.len, |&next| next);
        self.starts[index]..end
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

#[cfg(feature = "find")]
pub(crate) fn is_word(grapheme: &str) -> bool {
    Class::of(grapheme) == Class::Word
}
