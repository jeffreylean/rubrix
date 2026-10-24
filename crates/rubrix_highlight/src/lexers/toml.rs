//! TOML 1.0.
//!
//! A line is lexed in one of two modes. In key mode a bare or
//! quoted key is [`Kind::Property`], a `.` between key parts is a
//! delimiter, and `[table]` and `[[array]]` headers are one
//! [`Kind::Title`] run each. Mode switches to value mode after `=`,
//! where strings are [`Kind::String`] with [`Kind::StringEscape`]
//! escapes, dates and times are [`Kind::StringSpecial`], and
//! integers, floats, `inf`, and `nan` are [`Kind::Number`].
//!
//! Multi-line strings (`"""` and `'''`) and multi-line arrays span
//! lines. The state holds the open string and a stack of the open
//! arrays and inline tables, 48 levels deep. Deeper levels are
//! counted but read as arrays.
//!
//! Not handled: the whitespace that a line-ending backslash trims
//! at the start of the next line is plain string text, dates and
//! times are matched by shape, without range checks, and a bare
//! word that is no key, string, number, or date is
//! [`Kind::Plain`]. An array or inline table that is never closed
//! swallows the rest of the file as its contents, with no `[table]`
//! headers. Nothing checks that the text is valid TOML.

use memchr::{memchr, memchr2};

use super::space_end;
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    Basic,
    Literal,
}

const OPEN: u64 = 0b111;
const DEPTH_SHIFT: u32 = 8;
const DEPTH_MAX: u64 = 63;
const STACK_SHIFT: u32 = 16;
const STACK_LEVELS: u32 = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stack {
    depth: u32,
    tables: u64,
}

impl Stack {
    fn is_table(self) -> bool {
        match self.depth.checked_sub(1) {
            Some(top) if top < STACK_LEVELS => {
                self.tables >> top & 1 == 1
            }
            _ => false,
        }
    }

    fn push(&mut self, table: bool) {
        if self.depth < STACK_LEVELS {
            self.tables |= u64::from(table) << self.depth;
        }
        self.depth = (self.depth + 1).min(DEPTH_MAX as u32);
    }

    fn pop(&mut self) {
        let Some(top) = self.depth.checked_sub(1) else {
            return;
        };
        self.depth = top;
        if top < STACK_LEVELS {
            self.tables &= !(1 << top);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mode {
    open: Open,
    stack: Stack,
}

impl Mode {
    fn load(state: &State) -> Mode {
        let word = state.word();
        let depth = ((word >> DEPTH_SHIFT) & DEPTH_MAX) as u32;
        let levels = depth.min(STACK_LEVELS);
        let tables = (word >> STACK_SHIFT) & ((1 << levels) - 1);
        let open = match word & OPEN {
            1 => Open::Basic,
            2 => Open::Literal,
            _ => Open::Code,
        };
        Mode {
            open,
            stack: Stack { depth, tables },
        }
    }

    fn store(self) -> State {
        let open = match self.open {
            Open::Code => 0,
            Open::Basic => 1,
            Open::Literal => 2,
        };
        State::from_word(
            open | u64::from(self.stack.depth) << DEPTH_SHIFT
                | self.stack.tables << STACK_SHIFT,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Key,
    Value,
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let Mode { mut open, stack } = Mode::load(state);
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        stack,
        side: Side::Key,
    };
    if open == Open::Code {
        lexer.side = lexer.inside();
    } else {
        lexer.side = Side::Value;
    }
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Basic => lexer.multi_basic(),
            Open::Literal => lexer.multi_literal(),
        };
    }
    Mode {
        open,
        stack: lexer.stack,
    }
    .store()
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    stack: Stack,
    side: Side,
}

impl Lexer<'_, '_, '_> {
    fn emit(&mut self, end: usize, kind: Kind) {
        self.out.push(end, kind);
        self.i = end;
    }

    fn inside(&self) -> Side {
        if self.stack.depth > 0 && !self.stack.is_table() {
            Side::Value
        } else {
            Side::Key
        }
    }

    fn code(&mut self) -> Open {
        while self.i < self.s.len() {
            let b = self.s[self.i];
            if b.is_ascii_whitespace() {
                let end = space_end(self.s, self.i);
                self.out.space(end);
                self.i = end;
                continue;
            }
            match b {
                b'#' => self.emit(self.s.len(), Kind::Comment),
                b'=' => {
                    self.side = Side::Value;
                    self.emit(self.i + 1, Kind::Operator);
                }
                b',' => {
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                    self.side = self.inside();
                }
                b'{' => {
                    self.stack.push(true);
                    self.side = Side::Key;
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'}' | b']' => {
                    self.stack.pop();
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                    self.side = if self.stack.is_table() {
                        Side::Key
                    } else {
                        Side::Value
                    };
                }
                b'[' if self.side == Side::Key
                    && self.stack.depth == 0 =>
                {
                    self.header();
                }
                b'[' => {
                    self.stack.push(false);
                    self.side = Side::Value;
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                _ if self.side == Side::Key => self.key(b),
                _ => {
                    if let Some(open) = self.value(b) {
                        return open;
                    }
                }
            }
        }
        Open::Code
    }

    fn header(&mut self) {
        let s = self.s;
        let double = s.get(self.i + 1) == Some(&b'[');
        let mut j = self.i + 1 + usize::from(double);
        while j < s.len() {
            match s[j] {
                b'"' => j = basic_end(s, j + 1),
                b'\'' => j = literal_end(s, j + 1),
                b'#' => break,
                b']' => {
                    j += 1 + usize::from(
                        double && s.get(j + 1) == Some(&b']'),
                    );
                    break;
                }
                _ => j += 1,
            }
        }
        self.emit(j, Kind::Title);
    }

    fn key(&mut self, b: u8) {
        let s = self.s;
        match b {
            b'"' => {
                self.emit(basic_end(s, self.i + 1), Kind::Property)
            }
            b'\'' => {
                self.emit(literal_end(s, self.i + 1), Kind::Property);
            }
            b'.' => {
                self.emit(self.i + 1, Kind::PunctuationDelimiter);
            }
            _ if is_bare(b) => {
                let mut end = self.i + 1;
                while end < s.len() && is_bare(s[end]) {
                    end += 1;
                }
                self.emit(end, Kind::Property);
            }
            _ => self.stray(),
        }
    }

    fn value(&mut self, b: u8) -> Option<Open> {
        let s = self.s;
        match b {
            b'"' if s[self.i..].starts_with(b"\"\"\"") => {
                self.emit(self.i + 3, Kind::String);
                return Some(Open::Basic);
            }
            b'\'' if s[self.i..].starts_with(b"'''") => {
                self.emit(self.i + 3, Kind::String);
                return Some(Open::Literal);
            }
            b'"' => self.basic(),
            b'\'' => {
                self.emit(literal_end(s, self.i + 1), Kind::String);
            }
            b'0'..=b'9' | b'+' | b'-' => self.number(),
            _ if b.is_ascii_alphabetic() || b == b'_' => self.word(),
            _ => self.stray(),
        }
        None
    }

    fn basic(&mut self) {
        let s = self.s;
        let mut j = self.i + 1;
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                self.emit(k + 1, Kind::String);
                return;
            }
            let Some(len) = escape_len(s, k) else {
                j = k + 1;
                continue;
            };
            self.emit(k, Kind::String);
            j = k + len;
            self.emit(j, Kind::StringEscape);
        }
        self.emit(s.len(), Kind::String);
    }

    fn multi_basic(&mut self) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                let quotes = quote_run(s, k, b'"');
                if quotes >= 3 {
                    self.emit(k + quotes.min(5), Kind::String);
                    return Open::Code;
                }
                j = k + quotes;
                continue;
            }
            if s[k + 1..].iter().all(|&b| b == b' ' || b == b'\t') {
                self.emit(k, Kind::String);
                self.emit(s.len(), Kind::StringEscape);
                break;
            }
            let Some(len) = escape_len(s, k) else {
                j = k + 1;
                continue;
            };
            self.emit(k, Kind::String);
            j = k + len;
            self.emit(j, Kind::StringEscape);
        }
        self.emit(s.len(), Kind::String);
        Open::Basic
    }

    fn multi_literal(&mut self) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) = memchr(b'\'', &s[j..]).map(|k| j + k) {
            let quotes = quote_run(s, k, b'\'');
            if quotes >= 3 {
                self.emit(k + quotes.min(5), Kind::String);
                return Open::Code;
            }
            j = k + quotes;
        }
        self.emit(s.len(), Kind::String);
        Open::Literal
    }

    fn number(&mut self) {
        let s = self.s;
        if let Some(end) = date_end(s, self.i) {
            self.emit(end, Kind::StringSpecial);
            return;
        }
        let mut j = self.i;
        let signed = matches!(s[j], b'+' | b'-');
        if signed {
            j += 1;
        }
        let special =
            matches!(s.get(j..j + 3), Some(b"inf" | b"nan"));
        if special && !s.get(j + 3).is_some_and(|&b| is_bare(b)) {
            self.emit(j + 3, Kind::Number);
            return;
        }
        let radix = s.get(j) == Some(&b'0')
            && !signed
            && matches!(s.get(j + 1), Some(b'x' | b'o' | b'b'));
        if radix {
            j += 2;
            while j < s.len()
                && (s[j].is_ascii_hexdigit() || s[j] == b'_')
            {
                j += 1;
            }
            self.emit(j, Kind::Number);
            return;
        }
        let start = j;
        j = digits_end(s, j);
        if j == start {
            self.emit(self.i + 1, Kind::Plain);
            return;
        }
        if s.get(j) == Some(&b'.')
            && s.get(j + 1).is_some_and(u8::is_ascii_digit)
        {
            j = digits_end(s, j + 1);
        }
        if matches!(s.get(j), Some(b'e' | b'E')) {
            let sign = matches!(s.get(j + 1), Some(b'+' | b'-'));
            let k = j + 1 + usize::from(sign);
            if s.get(k).is_some_and(u8::is_ascii_digit) {
                j = digits_end(s, k);
            }
        }
        self.emit(j, Kind::Number);
    }

    fn word(&mut self) {
        let s = self.s;
        let mut end = self.i + 1;
        while end < s.len() && is_bare(s[end]) {
            end += 1;
        }
        let kind = match &s[self.i..end] {
            b"true" | b"false" => Kind::Boolean,
            b"inf" | b"nan" => Kind::Number,
            _ => Kind::Plain,
        };
        self.emit(end, kind);
    }

    fn stray(&mut self) {
        let mut end = self.i + 1;
        while end < self.s.len() && self.s[end] & 0xC0 == 0x80 {
            end += 1;
        }
        self.emit(end, Kind::Plain);
    }
}

const fn is_bare(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b >= 0x80
}

fn digits_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
        i += 1;
    }
    i
}

fn quote_run(s: &[u8], i: usize, quote: u8) -> usize {
    s[i..].iter().take_while(|&&b| b == quote).count()
}

fn basic_end(s: &[u8], from: usize) -> usize {
    let mut j = from;
    while let Some(k) = memchr2(b'"', b'\\', &s[j..]).map(|k| j + k) {
        if s[k] == b'"' {
            return k + 1;
        }
        j = (k + 2).min(s.len());
    }
    s.len()
}

fn literal_end(s: &[u8], from: usize) -> usize {
    memchr(b'\'', &s[from..]).map_or(s.len(), |k| from + k + 1)
}

fn escape_len(s: &[u8], k: usize) -> Option<usize> {
    let digits = match s.get(k + 1)? {
        b'b' | b't' | b'n' | b'f' | b'r' | b'e' | b'"' | b'\\' => {
            return Some(2);
        }
        b'x' => 2,
        b'u' => 4,
        b'U' => 8,
        _ => return None,
    };
    let hex = s.get(k + 2..k + 2 + digits)?;
    hex.iter().all(u8::is_ascii_hexdigit).then_some(2 + digits)
}

fn digits_at(s: &[u8], i: usize, count: usize) -> bool {
    s.get(i..i + count)
        .is_some_and(|d| d.iter().all(u8::is_ascii_digit))
}

fn date_end(s: &[u8], i: usize) -> Option<usize> {
    let word_end = |end: usize| {
        let glued =
            s.get(end).is_some_and(|&b| b.is_ascii_alphanumeric());
        (!glued).then_some(end)
    };
    let date = digits_at(s, i, 4)
        && s.get(i + 4) == Some(&b'-')
        && digits_at(s, i + 5, 2)
        && s.get(i + 7) == Some(&b'-')
        && digits_at(s, i + 8, 2);
    if !date {
        return time_end(s, i).and_then(word_end);
    }
    let mut end = i + 10;
    if matches!(s.get(end), Some(b'T' | b't' | b' '))
        && let Some(time) = time_end(s, end + 1)
    {
        end = time;
        match s.get(end) {
            Some(b'Z' | b'z') => end += 1,
            Some(b'+' | b'-')
                if digits_at(s, end + 1, 2)
                    && s.get(end + 3) == Some(&b':')
                    && digits_at(s, end + 4, 2) =>
            {
                end += 6;
            }
            _ => {}
        }
    }
    word_end(end)
}

fn time_end(s: &[u8], i: usize) -> Option<usize> {
    if !(digits_at(s, i, 2)
        && s.get(i + 2) == Some(&b':')
        && digits_at(s, i + 3, 2))
    {
        return None;
    }
    let mut end = i + 5;
    if s.get(end) == Some(&b':') && digits_at(s, end + 1, 2) {
        end += 3;
        if s.get(end) == Some(&b'.')
            && s.get(end + 1).is_some_and(u8::is_ascii_digit)
        {
            end = digits_end(s, end + 1);
        }
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::{Mode, Open, Stack};
    use crate::State;

    #[test]
    fn modes_round_trip() {
        let mut stack = Stack::default();
        for n in 0..70 {
            stack.push(n % 3 == 0);
        }
        for stack in [Stack::default(), stack] {
            for open in [Open::Code, Open::Basic, Open::Literal] {
                let mode = Mode { open, stack };
                assert_eq!(Mode::load(&mode.store()), mode);
            }
        }
        let code = Mode {
            open: Open::Code,
            stack: Stack::default(),
        };
        assert_eq!(code.store(), State::default());
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 1 << 40, 0xFFFF_FFFF, 7] {
            let mode = Mode::load(&State::from_word(word));
            assert_eq!(Mode::load(&mode.store()), mode);
        }
    }
}
