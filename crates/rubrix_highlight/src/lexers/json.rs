//! JSON and JSONC.
//!
//! Object keys are [`Kind::PropertyJsonKey`] and every other string
//! is [`Kind::String`]. A lexer cannot tell them apart without the
//! nesting, so the state holds a stack of object and array levels.
//! The first 28 levels live in the state's word. A deeper stack goes
//! to the tail, which holds levels up to 1,024. Levels past that are
//! counted but read as arrays.
//!
//! Handled: `//` and `/* */` comments, escapes in strings, numbers
//! with a fraction and an exponent, `true`, `false`, `null`, and
//! trailing commas. A string ends at the line end unless the line
//! ends in a backslash.
//!
//! Not handled: JSON5 (single-quoted strings, unquoted keys, hex
//! numbers, `Infinity`, `NaN`), JSON Lines, and any check that the
//! text is valid JSON. Bare words other than the three literals, and
//! other stray bytes, are [`Kind::Plain`].

use alloc::vec::Vec;

use memchr::{memchr, memchr2};

use super::space_end;
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    Comment,
    Str,
}

const OPEN: u64 = 0b11;
const KEY: u64 = 0b100;
const DEPTH_SHIFT: u32 = 4;
const STACK_SHIFT: u32 = 36;
const LOW_LEVELS: u32 = 28;
const MAX_LEVELS: u32 = 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Stack {
    depth: u32,
    low: u64,
    deep: Vec<u8>,
}

impl Stack {
    fn is_object(&self) -> bool {
        let Some(top) = self.depth.checked_sub(1) else {
            return false;
        };
        if top < LOW_LEVELS {
            self.low >> top & 1 == 1
        } else {
            let index = (top - LOW_LEVELS) as usize;
            top < MAX_LEVELS && self.deep.get(index) == Some(&1)
        }
    }

    fn push(&mut self, object: bool) {
        if self.depth < LOW_LEVELS {
            self.low |= u64::from(object) << self.depth;
        } else if self.depth < MAX_LEVELS {
            self.deep.push(u8::from(object));
        }
        self.depth = self.depth.saturating_add(1);
    }

    fn pop(&mut self) {
        let Some(top) = self.depth.checked_sub(1) else {
            return;
        };
        self.depth = top;
        if top < LOW_LEVELS {
            self.low &= !(1 << top);
        } else if top < MAX_LEVELS {
            self.deep.pop();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Mode {
    open: Open,
    key: bool,
    stack: Stack,
}

impl Mode {
    fn load(state: &State) -> Mode {
        let word = state.word();
        let depth = (word >> DEPTH_SHIFT) as u32;
        let levels = depth.min(LOW_LEVELS);
        let low = (word >> STACK_SHIFT) & ((1 << levels) - 1);
        let open = match word & OPEN {
            1 => Open::Comment,
            2 => Open::Str,
            _ => Open::Code,
        };
        Mode {
            open,
            key: word & KEY != 0,
            stack: Stack {
                depth,
                low,
                deep: state.tail().to_vec(),
            },
        }
    }

    fn store(&self) -> State {
        let open = match self.open {
            Open::Code => 0,
            Open::Comment => 1,
            Open::Str => 2,
        };
        let key = if self.key { KEY } else { 0 };
        let word = open
            | key
            | u64::from(self.stack.depth) << DEPTH_SHIFT
            | self.stack.low << STACK_SHIFT;
        State::with_tail(word, &self.stack.deep)
    }
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let Mode {
        mut open,
        key,
        stack,
    } = Mode::load(state);
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        key,
        stack,
    };
    if line.is_empty() && open == Open::Str {
        open = Open::Code;
        lexer.key = false;
    }
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Comment => lexer.comment(),
            Open::Str => lexer.continue_string(),
        };
    }
    Mode {
        open,
        key: lexer.key,
        stack: lexer.stack,
    }
    .store()
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    key: bool,
    stack: Stack,
}

impl Lexer<'_, '_, '_> {
    fn emit(&mut self, end: usize, kind: Kind) {
        self.out.push(end, kind);
        self.i = end;
    }

    fn code(&mut self) -> Open {
        while self.i < self.s.len() {
            let b = self.s[self.i];
            match b {
                _ if b.is_ascii_whitespace() => {
                    let end = space_end(self.s, self.i);
                    self.out.space(end);
                    self.i = end;
                }
                b'"' => {
                    if self.string(self.i + 1) {
                        return Open::Str;
                    }
                }
                b'-' | b'0'..=b'9' => self.number(),
                b'{' | b'[' => {
                    self.stack.push(b == b'{');
                    self.key = b == b'{';
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'}' | b']' => {
                    self.stack.pop();
                    self.key = false;
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b',' => {
                    self.key = self.stack.is_object();
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                }
                b':' => {
                    self.key = false;
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                }
                b'/' => match self.s.get(self.i + 1) {
                    Some(b'/') => {
                        self.emit(self.s.len(), Kind::Comment);
                    }
                    Some(b'*') => {
                        self.emit(self.i + 2, Kind::Comment);
                        return Open::Comment;
                    }
                    _ => self.emit(self.i + 1, Kind::Plain),
                },
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.word(),
                _ => self.stray(),
            }
        }
        Open::Code
    }

    fn comment(&mut self) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) = memchr(b'*', &s[j..]).map(|k| j + k) {
            if s.get(k + 1) == Some(&b'/') {
                self.emit(k + 2, Kind::Comment);
                return Open::Code;
            }
            j = k + 1;
        }
        self.emit(s.len(), Kind::Comment);
        Open::Comment
    }

    fn continue_string(&mut self) -> Open {
        if self.string(self.i) {
            Open::Str
        } else {
            Open::Code
        }
    }

    /// Lexes the string body from `from`, with the opening quote, if
    /// any, before it. Returns whether a backslash ends the line and
    /// so continues the string on the next line.
    fn string(&mut self, from: usize) -> bool {
        let kind = if self.key {
            Kind::PropertyJsonKey
        } else {
            Kind::String
        };
        let s = self.s;
        let mut j = from;
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                self.emit(k + 1, kind);
                self.key = false;
                return false;
            }
            self.emit(k, kind);
            j = escape_end(s, k);
            self.emit(j, Kind::StringEscape);
            if k + 1 == s.len() {
                return true;
            }
        }
        self.emit(s.len(), kind);
        self.key = false;
        false
    }

    fn number(&mut self) {
        let s = self.s;
        let mut j = self.i;
        if s[j] == b'-' {
            j += 1;
        }
        let digits = digits_end(s, j);
        let mut seen = digits > j;
        j = digits;
        if s.get(j) == Some(&b'.') {
            j += 1;
            let fraction = digits_end(s, j);
            seen |= fraction > j;
            j = fraction;
        }
        if seen && matches!(s.get(j), Some(b'e' | b'E')) {
            let sign = matches!(s.get(j + 1), Some(b'+' | b'-'));
            let k = j + 1 + usize::from(sign);
            if s.get(k).is_some_and(u8::is_ascii_digit) {
                j = digits_end(s, k);
            }
        }
        let kind = if seen { Kind::Number } else { Kind::Plain };
        self.emit(j, kind);
    }

    fn word(&mut self) {
        let s = self.s;
        let mut end = self.i + 1;
        while end < s.len()
            && (s[end].is_ascii_alphanumeric() || s[end] == b'_')
        {
            end += 1;
        }
        let kind = match &s[self.i..end] {
            b"true" | b"false" => Kind::Boolean,
            b"null" => Kind::ConstantBuiltin,
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

fn digits_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    i
}

fn escape_end(s: &[u8], i: usize) -> usize {
    match s.get(i + 1) {
        None => i + 1,
        Some(b'u') => {
            let hex = s[i + 2..].iter().take(4);
            i + 2 + hex.take_while(|b| b.is_ascii_hexdigit()).count()
        }
        Some(&b) => {
            let mut end = i + 2;
            if b >= 0x80 {
                while end < s.len() && s[end] & 0xC0 == 0x80 {
                    end += 1;
                }
            }
            end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, Open, Stack};
    use crate::State;

    #[test]
    fn modes_round_trip() {
        let mut deep = Stack::default();
        for n in 0..40 {
            deep.push(n % 3 == 0);
        }
        for stack in [Stack::default(), deep] {
            for open in [Open::Code, Open::Comment, Open::Str] {
                for key in [false, true] {
                    let mode = Mode {
                        open,
                        key,
                        stack: stack.clone(),
                    };
                    assert_eq!(Mode::load(&mode.store()), mode);
                }
            }
        }
        let code = Mode {
            open: Open::Code,
            key: false,
            stack: Stack::default(),
        };
        assert_eq!(code.store(), State::default());
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 1 << 40, 0xFFFF_FFFF, 3] {
            let mode = Mode::load(&State::from_word(word));
            assert_eq!(Mode::load(&mode.store()), mode);
        }
        let tail = State::with_tail(u64::MAX, &[1, 0, 7]);
        let mode = Mode::load(&tail);
        assert_eq!(Mode::load(&mode.store()), mode);
    }

    #[test]
    fn stack_tracks_levels_past_the_word() {
        let mut stack = Stack::default();
        for n in 0..2000 {
            stack.push(n % 2 == 0);
        }
        assert_eq!(stack.depth, 2000);
        assert!(!stack.is_object());
        for _ in 0..1000 {
            stack.pop();
        }
        assert_eq!(stack.depth, 1000);
        assert!(!stack.is_object());
        stack.pop();
        assert!(stack.is_object());
        for _ in 0..999 {
            stack.pop();
        }
        assert_eq!(stack, Stack::default());
    }
}
