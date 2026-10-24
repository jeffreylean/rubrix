//! The shell lexer, for `.sh`, `.bash`, `.zsh`, and files with a
//! `sh`, `bash`, or `zsh` shebang.
//!
//! Command names are `function`, built-ins `function.builtin`, and a
//! name that `name()` or `function name` defines is
//! `function.definition`. Flow words such as `if` and `done` are
//! `keyword.control`, and `local` or `export` are
//! `keyword.declaration`. A word is a command or a keyword only at
//! the start of a command, so `echo if` keeps `if` plain.
//!
//! `$name` and `${name}` are `variable`, and `$1`, `$@`, and the
//! like are `variable.special`. `$(`, `${`, and their closers are
//! `punctuation.special`. Heredoc delimiters are `string.special`
//! and the body is `string`, with expansions lexed inside it unless
//! the delimiter is quoted.
//!
//! A `#` starts a comment only at the start of a word, so `a#b` is
//! one word. `<<` opens a heredoc outside `$(( ))` and `(( ))`, and
//! `<<<` is a here-string.
//!
//! # State
//!
//! A stack of open constructs, with a bound of [`DEPTH`] that drops
//! deeper ones, lets a string, a `$(...)`, a `${...}`, or a heredoc
//! body span lines. The tail holds the stack bytes and then one
//! entry for each heredoc whose body has not ended: a flags byte,
//! the delimiter with its quotes removed, and a newline. The word
//! holds the stack depth and whether the line ended in a backslash.
//! A heredoc body starts on the line after its `<<`, so the line
//! that opens one lexes as code.
//!
//! Not handled: `case` patterns inside `$(...)`, which close it at
//! their `)`, and a second heredoc opened inside a heredoc body.

use alloc::borrow::Cow;
use alloc::vec::Vec;

use memchr::memchr;

use super::{ident_end, is_ident_start, space_end};
use crate::{Kind, Runs, State};

const DEPTH: usize = 32;
const CONT: u64 = 1;
const DEPTH_SHIFT: u32 = 8;
const QUOTED: u8 = 1;
const DASH: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Frame {
    Cmd = 1,
    Paren,
    Arith,
    Array,
    Brace,
    Tick,
    Dq,
    Sq,
    AnsiC,
    Here,
}

impl Frame {
    const fn from_byte(byte: u8) -> Frame {
        match byte {
            2 => Frame::Paren,
            3 => Frame::Arith,
            4 => Frame::Array,
            5 => Frame::Brace,
            6 => Frame::Tick,
            7 => Frame::Dq,
            8 => Frame::Sq,
            9 => Frame::AnsiC,
            10 => Frame::Here,
            _ => Frame::Cmd,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stack {
    frames: [Frame; DEPTH],
    len: usize,
}

impl Stack {
    fn load(bytes: &[u8]) -> Stack {
        let mut stack = Stack {
            frames: [Frame::Cmd; DEPTH],
            len: bytes.len().min(DEPTH),
        };
        for (frame, &byte) in stack.frames.iter_mut().zip(bytes) {
            *frame = Frame::from_byte(byte);
        }
        stack
    }

    fn top(&self) -> Option<Frame> {
        self.len.checked_sub(1).map(|n| self.frames[n])
    }

    fn push(&mut self, frame: Frame) -> bool {
        let room = self.len < DEPTH;
        if room {
            self.frames[self.len] = frame;
            self.len += 1;
        }
        room
    }

    fn pop(&mut self) {
        self.len = self.len.saturating_sub(1);
    }

    fn bytes(&self) -> impl Iterator<Item = u8> + '_ {
        self.frames[..self.len].iter().map(|&frame| frame as u8)
    }
}

fn front(queue: &[u8]) -> Option<(u8, &[u8])> {
    let (&flags, rest) = queue.split_first()?;
    let end = memchr(b'\n', rest).unwrap_or(rest.len());
    Some((flags, &rest[..end]))
}

fn closes_heredoc(line: &[u8], flags: u8, delimiter: &[u8]) -> bool {
    let tabs = if flags & DASH == 0 {
        0
    } else {
        line.iter().take_while(|&&b| b == b'\t').count()
    };
    &line[tabs..] == delimiter
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    Any,
    ForVar,
    FnName,
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let word = state.word();
    let tail = state.tail();
    let depth = ((word >> DEPTH_SHIFT) & 0xFF) as usize;
    let depth = depth.min(tail.len()).min(DEPTH);
    let mut lexer = Lexer {
        line,
        s: line.as_bytes(),
        i: 0,
        out,
        stack: Stack::load(&tail[..depth]),
        queue: Cow::Borrowed(&tail[depth..]),
        queue_changed: false,
        opened_heredoc: false,
        cmd: false,
        restore_cmd: false,
        expect: Expect::Any,
        awaiting_in: false,
        cont: false,
    };
    lexer.run(word & CONT != 0);
    lexer.store(state, &tail[..depth])
}

struct Lexer<'a, 'o, 'r> {
    line: &'a str,
    s: &'a [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    stack: Stack,
    queue: Cow<'a, [u8]>,
    queue_changed: bool,
    opened_heredoc: bool,
    cmd: bool,
    restore_cmd: bool,
    expect: Expect,
    awaiting_in: bool,
    cont: bool,
}

impl Lexer<'_, '_, '_> {
    fn run(&mut self, continued: bool) {
        let code = matches!(
            self.stack.top(),
            None | Some(Frame::Cmd | Frame::Paren | Frame::Tick)
        );
        self.cmd = code && !continued;
        while self.i < self.s.len() {
            match self.stack.top() {
                Some(Frame::Dq) => self.string(true),
                Some(Frame::Sq) => self.single_body(),
                Some(Frame::AnsiC) => self.ansi_c(),
                Some(Frame::Here) => self.here(),
                _ => self.code(),
            }
        }
        if self.opened_heredoc && !self.stack.push(Frame::Here) {
            self.queue = Cow::Borrowed(&[]);
            self.queue_changed = true;
        }
    }

    fn store(self, old: &State, old_frames: &[u8]) -> State {
        let word = u64::from(self.cont)
            | (self.stack.len as u64) << DEPTH_SHIFT;
        let same_frames =
            self.stack.bytes().eq(old_frames.iter().copied());
        if !self.queue_changed && same_frames {
            return old.with_word(word);
        }
        let mut tail: Vec<u8> = self.stack.bytes().collect();
        tail.extend_from_slice(&self.queue);
        State::with_tail(word, &tail)
    }

    fn emit(&mut self, end: usize, kind: Kind) {
        self.out.push(end, kind);
        self.i = end;
    }

    fn at(&self, offset: usize) -> Option<u8> {
        self.s.get(self.i + offset).copied()
    }

    fn open(&mut self, frame: Frame, cmd: bool) -> bool {
        self.stack.push(frame);
        self.cmd = cmd;
        if cmd {
            self.restore_cmd = false;
        }
        true
    }

    fn start_command(&mut self) {
        self.cmd = !matches!(
            self.stack.top(),
            Some(Frame::Arith | Frame::Array)
        );
        self.restore_cmd = false;
    }

    fn code(&mut self) {
        let s = self.s;
        while self.i < s.len() {
            let b = s[self.i];
            if b.is_ascii_whitespace() {
                let end = space_end(s, self.i);
                self.out.space(end);
                self.i = end;
                if core::mem::take(&mut self.restore_cmd) {
                    self.cmd = true;
                }
                continue;
            }
            let changed = match b {
                b'#' if self.starts_comment() => {
                    self.emit(s.len(), Kind::Comment);
                    false
                }
                b'\'' => self.single(),
                b'"' => {
                    self.emit(self.i + 1, Kind::String);
                    self.open(Frame::Dq, false)
                }
                b'`' => self.backtick(),
                b'$' => self.dollar(false),
                b'\\' => {
                    self.escape();
                    false
                }
                b'(' => self.paren(),
                b')' => self.close_paren(),
                b'}' if self.stack.top() == Some(Frame::Brace) => {
                    self.emit(self.i + 1, Kind::PunctuationSpecial);
                    self.stack.pop();
                    self.cmd = false;
                    true
                }
                b';' => {
                    let len = match (self.at(1), self.at(2)) {
                        (Some(b';'), Some(b'&')) => 3,
                        (Some(b';' | b'&'), _) => 2,
                        _ => 1,
                    };
                    self.emit(
                        self.i + len,
                        Kind::PunctuationDelimiter,
                    );
                    self.start_command();
                    false
                }
                b'&' if self.at(1) == Some(b'>') => self.redirect(),
                b'&' | b'|' => {
                    let doubled = self.at(1) == Some(b)
                        || (b == b'|' && self.at(1) == Some(b'&'));
                    let len = if doubled { 2 } else { 1 };
                    self.emit(self.i + len, Kind::Operator);
                    self.start_command();
                    false
                }
                b'<' | b'>' => self.redirect(),
                _ => {
                    self.word();
                    false
                }
            };
            if changed {
                return;
            }
        }
    }

    fn starts_comment(&self) -> bool {
        self.stack.top() != Some(Frame::Brace)
            && (self.i == 0
                || self.s[self.i - 1].is_ascii_whitespace()
                || matches!(
                    self.s[self.i - 1],
                    b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>'
                ))
    }

    fn single(&mut self) -> bool {
        let start = self.i;
        self.cmd = false;
        match memchr(b'\'', &self.s[start + 1..]) {
            Some(k) => {
                self.emit(start + k + 2, Kind::String);
                false
            }
            None => {
                self.emit(self.s.len(), Kind::String);
                self.open(Frame::Sq, false)
            }
        }
    }

    fn single_body(&mut self) {
        match memchr(b'\'', &self.s[self.i..]) {
            Some(k) => {
                self.emit(self.i + k + 1, Kind::String);
                self.stack.pop();
                self.cmd = false;
            }
            None => self.emit(self.s.len(), Kind::String),
        }
    }

    fn ansi_c(&mut self) {
        let s = self.s;
        let found = s[self.i..]
            .iter()
            .position(|&b| b == b'\'' || b == b'\\')
            .map(|k| self.i + k);
        let Some(k) = found else {
            self.emit(s.len(), Kind::String);
            return;
        };
        if s[k] == b'\'' {
            self.emit(k + 1, Kind::String);
            self.stack.pop();
            self.cmd = false;
        } else {
            self.emit(k, Kind::String);
            let escaped = self.line[k + 1..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            self.emit(k + 1 + escaped, Kind::StringEscape);
        }
    }

    fn backtick(&mut self) -> bool {
        self.emit(self.i + 1, Kind::PunctuationSpecial);
        if self.stack.top() == Some(Frame::Tick) {
            self.stack.pop();
            self.cmd = false;
            true
        } else {
            self.open(Frame::Tick, true)
        }
    }

    fn paren(&mut self) -> bool {
        if self.expect == Expect::ForVar {
            self.expect = Expect::Any;
            self.awaiting_in = false;
        }
        let s = self.s;
        let i = self.i;
        let frame = if i > 0 && s[i - 1] == b'(' {
            Frame::Arith
        } else if i > 0 && s[i - 1] == b'=' {
            Frame::Array
        } else if matches!(
            self.stack.top(),
            Some(Frame::Arith | Frame::Array)
        ) {
            Frame::Arith
        } else {
            Frame::Paren
        };
        self.emit(i + 1, Kind::PunctuationBracket);
        self.open(frame, frame == Frame::Paren)
    }

    fn close_paren(&mut self) -> bool {
        let kind = match self.stack.top() {
            Some(Frame::Cmd) => Kind::PunctuationSpecial,
            Some(Frame::Paren | Frame::Arith | Frame::Array) => {
                Kind::PunctuationBracket
            }
            _ => {
                self.emit(self.i + 1, Kind::PunctuationBracket);
                self.start_command();
                return false;
            }
        };
        self.emit(self.i + 1, kind);
        self.cmd = self.stack.top() == Some(Frame::Paren);
        self.stack.pop();
        true
    }

    fn escape(&mut self) {
        let at = self.i;
        match self.line[at + 1..].chars().next() {
            None => {
                self.emit(at + 1, Kind::StringEscape);
                self.cont = true;
            }
            Some(c) => {
                self.emit(at + 1 + c.len_utf8(), Kind::StringEscape);
                self.cmd = false;
            }
        }
    }

    fn dollar(&mut self, in_string: bool) -> bool {
        let s = self.s;
        let i = self.i;
        match self.at(1) {
            Some(b'(') => {
                self.emit(i + 2, Kind::PunctuationSpecial);
                self.open(Frame::Cmd, true)
            }
            Some(b'{') => {
                self.emit(i + 2, Kind::PunctuationSpecial);
                self.open(Frame::Brace, false);
                self.brace_start();
                true
            }
            Some(b'\'') if !in_string => {
                self.emit(i + 2, Kind::String);
                self.open(Frame::AnsiC, false)
            }
            Some(b'"') if !in_string => {
                self.emit(i + 2, Kind::String);
                self.open(Frame::Dq, false)
            }
            Some(b) if is_ident_start(b) => {
                self.emit(ident_end(s, i + 1), Kind::Variable);
                false
            }
            Some(
                b'0'..=b'9' | b'@' | b'*' | b'#' | b'?' | b'$' | b'!',
            ) => {
                self.emit(i + 2, Kind::VariableSpecial);
                false
            }
            _ => {
                let kind = if in_string {
                    Kind::String
                } else {
                    Kind::PunctuationSpecial
                };
                self.emit(i + 1, kind);
                false
            }
        }
    }

    fn brace_start(&mut self) {
        let s = self.s;
        if matches!(self.at(0), Some(b'!' | b'#'))
            && self.at(1).is_some_and(|b| b != b'}')
        {
            self.emit(self.i + 1, Kind::Operator);
        }
        let j = self.i;
        let name = match s.get(j) {
            Some(&b) if is_ident_start(b) => {
                Some((ident_end(s, j), Kind::Variable))
            }
            Some(b'0'..=b'9') => {
                let digits =
                    s[j..].iter().take_while(|b| b.is_ascii_digit());
                Some((j + digits.count(), Kind::VariableSpecial))
            }
            Some(b'@' | b'*' | b'#' | b'?' | b'$' | b'!') => {
                Some((j + 1, Kind::VariableSpecial))
            }
            _ => None,
        };
        if let Some((end, kind)) = name {
            self.emit(end, kind);
        }
        if self.at(0) == Some(b'[') {
            let j = self.i;
            self.emit(j + 1, Kind::PunctuationBracket);
            let close = memchr(b']', &s[j + 1..]).map(|k| j + 1 + k);
            let end = close.unwrap_or(s.len());
            let kind = match &s[j + 1..end] {
                b"@" | b"*" => Kind::VariableSpecial,
                _ => Kind::Plain,
            };
            self.emit(end, kind);
            if close.is_some() {
                self.emit(end + 1, Kind::PunctuationBracket);
            }
        }
        let operator = s[self.i..]
            .iter()
            .take_while(|b| b":-=+?#%/^,@".contains(b))
            .count();
        self.emit(self.i + operator, Kind::Operator);
    }

    fn redirect(&mut self) -> bool {
        let b = self.s[self.i];
        let (next, after) = (self.at(1), self.at(2));
        let len = match (b, next, after) {
            (b'<', Some(b'<'), _) => return self.heredoc(),
            (b'<' | b'>', Some(b'('), _) => {
                self.emit(self.i + 2, Kind::PunctuationSpecial);
                return self.open(Frame::Cmd, true);
            }
            (b'&', _, Some(b'>')) => 3,
            (b'&', ..) | (b'>', Some(b'>' | b'&' | b'|'), _) => 2,
            (b'<', Some(b'&' | b'>'), _) => 2,
            _ => 1,
        };
        self.emit(self.i + len, Kind::Operator);
        false
    }

    fn heredoc(&mut self) -> bool {
        let s = self.s;
        let start = self.i;
        if self.at(2) == Some(b'<') {
            self.emit(start + 3, Kind::Operator);
            return false;
        }
        let dash = self.at(2) == Some(b'-');
        let operator = start + 2 + usize::from(dash);
        let word = space_end(s, operator);
        let (end, quoted, delimiter) = delimiter(s, word);
        if delimiter.is_empty()
            || self.stack.top() == Some(Frame::Arith)
        {
            self.emit(start + 2, Kind::Operator);
            return false;
        }
        let flags =
            (u8::from(quoted) * QUOTED) | (u8::from(dash) * DASH);
        let queue = self.queue.to_mut();
        queue.push(flags);
        queue.extend_from_slice(&delimiter);
        queue.push(b'\n');
        self.queue_changed = true;
        self.opened_heredoc = true;
        self.emit(operator, Kind::Operator);
        self.out.space(word);
        self.emit(end, Kind::StringSpecial);
        false
    }

    fn here(&mut self) {
        let (flags, closes) = match front(&self.queue) {
            Some((flags, delimiter)) => (
                flags,
                self.i == 0
                    && closes_heredoc(self.s, flags, delimiter),
            ),
            None => (0, false),
        };
        if closes {
            self.emit(self.s.len(), Kind::StringSpecial);
            self.pop_heredoc();
            if self.queue.is_empty() {
                self.stack.pop();
            }
        } else if flags & QUOTED != 0 {
            self.emit(self.s.len(), Kind::String);
        } else {
            self.string(false);
        }
    }

    fn pop_heredoc(&mut self) {
        let len = front(&self.queue)
            .map_or(0, |(_, delimiter)| delimiter.len() + 2)
            .min(self.queue.len());
        match &mut self.queue {
            Cow::Borrowed(bytes) => *bytes = &bytes[len..],
            Cow::Owned(bytes) => {
                bytes.drain(..len);
            }
        }
        self.queue_changed = true;
    }

    fn string(&mut self, quoted: bool) {
        let s = self.s;
        loop {
            let found = s[self.i..]
                .iter()
                .position(|&b| {
                    matches!(b, b'$' | b'`' | b'\\')
                        || (quoted && b == b'"')
                })
                .map(|k| self.i + k);
            let Some(k) = found else {
                self.emit(s.len(), Kind::String);
                return;
            };
            match s[k] {
                b'"' => {
                    self.emit(k + 1, Kind::String);
                    self.stack.pop();
                    self.cmd = false;
                    return;
                }
                b'\\' => {
                    let escapes = match s.get(k + 1) {
                        Some(b'$' | b'`' | b'\\') | None => true,
                        Some(b'"') => quoted,
                        _ => false,
                    };
                    if escapes {
                        self.emit(k, Kind::String);
                        self.emit(
                            (k + 2).min(s.len()),
                            Kind::StringEscape,
                        );
                    } else {
                        self.emit(k + 1, Kind::String);
                    }
                }
                b'$' => {
                    self.emit(k, Kind::String);
                    if self.dollar(true) {
                        return;
                    }
                }
                _ => {
                    self.emit(k, Kind::String);
                    self.emit(k + 1, Kind::PunctuationSpecial);
                    self.open(Frame::Tick, true);
                    return;
                }
            }
        }
    }

    fn word(&mut self) {
        let s = self.s;
        let start = self.i;
        let in_brace = self.stack.top() == Some(Frame::Brace);
        if !in_brace && is_ident_start(s[start]) {
            let name = ident_end(s, start);
            let eq = match (s.get(name), s.get(name + 1)) {
                (Some(b'='), _) => Some(name),
                (Some(b'+'), Some(b'=')) => Some(name + 1),
                _ => None,
            };
            if let Some(eq) = eq {
                self.emit(name, Kind::Variable);
                self.emit(eq + 1, Kind::Operator);
                if self.cmd {
                    self.cmd = false;
                    self.restore_cmd = true;
                }
                return;
            }
        }
        if self.cmd
            && self.stack.top().is_none()
            && let Some(close) = case_pattern_end(s, start)
        {
            self.cmd = false;
            self.emit(close, Kind::Plain);
            return;
        }
        let mut end = start + 1;
        while end < s.len() && !is_delimiter(s[end], in_brace) {
            end += 1;
        }
        let kind = self.classify(&s[start..end], end);
        self.emit(end, kind);
    }

    fn classify(&mut self, word: &[u8], end: usize) -> Kind {
        match core::mem::replace(&mut self.expect, Expect::Any) {
            Expect::FnName => {
                self.cmd = true;
                return Kind::FunctionDefinition;
            }
            Expect::ForVar => {
                self.cmd = false;
                return Kind::Variable;
            }
            Expect::Any => {}
        }
        let cmd = core::mem::replace(&mut self.cmd, false);
        match word {
            b"[" | b"[[" | b"]" | b"]]" => {
                return Kind::PunctuationBracket;
            }
            b"=" | b"==" | b"!=" | b"=~" if !cmd => {
                return Kind::Operator;
            }
            _ => {}
        }
        if !cmd {
            return self.argument(word);
        }
        let s = self.s;
        let after = space_end(s, end);
        if word.iter().all(u8::is_ascii_digit)
            && matches!(s.get(end), Some(b'<' | b'>'))
        {
            self.cmd = true;
            return Kind::Number;
        }
        match word {
            b"{" => {
                self.cmd = true;
                return Kind::PunctuationBracket;
            }
            b"}" => return Kind::PunctuationBracket,
            b"!" => {
                self.cmd = true;
                return Kind::Operator;
            }
            _ => {}
        }
        if let Some(kind) = self.keyword(word) {
            return kind;
        }
        let defines = s.get(after) == Some(&b'(')
            && s.get(space_end(s, after + 1)) == Some(&b')');
        if defines {
            self.cmd = true;
            return Kind::FunctionDefinition;
        }
        if is_builtin(word) {
            Kind::FunctionBuiltin
        } else {
            Kind::Function
        }
    }

    fn keyword(&mut self, word: &[u8]) -> Option<Kind> {
        Some(match word {
            b"if" | b"then" | b"else" | b"elif" | b"while"
            | b"until" | b"do" => {
                self.cmd = true;
                Kind::KeywordControl
            }
            b"time" => {
                self.cmd = true;
                Kind::Keyword
            }
            b"fi" | b"done" | b"esac" | b"return" | b"break"
            | b"continue" | b"exit" => Kind::KeywordControl,
            b"for" | b"select" => {
                self.expect = Expect::ForVar;
                self.awaiting_in = true;
                Kind::KeywordControl
            }
            b"case" => {
                self.awaiting_in = true;
                Kind::KeywordControl
            }
            b"function" => {
                self.expect = Expect::FnName;
                Kind::KeywordDeclaration
            }
            b"local" | b"declare" | b"typeset" | b"readonly"
            | b"export" => Kind::KeywordDeclaration,
            _ => return None,
        })
    }

    fn argument(&mut self, word: &[u8]) -> Kind {
        if self.awaiting_in && word == b"in" {
            self.awaiting_in = false;
            return Kind::KeywordControl;
        }
        match word {
            [b'-', first, ..]
                if first.is_ascii_alphabetic() || *first == b'-' =>
            {
                Kind::Operator
            }
            _ if word.iter().all(u8::is_ascii_digit) => Kind::Number,
            _ => Kind::Plain,
        }
    }
}

fn delimiter(s: &[u8], start: usize) -> (usize, bool, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut quoted = false;
    let mut j = start;
    while j < s.len() {
        match s[j] {
            quote @ (b'\'' | b'"') => {
                quoted = true;
                let close = memchr(quote, &s[j + 1..])
                    .map_or(s.len(), |k| j + 1 + k);
                bytes.extend_from_slice(&s[j + 1..close]);
                j = (close + 1).min(s.len());
            }
            b'\\' => {
                quoted = true;
                if let Some(&next) = s.get(j + 1) {
                    bytes.push(next);
                }
                j = (j + 2).min(s.len());
            }
            b if is_delimiter(b, false) => break,
            b => {
                bytes.push(b);
                j += 1;
            }
        }
    }
    (j, quoted, bytes)
}

fn case_pattern_end(s: &[u8], start: usize) -> Option<usize> {
    let len = s[start..]
        .iter()
        .take_while(|&&b| {
            b.is_ascii_alphanumeric()
                || b"_*?.-|[]/+~,:@%^=".contains(&b)
        })
        .count();
    (len > 0 && s.get(start + len) == Some(&b')'))
        .then_some(start + len)
}

const fn is_delimiter(b: u8, in_brace: bool) -> bool {
    b.is_ascii_whitespace()
        || matches!(
            b,
            b';' | b'&'
                | b'|'
                | b'('
                | b')'
                | b'<'
                | b'>'
                | b'\''
                | b'"'
                | b'`'
                | b'$'
                | b'\\'
        )
        || (in_brace && b == b'}')
}

fn is_builtin(word: &[u8]) -> bool {
    matches!(
        word,
        b"." | b":"
            | b"alias"
            | b"bg"
            | b"bind"
            | b"builtin"
            | b"caller"
            | b"cd"
            | b"command"
            | b"compgen"
            | b"complete"
            | b"dirs"
            | b"disown"
            | b"echo"
            | b"enable"
            | b"eval"
            | b"exec"
            | b"false"
            | b"fc"
            | b"fg"
            | b"getopts"
            | b"hash"
            | b"help"
            | b"history"
            | b"jobs"
            | b"kill"
            | b"let"
            | b"mapfile"
            | b"popd"
            | b"printf"
            | b"pushd"
            | b"pwd"
            | b"read"
            | b"readarray"
            | b"set"
            | b"shift"
            | b"shopt"
            | b"source"
            | b"suspend"
            | b"test"
            | b"times"
            | b"trap"
            | b"true"
            | b"type"
            | b"ulimit"
            | b"umask"
            | b"unalias"
            | b"unset"
            | b"wait"
    )
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use crate::{Lang, State, check_line, lex_line};

    #[test]
    fn any_state_lexes() {
        let lines = [
            "",
            "cat <<-'EOF' > out",
            "echo \"$(a \"${b:-c}\")\" 'x' $'y\\n' `z` # c",
            "EOF",
            "\tEOF",
            "x=(1 2) ${a[@]##*/}",
        ];
        let tails: [&[u8]; 6] = [
            &[],
            &[7],
            &[1, 1, 1, 6, 7, 8, 9, 10, 200],
            &[10, 3, b'E', b'O', b'F', b'\n'],
            &[0, 255, 1, 2, 3],
            &[10, 0, b'a'],
        ];
        for word in [0, 1, 3 << 8, u64::MAX, 40 << 8 | 1, 2 << 8] {
            for tail in tails {
                let state = State::with_tail(word, tail);
                for line in lines {
                    let mut runs = Vec::new();
                    lex_line(Lang::Shell, line, &state, &mut runs);
                    check_line(line, &runs).unwrap();
                }
            }
        }
    }
}
