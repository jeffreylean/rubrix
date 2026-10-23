//! Rust: keywords, identifiers, capitalised types, lifetimes and
//! chars, strings, raw strings with any number of hashes, byte and C
//! strings, nested block comments, doc comments, numbers with
//! suffixes, attributes, macro calls, operators, and punctuation.
//! Strings, raw strings, block comments, and attributes continue
//! across lines.

use memchr::{memchr, memchr2};

use super::{ident_end, is_ident_start, space_end};
use crate::{Kind, Runs, State};

/// What a line break leaves open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    /// A block comment nested `depth` deep.
    Comment {
        depth: u32,
        doc: bool,
    },
    /// A `"`, `b"`, or `c"` string.
    Str,
    /// A raw string that `"` and `hashes` `#` close.
    Raw {
        hashes: u32,
    },
}

/// The lexer state at a line break: what is open, and how many
/// brackets deep the line is inside an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mode {
    open: Open,
    attr: u32,
}

// The state word: bits 0..2 hold the `Open` tag, bit 2 the doc
// flag, bits 8..32 the attribute depth, and bits 32..64 the comment
// depth or the hash count.
const TAG: u64 = 0b11;
const DOC: u64 = 0b100;
const ATTR_SHIFT: u32 = 8;
const ATTR_MAX: u32 = (1 << 24) - 1;
const COUNT_SHIFT: u32 = 32;

impl Mode {
    fn load(state: &State) -> Mode {
        let word = state.word();
        let count = (word >> COUNT_SHIFT) as u32;
        let open = match word & TAG {
            1 => Open::Comment {
                depth: count.max(1),
                doc: word & DOC != 0,
            },
            2 => Open::Str,
            3 => Open::Raw { hashes: count },
            _ => Open::Code,
        };
        let attr = (word >> ATTR_SHIFT) as u32 & ATTR_MAX;
        Mode { open, attr }
    }

    fn store(self) -> State {
        let (tag, flags, count) = match self.open {
            Open::Code => (0, 0, 0),
            Open::Comment { depth, doc } => {
                (1, if doc { DOC } else { 0 }, depth)
            }
            Open::Str => (2, 0, 0),
            Open::Raw { hashes } => (3, 0, hashes),
        };
        let attr = u64::from(self.attr.min(ATTR_MAX)) << ATTR_SHIFT;
        let count = u64::from(count) << COUNT_SHIFT;
        State::from_word(tag | flags | attr | count)
    }
}

/// The token before the current one, where it changes how an
/// identifier reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    Other,
    /// The `fn` keyword, so the next identifier is a definition.
    Fn,
    /// A `.`, so the next identifier is a field or a method.
    Dot,
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let Mode { mut open, attr } = Mode::load(state);
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        attr,
        prev: Prev::Other,
    };
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Comment { depth, doc } => lexer.comment(depth, doc),
            Open::Str => lexer.string(),
            Open::Raw { hashes } => lexer.raw(hashes),
        };
    }
    Mode {
        open,
        attr: lexer.attr,
    }
    .store()
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    /// Brackets open inside an attribute. While it is above 0, every
    /// token is part of the attribute's one run.
    attr: u32,
    prev: Prev,
}

impl Lexer<'_, '_, '_> {
    fn emit(&mut self, end: usize, kind: Kind) {
        let kind = if self.attr > 0 { Kind::Attribute } else { kind };
        self.out.push(end, kind);
        self.i = end;
    }

    fn at(&self, offset: usize) -> Option<u8> {
        self.s.get(self.i + offset).copied()
    }

    /// Lexes tokens until the line ends or a string or block comment
    /// opens, and returns what is open.
    fn code(&mut self) -> Open {
        while self.i < self.s.len() {
            let b = self.s[self.i];
            if b.is_ascii_whitespace() {
                let end = space_end(self.s, self.i);
                self.out.space(end);
                self.i = end;
                continue;
            }
            let prev =
                core::mem::replace(&mut self.prev, Prev::Other);
            match b {
                b'/' => match self.at(1) {
                    Some(b'/') => {
                        let doc = match (self.at(2), self.at(3)) {
                            (Some(b'!'), _) => true,
                            (Some(b'/'), next) => next != Some(b'/'),
                            _ => false,
                        };
                        self.emit(self.s.len(), comment_kind(doc));
                    }
                    Some(b'*') => {
                        let doc = match (self.at(2), self.at(3)) {
                            (Some(b'!'), _) => true,
                            (Some(b'*'), next) => {
                                !matches!(next, Some(b'*' | b'/'))
                            }
                            _ => false,
                        };
                        self.emit(self.i + 2, comment_kind(doc));
                        return Open::Comment { depth: 1, doc };
                    }
                    _ => self.emit(self.i + 1, Kind::Operator),
                },
                b'"' => {
                    self.emit(self.i + 1, Kind::String);
                    return Open::Str;
                }
                b'\'' => self.quote(),
                b'0'..=b'9' => self.number(),
                b'#' => self.hash(),
                b'$' => self.dollar(),
                b'[' => {
                    if self.attr > 0 {
                        self.attr = self.attr.saturating_add(1);
                    }
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b']' => {
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                    self.attr = self.attr.saturating_sub(1);
                }
                b'(' | b')' | b'{' | b'}' => {
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'.' => self.dot(),
                b':' => {
                    let len =
                        if self.at(1) == Some(b':') { 2 } else { 1 };
                    self.emit(
                        self.i + len,
                        Kind::PunctuationDelimiter,
                    );
                }
                b',' | b';' => {
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                }
                b'+' | b'-' | b'*' | b'%' | b'^' | b'!' | b'&'
                | b'|' | b'=' | b'<' | b'>' | b'?' | b'@' | b'~' => {
                    self.emit(self.i + 1, Kind::Operator);
                }
                _ if is_ident_start(b) => {
                    if let Some(open) = self.word(prev) {
                        return open;
                    }
                }
                _ => self.emit(self.i + 1, Kind::Plain),
            }
        }
        Open::Code
    }

    fn comment(&mut self, mut depth: u32, doc: bool) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) =
            memchr2(b'*', b'/', &s[j..]).map(|k| j + k)
        {
            match (s[k], s.get(k + 1)) {
                (b'*', Some(b'/')) => {
                    depth -= 1;
                    j = k + 2;
                    if depth == 0 {
                        self.emit(j, comment_kind(doc));
                        return Open::Code;
                    }
                }
                (b'/', Some(b'*')) => {
                    depth = depth.saturating_add(1);
                    j = k + 2;
                }
                _ => j = k + 1,
            }
        }
        self.emit(s.len(), comment_kind(doc));
        Open::Comment { depth, doc }
    }

    fn string(&mut self) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                self.emit(k + 1, Kind::String);
                return Open::Code;
            }
            self.emit(k, Kind::String);
            j = escape_end(s, k);
            self.emit(j, Kind::StringEscape);
        }
        self.emit(s.len(), Kind::String);
        Open::Str
    }

    fn raw(&mut self, hashes: u32) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) = memchr(b'"', &s[j..]).map(|k| j + k) {
            let end = k + 1 + hashes as usize;
            let closes = s
                .get(k + 1..end)
                .is_some_and(|tail| tail.iter().all(|&b| b == b'#'));
            if closes {
                self.emit(end, Kind::String);
                return Open::Code;
            }
            j = k + 1;
        }
        self.emit(s.len(), Kind::String);
        Open::Raw { hashes }
    }

    /// A char literal, a lifetime or label, or a stray `'`.
    fn quote(&mut self) {
        let s = self.s;
        if let Some(end) = char_end(s, self.i) {
            self.emit(end, Kind::StringChar);
        } else if self.at(1).is_some_and(is_ident_start) {
            self.emit(ident_end(s, self.i + 1), Kind::Label);
        } else {
            self.emit(self.i + 1, Kind::Punctuation);
        }
    }

    fn number(&mut self) {
        let s = self.s;
        let mut j = self.i + 1;
        let radix = matches!(self.at(1), Some(b'x' | b'o' | b'b'));
        if s[self.i] == b'0' && radix {
            j = ident_end(s, j + 1);
        } else {
            j = digits_end(s, j);
            if s.get(j) == Some(&b'.') {
                match s.get(j + 1) {
                    Some(b) if b.is_ascii_digit() => {
                        j = digits_end(s, j + 1);
                    }
                    Some(&b) if b == b'.' || is_ident_start(b) => {}
                    _ => j += 1,
                }
            }
            if matches!(s.get(j), Some(b'e' | b'E')) {
                let sign = matches!(s.get(j + 1), Some(b'+' | b'-'));
                let k = j + 1 + usize::from(sign);
                if s.get(k).is_some_and(u8::is_ascii_digit) {
                    j = digits_end(s, k);
                }
            }
            j = ident_end(s, j);
        }
        self.emit(j, Kind::Number);
    }

    /// An attribute opener, or a lone `#`.
    fn hash(&mut self) {
        let opener = match (self.at(1), self.at(2)) {
            (Some(b'['), _) => 2,
            (Some(b'!'), Some(b'[')) => 3,
            _ => {
                self.emit(self.i + 1, Kind::Punctuation);
                return;
            }
        };
        self.attr = self.attr.saturating_add(1);
        self.emit(self.i + opener, Kind::Attribute);
    }

    /// A macro metavariable such as `$x`, or a lone `$`.
    fn dollar(&mut self) {
        if self.at(1).is_some_and(is_ident_start) {
            let end = ident_end(self.s, self.i + 1);
            self.emit(end, Kind::VariableSpecial);
        } else {
            self.emit(self.i + 1, Kind::PunctuationSpecial);
        }
    }

    fn dot(&mut self) {
        if self.at(1) != Some(b'.') {
            self.emit(self.i + 1, Kind::PunctuationDelimiter);
            self.prev = Prev::Dot;
            return;
        }
        let len = if matches!(self.at(2), Some(b'.' | b'=')) {
            3
        } else {
            2
        };
        self.emit(self.i + len, Kind::Operator);
    }

    /// An identifier, keyword, or string prefix. Returns what opens
    /// when the word starts a string.
    fn word(&mut self, prev: Prev) -> Option<Open> {
        let s = self.s;
        let end = ident_end(s, self.i);
        let word = &s[self.i..end];
        match (word, s.get(end)) {
            (b"b" | b"c", Some(b'"')) => {
                self.emit(end + 1, Kind::String);
                return Some(Open::Str);
            }
            (b"b", Some(b'\'')) => {
                if let Some(char_end) = char_end(s, end) {
                    self.emit(char_end, Kind::StringChar);
                    return None;
                }
            }
            (b"r" | b"br" | b"cr", Some(b'"' | b'#')) => {
                let hashes = s[end..]
                    .iter()
                    .take_while(|&&b| b == b'#')
                    .count();
                let quote = end + hashes;
                if s.get(quote) == Some(&b'"') {
                    self.emit(quote + 1, Kind::String);
                    let hashes = hashes as u32;
                    return Some(Open::Raw { hashes });
                }
                let raw_ident = s
                    .get(end + 1)
                    .is_some_and(|&b| is_ident_start(b));
                if word == b"r" && raw_ident {
                    let name_end = ident_end(s, end + 1);
                    let name = &s[end + 1..name_end];
                    self.emit(
                        name_end,
                        name_kind(s, name, name_end, prev),
                    );
                    return None;
                }
            }
            _ => {}
        }
        let kind = keyword(word)
            .unwrap_or_else(|| name_kind(s, word, end, prev));
        let macro_bang = usize::from(kind == Kind::FunctionMacro);
        self.emit(end + macro_bang, kind);
        if word == b"fn" {
            self.prev = Prev::Fn;
        }
        None
    }
}

fn comment_kind(doc: bool) -> Kind {
    if doc { Kind::CommentDoc } else { Kind::Comment }
}

fn keyword(word: &[u8]) -> Option<Kind> {
    Some(match word {
        b"break" | b"continue" | b"else" | b"for" | b"if"
        | b"loop" | b"match" | b"return" | b"while" => {
            Kind::KeywordControl
        }
        b"as" | b"async" | b"await" | b"const" | b"crate"
        | b"dyn" | b"enum" | b"extern" | b"fn" | b"impl" | b"in"
        | b"let" | b"mod" | b"move" | b"mut" | b"pub" | b"ref"
        | b"static" | b"struct" | b"super" | b"trait" | b"type"
        | b"unsafe" | b"use" | b"where" => Kind::Keyword,
        b"self" => Kind::VariableSpecial,
        b"Self" => Kind::Type,
        b"true" | b"false" => Kind::Boolean,
        b"bool" | b"char" | b"str" | b"f16" | b"f32" | b"f64"
        | b"f128" | b"i8" | b"i16" | b"i32" | b"i64" | b"i128"
        | b"isize" | b"u8" | b"u16" | b"u32" | b"u64" | b"u128"
        | b"usize" => Kind::TypeBuiltin,
        _ => return None,
    })
}

/// The kind of a name that is not a keyword, from its shape and the
/// tokens around it. The name ends at byte `end` of `s`.
fn name_kind(s: &[u8], name: &[u8], end: usize, prev: Prev) -> Kind {
    let next = s.get(end).copied();
    let path = next == Some(b':') && s.get(end + 1) == Some(&b':');
    let call =
        next == Some(b'(') || (path && s.get(end + 2) == Some(&b'<'));
    if next == Some(b'!') && s.get(end + 1) != Some(&b'=') {
        Kind::FunctionMacro
    } else if prev == Prev::Fn {
        Kind::FunctionDefinition
    } else if prev == Prev::Dot {
        if call {
            Kind::FunctionMethod
        } else {
            Kind::Property
        }
    } else if is_capitalised(name) {
        if is_constant(name) {
            Kind::Constant
        } else {
            Kind::Type
        }
    } else if call {
        Kind::Function
    } else if path {
        Kind::Namespace
    } else {
        Kind::Variable
    }
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

/// `SCREAMING_CASE`: two or more uppercase letters and no lowercase
/// ones.
fn is_constant(name: &[u8]) -> bool {
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

/// The end of the char literal whose `'` is at byte `i`, if one
/// starts there.
fn char_end(s: &[u8], i: usize) -> Option<usize> {
    let body = i + 1;
    let first = *s.get(body)?;
    let after = if first == b'\\' {
        escape_end(s, body)
    } else {
        body + char_len(first)
    };
    (s.get(after) == Some(&b'\'')).then_some(after + 1)
}

/// The end of the escape whose `\` is at byte `i`. A `\` that ends a
/// line continues the string on the next line.
fn escape_end(s: &[u8], i: usize) -> usize {
    match s.get(i + 1) {
        None => i + 1,
        Some(b'x') => {
            let hex = s[i + 2..].iter().take(2);
            i + 2 + hex.take_while(|b| b.is_ascii_hexdigit()).count()
        }
        Some(b'u') if s.get(i + 2) == Some(&b'{') => {
            let digits = s[i + 3..]
                .iter()
                .take_while(|&&b| b.is_ascii_hexdigit() || b == b'_')
                .count();
            let close = i + 3 + digits;
            close + usize::from(s.get(close) == Some(&b'}'))
        }
        Some(&b) => i + 1 + char_len(b),
    }
}

/// The byte length of the char whose first byte is `lead`.
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
    use super::{Mode, Open};
    use crate::State;

    #[test]
    fn modes_round_trip() {
        let opens = [
            Open::Code,
            Open::Comment {
                depth: 1,
                doc: false,
            },
            Open::Comment {
                depth: u32::MAX,
                doc: true,
            },
            Open::Str,
            Open::Raw { hashes: 0 },
            Open::Raw { hashes: 300 },
        ];
        for open in opens {
            for attr in [0, 1, 7] {
                let mode = Mode { open, attr };
                assert_eq!(Mode::load(&mode.store()), mode);
            }
        }
        let code = Mode {
            open: Open::Code,
            attr: 0,
        };
        assert_eq!(code.store(), State::default());
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 1 << 40, 0xFFFF_FFFF] {
            let mode = Mode::load(&State::from_word(word));
            assert_eq!(Mode::load(&mode.store()), mode);
        }
    }
}
