use alloc::borrow::Cow;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use core::ops::Range;

use rubrix_highlight::Lines;

use crate::change::{Change, Rep};

/// The most lines a chunk holds. A chunk that grows past it splits
/// into chunks of at most half of it, and a chunk left with no line
/// is dropped.
const MAX_LINES: usize = 512;

/// The line ending a buffer gives new line breaks: the ending of the
/// text's first line break, or [`Eol::Lf`] when it has none.
///
/// The buffer itself never rewrites text. [`Editor`](crate::Editor)
/// normalizes typed and pasted text to it, and Enter inserts it, so
/// the lines a file came with keep their own endings and the file
/// round-trips byte for byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
        }
    }

    /// `text` with every `\r\n`, lone `\n` and lone `\r` written as
    /// this ending. Borrows when nothing changes.
    pub fn normalize(self, text: &str) -> Cow<'_, str> {
        let eol = self.as_str();
        let mut out: Option<String> = None;
        let (mut copied, mut at) = (0, 0);
        while let Some(found) = text[at..].find(['\r', '\n']) {
            let i = at + found;
            let len =
                if text[i..].starts_with("\r\n") { 2 } else { 1 };
            if &text[i..i + len] != eol {
                let out = out.get_or_insert_with(String::new);
                out.push_str(&text[copied..i]);
                out.push_str(eol);
                copied = i + len;
            }
            at = i + len;
        }
        match out {
            None => Cow::Borrowed(text),
            Some(mut out) => {
                out.push_str(&text[copied..]);
                Cow::Owned(out)
            }
        }
    }
}

/// Lines that share one `Arc`, so a snapshot copies a pointer per
/// chunk and the first write after it copies the chunk.
#[derive(Clone, Debug)]
struct Chunk {
    /// Each line without its `\n`. A `\r` before the `\n` stays.
    lines: Vec<String>,
    /// The sum of `len() + 1` over `lines`.
    bytes: usize,
}

impl Chunk {
    fn new(lines: Vec<String>) -> Chunk {
        let bytes = lines.iter().map(|line| line.len() + 1).sum();
        Chunk { lines, bytes }
    }
}

/// `lines`, at least one, in as few chunks of at most
/// `MAX_LINES / 2` lines as hold them, of near equal size.
fn chunked(lines: Vec<String>) -> impl Iterator<Item = Arc<Chunk>> {
    let pieces = lines.len().div_ceil(MAX_LINES / 2);
    let size = lines.len().div_ceil(pieces);
    let mut lines = lines.into_iter().peekable();
    core::iter::from_fn(move || {
        lines.peek()?;
        Some(Arc::new(Chunk::new(
            lines.by_ref().take(size).collect(),
        )))
    })
}

/// A byte offset as a line of a chunk and an offset in that line.
#[derive(Clone, Copy)]
struct Pos {
    chunk: usize,
    index: usize,
    rel: usize,
}

/// The line that holds an offset.
pub(crate) struct Spot<'a> {
    pub(crate) line: u32,
    /// The offset of the line's first byte.
    pub(crate) start: usize,
    /// The line up to [`Buffer::line_end`].
    pub(crate) content: &'a str,
    /// The offset from `start`, at most `content.len()`.
    pub(crate) rel: usize,
}

/// A text as an array of lines, held in `Arc`-shared chunks of at
/// most 512 lines.
///
/// The lines are the text split at every `\n`, so the empty text is
/// one empty line. `line_starts[c]` and `byte_starts[c]` are the
/// first line and the first byte of chunk `c`, with the totals at the
/// end. Both derive from the chunks and are rebuilt once per
/// [`Buffer::replace`], from the first chunk it touched.
///
/// A buffer stores the bytes it is given. It never normalizes line
/// endings, so undo and redo restore exactly what they removed.
#[derive(Debug)]
pub struct Buffer {
    chunks: Vec<Arc<Chunk>>,
    line_starts: Vec<u32>,
    byte_starts: Vec<usize>,
    eol: Eol,
}

impl Buffer {
    /// Splits `text` at every `\n` and detects its [`Eol`].
    ///
    /// # Panics
    ///
    /// Panics if `text` is 4 GiB or longer.
    pub fn new(text: &str) -> Buffer {
        assert!(
            u32::try_from(text.len()).is_ok(),
            "a text of 4 GiB or more"
        );
        let eol = match text.find('\n') {
            Some(i) if text[..i].ends_with('\r') => Eol::Crlf,
            _ => Eol::Lf,
        };
        let mut buffer = Buffer {
            chunks: chunked(
                text.split('\n').map(String::from).collect(),
            )
            .collect(),
            line_starts: Vec::from([0]),
            byte_starts: Vec::from([0]),
            eol,
        };
        buffer.reindex(0);
        buffer
    }

    pub fn eol(&self) -> Eol {
        self.eol
    }

    /// The length of the text in bytes, every `\r` and `\n` included.
    pub fn len(&self) -> usize {
        self.total_bytes() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// At least 1.
    pub fn line_count(&self) -> u32 {
        self.line_starts[self.chunks.len()]
    }

    /// The line that holds `byte`. The offset of a line's `\n`
    /// belongs to that line, and `len()` to the last line.
    ///
    /// # Panics
    ///
    /// Panics if `byte` is past `len()`.
    pub fn line_of(&self, byte: usize) -> u32 {
        self.spot(byte).line
    }

    /// The offset of the line's first byte. Takes `0..=line_count()`,
    /// and `line_start(line_count())` is `len() + 1`, where a line
    /// past the last would start, as `rubrix_highlight` counts it.
    ///
    /// # Panics
    ///
    /// Panics if `line` is past `line_count()`.
    pub fn line_start(&self, line: u32) -> usize {
        assert!(line <= self.line_count(), "line past the end");
        if line == self.line_count() {
            return self.total_bytes();
        }
        let (chunk, index) = self.locate(line);
        let before = &self.chunks[chunk].lines[..index];
        self.byte_starts[chunk]
            + before.iter().map(|line| line.len() + 1).sum::<usize>()
    }

    /// The offset just after the line's content: before the `\r` of a
    /// `\r\n`, else before the `\n`, or `len()` on the last line. End
    /// puts the caret here. A `\r` that ends the last line has no
    /// `\n` after it, so it counts as content here, though
    /// `rubrix_highlight` reads it as a terminator.
    ///
    /// # Panics
    ///
    /// Panics if `line` is not a line of the text.
    pub fn line_end(&self, line: u32) -> usize {
        let content = self.content(line).len();
        self.line_start(line) + content
    }

    /// The text in `range`. Borrowed when the range lies inside one
    /// line, joined with `\n` otherwise.
    ///
    /// # Panics
    ///
    /// Panics if `range` is not on char boundaries inside the text.
    pub fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        assert!(
            range.start <= range.end && range.end <= self.len(),
            "range outside the text"
        );
        self.between(self.find(range.start), self.find(range.end))
    }

    /// Replaces each range of the text with its text, all at once,
    /// and returns the [`Change`] it made. The ranges are offsets of
    /// the text before the call. A replacement that removes nothing
    /// and inserts nothing is dropped.
    ///
    /// The ranges must be sorted, must not overlap, and must lie on
    /// char boundaries inside the text. Ranges may touch, and several
    /// empty ranges may share an offset, in which case their texts go
    /// in in the order given. The undo of deletions at touching
    /// carets is such a batch. The text goes in as given.
    ///
    /// One call reindexes the chunks once, so a keystroke at 100
    /// carets costs one reindex.
    ///
    /// # Panics
    ///
    /// Panics if the ranges break those rules, or if the text grows
    /// to 4 GiB or more.
    #[must_use = "the change keeps highlights, rows and carets in \
                  step"]
    pub fn replace<S: AsRef<str>>(
        &mut self,
        edits: impl IntoIterator<Item = (Range<usize>, S)>,
    ) -> Change {
        let edits: Vec<(Range<usize>, S)> =
            edits.into_iter().collect();
        let (mut end, mut len) = (0, self.len());
        for (range, text) in &edits {
            assert!(
                end <= range.start
                    && range.start <= range.end
                    && range.end <= self.len(),
                "replacements out of order, overlapping or outside \
                 the text"
            );
            end = range.end;
            len = len - range.len() + text.as_ref().len();
        }
        assert!(
            u32::try_from(len).is_ok(),
            "a text of 4 GiB or more"
        );
        let (Some(first), Some(last)) = (edits.first(), edits.last())
        else {
            return Change::new(Vec::new());
        };
        let touched = self.find(first.0.start).chunk
            ..self.find(last.0.end).chunk + 1;
        let mut reps: Vec<Rep> = edits
            .iter()
            .rev()
            .filter(|(range, text)| {
                !range.is_empty() || !text.as_ref().is_empty()
            })
            .map(|(range, text)| {
                self.splice(range.clone(), text.as_ref())
            })
            .collect();
        reps.reverse();
        let from = touched.start;
        self.restructure(touched);
        self.reindex(from);
        Change::new(reps)
    }

    /// A copy that shares every chunk, O(chunks): about 400 `Arc`
    /// clones at 100k lines. The first write to a shared chunk, in
    /// either copy, copies that chunk.
    pub fn snapshot(&self) -> Buffer {
        Buffer {
            chunks: self.chunks.clone(),
            line_starts: self.line_starts.clone(),
            byte_starts: self.byte_starts.clone(),
            eol: self.eol,
        }
    }

    /// The line up to [`Buffer::line_end`].
    pub(crate) fn content(&self, line: u32) -> &str {
        let text = self.line(line);
        match text.strip_suffix('\r') {
            Some(content) if line + 1 < self.line_count() => content,
            _ => text,
        }
    }

    /// The line that holds `at`.
    ///
    /// # Panics
    ///
    /// Panics if `at` is past `len()`.
    pub(crate) fn spot(&self, at: usize) -> Spot<'_> {
        assert!(at <= self.len(), "offset past the end of the text");
        let pos = self.find(at);
        let line = self.line_starts[pos.chunk] + pos.index as u32;
        let content = self.content(line);
        Spot {
            line,
            start: at - pos.rel,
            content,
            rel: pos.rel.min(content.len()),
        }
    }

    fn total_bytes(&self) -> usize {
        self.byte_starts[self.chunks.len()]
    }

    fn locate(&self, line: u32) -> (usize, usize) {
        let chunk =
            self.line_starts.partition_point(|&s| s <= line) - 1;
        (chunk, (line - self.line_starts[chunk]) as usize)
    }

    /// Where `byte`, at most `len()`, lies. Inside
    /// [`Buffer::replace`] it stays right for every offset up to the
    /// start of the replacements already made, because those change
    /// no chunk before theirs and no byte before their start, and the
    /// stale starts of later chunks still lie past `byte`.
    fn find(&self, byte: usize) -> Pos {
        let starts = &self.byte_starts[..self.chunks.len()];
        let chunk = starts.partition_point(|&s| s <= byte) - 1;
        let lines = &self.chunks[chunk].lines;
        let (mut index, mut rel) = (0, byte - starts[chunk]);
        while rel > lines[index].len() {
            rel -= lines[index].len() + 1;
            index += 1;
        }
        Pos { chunk, index, rel }
    }

    fn between(&self, from: Pos, to: Pos) -> Cow<'_, str> {
        let mut lines = self.chunks[from.chunk..]
            .iter()
            .flat_map(|chunk| &chunk.lines)
            .skip(from.index);
        let first = lines.next().expect("a line at the start");
        let count = self.line_starts[to.chunk] as usize + to.index
            - (self.line_starts[from.chunk] as usize + from.index);
        if count == 0 {
            return Cow::Borrowed(&first[from.rel..to.rel]);
        }
        let mut text = String::from(&first[from.rel..]);
        for line in lines.by_ref().take(count - 1) {
            text.push('\n');
            text.push_str(line);
        }
        let last = lines.next().expect("a line at the end");
        text.push('\n');
        text.push_str(&last[..to.rel]);
        Cow::Owned(text)
    }

    /// Replaces `range` with `text` in the lines, leaving the chunk
    /// list and its index for `restructure` and `reindex`, and
    /// returns the replacement without its `after`.
    fn splice(&mut self, range: Range<usize>, text: &str) -> Rep {
        let (from, to) =
            (self.find(range.start), self.find(range.end));
        let rep = Rep {
            start: range.start,
            old: self.between(from, to).into_owned(),
            new: String::from(text),
            line: self.line_starts[from.chunk] + from.index as u32,
            after: 0,
        };
        let tail = if from.chunk == to.chunk {
            let chunk = Arc::make_mut(&mut self.chunks[from.chunk]);
            chunk.bytes = chunk.bytes - rep.old.len() + text.len();
            let tail = chunk.lines[to.index].split_off(to.rel);
            chunk.lines.drain(from.index + 1..=to.index);
            tail
        } else {
            let last = Arc::make_mut(&mut self.chunks[to.chunk]);
            let tail = last.lines[to.index].split_off(to.rel);
            last.lines.drain(..=to.index);
            last.bytes -= range.end - self.byte_starts[to.chunk]
                + tail.len()
                + 1;
            for chunk in &mut self.chunks[from.chunk + 1..to.chunk] {
                *chunk = Arc::new(Chunk::new(Vec::new()));
            }
            let first = Arc::make_mut(&mut self.chunks[from.chunk]);
            first.lines.truncate(from.index + 1);
            first.bytes = range.start - self.byte_starts[from.chunk]
                + text.len()
                + tail.len()
                + 1;
            tail
        };
        let lines =
            &mut Arc::make_mut(&mut self.chunks[from.chunk]).lines;
        let mut parts = text.split('\n');
        let line = &mut lines[from.index];
        line.truncate(from.rel);
        line.push_str(parts.next().unwrap_or_default());
        let mut new: Vec<String> = parts.map(String::from).collect();
        match new.last_mut() {
            Some(last) => last.push_str(&tail),
            None => lines[from.index].push_str(&tail),
        }
        lines.splice(from.index + 1..from.index + 1, new);
        rep
    }

    /// Drops the chunks in `touched` that hold no line and splits
    /// those past `MAX_LINES`. The first touched chunk keeps the line
    /// its first replacement started on, so a chunk always remains.
    fn restructure(&mut self, touched: Range<usize>) {
        let fits = |chunk: &Arc<Chunk>| {
            (1..=MAX_LINES).contains(&chunk.lines.len())
        };
        if self.chunks[touched.clone()].iter().all(fits) {
            return;
        }
        let old: Vec<Arc<Chunk>> =
            self.chunks.drain(touched.clone()).collect();
        let mut new = Vec::with_capacity(old.len());
        for chunk in old {
            if fits(&chunk) {
                new.push(chunk);
            } else if !chunk.lines.is_empty() {
                new.extend(chunked(
                    Arc::unwrap_or_clone(chunk).lines,
                ));
            }
        }
        self.chunks.splice(touched.start..touched.start, new);
    }

    fn reindex(&mut self, from: usize) {
        self.line_starts.truncate(from + 1);
        self.byte_starts.truncate(from + 1);
        let (mut line, mut byte) =
            (self.line_starts[from], self.byte_starts[from]);
        for chunk in &self.chunks[from..] {
            line += chunk.lines.len() as u32;
            byte += chunk.bytes;
            self.line_starts.push(line);
            self.byte_starts.push(byte);
        }
    }
}

impl Lines for Buffer {
    fn line(&self, n: u32) -> &str {
        let (chunk, index) = self.locate(n);
        &self.chunks[chunk].lines[index]
    }
}

/// Writes the text, the lines joined with `\n`.
impl fmt::Display for Buffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut lines =
            self.chunks.iter().flat_map(|chunk| &chunk.lines);
        if let Some(first) = lines.next() {
            f.write_str(first)?;
        }
        for line in lines {
            f.write_char('\n')?;
            f.write_str(line)?;
        }
        Ok(())
    }
}
