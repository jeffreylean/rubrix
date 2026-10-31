//! Find and replace over a [`Buffer`], behind the `find` feature.
//!
//! A [`Search`] copies the buffer's text into one string and scans
//! it with a regex, so a match can run across lines. It works in
//! slices: [`Search::step`] does as much as a time budget allows and
//! picks up where it stopped on the next call, so a view can spread a
//! search of a large buffer over frames. [`Search::edited`] follows
//! an edit, keeping the matches before the edited line and scanning
//! again from there.

use std::borrow::Cow;
use std::ops::Range;
use std::string::{String, ToString};
use std::time::{Duration, Instant};
use std::vec::Vec;

use regex::{Captures, Regex, RegexBuilder};
use rubrix_highlight::Lines;

use crate::history::Kind;
use crate::motion::{is_word, word_at};
use crate::{Buffer, Change, Editor, Selection, Selections};

/// A search stops after this many matches, as Pierre's does.
pub const MAX_MATCHES: usize = 100_000;

/// What to look for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    /// Without it, case is ignored.
    pub match_case: bool,
    /// Keeps only matches with no word character just before or just
    /// after them, as the word motions class characters.
    pub whole_word: bool,
    /// Reads `text` as a regex, with `^` and `$` at line breaks.
    /// Otherwise `text` is matched as written.
    pub regex: bool,
}

#[derive(Debug)]
enum Pattern {
    Empty,
    Invalid(String),
    Regex(Regex),
}

/// The matches of one [`Query`] in a buffer, found a slice at a time.
///
/// A search keeps its own copy of the text, so it must see every
/// change made to the buffer it scans, through [`Search::edited`].
#[derive(Debug)]
pub struct Search {
    pattern: Pattern,
    whole_word: bool,
    expand: bool,
    text: String,
    copied: u32,
    at: Option<usize>,
    matches: Vec<Range<usize>>,
}

impl Search {
    pub fn new(query: &Query) -> Search {
        let mut search = Search {
            pattern: Pattern::Empty,
            whole_word: false,
            expand: false,
            text: String::new(),
            copied: 0,
            at: None,
            matches: Vec::new(),
        };
        search.set_query(query);
        search
    }

    /// Starts over with `query`, keeping the copied text.
    pub fn set_query(&mut self, query: &Query) {
        self.pattern = compile(query);
        self.whole_word = query.whole_word;
        self.expand = query.regex;
        self.matches.clear();
        self.at =
            matches!(self.pattern, Pattern::Regex(_)).then_some(0);
    }

    /// Follows a change made to `buffer`, which holds the text after
    /// it. Keeps the matches that end before the line above the first
    /// edited line, so a match that ran from that line into the edit
    /// is found again, and scans on from there.
    pub fn edited(&mut self, buffer: &Buffer, change: &Change) {
        let Some(first) = change.replacements().next() else {
            return;
        };
        let line = buffer.line_of(first.range.start);
        if self.copied > line {
            self.text
                .truncate(buffer.line_start(line).saturating_sub(1));
            self.copied = line;
        }
        let from = buffer.line_start(line.saturating_sub(1));
        let kept = self.matches.partition_point(|m| m.end <= from);
        let restart = self
            .matches
            .get(kept)
            .map_or(from, |m| m.start.min(from));
        self.matches.truncate(kept);
        if let Pattern::Regex(_) = self.pattern {
            self.at =
                Some(self.at.map_or(restart, |at| at.min(restart)));
        }
    }

    /// Scans for about `budget`, and returns whether the search has
    /// ended. A single regex step can run past the budget when the
    /// regex has no literal to look for and no match is near.
    pub fn step(
        &mut self,
        buffer: &Buffer,
        budget: Duration,
    ) -> bool {
        self.advance(buffer, Some(Instant::now() + budget))
    }

    /// Scans to the end.
    pub fn run(&mut self, buffer: &Buffer) {
        self.advance(buffer, None);
    }

    /// The matches found so far, in buffer order. None is empty, and
    /// none overlaps another.
    pub fn matches(&self) -> &[Range<usize>] {
        &self.matches
    }

    pub fn is_done(&self) -> bool {
        self.at.is_none()
    }

    /// Whether the search stopped at [`MAX_MATCHES`].
    pub fn is_capped(&self) -> bool {
        self.matches.len() >= MAX_MATCHES
    }

    /// Why a regex query does not compile.
    pub fn error(&self) -> Option<&str> {
        match &self.pattern {
            Pattern::Invalid(error) => Some(error),
            _ => None,
        }
    }

    fn advance(
        &mut self,
        buffer: &Buffer,
        deadline: Option<Instant>,
    ) -> bool {
        let Some(mut at) = self.at else {
            return true;
        };
        if !self.copy(buffer, deadline) {
            return false;
        }
        let Pattern::Regex(regex) = &self.pattern else {
            self.at = None;
            return true;
        };
        let mut steps = 0_u32;
        while self.matches.len() < MAX_MATCHES {
            let Some(found) = regex.find_at(&self.text, at) else {
                break;
            };
            let range = found.range();
            if !range.is_empty()
                && (!self.whole_word
                    || whole_word(&self.text, &range))
            {
                at = range.end;
                self.matches.push(range);
            } else {
                let Some(c) = self.text[range.start..].chars().next()
                else {
                    break;
                };
                at = range.start + c.len_utf8();
            }
            steps += 1;
            if steps.is_multiple_of(32)
                && deadline.is_some_and(|end| Instant::now() >= end)
            {
                self.at = Some(at);
                return false;
            }
        }
        self.at = None;
        true
    }

    fn copy(
        &mut self,
        buffer: &Buffer,
        deadline: Option<Instant>,
    ) -> bool {
        let count = buffer.line_count();
        if self.copied == 0 {
            self.text.reserve(buffer.len());
        }
        while self.copied < count {
            if self.copied > 0 {
                self.text.push('\n');
            }
            self.text.push_str(buffer.line(self.copied));
            self.copied += 1;
            if self.copied.is_multiple_of(1024)
                && deadline.is_some_and(|end| Instant::now() >= end)
            {
                return self.copied == count;
            }
        }
        debug_assert_eq!(
            self.text.len(),
            buffer.len(),
            "a stale copy"
        );
        true
    }

    fn replacement<'t>(
        &self,
        index: usize,
        template: &'t str,
    ) -> Cow<'t, str> {
        let range = &self.matches[index];
        let captures = match &self.pattern {
            Pattern::Regex(regex) if self.expand => {
                regex.captures_at(&self.text, range.start)
            }
            _ => None,
        };
        match captures {
            Some(captures)
                if captures.get_match().range() == *range =>
            {
                Cow::Owned(expand(template, &captures))
            }
            _ => Cow::Borrowed(template),
        }
    }
}

/// The word under a bare primary caret, or just before it, which a
/// find bar opens with. `None` for a range or when no word touches
/// the caret.
pub fn seed(
    buffer: &Buffer,
    selections: &Selections,
) -> Option<Range<usize>> {
    let caret = selections.primary();
    if !caret.is_caret() {
        return None;
    }
    let spot = buffer.spot(caret.head);
    let before =
        spot.content.floor_char_boundary(spot.rel.saturating_sub(1));
    [spot.rel, before]
        .into_iter()
        .map(|at| word_at(spot.content, at))
        .find(|word| is_word(&spot.content[word.clone()]))
        .map(|word| spot.start + word.start..spot.start + word.end)
}

/// Replaces match `index` of `search` with `template`, as one undo
/// step, and puts one caret after the new text. A regex query expands
/// `$n` in `template` to group n, `$&` to the whole match and `$$` to
/// `$`, and a group that did not take part gives nothing. A plain
/// query takes `template` as written. Finishes the search first, so
/// its matches hold. Feed the change to [`Search::edited`] as any
/// other.
pub fn replace(
    editor: &mut Editor,
    search: &mut Search,
    index: usize,
    template: &str,
) -> Option<Change> {
    search.run(editor.buffer());
    let range = search.matches.get(index)?.clone();
    let text = search.replacement(index, template);
    let caret = range.start + text.len();
    commit(editor, Vec::from([(range, text)]), Some(caret))
}

/// Replaces every match of `search` with `template`, expanded as
/// [`replace`] does, as one undo step, so one undo restores them all.
/// Finishes the search first, so its matches hold. Feed the change to
/// [`Search::edited`] as any other.
pub fn replace_all(
    editor: &mut Editor,
    search: &mut Search,
    template: &str,
) -> Option<Change> {
    search.run(editor.buffer());
    let edits = (0..search.matches.len())
        .map(|i| {
            (
                search.matches[i].clone(),
                search.replacement(i, template),
            )
        })
        .collect();
    commit(editor, edits, None)
}

fn compile(query: &Query) -> Pattern {
    if query.text.is_empty() {
        return Pattern::Empty;
    }
    let source = if query.regex {
        Cow::Borrowed(query.text.as_str())
    } else {
        Cow::Owned(regex::escape(&query.text))
    };
    let built = RegexBuilder::new(&source)
        .case_insensitive(!query.match_case)
        .multi_line(true)
        .crlf(true)
        .build();
    match built {
        Ok(regex) => Pattern::Regex(regex),
        Err(error) => Pattern::Invalid(
            error
                .to_string()
                .lines()
                .last()
                .unwrap_or_default()
                .trim_start_matches("error: ")
                .to_string(),
        ),
    }
}

fn whole_word(text: &str, range: &Range<usize>) -> bool {
    let word = |c: Option<char>| {
        c.is_some_and(|c| is_word(c.encode_utf8(&mut [0; 4])))
    };
    !word(text[..range.start].chars().next_back())
        && !word(text[range.end..].chars().next())
}

fn expand(template: &str, captures: &Captures<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        let digits =
            after.bytes().take_while(u8::is_ascii_digit).count();
        rest = if let Some(after) = after.strip_prefix('$') {
            out.push('$');
            after
        } else if let Some(after) = after.strip_prefix('&') {
            out.push_str(captures.get_match().as_str());
            after
        } else if digits > 0 {
            let group = after[..digits].parse().unwrap_or(usize::MAX);
            out.push_str(
                captures.get(group).map_or("", |m| m.as_str()),
            );
            &after[digits..]
        } else {
            out.push('$');
            after
        };
    }
    out.push_str(rest);
    out
}

fn commit(
    editor: &mut Editor,
    edits: Vec<(Range<usize>, Cow<'_, str>)>,
    caret: Option<usize>,
) -> Option<Change> {
    let mut buffer = editor.buffer().snapshot();
    let change = buffer.replace(edits);
    if change.is_empty() {
        return None;
    }
    let before = editor.selections().clone();
    let after = match caret {
        Some(at) => Selections::new(Selection::caret(at)),
        None => {
            let mut after = before.clone();
            after.map_through(&change);
            after
        }
    };
    let mut history = editor.history().clone();
    history.record(
        change.clone(),
        Kind::Boundary,
        before,
        after.clone(),
    );
    *editor = Editor::from_parts(buffer, after, history);
    Some(change)
}
