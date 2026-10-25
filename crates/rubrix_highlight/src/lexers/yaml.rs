//! YAML 1.2 block and flow styles.
//!
//! A mapping key is [`Kind::Property`], a list dash is
//! [`Kind::PunctuationListMarker`], and `---` and `...` are
//! [`Kind::PunctuationSpecial`]. An anchor is [`Kind::Label`], an
//! alias [`Kind::LabelAlias`], and a tag [`Kind::Type`]. A scalar is
//! [`Kind::Boolean`], [`Kind::ConstantBuiltin`] for null,
//! [`Kind::Number`], or else [`Kind::String`]. A plain scalar right
//! after a `!!str` tag on the same line is always [`Kind::String`].
//! Quoted strings have [`Kind::StringEscape`] escapes in the
//! double-quoted form.
//!
//! Three things span lines, and the state holds them: a `|` or `>`
//! block scalar, a quoted string, and a flow collection's depth. A
//! block scalar ends on the first non-blank line indented no deeper
//! than the key or dash that holds it, so the state keeps that
//! indent, the digit of an indentation indicator, and the indent of
//! the first body line. Indents above 65,534 columns and flow depths
//! above 255 saturate.
//!
//! Not handled: a plain scalar does not continue onto the next
//! line, so a continuation line that holds `: ` reads as a key.
//! Directives (`%YAML`), tabs as indentation, and timestamps (plain
//! strings here) get no special kind. A `!!str` tag ends its effect
//! at the line end, so a scalar on the next line is read by its
//! shape. As in YAML 1.2, only `true` and `false` are booleans, so
//! `yes`, `no`, `on`, `off`, `y`, and `n` are strings. Nothing
//! checks that the text is valid YAML.

use core::mem;

use memchr::{memchr, memchr2};

use super::space_end;
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    Block {
        parent_plus_one: u16,
        indent_plus_one: u16,
        digit: u8,
    },
    Double,
    Single,
}

const OPEN: u64 = 0b111;
const FLOW_SHIFT: u32 = 8;
const PARENT_SHIFT: u32 = 16;
const INDENT_SHIFT: u32 = 32;
const DIGIT_SHIFT: u32 = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mode {
    open: Open,
    flow: u8,
}

impl Mode {
    fn load(state: &State) -> Mode {
        let word = state.word();
        let open = match word & OPEN {
            1 => Open::Block {
                parent_plus_one: (word >> PARENT_SHIFT) as u16,
                indent_plus_one: (word >> INDENT_SHIFT) as u16,
                digit: (word >> DIGIT_SHIFT) as u8 & 0xF,
            },
            2 => Open::Double,
            3 => Open::Single,
            _ => Open::Code,
        };
        Mode {
            open,
            flow: (word >> FLOW_SHIFT) as u8,
        }
    }

    fn store(self) -> State {
        let open = match self.open {
            Open::Code => 0,
            Open::Block {
                parent_plus_one,
                indent_plus_one,
                digit,
            } => {
                1 | u64::from(parent_plus_one) << PARENT_SHIFT
                    | u64::from(indent_plus_one) << INDENT_SHIFT
                    | u64::from(digit) << DIGIT_SHIFT
            }
            Open::Double => 2,
            Open::Single => 3,
        };
        State::from_word(open | u64::from(self.flow) << FLOW_SHIFT)
    }
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let Mode { mut open, flow } = Mode::load(state);
    let s = line.as_bytes();
    if let Open::Block {
        parent_plus_one,
        indent_plus_one,
        digit,
    } = open
    {
        if let Some(indent_plus_one) =
            block_line(s, parent_plus_one, indent_plus_one, digit)
        {
            if !s.is_empty() {
                out.push(s.len(), Kind::String);
            }
            return Mode {
                open: Open::Block {
                    parent_plus_one,
                    indent_plus_one,
                    digit,
                },
                flow,
            }
            .store();
        }
        open = Open::Code;
    }
    let lead = s.iter().take_while(|&&b| b == b' ').count();
    let mut lexer = Lexer {
        s,
        i: 0,
        out,
        flow: u32::from(flow),
        can_key: open == Open::Code,
        parent_plus_one: lead + 1,
        next: Open::Code,
        str_tag: false,
    };
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code | Open::Block { .. } => lexer.code(),
            Open::Double => lexer.double(),
            Open::Single => lexer.single(),
        };
    }
    Mode {
        open,
        flow: lexer.flow as u8,
    }
    .store()
}

fn block_line(
    s: &[u8],
    parent_plus_one: u16,
    indent_plus_one: u16,
    digit: u8,
) -> Option<u16> {
    let lead = s.iter().take_while(|&&b| b == b' ').count();
    if s[lead..].iter().all(|&b| b == b' ' || b == b'\t') {
        return Some(indent_plus_one);
    }
    if lead == 0 && is_marker(s) {
        return None;
    }
    let needed = if digit > 0 {
        usize::from(parent_plus_one) + usize::from(digit)
    } else if indent_plus_one == 0 {
        usize::from(parent_plus_one) + 1
    } else {
        usize::from(indent_plus_one)
    };
    if lead + 1 < needed {
        return None;
    }
    let first = (lead + 1).min(usize::from(u16::MAX)) as u16;
    Some(if digit == 0 && indent_plus_one == 0 {
        first
    } else {
        indent_plus_one
    })
}

fn is_marker(s: &[u8]) -> bool {
    (s.starts_with(b"---") || s.starts_with(b"..."))
        && s.get(3).is_none_or(|&b| is_blank(b))
}

const fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    flow: u32,
    can_key: bool,
    parent_plus_one: usize,
    next: Open,
    str_tag: bool,
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
            let next = self.at(1);
            let spaced = next.is_none_or(is_blank);
            let block = self.flow == 0;
            match b {
                b'#' if self.i == 0
                    || self.s[self.i - 1].is_ascii_whitespace() =>
                {
                    self.emit(self.s.len(), Kind::Comment);
                }
                b'-' | b'.' if self.i == 0 && is_marker(self.s) => {
                    self.emit(3, Kind::PunctuationSpecial);
                    self.flow = 0;
                    self.parent_plus_one = 0;
                    self.can_key = true;
                }
                b'-' if block && spaced => {
                    self.parent_plus_one = self.i + 1;
                    self.emit(
                        self.i + 1,
                        Kind::PunctuationListMarker,
                    );
                    self.can_key = true;
                }
                b'?' if spaced => {
                    if block {
                        self.parent_plus_one = self.i + 1;
                    }
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                    self.can_key = true;
                }
                b':' if spaced
                    || (!block
                        && matches!(
                            next,
                            Some(b',' | b']' | b'}')
                        )) =>
                {
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                    self.can_key = false;
                }
                b'&' | b'*' | b'!' => self.property(b),
                b'|' | b'>' => {
                    if !(block && self.block_header()) {
                        self.plain();
                    }
                }
                b'[' | b'{' => {
                    self.str_tag = false;
                    self.flow = (self.flow + 1).min(255);
                    self.can_key = true;
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b']' | b'}' if !block => {
                    self.flow -= 1;
                    self.can_key = false;
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b',' if !block => {
                    self.can_key = true;
                    self.emit(self.i + 1, Kind::PunctuationDelimiter);
                }
                b'"' => self.quoted(true),
                b'\'' => self.quoted(false),
                _ => self.plain(),
            }
        }
        self.next
    }

    fn property(&mut self, lead: u8) {
        let s = self.s;
        let mut end = self.i + 1;
        while end < s.len()
            && !s[end].is_ascii_whitespace()
            && !(self.flow > 0
                && matches!(s[end], b',' | b'[' | b']' | b'{' | b'}'))
        {
            end += 1;
        }
        if end == self.i + 1 && lead != b'!' {
            self.plain();
            return;
        }
        let kind = match lead {
            b'&' => Kind::Label,
            b'*' => Kind::LabelAlias,
            _ => {
                self.str_tag = &self.s[self.i..end] == b"!!str";
                Kind::Type
            }
        };
        self.emit(end, kind);
    }

    fn block_header(&mut self) -> bool {
        let s = self.s;
        let mut end = self.i + 1;
        let mut digit = 0;
        let mut chomp = false;
        for _ in 0..2 {
            match s.get(end) {
                Some(b'+' | b'-') if !chomp => chomp = true,
                Some(&d @ b'1'..=b'9') if digit == 0 => {
                    digit = d - b'0';
                }
                _ => break,
            }
            end += 1;
        }
        let rest = space_end(s, end);
        let tail_ok =
            rest == s.len() || (rest > end && s[rest] == b'#');
        if !tail_ok {
            return false;
        }
        self.emit(end, Kind::PunctuationDelimiter);
        self.next = Open::Block {
            parent_plus_one: self
                .parent_plus_one
                .min(usize::from(u16::MAX))
                as u16,
            indent_plus_one: 0,
            digit,
        };
        true
    }

    fn plain(&mut self) {
        let s = self.s;
        let start = self.i;
        let flow = self.flow > 0;
        let tagged = mem::take(&mut self.str_tag);
        let mut j = start;
        let mut stop = s.len();
        let mut is_key = false;
        while let Some(p) = scan(&s[j..], flow).map(|p| j + p) {
            match s[p] {
                b':' => {
                    let next = s.get(p + 1);
                    let ends = next.is_none_or(|&b| is_blank(b))
                        || (flow
                            && matches!(
                                next,
                                Some(b',' | b']' | b'}')
                            ));
                    if ends && self.can_key {
                        stop = p;
                        is_key = true;
                        break;
                    }
                    j = p + 1;
                }
                b'#' => {
                    if p > start && is_blank(s[p - 1]) {
                        stop = p;
                        break;
                    }
                    j = p + 1;
                }
                _ => {
                    stop = p;
                    break;
                }
            }
        }
        let mut end = stop;
        while end > start + 1 && s[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        if is_key {
            if !flow {
                self.parent_plus_one = start + 1;
            }
            self.emit(end, Kind::Property);
        } else {
            let kind = if tagged {
                Kind::String
            } else {
                scalar_kind(&s[start..end])
            };
            self.emit(end, kind);
        }
        self.can_key = false;
    }

    fn quoted(&mut self, double: bool) {
        let s = self.s;
        let start = self.i;
        if self.can_key
            && let Some(end) = quoted_end(s, start + 1, double)
        {
            let colon = s[end..]
                .iter()
                .position(|&b| !is_blank(b))
                .map_or(s.len(), |p| end + p);
            let key = s.get(colon) == Some(&b':')
                && (self.flow > 0
                    || s.get(colon + 1).is_none_or(|&b| is_blank(b)));
            if key {
                if self.flow == 0 {
                    self.parent_plus_one = start + 1;
                }
                self.emit(end, Kind::Property);
                self.out.space(colon);
                self.i = colon;
                self.emit(colon + 1, Kind::PunctuationDelimiter);
                self.can_key = false;
                return;
            }
        }
        let closed = if double {
            self.double_body(start + 1)
        } else {
            self.single_body(start + 1)
        };
        if !closed {
            self.next =
                if double { Open::Double } else { Open::Single };
        }
    }

    fn double(&mut self) -> Open {
        if self.double_body(self.i) {
            Open::Code
        } else {
            Open::Double
        }
    }

    fn single(&mut self) -> Open {
        if self.single_body(self.i) {
            Open::Code
        } else {
            Open::Single
        }
    }

    fn double_body(&mut self, from: usize) -> bool {
        let s = self.s;
        let mut j = from;
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                self.emit(k + 1, Kind::String);
                self.can_key = false;
                return true;
            }
            self.emit(k, Kind::String);
            j = escape_end(s, k);
            self.emit(j, Kind::StringEscape);
        }
        self.emit(s.len(), Kind::String);
        false
    }

    fn single_body(&mut self, from: usize) -> bool {
        match quoted_end(self.s, from, false) {
            Some(end) => {
                self.emit(end, Kind::String);
                self.can_key = false;
                true
            }
            None => {
                self.emit(self.s.len(), Kind::String);
                false
            }
        }
    }
}

fn scan(rest: &[u8], flow: bool) -> Option<usize> {
    if flow {
        rest.iter().position(|b| {
            matches!(
                b,
                b':' | b'#' | b',' | b'[' | b']' | b'{' | b'}'
            )
        })
    } else {
        memchr2(b':', b'#', rest)
    }
}

fn quoted_end(s: &[u8], from: usize, double: bool) -> Option<usize> {
    let mut j = from;
    if double {
        while let Some(k) =
            memchr2(b'"', b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == b'"' {
                return Some(k + 1);
            }
            j = (k + 2).min(s.len());
        }
    } else {
        while let Some(k) = memchr(b'\'', &s[j..]).map(|k| j + k) {
            if s.get(k + 1) != Some(&b'\'') {
                return Some(k + 1);
            }
            j = k + 2;
        }
    }
    None
}

fn escape_end(s: &[u8], k: usize) -> usize {
    let digits = match s.get(k + 1) {
        None => return k + 1,
        Some(b'x') => 2,
        Some(b'u') => 4,
        Some(b'U') => 8,
        Some(&b) => {
            let mut end = k + 2;
            if b >= 0x80 {
                while end < s.len() && s[end] & 0xC0 == 0x80 {
                    end += 1;
                }
            }
            return end;
        }
    };
    let hex = s[k + 2..].iter().take(digits);
    k + 2 + hex.take_while(|b| b.is_ascii_hexdigit()).count()
}

fn scalar_kind(text: &[u8]) -> Kind {
    match text {
        b"~" | b"null" | b"Null" | b"NULL" => Kind::ConstantBuiltin,
        b"true" | b"True" | b"TRUE" | b"false" | b"False"
        | b"FALSE" => Kind::Boolean,
        _ if is_number(text) => Kind::Number,
        _ => Kind::String,
    }
}

fn is_number(text: &[u8]) -> bool {
    let unsigned = match text {
        [b'+' | b'-', rest @ ..] => rest,
        _ => text,
    };
    if matches!(
        unsigned,
        b".inf" | b".Inf" | b".INF" | b".nan" | b".NaN" | b".NAN"
    ) {
        return true;
    }
    if let Some(hex) = unsigned.strip_prefix(b"0x") {
        return !hex.is_empty()
            && hex.iter().all(u8::is_ascii_hexdigit);
    }
    if let Some(oct) = unsigned.strip_prefix(b"0o") {
        return !oct.is_empty()
            && oct.iter().all(|b| matches!(b, b'0'..=b'7'));
    }
    let digits = |s: &[u8]| {
        s.iter()
            .take_while(|&&b| b.is_ascii_digit() || b == b'_')
            .count()
    };
    let whole = digits(unsigned);
    let mut rest = &unsigned[whole..];
    let mut seen = whole > 0;
    if let [b'.', tail @ ..] = rest {
        let fraction = digits(tail);
        seen |= fraction > 0;
        rest = &tail[fraction..];
    }
    if seen && let [b'e' | b'E', tail @ ..] = rest {
        let tail = match tail {
            [b'+' | b'-', tail @ ..] => tail,
            _ => tail,
        };
        let exponent = digits(tail);
        if exponent == 0 {
            return false;
        }
        rest = &tail[exponent..];
    }
    seen && rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::{Mode, Open, is_number};
    use crate::State;

    #[test]
    fn modes_round_trip() {
        let opens = [
            Open::Code,
            Open::Block {
                parent_plus_one: 1,
                indent_plus_one: 0,
                digit: 0,
            },
            Open::Block {
                parent_plus_one: u16::MAX,
                indent_plus_one: 7,
                digit: 9,
            },
            Open::Double,
            Open::Single,
        ];
        for open in opens {
            for flow in [0, 1, 255] {
                let mode = Mode { open, flow };
                assert_eq!(Mode::load(&mode.store()), mode);
            }
        }
        let code = Mode {
            open: Open::Code,
            flow: 0,
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

    #[test]
    fn numbers() {
        for yes in [
            "0", "-12", "+3.5", ".5", "1e3", "2.5E-3", "0x1F",
            "0o17", ".inf", "-.inf", "1_000",
        ] {
            assert!(is_number(yes.as_bytes()), "{yes}");
        }
        for no in [
            "", "-", ".", "e3", "1e", "1.2.3", "0x", "12ab", "v1.2",
            "1-2",
        ] {
            assert!(!is_number(no.as_bytes()), "{no}");
        }
    }
}
