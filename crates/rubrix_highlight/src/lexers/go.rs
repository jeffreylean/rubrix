use memchr::{memchr, memchr2};

use super::{ident_end, is_ident_start, space_end};
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    Comment,
    Raw,
}

impl Open {
    fn load(state: &State) -> Open {
        match state.word() {
            1 => Open::Comment,
            2 => Open::Raw,
            _ => Open::Code,
        }
    }

    fn store(self) -> State {
        State::from_word(match self {
            Open::Code => 0,
            Open::Comment => 1,
            Open::Raw => 2,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    Other,
    Func,
    Type,
    Package,
    Var,
    Const,
    Dot,
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let mut open = Open::load(state);
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        first: open == Open::Code,
        prev: Prev::Other,
        params: 0,
        after_params: false,
    };
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Comment => lexer.comment(),
            Open::Raw => lexer.raw(),
        };
    }
    open.store()
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    /// No token yet on this line, so a name followed by a type is a
    /// declaration.
    first: bool,
    prev: Prev,
    /// Open parentheses of the parameter list after `func`, or 0.
    params: u32,
    /// The token before this one closed a parameter list.
    after_params: bool,
}

impl Lexer<'_, '_, '_> {
    fn emit(&mut self, end: usize, kind: Kind) {
        self.out.push(end, kind);
        self.i = end;
    }

    fn at(&self, offset: usize) -> Option<u8> {
        self.s.get(self.i + offset).copied()
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
            let first = core::mem::replace(&mut self.first, false);
            let prev =
                core::mem::replace(&mut self.prev, Prev::Other);
            let after = core::mem::take(&mut self.after_params);
            match b {
                b'/' => match self.at(1) {
                    Some(b'/') => {
                        self.emit(self.s.len(), Kind::Comment);
                    }
                    Some(b'*') => {
                        self.emit(self.i + 2, Kind::Comment);
                        return Open::Comment;
                    }
                    _ => self.emit(self.i + 1, Kind::Operator),
                },
                b'"' => self.string(),
                b'`' => {
                    self.emit(self.i + 1, Kind::String);
                    return Open::Raw;
                }
                b'\'' => self.rune(),
                b'0'..=b'9' => self.number(),
                b'.' => self.dot(),
                b'(' => {
                    if prev == Prev::Func {
                        self.params = 1;
                    } else if self.params > 0 {
                        self.params += 1;
                    }
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b')' => {
                    if self.params > 0 {
                        self.params -= 1;
                        self.after_params = self.params == 0;
                    }
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'[' | b']' | b'{' | b'}' => {
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b':' => {
                    if self.at(1) == Some(b'=') {
                        self.emit(self.i + 2, Kind::Operator);
                    } else {
                        self.emit(
                            self.i + 1,
                            Kind::PunctuationDelimiter,
                        );
                    }
                }
                b',' | b';' => {
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                }
                b'+' | b'-' | b'*' | b'%' | b'^' | b'!' | b'&'
                | b'|' | b'=' | b'<' | b'>' | b'~' => {
                    self.emit(self.i + 1, Kind::Operator);
                }
                _ if is_ident_start(b) => {
                    self.word(prev, after, first)
                }
                _ => self.emit(self.i + 1, Kind::Plain),
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

    fn raw(&mut self) -> Open {
        let s = self.s;
        match memchr(b'`', &s[self.i..]) {
            Some(k) => {
                self.emit(self.i + k + 1, Kind::String);
                Open::Code
            }
            None => {
                self.emit(s.len(), Kind::String);
                Open::Raw
            }
        }
    }

    fn string(&mut self) {
        let s = self.s;
        let mut j = self.i + 1;
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                self.emit(k + 1, Kind::String);
                return;
            }
            self.emit(k, Kind::String);
            j = escape_end(s, k);
            self.emit(j, Kind::StringEscape);
        }
        self.emit(s.len(), Kind::String);
    }

    fn rune(&mut self) {
        let s = self.s;
        let body = self.i + 1;
        let end = match s.get(body) {
            None | Some(b'\'') => None,
            Some(b'\\') => Some(escape_end(s, body)),
            Some(&b) => Some(body + char_len(b)),
        };
        match end {
            Some(end) if s.get(end) == Some(&b'\'') => {
                self.emit(end + 1, Kind::StringChar);
            }
            _ => self.emit(self.i + 1, Kind::Punctuation),
        }
    }

    fn number(&mut self) {
        let s = self.s;
        let mut j = self.i;
        let radix = s[j] == b'0' && self.at(1).is_some_and(is_radix);
        if radix {
            let hex = matches!(self.at(1), Some(b'x' | b'X'));
            j += 2;
            while s.get(j).is_some_and(|&b| {
                b == b'_'
                    || if hex {
                        b.is_ascii_hexdigit() || b == b'.'
                    } else {
                        b.is_ascii_digit()
                    }
            }) {
                j += 1;
            }
            if hex {
                j = exponent_end(s, j, b'p', b'P');
            }
        } else {
            if s[j] != b'.' {
                j = digits_end(s, j);
            }
            if s.get(j) == Some(&b'.') {
                j = digits_end(s, j + 1);
            }
            j = exponent_end(s, j, b'e', b'E');
        }
        if s.get(j) == Some(&b'i') {
            j += 1;
        }
        self.emit(j, Kind::Number);
    }

    fn dot(&mut self) {
        if self.at(1).is_some_and(|b| b.is_ascii_digit()) {
            self.number();
        } else if self.at(1) == Some(b'.') && self.at(2) == Some(b'.')
        {
            self.emit(self.i + 3, Kind::Operator);
        } else {
            self.emit(self.i + 1, Kind::PunctuationDelimiter);
            self.prev = Prev::Dot;
        }
    }

    fn word(&mut self, prev: Prev, after_params: bool, first: bool) {
        let s = self.s;
        let end = ident_end(s, self.i);
        let word = &s[self.i..end];
        let next = s.get(end).copied();
        let call = next == Some(b'(');
        let kind = match prev {
            Prev::Func => Kind::FunctionDefinition,
            Prev::Type => Kind::TypeDefinition,
            Prev::Package => Kind::Namespace,
            Prev::Var => Kind::Variable,
            Prev::Const => Kind::Constant,
            Prev::Dot if call => Kind::FunctionMethod,
            Prev::Dot => Kind::Property,
            Prev::Other
                if after_params
                    && matches!(next, Some(b'(' | b'[')) =>
            {
                Kind::FunctionDefinition
            }
            Prev::Other => keyword(word).unwrap_or_else(|| {
                if first && declares(s, end) {
                    Kind::Property
                } else {
                    name_kind(word, call)
                }
            }),
        };
        self.emit(end, kind);
        if prev == Prev::Other {
            self.prev = match word {
                b"func" => Prev::Func,
                b"type" => Prev::Type,
                b"package" => Prev::Package,
                b"var" => Prev::Var,
                b"const" => Prev::Const,
                _ => Prev::Other,
            };
        }
    }
}

/// Whether a name at the start of a line is followed by a type, as
/// in a struct field or a var block entry.
fn declares(s: &[u8], end: usize) -> bool {
    let rest = &s[end..];
    let space = space_end(rest, 0);
    space > 0
        && rest.get(space).is_some_and(|&b| {
            is_ident_start(b) || b == b'*' || b == b'['
        })
}

const fn is_radix(b: u8) -> bool {
    matches!(b, b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
}

fn exponent_end(s: &[u8], j: usize, lower: u8, upper: u8) -> usize {
    if !matches!(s.get(j), Some(&b) if b == lower || b == upper) {
        return j;
    }
    let sign = matches!(s.get(j + 1), Some(b'+' | b'-'));
    let k = j + 1 + usize::from(sign);
    if s.get(k).is_some_and(u8::is_ascii_digit) {
        digits_end(s, k)
    } else {
        j
    }
}

fn keyword(word: &[u8]) -> Option<Kind> {
    Some(match word {
        b"break" | b"case" | b"continue" | b"default" | b"else"
        | b"fallthrough" | b"for" | b"goto" | b"if" | b"range"
        | b"return" | b"select" | b"switch" => Kind::KeywordControl,
        b"chan" | b"const" | b"defer" | b"func" | b"go"
        | b"interface" | b"map" | b"struct" | b"type" | b"var" => {
            Kind::Keyword
        }
        b"import" | b"package" => Kind::KeywordImport,
        b"true" | b"false" => Kind::Boolean,
        b"nil" | b"iota" => Kind::ConstantBuiltin,
        b"any" | b"bool" | b"byte" | b"comparable" | b"complex64"
        | b"complex128" | b"error" | b"float32" | b"float64"
        | b"int" | b"int8" | b"int16" | b"int32" | b"int64"
        | b"rune" | b"string" | b"uint" | b"uint8" | b"uint16"
        | b"uint32" | b"uint64" | b"uintptr" => Kind::TypeBuiltin,
        _ => return None,
    })
}

fn name_kind(name: &[u8], call: bool) -> Kind {
    if call {
        if is_builtin_function(name) {
            Kind::FunctionBuiltin
        } else {
            Kind::Function
        }
    } else if is_capitalised(name) {
        if is_screaming_case(name) {
            Kind::Constant
        } else {
            Kind::Type
        }
    } else {
        Kind::Variable
    }
}

fn is_builtin_function(name: &[u8]) -> bool {
    matches!(
        name,
        b"append"
            | b"cap"
            | b"clear"
            | b"close"
            | b"complex"
            | b"copy"
            | b"delete"
            | b"imag"
            | b"len"
            | b"make"
            | b"max"
            | b"min"
            | b"new"
            | b"panic"
            | b"print"
            | b"println"
            | b"real"
            | b"recover"
    )
}

fn is_capitalised(name: &[u8]) -> bool {
    match name.first() {
        Some(b) if b.is_ascii() => b.is_ascii_uppercase(),
        _ => core::str::from_utf8(name)
            .ok()
            .and_then(|name| name.chars().next())
            .is_some_and(char::is_uppercase),
    }
}

fn is_screaming_case(name: &[u8]) -> bool {
    let upper =
        name.iter().filter(|b| b.is_ascii_uppercase()).count();
    upper > 1
        && name.iter().all(|&b| {
            b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
        })
}

fn digits_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
        i += 1;
    }
    i
}

fn escape_end(s: &[u8], i: usize) -> usize {
    let digits =
        |count: usize, accept: fn(&u8) -> bool, from: usize| {
            let rest = s.get(from..).unwrap_or_default();
            from + rest
                .iter()
                .take(count)
                .take_while(|b| accept(b))
                .count()
        };
    match s.get(i + 1) {
        None => i + 1,
        Some(b'x') => digits(2, u8::is_ascii_hexdigit, i + 2),
        Some(b'u') => digits(4, u8::is_ascii_hexdigit, i + 2),
        Some(b'U') => digits(8, u8::is_ascii_hexdigit, i + 2),
        Some(b'0'..=b'7') => {
            digits(3, |b| (b'0'..=b'7').contains(b), i + 1)
        }
        Some(&b) => i + 1 + char_len(b),
    }
}

const fn char_len(lead: u8) -> usize {
    match lead {
        0..0x80 => 1,
        0x80..0xE0 => 2,
        0xE0..0xF0 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::Open;
    use crate::State;

    #[test]
    fn opens_round_trip() {
        for open in [Open::Code, Open::Comment, Open::Raw] {
            assert_eq!(Open::load(&open.store()), open);
        }
        assert_eq!(Open::Code.store(), State::default());
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 3, 1 << 40, 0xFFFF_FFFF] {
            let open = Open::load(&State::from_word(word));
            assert_eq!(Open::load(&open.store()), open);
        }
    }
}
