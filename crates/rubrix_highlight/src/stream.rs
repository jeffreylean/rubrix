use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use crate::{Highlights, Lang, Run, lex_line};

/// Highlights a text that arrives in chunks.
///
/// Each line is lexed once, when its `\n` arrives, from the end
/// state of the line before. The bytes after the last `\n` wait,
/// unlexed, as the unfinished line, and [`Stream::partial`] lexes
/// them on request. A `\r\n` pair may split across chunks, because
/// the `\r` waits in the unfinished line until the `\n` ends it.
///
/// [`Stream::finish`] returns the same [`Highlights`] that
/// [`highlight`](crate::highlight) gives for the concatenated
/// chunks, wherever the chunks split.
#[derive(Clone, Debug)]
pub struct Stream {
    committed: Highlights,
    tail: String,
    tail_start: usize,
}

impl Stream {
    pub fn new(lang: Lang) -> Stream {
        Stream {
            committed: Highlights::new(lang, 0, 0),
            tail: String::new(),
            tail_start: 0,
        }
    }

    /// Appends `chunk` and returns the lines it completed, which
    /// [`Stream::runs`] reads. The range is empty when the chunk
    /// holds no `\n`.
    ///
    /// # Panics
    ///
    /// Panics when the text grows to 4 GiB or more.
    pub fn push(&mut self, chunk: &str) -> Range<u32> {
        assert!(
            self.tail_start + self.tail.len() + chunk.len()
                < u32::MAX as usize,
            "text of 4 GiB or more"
        );
        let first = self.committed.line_count();
        let mut rest = chunk;
        if !self.tail.is_empty() {
            let Some(end) = memchr::memchr(b'\n', rest.as_bytes())
            else {
                self.tail.push_str(rest);
                return first..first;
            };
            self.tail.push_str(&rest[..end]);
            self.committed.push_line(self.tail_start, &self.tail);
            self.tail_start += self.tail.len() + 1;
            self.tail.clear();
            rest = &rest[end + 1..];
        }
        let mut start = 0;
        for end in memchr::memchr_iter(b'\n', rest.as_bytes()) {
            self.committed
                .push_line(self.tail_start, &rest[start..end]);
            self.tail_start += end - start + 1;
            start = end + 1;
        }
        self.tail.push_str(&rest[start..]);
        first..self.committed.line_count()
    }

    /// The runs of a line that [`Stream::push`] returned.
    ///
    /// # Panics
    ///
    /// Panics if `line` is not yet committed.
    pub fn runs(&self, line: u32) -> &[Run] {
        self.committed.stored_runs(line)
    }

    /// The runs of the unfinished last line, as they would be if
    /// the text ended here. A `\r` at the end of the tail is left
    /// out, because it is probably the start of a `\r\n`. The next
    /// chunk can change these runs, and [`Stream::push`] never
    /// returns them.
    pub fn partial(&self) -> Vec<Run> {
        let content =
            self.tail.strip_suffix('\r').unwrap_or(&self.tail);
        let mut runs = Vec::new();
        let state =
            self.committed.start_state(self.committed.line_count());
        lex_line(self.committed.lang(), content, state, &mut runs);
        runs
    }

    /// Commits the last line, which is empty when the text ends
    /// with `\n`, and returns the highlights of the whole text.
    pub fn finish(mut self) -> Highlights {
        self.committed.push_line(self.tail_start, &self.tail);
        self.committed
    }
}
