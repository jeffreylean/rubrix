//! Syntax highlighting for rubrix.
//!
//! [`highlight`] lexes a whole text into [`Highlights`], which holds
//! the [`Run`]s of each line and the lexer [`State`] at the end of
//! each line. [`lex_line`] lexes one line from a saved state, so a
//! caller can resume anywhere in a text.
//!
//! # Lines and runs
//!
//! Lines split on `\n`. One `\r` at the end of a line belongs to its
//! terminator, so a text with n `\n` bytes has n + 1 lines and the
//! empty text is one empty line. Runs cover a line's content
//! exactly, from byte 0, and never hold a terminator.
//!
//! Runs are non-empty, end on char boundaries, and two adjacent runs
//! never share a kind. Whitespace between tokens belongs to the run
//! before it, and leading whitespace to the line's first token, as
//! in Pierre's lexers. Whitespace inside a string or a comment is
//! part of it. An empty line has no runs.

#![no_std]

extern crate alloc;

mod kind;
mod lang;
mod lexers {
    pub(crate) mod diff;
    pub(crate) mod go;
    pub(crate) mod json;
    pub(crate) mod plain;
    pub(crate) mod python;
    pub(crate) mod rust;
    pub(crate) mod shell;
    pub(crate) mod toml;
    pub(crate) mod ts;
    pub(crate) mod yaml;

    const fn is_ident_start(b: u8) -> bool {
        b.is_ascii_alphabetic() || b == b'_' || !b.is_ascii()
    }

    const fn is_ident_continue(b: u8) -> bool {
        is_ident_start(b) || b.is_ascii_digit()
    }

    fn ident_end(s: &[u8], mut i: usize) -> usize {
        while i < s.len() && is_ident_continue(s[i]) {
            i += 1;
        }
        i
    }

    fn space_end(s: &[u8], mut i: usize) -> usize {
        while i < s.len() && s[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    }
}

use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

pub use kind::Kind;
pub use lang::Lang;
use runs::Runs;
pub use state::State;

/// One stretch of a line with one kind.
///
/// A run starts where the run before it ends, or at 0, and ends at
/// `end`, a byte offset into the line's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    pub end: u32,
    pub kind: Kind,
}

mod state {
    use alloc::sync::Arc;

    /// The lexer state at a line boundary, which the next line
    /// starts from. The default is the state at the start of a text.
    ///
    /// A lexer reads a state it did not write, such as another
    /// language's, as some state of its own and never panics.
    #[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
    pub struct State {
        word: u64,
        tail: Option<Arc<[u8]>>,
    }

    pub(crate) static INITIAL: State = State {
        word: 0,
        tail: None,
    };

    #[allow(
        dead_code,
        reason = "lexers that keep unbounded data use the tail"
    )]
    impl State {
        pub(crate) const fn from_word(word: u64) -> State {
            State { word, tail: None }
        }

        pub(crate) fn with_tail(word: u64, tail: &[u8]) -> State {
            let tail = (!tail.is_empty()).then(|| Arc::from(tail));
            State { word, tail }
        }

        pub(crate) fn with_word(&self, word: u64) -> State {
            State {
                word,
                tail: self.tail.clone(),
            }
        }

        pub(crate) const fn word(&self) -> u64 {
            self.word
        }

        pub(crate) fn tail(&self) -> &[u8] {
            self.tail.as_deref().unwrap_or_default()
        }
    }
}

/// Lexes one line's content from `state`, appends its runs to `out`,
/// and returns the state at the end of the line.
///
/// `line` holds no `\n` and no `\r` that ends the line. The result
/// depends only on `lang`, `line`, and `state`, so lexing a line
/// again from its saved start state gives the same runs.
///
/// # Panics
///
/// Panics if `line` is 4 GiB or longer.
pub fn lex_line(
    lang: Lang,
    line: &str,
    state: &State,
    out: &mut Vec<Run>,
) -> State {
    assert!(line.len() < u32::MAX as usize, "line of 4 GiB or more");
    debug_assert!(!line.contains('\n'), "line holds a newline");
    let mut runs = Runs::new(out, line);
    let next = match lang {
        Lang::Plain => lexers::plain::lex(line, state, &mut runs),
        Lang::Rust => lexers::rust::lex(line, state, &mut runs),
        Lang::TypeScript => lexers::ts::lex(line, state, &mut runs),
        Lang::Python => lexers::python::lex(line, state, &mut runs),
        Lang::Go => lexers::go::lex(line, state, &mut runs),
        Lang::Shell => lexers::shell::lex(line, state, &mut runs),
        Lang::Diff => lexers::diff::lex(line, state, &mut runs),
        Lang::Json => lexers::json::lex(line, state, &mut runs),
        Lang::Toml => lexers::toml::lex(line, state, &mut runs),
        Lang::Yaml => lexers::yaml::lex(line, state, &mut runs),
    };
    runs.finish();
    next
}

/// Lexes a whole text.
///
/// # Panics
///
/// Panics if `text` is 4 GiB or longer.
pub fn highlight(lang: Lang, text: &str) -> Highlights {
    assert!(text.len() < u32::MAX as usize, "text of 4 GiB or more");
    let mut highlights = Highlights::new(lang, text.len());
    let mut start = 0;
    for end in memchr::memchr_iter(b'\n', text.as_bytes()) {
        highlights.push_line(start, &text[start..end]);
        start = end + 1;
    }
    highlights.push_line(start, &text[start..]);
    highlights
}

/// The byte range within its line and the kind of each run.
pub fn spans(
    runs: &[Run],
) -> impl Iterator<Item = (Range<usize>, Kind)> + '_ {
    let mut start = 0;
    runs.iter().map(move |run| {
        let range = start..run.end as usize;
        start = range.end;
        (range, run.kind)
    })
}

/// Checks that `runs` cover `line` in the form [`lex_line`]
/// promises: non-empty runs that end on char boundaries, no two
/// adjacent runs of one kind, and the last run at the line's end.
pub fn check_line(line: &str, runs: &[Run]) -> Result<(), Problem> {
    let mut start = 0;
    let mut previous = None;
    for run in runs {
        let end = run.end as usize;
        if end <= start {
            return Err(Problem::EmptyRun);
        }
        if end > line.len() {
            return Err(Problem::PastEnd);
        }
        if !line.is_char_boundary(end) {
            return Err(Problem::NotCharBoundary);
        }
        if previous == Some(run.kind) {
            return Err(Problem::SameKind);
        }
        previous = Some(run.kind);
        start = end;
    }
    if start == line.len() {
        Ok(())
    } else {
        Err(Problem::ShortOfEnd)
    }
}

/// A rule that one line's runs break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Problem {
    EmptyRun,
    PastEnd,
    NotCharBoundary,
    SameKind,
    ShortOfEnd,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Problem::EmptyRun => "a run is empty",
            Problem::PastEnd => "a run ends past the line",
            Problem::NotCharBoundary => "a run ends inside a char",
            Problem::SameKind => "two adjacent runs share a kind",
            Problem::ShortOfEnd => {
                "the runs stop before the line ends"
            }
        })
    }
}

/// The first difference [`Highlights::check`] found between the
/// highlights and the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckError {
    Runs { line: u32, problem: Problem },
    LineStart { line: u32 },
    LineCount { text: u32, highlights: u32 },
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::Runs { line, problem } => {
                write!(f, "line {line}: {problem}")
            }
            CheckError::LineStart { line } => {
                write!(f, "line {line} starts at another byte")
            }
            CheckError::LineCount { text, highlights } => write!(
                f,
                "the text has {text} lines and the highlights \
                 {highlights}"
            ),
        }
    }
}

impl core::error::Error for CheckError {}

/// The runs and end states of every line of one text.
#[derive(Clone, Debug)]
pub struct Highlights {
    lang: Lang,
    lines: Vec<Line>,
    runs: Vec<Run>,
}

#[derive(Clone, Debug)]
struct Line {
    start: u32,
    runs: Range<u32>,
    end: State,
}

impl Highlights {
    pub(crate) fn new(lang: Lang, text_len: usize) -> Highlights {
        Highlights {
            lang,
            lines: Vec::with_capacity(text_len / 24 + 1),
            runs: Vec::with_capacity(text_len / 4 + 1),
        }
    }

    pub(crate) fn push_line(&mut self, start: usize, raw: &str) {
        let content = raw.strip_suffix('\r').unwrap_or(raw);
        let first = self.runs.len();
        let state = self
            .lines
            .last()
            .map_or(&state::INITIAL, |line| &line.end);
        let end = lex_line(self.lang, content, state, &mut self.runs);
        self.lines.push(Line {
            start: start as u32,
            runs: first as u32..self.runs.len() as u32,
            end,
        });
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// At least 1.
    pub fn line_count(&self) -> u32 {
        self.lines.len() as u32
    }

    /// The byte range of the line's content, without its terminator,
    /// in the text these highlights were built from.
    pub fn line_range(&self, line: u32) -> Range<usize> {
        let start = self.lines[line as usize].start as usize;
        let len =
            self.stored_runs(line).last().map_or(0, |run| run.end);
        start..start + len as usize
    }

    /// The runs saved when the line was last lexed.
    pub fn stored_runs(&self, line: u32) -> &[Run] {
        let runs = &self.lines[line as usize].runs;
        &self.runs[runs.start as usize..runs.end as usize]
    }

    /// The state the line starts from. Takes `0..=line_count()`, and
    /// `start_state(line_count())` is the state at the end of the
    /// text.
    pub fn start_state(&self, line: u32) -> &State {
        match line {
            0 => &state::INITIAL,
            n => &self.lines[n as usize - 1].end,
        }
    }

    /// Checks these highlights against the text they came from: the
    /// same lines at the same byte offsets, each passing
    /// [`check_line`].
    pub fn check(&self, text: &str) -> Result<(), CheckError> {
        let mut start = 0;
        let mut count = 0;
        for (line, raw) in (0..).zip(text.split('\n')) {
            count = line + 1;
            if line >= self.line_count() {
                continue;
            }
            if self.lines[line as usize].start as usize != start {
                return Err(CheckError::LineStart { line });
            }
            let content = raw.strip_suffix('\r').unwrap_or(raw);
            check_line(content, self.stored_runs(line)).map_err(
                |problem| CheckError::Runs { line, problem },
            )?;
            start += raw.len() + 1;
        }
        if count == self.line_count() {
            Ok(())
        } else {
            Err(CheckError::LineCount {
                text: count,
                highlights: self.line_count(),
            })
        }
    }

    /// One `line start..end kind text` row per run, the format of
    /// the golden files. Lines count from 0, offsets are within the
    /// line, and the text is quoted. `text` is the text these
    /// highlights came from.
    pub fn display<'a>(
        &'a self,
        text: &'a str,
    ) -> impl fmt::Display + 'a {
        Rows {
            highlights: self,
            text,
        }
    }
}

/// Equal when the language and every line's runs and end state are
/// equal, however the runs are stored. [`Highlights::check`]
/// compares line starts with a text.
impl PartialEq for Highlights {
    fn eq(&self, other: &Highlights) -> bool {
        self.lang == other.lang
            && self.line_count() == other.line_count()
            && (0..self.line_count()).all(|n| {
                let a = &self.lines[n as usize];
                let b = &other.lines[n as usize];
                a.end == b.end
                    && self.stored_runs(n) == other.stored_runs(n)
            })
    }
}

impl Eq for Highlights {}

struct Rows<'a> {
    highlights: &'a Highlights,
    text: &'a str,
}

impl fmt::Display for Rows<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let highlights = self.highlights;
        for n in 0..highlights.line_count() {
            let line = &self.text[highlights.line_range(n)];
            for (range, kind) in spans(highlights.stored_runs(n)) {
                let piece = &line[range.clone()];
                let Range { start, end } = range;
                writeln!(f, "{n} {start}..{end} {kind} {piece:?}")?;
            }
        }
        Ok(())
    }
}

mod runs {
    use alloc::vec::Vec;

    use crate::{Kind, Run};

    pub(crate) struct Runs<'a> {
        out: &'a mut Vec<Run>,
        line: &'a str,
        base: usize,
        consumed: usize,
    }

    impl<'a> Runs<'a> {
        pub(super) fn new(
            out: &'a mut Vec<Run>,
            line: &'a str,
        ) -> Runs<'a> {
            let base = out.len();
            Runs {
                out,
                line,
                base,
                consumed: 0,
            }
        }

        pub(crate) fn push(&mut self, end: usize, kind: Kind) {
            if end == self.consumed {
                return;
            }
            self.advance(end);
            let end = end as u32;
            match self.out[self.base..].last_mut() {
                Some(run) if run.kind == kind => run.end = end,
                _ => self.out.push(Run { end, kind }),
            }
        }

        pub(crate) fn space(&mut self, end: usize) {
            self.advance(end);
            if let Some(run) = self.out[self.base..].last_mut() {
                run.end = end as u32;
            }
        }

        fn advance(&mut self, end: usize) {
            debug_assert!(end >= self.consumed, "runs go backwards");
            debug_assert!(
                self.line.is_char_boundary(end),
                "a run ends inside a char or past the line"
            );
            self.consumed = end;
        }

        pub(super) fn finish(self) {
            let len = self.line.len();
            debug_assert_eq!(
                self.consumed, len,
                "runs must cover the line"
            );
            if self.out.len() == self.base && len > 0 {
                debug_assert!(
                    self.line.trim_ascii().is_empty(),
                    "a line with text has no runs"
                );
                self.out.push(Run {
                    end: len as u32,
                    kind: Kind::Plain,
                });
            }
        }
    }
}
