//! Find and replace over a [`Buffer`], behind the `find` feature.
//!
//! A [`Search`] copies the buffer's text into one string and scans
//! it with a regex, so a match can run across lines. It works in
//! slices: [`Search::step`] does as much as a time budget allows and
//! picks up where it stopped on the next call, so a view can spread a
//! search of a large buffer over frames. It steps the regex's lazy
//! DFAs by hand, so it can stop inside the scan for one match, which
//! a `Regex` call cannot. [`Search::edited`] follows an edit, keeping
//! the matches that end before the first line a match reaching the
//! edit could start on, and scanning again from there.

use std::borrow::Cow;
use std::boxed::Box;
use std::ops::Range;
use std::string::{String, ToString};
use std::time::{Duration, Instant};
use std::vec::Vec;

use regex::{Captures, Regex, RegexBuilder};
use regex_automata::hybrid::LazyStateID;
use regex_automata::hybrid::dfa::Config as DfaConfig;
use regex_automata::hybrid::regex::{
    Builder as LazyBuilder, Cache, Regex as LazyRegex,
};
use regex_automata::util::prefilter::Prefilter;
use regex_automata::util::syntax::Config as SyntaxConfig;
use regex_automata::{Anchored, Input, MatchError, MatchKind, Span};
use regex_syntax::ParserBuilder;
use regex_syntax::ast::Ast;
use regex_syntax::ast::parse::Parser;
use regex_syntax::ast::print::Printer;
use regex_syntax::hir::{Class, Hir, HirKind};
use rubrix_highlight::Lines;

use crate::history::Kind;
use crate::motion::{is_word, snap, word_at};
use crate::{Buffer, Change, Editor, Selection, Selections};

/// A search stops after this many matches, as Pierre's does.
pub const MAX_MATCHES: usize = 100_000;

const BYTES_PER_CLOCK_CHECK: usize = 1 << 14;
const PREFILTER_WINDOW: usize = 1 << 16;

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
    Regex {
        regex: Regex,
        lazy: Option<Box<Lazy>>,
        most_breaks: Option<u32>,
    },
}

#[derive(Debug)]
struct Lazy {
    regex: LazyRegex,
    cache: Cache,
    universal_start: bool,
}

#[derive(Clone, Copy, Debug)]
enum Leg {
    Forward {
        at: usize,
        sid: LazyStateID,
        last_end: Option<usize>,
    },
    Reverse {
        at: usize,
        sid: LazyStateID,
        end: usize,
        start: Option<usize>,
    },
}

enum Next {
    Found(Range<usize>),
    Nothing,
    Paused,
}

enum Scan<T> {
    Unfinished,
    Done(T),
}

/// The matches of one [`Query`] in a buffer, found a slice at a time.
///
/// A match counts only when both its ends are grapheme cluster
/// boundaries, so the editor can select it as it stands.
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
    leg: Option<Leg>,
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
            leg: None,
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
        self.leg = None;
        self.at = matches!(self.pattern, Pattern::Regex { .. })
            .then_some(0);
    }

    /// Follows a change made to `buffer`, which holds the text after
    /// it. A pattern with no bound on its line breaks scans again
    /// from the last match before the edited line, so it misses a
    /// branch that starts before that match, such as `x|a[\s\S]*z`.
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
        let Pattern::Regex { most_breaks, .. } = self.pattern else {
            return;
        };
        let from = match most_breaks {
            Some(breaks) => {
                buffer.line_start(line.saturating_sub(breaks))
            }
            None => {
                let line_start = buffer.line_start(line);
                let before = self
                    .matches
                    .partition_point(|m| m.end <= line_start);
                before
                    .checked_sub(1)
                    .map_or(0, |last| self.matches[last].start)
            }
        };
        let kept = self.matches.partition_point(|m| m.end <= from);
        let restart = self
            .matches
            .get(kept)
            .map_or(from, |m| m.start.min(from));
        self.matches.truncate(kept);
        self.leg = None;
        self.at = Some(self.at.map_or(restart, |at| at.min(restart)));
    }

    /// Scans for about `budget`, and returns whether the search has
    /// ended. It can run past the budget when the lazy DFA cannot
    /// scan the pattern, as with a Unicode `\b` next to non-ASCII
    /// text, and the scan falls back to one `Regex` call.
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
        let Pattern::Regex { regex, lazy, .. } = &mut self.pattern
        else {
            self.at = None;
            return true;
        };
        let mut steps = 0_u32;
        while self.matches.len() < MAX_MATCHES {
            let next = lazy.as_mut().map(|lazy| {
                lazy.find(&self.text, at, &mut self.leg, deadline)
            });
            let found = match next {
                Some(Ok(Next::Paused)) => {
                    self.at = Some(at);
                    return false;
                }
                Some(Ok(Next::Found(range))) => Some(range),
                Some(Ok(Next::Nothing)) => None,
                Some(Err(_)) | None => {
                    regex.find_at(&self.text, at).map(|m| m.range())
                }
            };
            let Some(range) = found else {
                break;
            };
            if !range.is_empty()
                && (!self.whole_word
                    || whole_word(&self.text, &range))
                && on_cluster_boundaries(buffer, &self.text, &range)
            {
                at = range.end;
                self.matches.push(range);
            } else if range.start < self.text.len() {
                at = self.text.ceil_char_boundary(range.start + 1);
            } else {
                break;
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
            Pattern::Regex { regex, .. } if self.expand => {
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
    build(&source, !query.match_case).unwrap_or_else(|error| {
        Pattern::Invalid(
            error
                .lines()
                .last()
                .unwrap_or_default()
                .trim_start_matches("error: ")
                .to_string(),
        )
    })
}

fn build(
    source: &str,
    case_insensitive: bool,
) -> Result<Pattern, String> {
    let mut ast = Parser::new()
        .parse(source)
        .map_err(|error| error.to_string())?;
    crlf_newlines(&mut ast);
    let mut pattern = String::with_capacity(source.len());
    Printer::new()
        .print(&ast, &mut pattern)
        .expect("a String takes every write");
    let hir = ParserBuilder::new()
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .crlf(true)
        .build()
        .parse(&pattern)
        .map_err(|error| error.to_string())?;
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(case_insensitive)
        .multi_line(true)
        .crlf(true)
        .build()
        .map_err(|error| error.to_string())?;
    Ok(Pattern::Regex {
        regex,
        lazy: Lazy::new(&pattern, &hir, case_insensitive),
        most_breaks: most_breaks(&hir),
    })
}

impl Lazy {
    fn new(
        pattern: &str,
        hir: &Hir,
        case_insensitive: bool,
    ) -> Option<Box<Lazy>> {
        let prefilter =
            Prefilter::from_hir_prefix(MatchKind::LeftmostFirst, hir);
        let regex = LazyBuilder::new()
            .syntax(
                SyntaxConfig::new()
                    .case_insensitive(case_insensitive)
                    .multi_line(true)
                    .crlf(true),
            )
            .dfa(
                DfaConfig::new()
                    .specialize_start_states(prefilter.is_some())
                    .prefilter(prefilter)
                    .unicode_word_boundary(true)
                    .minimum_cache_clear_count(None),
            )
            .build(pattern)
            .ok()?;
        let universal_start = regex
            .forward()
            .get_nfa()
            .look_set_prefix_any()
            .is_empty();
        Some(Box::new(Lazy {
            cache: regex.create_cache(),
            regex,
            universal_start,
        }))
    }

    fn find(
        &mut self,
        text: &str,
        from: usize,
        leg: &mut Option<Leg>,
        deadline: Option<Instant>,
    ) -> Result<Next, MatchError> {
        let hay = text.as_bytes();
        let mut current = match leg.take() {
            Some(leg) => leg,
            None => Leg::Forward {
                at: from,
                sid: self.regex.forward().start_state_forward(
                    self.cache.forward_mut(),
                    &Input::new(hay).span(from..hay.len()),
                )?,
                last_end: None,
            },
        };
        loop {
            match &mut current {
                Leg::Forward { at, sid, last_end } => {
                    match self.forward(hay, at, sid, last_end)? {
                        Scan::Unfinished => {}
                        Scan::Done(None) => return Ok(Next::Nothing),
                        Scan::Done(Some(end)) if end == from => {
                            return Ok(Next::Found(from..from));
                        }
                        Scan::Done(Some(end)) => {
                            let input = Input::new(hay)
                                .span(from..end)
                                .anchored(Anchored::Yes);
                            let sid = self
                                .regex
                                .reverse()
                                .start_state_reverse(
                                    self.cache.reverse_mut(),
                                    &input,
                                )?;
                            current = Leg::Reverse {
                                at: end,
                                sid,
                                end,
                                start: None,
                            };
                            continue;
                        }
                    }
                }
                Leg::Reverse {
                    at,
                    sid,
                    end,
                    start,
                } => {
                    if let Scan::Done(start) =
                        self.reverse(hay, from, at, sid, start)?
                    {
                        return Ok(Next::Found(start..*end));
                    }
                }
            }
            if deadline.is_some_and(|end| Instant::now() >= end) {
                *leg = Some(current);
                return Ok(Next::Paused);
            }
        }
    }

    fn forward(
        &mut self,
        hay: &[u8],
        at: &mut usize,
        sid: &mut LazyStateID,
        last_end: &mut Option<usize>,
    ) -> Result<Scan<Option<usize>>, MatchError> {
        let dfa = self.regex.forward();
        let cache = self.cache.forward_mut();
        let stop =
            at.saturating_add(BYTES_PER_CLOCK_CHECK).min(hay.len());
        while *at < stop {
            if sid.is_start()
                && last_end.is_none()
                && let Some(pre) = dfa.get_config().get_prefilter()
            {
                let needle = pre.max_needle_len();
                let window = at
                    .saturating_add(PREFILTER_WINDOW.max(2 * needle))
                    .min(hay.len());
                let next =
                    match pre.find(hay, Span::from(*at..window)) {
                        Some(span) => span.start,
                        None if window == hay.len() => {
                            return Ok(Scan::Done(None));
                        }
                        None => window + 1 - needle,
                    };
                if next > *at {
                    *at = next;
                    if !self.universal_start {
                        *sid = dfa.start_state_forward(
                            cache,
                            &Input::new(hay).span(next..hay.len()),
                        )?;
                    }
                    continue;
                }
            }
            *sid = dfa
                .next_state(cache, *sid, hay[*at])
                .map_err(|_| MatchError::gave_up(*at))?;
            if sid.is_match() {
                *last_end = Some(*at);
            } else if sid.is_dead() {
                return Ok(Scan::Done(*last_end));
            } else if sid.is_quit() {
                return Err(MatchError::quit(hay[*at], *at));
            }
            *at += 1;
        }
        if *at < hay.len() {
            return Ok(Scan::Unfinished);
        }
        *sid = dfa
            .next_eoi_state(cache, *sid)
            .map_err(|_| MatchError::gave_up(*at))?;
        if sid.is_match() {
            *last_end = Some(*at);
        }
        Ok(Scan::Done(*last_end))
    }

    fn reverse(
        &mut self,
        hay: &[u8],
        from: usize,
        at: &mut usize,
        sid: &mut LazyStateID,
        start: &mut Option<usize>,
    ) -> Result<Scan<usize>, MatchError> {
        let dfa = self.regex.reverse();
        let cache = self.cache.reverse_mut();
        let stop = at.saturating_sub(BYTES_PER_CLOCK_CHECK).max(from);
        let found = |start: Option<usize>| {
            start.expect("a reverse scan finds the forward match")
        };
        while *at > stop {
            let byte = hay[*at - 1];
            *sid = dfa
                .next_state(cache, *sid, byte)
                .map_err(|_| MatchError::gave_up(*at - 1))?;
            if sid.is_match() {
                *start = Some(*at);
            } else if sid.is_dead() {
                return Ok(Scan::Done(found(*start)));
            } else if sid.is_quit() {
                return Err(MatchError::quit(byte, *at - 1));
            }
            *at -= 1;
        }
        if *at > from {
            return Ok(Scan::Unfinished);
        }
        match from.checked_sub(1).map(|before| hay[before]) {
            Some(byte) => {
                *sid = dfa
                    .next_state(cache, *sid, byte)
                    .map_err(|_| MatchError::gave_up(from))?;
                if sid.is_quit() {
                    return Err(MatchError::quit(byte, from - 1));
                }
            }
            None => {
                *sid = dfa
                    .next_eoi_state(cache, *sid)
                    .map_err(|_| MatchError::gave_up(from))?;
            }
        }
        if sid.is_match() {
            *start = Some(from);
        }
        Ok(Scan::Done(found(*start)))
    }
}

fn crlf_newlines(ast: &mut Ast) {
    match ast {
        Ast::Literal(literal) if literal.c == '\n' => {
            *ast = Parser::new()
                .parse(r"(?:\r?\n)")
                .expect("a valid pattern");
        }
        Ast::Repetition(repetition) => {
            crlf_newlines(&mut repetition.ast)
        }
        Ast::Group(group) => crlf_newlines(&mut group.ast),
        Ast::Alternation(alternation) => {
            alternation.asts.iter_mut().for_each(crlf_newlines);
        }
        Ast::Concat(concat) => {
            concat.asts.iter_mut().for_each(crlf_newlines);
        }
        _ => {}
    }
}

fn most_breaks(hir: &Hir) -> Option<u32> {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => Some(0),
        HirKind::Literal(literal) => u32::try_from(
            literal.0.iter().filter(|&&b| b == b'\n').count(),
        )
        .ok(),
        HirKind::Class(Class::Unicode(class)) => Some(u32::from(
            class
                .ranges()
                .iter()
                .any(|r| r.start() <= '\n' && '\n' <= r.end()),
        )),
        HirKind::Class(Class::Bytes(class)) => Some(u32::from(
            class
                .ranges()
                .iter()
                .any(|r| r.start() <= b'\n' && b'\n' <= r.end()),
        )),
        HirKind::Repetition(repetition) => {
            match most_breaks(&repetition.sub)? {
                0 => Some(0),
                breaks => breaks.checked_mul(repetition.max?),
            }
        }
        HirKind::Capture(capture) => most_breaks(&capture.sub),
        HirKind::Concat(subs) => {
            subs.iter().try_fold(0_u32, |sum, sub| {
                sum.checked_add(most_breaks(sub)?)
            })
        }
        HirKind::Alternation(subs) => {
            subs.iter().try_fold(0_u32, |most, sub| {
                Some(most.max(most_breaks(sub)?))
            })
        }
    }
}

fn on_cluster_boundaries(
    buffer: &Buffer,
    text: &str,
    range: &Range<usize>,
) -> bool {
    let bytes = text.as_bytes();
    let on_boundary = |at: usize| {
        match (at.checked_sub(1).map(|i| bytes[i]), bytes.get(at)) {
            // Between two ASCII bytes, only `\r\n` stays one cluster.
            (Some(before), Some(&after))
                if before.is_ascii() && after.is_ascii() =>
            {
                (before, after) != (b'\r', b'\n')
            }
            _ => snap(buffer, at) == at,
        }
    };
    on_boundary(range.start) && on_boundary(range.end)
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
