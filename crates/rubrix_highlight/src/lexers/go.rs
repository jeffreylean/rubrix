//! The Go lexer.
//!
//! It covers raw backtick strings across lines, interpreted strings,
//! runes, block comments across lines, numbers in every radix with
//! underscores, hex floats, and the `i` suffix, struct tags, `func`
//! names and method names after a receiver, and type names after
//! `type`, including the names declared in a `type ( ... )` group.
//!
//! The state between lines is whether a block comment or a raw
//! string is open, and how deeply nested the lexer is inside a
//! `type (` group (capped at 255).
//!
//! A capitalised name is `type` only where Go puts a type: after a
//! name (`x Foo`, `var x Foo`), after `*`, `...`, `]`, `chan`, or
//! `)`, inside the `[ ]` of `map[` and of a type or function name,
//! or directly before `{` as in `Foo{`. Anywhere else it is a
//! `variable`, or a `constant` when it is all capitals, because
//! capitals mark exported names in Go and not types. A name after a
//! `.` is a `property`, even when it names a type, as in
//! `pkg.Type`. A name that starts a line or follows `struct {` and
//! is followed by a type is a field and a `property`.
//!
//! What it does not handle:
//!
//! - Names declared in a `var (` or `const (` group are not told from
//!   fields: a name that starts a line and is followed by a type is a
//!   `property`, and any other is a `variable` or a `constant` by its
//!   spelling.
//! - No name in a field list such as `X, Y int` is a field. Each is a
//!   `variable`.
//! - A parameter list that spans lines gives its names the field
//!   treatment when they start a line.
//! - A capitalised type after a comma, such as the second result in
//!   `(Foo, Bar)`, is a `variable`.
//! - There is no `string.doc`. Doc comments are `comment`.
//! - A rune with more than one character is a stray quote followed by
//!   the rest as code.

use memchr::{memchr, memchr2};

use super::{ident_end, is_ident_start, space_end};
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    Comment,
    Raw,
}

const OPEN_TAG: u64 = 0b11;
const GROUP_SHIFT: u32 = 8;
const GROUP_MAX: u32 = 255;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mode {
    open: Open,
    group: u32,
}

impl Mode {
    fn load(state: &State) -> Mode {
        let word = state.word();
        let open = match word & OPEN_TAG {
            1 => Open::Comment,
            2 => Open::Raw,
            _ => Open::Code,
        };
        let group = (word >> GROUP_SHIFT) as u32 & GROUP_MAX;
        Mode { open, group }
    }

    fn store(self) -> State {
        let tag = match self.open {
            Open::Code => 0,
            Open::Comment => 1,
            Open::Raw => 2,
        };
        State::from_word(tag | u64::from(self.group) << GROUP_SHIFT)
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
    TypePos,
    Generic,
    Struct,
    Map,
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let Mode { mut open, group } = Mode::load(state);
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        at_line_start: open == Open::Code,
        prev: Prev::Other,
        param_depth: 0,
        prev_closed_params: false,
        group,
        type_args: 0,
    };
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Comment => lexer.comment(),
            Open::Raw => lexer.raw(),
        };
    }
    Mode {
        open,
        group: lexer.group,
    }
    .store()
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    at_line_start: bool,
    prev: Prev,
    param_depth: u32,
    prev_closed_params: bool,
    group: u32,
    type_args: u32,
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
            let at_line_start =
                core::mem::replace(&mut self.at_line_start, false);
            let prev =
                core::mem::replace(&mut self.prev, Prev::Other);
            let after = core::mem::take(&mut self.prev_closed_params);
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
                        self.param_depth = 1;
                    } else if self.param_depth > 0 {
                        self.param_depth += 1;
                    }
                    if prev == Prev::Type && self.group == 0 {
                        self.group = 1;
                    } else {
                        self.nest(true);
                    }
                    if matches!(
                        prev,
                        Prev::Func | Prev::Dot | Prev::TypePos
                    ) {
                        self.prev = Prev::TypePos;
                    }
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b')' => {
                    if self.param_depth > 0 {
                        self.param_depth -= 1;
                        self.prev_closed_params =
                            self.param_depth == 0;
                    }
                    self.nest(false);
                    self.prev = Prev::TypePos;
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'[' => {
                    if matches!(prev, Prev::Generic | Prev::Map) {
                        self.type_args = 1;
                    } else if self.type_args > 0 {
                        self.type_args += 1;
                    }
                    self.nest(true);
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b']' => {
                    self.type_args = self.type_args.saturating_sub(1);
                    self.nest(false);
                    if self.at(1).is_some_and(is_ident_start) {
                        self.prev = Prev::TypePos;
                    }
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'{' => {
                    if prev == Prev::Struct {
                        self.at_line_start = true;
                    }
                    self.nest(true);
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'}' => {
                    self.nest(false);
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
                b'*' => {
                    if self.is_pointer() {
                        self.prev = Prev::TypePos;
                    }
                    self.emit(self.i + 1, Kind::Operator);
                }
                b'+' | b'-' | b'%' | b'^' | b'!' | b'&' | b'|'
                | b'=' | b'<' | b'>' | b'~' => {
                    self.emit(self.i + 1, Kind::Operator);
                }
                _ if is_ident_start(b) => {
                    self.word(prev, after, at_line_start)
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
            if self.at(3).is_some_and(is_ident_start) {
                self.prev = Prev::TypePos;
            }
            self.emit(self.i + 3, Kind::Operator);
        } else {
            self.emit(self.i + 1, Kind::PunctuationDelimiter);
            self.prev = Prev::Dot;
        }
    }

    fn nest(&mut self, open: bool) {
        if self.group > 0 {
            self.group = if open {
                (self.group + 1).min(GROUP_MAX)
            } else {
                self.group - 1
            };
        }
    }

    fn is_pointer(&self) -> bool {
        let before = self.i.checked_sub(1).map(|j| self.s[j]);
        self.at(1).is_some_and(is_ident_start)
            && before.is_none_or(|b| {
                b.is_ascii_whitespace() || b"([,]{=*".contains(&b)
            })
    }

    fn word(
        &mut self,
        prev: Prev,
        prev_closed_params: bool,
        at_line_start: bool,
    ) {
        let s = self.s;
        let end = ident_end(s, self.i);
        let word = &s[self.i..end];
        let next = s.get(end).copied();
        let call = next == Some(b'(');
        let reserved = keyword(word);
        let kind = match prev {
            Prev::Func => Kind::FunctionDefinition,
            Prev::Type => Kind::TypeDefinition,
            Prev::Package => Kind::Namespace,
            Prev::Var => Kind::Variable,
            Prev::Const => Kind::Constant,
            Prev::Dot if call => Kind::FunctionMethod,
            Prev::Dot => Kind::Property,
            _ if prev_closed_params && call => {
                Kind::FunctionDefinition
            }
            _ => reserved.unwrap_or_else(|| {
                if at_line_start && self.group == 1 {
                    Kind::TypeDefinition
                } else if at_line_start && declares_name(s, end) {
                    Kind::Property
                } else {
                    let type_position =
                        prev == Prev::TypePos || self.type_args > 0;
                    name_kind(word, call, type_position, next)
                }
            }),
        };
        self.emit(end, kind);
        self.prev = match prev {
            Prev::Func | Prev::Type => Prev::Generic,
            Prev::Package | Prev::Dot => Prev::Other,
            Prev::Var | Prev::Const => Prev::TypePos,
            _ => match word {
                b"func" => Prev::Func,
                b"type" => Prev::Type,
                b"package" => Prev::Package,
                b"var" => Prev::Var,
                b"const" => Prev::Const,
                b"struct" => Prev::Struct,
                b"map" => Prev::Map,
                b"chan" => Prev::TypePos,
                _ if reserved.is_some() => Prev::Other,
                _ if matches!(
                    kind,
                    Kind::Type | Kind::TypeDefinition
                ) =>
                {
                    Prev::Generic
                }
                _ => Prev::TypePos,
            },
        };
    }
}

fn declares_name(s: &[u8], end: usize) -> bool {
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

fn name_kind(
    name: &[u8],
    call: bool,
    type_position: bool,
    next: Option<u8>,
) -> Kind {
    if call {
        if is_builtin_function(name) {
            Kind::FunctionBuiltin
        } else {
            Kind::Function
        }
    } else if !is_capitalised(name) {
        Kind::Variable
    } else if type_position || next == Some(b'{') {
        Kind::Type
    } else if is_screaming_case(name) {
        Kind::Constant
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
    use super::{GROUP_MAX, Mode, Open};
    use crate::State;

    #[test]
    fn modes_round_trip() {
        for open in [Open::Code, Open::Comment, Open::Raw] {
            for group in [0, 1, GROUP_MAX] {
                let mode = Mode { open, group };
                assert_eq!(Mode::load(&mode.store()), mode);
            }
        }
        let code = Mode {
            open: Open::Code,
            group: 0,
        };
        assert_eq!(code.store(), State::default());
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 3, 1 << 40, 0xFFFF_FFFF] {
            let mode = Mode::load(&State::from_word(word));
            assert_eq!(Mode::load(&mode.store()), mode);
        }
    }
}
