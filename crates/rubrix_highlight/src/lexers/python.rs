//! The Python lexer.
//!
//! It covers strings with every prefix, triple quotes across lines,
//! f- and t-string replacement fields with nested quotes, format
//! specs, and fields that span lines, numbers with underscores and
//! the `j` suffix, comments, backslash continuations, decorators,
//! and `def` and `class` names.
//!
//! The state between lines is a stack of up to seven string and
//! field frames, the depth of brackets open in code outside strings
//! (capped at 15), and whether the last line ended in a backslash.
//! The last two decide whether a line that starts with `@` is a
//! decorator or a matrix multiply. A replacement field in a
//! single-quoted f-string stays open at the end of a line, as Python
//! 3.12 and later allow, when the line ends in an operator, a
//! delimiter, or a backslash, or inside brackets. Any other open
//! field, such as `f"{x` followed by a line that starts with `}`, ends
//! at its line, so invalid code cannot swallow the lines after it. A
//! string body, a format spec, or an unterminated string ends at its
//! line, unless it is triple-quoted or the line ends in a backslash.
//!
//! In an f-string, a backslash directly before `{` is a plain
//! character, and a single `{` after it opens a field, as in Python.
//!
//! What it does not handle:
//!
//! - `Kind::StringDoc` is never emitted. A docstring is `string`,
//!   because telling it from other strings needs the parse tree.
//! - Soft keywords (`match`, `case`, `type`, `_`) are not told from
//!   names. `type` is a built-in type everywhere.
//! - Parameter names, annotations, and keyword arguments are
//!   `variable`, like any other name.
//! - A name is a type or a constant by its spelling alone: a
//!   capitalised name is a type, and an all-capitals name is a
//!   constant.
//! - Nesting deeper than seven open strings and fields is lexed as
//!   plain text from the eighth on. More than 15 open brackets count
//!   as 15.
//! - Invalid code is lexed without a diagnostic. `ur"x"` is a string,
//!   and a prefix that Python rejects is a name followed by a string.

use memchr::{memchr2, memchr3};

use super::{ident_end, is_ident_start, space_end};
use crate::{Kind, Runs, State};

const TAG: u8 = 0b11;
const STR: u8 = 1;
const FIELD: u8 = 2;

const DOUBLE: u8 = 1 << 2;
const TRIPLE: u8 = 1 << 3;
const RAW: u8 = 1 << 4;
const FMT: u8 = 1 << 5;

const SPEC: u8 = 1 << 2;
const DEPTH_SHIFT: u32 = 3;
const DEPTH_MAX: u8 = (1 << 5) - 1;

const MAX_FRAMES: u32 = 7;
const LEN_SHIFT: u32 = 56;
const JOINED_SHIFT: u32 = 59;
const BRACKETS_SHIFT: u32 = 60;
const BRACKETS_MAX: u32 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Stack {
    frames: u64,
    len: u32,
    brackets: u32,
    joined: bool,
}

impl Stack {
    fn load(state: &State) -> Stack {
        let word = state.word();
        let len = (word >> LEN_SHIFT) as u32 & MAX_FRAMES;
        let frames = word & ((1 << (8 * len)) - 1);
        Stack {
            frames,
            len,
            brackets: (word >> BRACKETS_SHIFT) as u32 & BRACKETS_MAX,
            joined: word >> JOINED_SHIFT & 1 != 0,
        }
    }

    fn store(self) -> State {
        State::from_word(
            self.frames
                | u64::from(self.len) << LEN_SHIFT
                | u64::from(self.joined) << JOINED_SHIFT
                | u64::from(self.brackets) << BRACKETS_SHIFT,
        )
    }

    fn frame(self, index: u32) -> u8 {
        (self.frames >> (8 * index)) as u8
    }

    fn top(self) -> u8 {
        match self.len {
            0 => 0,
            len => self.frame(len - 1),
        }
    }

    fn push(&mut self, frame: u8) -> bool {
        if self.len == MAX_FRAMES {
            return false;
        }
        self.frames |= u64::from(frame) << (8 * self.len);
        self.len += 1;
        true
    }

    fn truncate(&mut self, len: u32) {
        if len < self.len {
            self.len = len;
            self.frames &= (1 << (8 * len)) - 1;
        }
    }

    fn pop(&mut self) {
        self.truncate(self.len.saturating_sub(1));
    }

    fn set_top(&mut self, frame: u8) {
        if let Some(index) = self.len.checked_sub(1) {
            self.frames &= !(0xFF << (8 * index));
            self.frames |= u64::from(frame) << (8 * index);
        }
    }

    fn owner(self) -> Option<(u32, u8)> {
        (0..self.len)
            .rev()
            .map(|index| (index, self.frame(index)))
            .find(|&(_, frame)| frame & TAG == STR)
    }

    fn drop_single_line_frames(
        &mut self,
        continued: bool,
        dangling: bool,
    ) {
        let top = self.top();
        if top & (TAG | SPEC) == FIELD
            && (top >> DEPTH_SHIFT > 0 || dangling)
        {
            return;
        }
        let mut multiline = false;
        for index in 0..self.len {
            let frame = self.frame(index);
            let keep = match frame & TAG {
                STR => {
                    multiline = frame & TRIPLE != 0;
                    multiline || (continued && index + 1 == self.len)
                }
                FIELD => multiline,
                _ => true,
            };
            if !keep {
                self.truncate(index);
                return;
            }
        }
    }
}

const fn quote_of(frame: u8) -> u8 {
    if frame & DOUBLE != 0 { b'"' } else { b'\'' }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    Other,
    Def,
    Class,
    Dot,
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let mut stack = Stack::load(state);
    let first =
        stack.len == 0 && stack.brackets == 0 && !stack.joined;
    stack.joined = false;
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        stack,
        first,
        continued: false,
        dangling: false,
        prev: Prev::Other,
    };
    while lexer.i < lexer.s.len() {
        let top = lexer.stack.top();
        match top & TAG {
            STR => lexer.string(top),
            FIELD if top & SPEC != 0 => lexer.spec(),
            _ => lexer.code(),
        }
    }
    lexer
        .stack
        .drop_single_line_frames(lexer.continued, lexer.dangling);
    lexer.stack.store()
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    stack: Stack,
    first: bool,
    continued: bool,
    dangling: bool,
    prev: Prev,
}

impl Lexer<'_, '_, '_> {
    fn emit(&mut self, end: usize, kind: Kind) {
        self.out.push(end, kind);
        self.i = end;
        self.dangling = matches!(
            kind,
            Kind::Operator
                | Kind::Punctuation
                | Kind::PunctuationDelimiter
        );
    }

    fn at(&self, offset: usize) -> Option<u8> {
        self.s.get(self.i + offset).copied()
    }

    fn code(&mut self) {
        let field = self.stack.top() & TAG == FIELD;
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
            match b {
                b'#' => self.emit(self.s.len(), Kind::Comment),
                b'"' | b'\'' => {
                    if self.try_open_string(self.i, false, false) {
                        return;
                    }
                    self.emit(self.i + 1, Kind::Plain);
                }
                b'0'..=b'9' => self.number(),
                b'.' => {
                    if self.at(1).is_some_and(|b| b.is_ascii_digit())
                    {
                        self.number();
                    } else {
                        self.dot();
                    }
                }
                b'@' => self.at_sign(first),
                b'(' | b'[' | b'{' => {
                    self.nest(field, 1);
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b')' | b']' => {
                    self.nest(field, -1);
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b'}' => {
                    if field && self.field_depth() == 0 {
                        self.emit(
                            self.i + 1,
                            Kind::PunctuationSpecial,
                        );
                        self.stack.pop();
                        return;
                    }
                    self.nest(field, -1);
                    self.emit(self.i + 1, Kind::PunctuationBracket);
                }
                b':' => {
                    if field && self.field_depth() == 0 {
                        self.emit(
                            self.i + 1,
                            Kind::PunctuationSpecial,
                        );
                        let top = self.stack.top();
                        self.stack.set_top(top | SPEC);
                        return;
                    }
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
                b'!' => {
                    let conversion = field
                        && matches!(
                            self.at(1),
                            Some(b'r' | b's' | b'a')
                        )
                        && matches!(self.at(2), Some(b':' | b'}'));
                    if conversion {
                        self.emit(
                            self.i + 2,
                            Kind::PunctuationSpecial,
                        );
                    } else {
                        self.emit(self.i + 1, Kind::Operator);
                    }
                }
                b'\\' => {
                    self.stack.joined = self.stack.len == 0
                        && self.i + 1 == self.s.len();
                    self.emit(self.i + 1, Kind::Punctuation);
                }
                b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'&'
                | b'|' | b'=' | b'<' | b'>' | b'~' => {
                    self.emit(self.i + 1, Kind::Operator);
                }
                _ if is_ident_start(b) => {
                    if self.word(prev) {
                        return;
                    }
                }
                _ => self.emit(self.i + 1, Kind::Plain),
            }
        }
    }

    fn field_depth(&self) -> u8 {
        self.stack.top() >> DEPTH_SHIFT
    }

    fn nest(&mut self, field: bool, by: i8) {
        if field {
            self.bump_depth(by);
        } else {
            self.stack.brackets = self
                .stack
                .brackets
                .saturating_add_signed(i32::from(by))
                .min(BRACKETS_MAX);
        }
    }

    fn bump_depth(&mut self, by: i8) {
        let top = self.stack.top();
        let depth = (top >> DEPTH_SHIFT)
            .saturating_add_signed(by)
            .min(DEPTH_MAX);
        let flags = top & ((1 << DEPTH_SHIFT) - 1);
        self.stack.set_top(depth << DEPTH_SHIFT | flags);
    }

    fn try_open_string(
        &mut self,
        quote: usize,
        raw: bool,
        fmt: bool,
    ) -> bool {
        let s = self.s;
        let q = s[quote];
        let triple = s.get(quote + 1) == Some(&q)
            && s.get(quote + 2) == Some(&q);
        let mut frame = STR;
        for (flag, set) in [
            (DOUBLE, q == b'"'),
            (TRIPLE, triple),
            (RAW, raw),
            (FMT, fmt),
        ] {
            if set {
                frame |= flag;
            }
        }
        if !self.stack.push(frame) {
            return false;
        }
        self.emit(quote + if triple { 3 } else { 1 }, Kind::String);
        true
    }

    fn string(&mut self, frame: u8) {
        let s = self.s;
        let quote = quote_of(frame);
        let triple = frame & TRIPLE != 0;
        let mut j = self.i;
        loop {
            let rest = &s[j..];
            let found = if frame & FMT != 0 {
                memchr3(quote, b'\\', b'{', rest)
            } else {
                memchr2(quote, b'\\', rest)
            };
            let Some(k) = found.map(|k| j + k) else { break };
            match s[k] {
                b'\\' => {
                    if k + 1 == s.len() {
                        self.emit(k, Kind::String);
                        let kind = if frame & RAW != 0 {
                            Kind::String
                        } else {
                            Kind::StringEscape
                        };
                        self.emit(s.len(), kind);
                        self.continued = true;
                        return;
                    }
                    if frame & FMT != 0 && s[k + 1] == b'{' {
                        j = k + 1;
                    } else if frame & RAW != 0 {
                        j = k + 2;
                    } else {
                        self.emit(k, Kind::String);
                        j = escape_end(s, k);
                        self.emit(j, Kind::StringEscape);
                    }
                }
                b'{' => {
                    if s.get(k + 1) == Some(&b'{') {
                        j = k + 2;
                    } else if self.stack.len < MAX_FRAMES {
                        self.emit(k, Kind::String);
                        self.emit(k + 1, Kind::PunctuationSpecial);
                        self.stack.push(FIELD);
                        return;
                    } else {
                        j = k + 1;
                    }
                }
                _ => {
                    if closes(s, k, quote, triple) {
                        let len = if triple { 3 } else { 1 };
                        self.emit(k + len, Kind::String);
                        self.stack.pop();
                        return;
                    }
                    j = k + 1;
                }
            }
        }
        self.emit(s.len(), Kind::String);
    }

    fn spec(&mut self) {
        let s = self.s;
        let owner = self.stack.owner();
        let (quote, triple) = owner
            .map_or((b'{', false), |(_, frame)| {
                (quote_of(frame), frame & TRIPLE != 0)
            });
        let mut j = self.i;
        while let Some(k) =
            memchr3(b'{', b'}', quote, &s[j..]).map(|k| j + k)
        {
            match s[k] {
                b'{' if quote != b'{' => {
                    if self.stack.len == MAX_FRAMES {
                        j = k + 1;
                        continue;
                    }
                    self.emit(k, Kind::String);
                    self.emit(k + 1, Kind::PunctuationSpecial);
                    self.stack.push(FIELD);
                    return;
                }
                b'}' => {
                    self.emit(k, Kind::String);
                    self.emit(k + 1, Kind::PunctuationSpecial);
                    self.stack.pop();
                    return;
                }
                _ => {
                    if let Some((index, _)) = owner
                        && closes(s, k, quote, triple)
                    {
                        let len = if triple { 3 } else { 1 };
                        self.emit(k + len, Kind::String);
                        self.stack.truncate(index);
                        return;
                    }
                    j = k + 1;
                }
            }
        }
        self.emit(s.len(), Kind::String);
    }

    fn number(&mut self) {
        let s = self.s;
        let mut j = self.i;
        let radix = s[j] == b'0'
            && matches!(
                self.at(1),
                Some(b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            );
        if radix {
            j += 2;
            while s
                .get(j)
                .is_some_and(|&b| b.is_ascii_hexdigit() || b == b'_')
            {
                j += 1;
            }
        } else {
            if s[j] != b'.' {
                j = digits_end(s, j);
            }
            if s.get(j) == Some(&b'.') {
                j = digits_end(s, j + 1);
            }
            if matches!(s.get(j), Some(b'e' | b'E')) {
                let sign = matches!(s.get(j + 1), Some(b'+' | b'-'));
                let k = j + 1 + usize::from(sign);
                if s.get(k).is_some_and(u8::is_ascii_digit) {
                    j = digits_end(s, k);
                }
            }
            if matches!(s.get(j), Some(b'j' | b'J')) {
                j += 1;
            }
        }
        self.emit(j, Kind::Number);
    }

    fn dot(&mut self) {
        if self.at(1) == Some(b'.') && self.at(2) == Some(b'.') {
            self.emit(self.i + 3, Kind::ConstantBuiltin);
        } else {
            self.emit(self.i + 1, Kind::PunctuationDelimiter);
            self.prev = Prev::Dot;
        }
    }

    fn at_sign(&mut self, first: bool) {
        let s = self.s;
        if !(first && self.at(1).is_some_and(is_ident_start)) {
            self.emit(self.i + 1, Kind::Operator);
            return;
        }
        let mut end = ident_end(s, self.i + 1);
        while s.get(end) == Some(&b'.')
            && s.get(end + 1).copied().is_some_and(is_ident_start)
        {
            end = ident_end(s, end + 1);
        }
        self.emit(end, Kind::FunctionDecorator);
    }

    fn word(&mut self, prev: Prev) -> bool {
        let s = self.s;
        let end = ident_end(s, self.i);
        let word = &s[self.i..end];
        if matches!(s.get(end), Some(b'"' | b'\''))
            && let Some((raw, fmt)) = string_prefix(word)
            && self.try_open_string(end, raw, fmt)
        {
            return true;
        }
        let call = s.get(end) == Some(&b'(');
        let kind = match prev {
            Prev::Def => Kind::FunctionDefinition,
            Prev::Class => Kind::TypeDefinition,
            Prev::Dot if word == b"import" => Kind::KeywordImport,
            Prev::Dot if call => Kind::FunctionMethod,
            Prev::Dot => Kind::Property,
            Prev::Other => {
                keyword(word).unwrap_or_else(|| name_kind(word, call))
            }
        };
        self.emit(end, kind);
        if prev == Prev::Other {
            match word {
                b"def" => self.prev = Prev::Def,
                b"class" => self.prev = Prev::Class,
                _ => {}
            }
        }
        false
    }
}

fn closes(s: &[u8], at: usize, quote: u8, triple: bool) -> bool {
    !triple || s.get(at + 1..at + 3) == Some(&[quote, quote][..])
}

fn string_prefix(word: &[u8]) -> Option<(bool, bool)> {
    let mut raw = false;
    let mut fmt = false;
    let mut bytes = false;
    for (n, &b) in word.iter().enumerate() {
        let flag = match b.to_ascii_lowercase() {
            b'r' => &mut raw,
            b'f' | b't' => &mut fmt,
            b'b' | b'u' => &mut bytes,
            _ => return None,
        };
        if *flag || n > 1 {
            return None;
        }
        *flag = true;
    }
    let valid = match word.len() {
        1 => true,
        _ => raw && (fmt || bytes),
    };
    valid.then_some((raw, fmt))
}

fn keyword(word: &[u8]) -> Option<Kind> {
    Some(match word {
        b"if" | b"elif" | b"else" | b"for" | b"while" | b"break"
        | b"continue" | b"return" | b"try" | b"except"
        | b"finally" | b"raise" | b"yield" | b"with" | b"assert" => {
            Kind::KeywordControl
        }
        b"def" | b"class" | b"lambda" | b"global" | b"nonlocal"
        | b"async" | b"await" | b"del" | b"pass" | b"as" => {
            Kind::Keyword
        }
        b"import" | b"from" => Kind::KeywordImport,
        b"and" | b"or" | b"not" | b"in" | b"is" => {
            Kind::KeywordOperator
        }
        b"True" | b"False" => Kind::Boolean,
        b"None" | b"Ellipsis" | b"NotImplemented" | b"__debug__" => {
            Kind::ConstantBuiltin
        }
        b"self" | b"cls" => Kind::VariableSpecial,
        b"bool" | b"bytearray" | b"bytes" | b"complex" | b"dict"
        | b"float" | b"frozenset" | b"int" | b"list"
        | b"memoryview" | b"object" | b"set" | b"slice" | b"str"
        | b"tuple" | b"type" => Kind::TypeBuiltin,
        b"abs" | b"aiter" | b"all" | b"anext" | b"any" | b"ascii"
        | b"bin" | b"breakpoint" | b"callable" | b"chr"
        | b"classmethod" | b"compile" | b"delattr" | b"dir"
        | b"divmod" | b"enumerate" | b"eval" | b"exec"
        | b"filter" | b"format" | b"getattr" | b"globals"
        | b"hasattr" | b"hash" | b"help" | b"hex" | b"id"
        | b"input" | b"isinstance" | b"issubclass" | b"iter"
        | b"len" | b"locals" | b"map" | b"max" | b"min" | b"next"
        | b"oct" | b"open" | b"ord" | b"pow" | b"print"
        | b"property" | b"range" | b"repr" | b"reversed"
        | b"round" | b"setattr" | b"sorted" | b"staticmethod"
        | b"sum" | b"super" | b"vars" | b"zip" | b"__import__" => {
            Kind::FunctionBuiltin
        }
        _ => return None,
    })
}

fn name_kind(name: &[u8], call: bool) -> Kind {
    let private = name.iter().take_while(|&&b| b == b'_').count();
    let bare = &name[private..];
    if is_capitalised(bare) {
        if is_screaming_case(bare) {
            Kind::Constant
        } else {
            Kind::Type
        }
    } else if call {
        Kind::Function
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
    let hex = |count: usize| {
        let digits = s.get(i + 2..).unwrap_or_default();
        let digits = digits.iter().take(count);
        i + 2 + digits.take_while(|b| b.is_ascii_hexdigit()).count()
    };
    match s.get(i + 1) {
        None => i + 1,
        Some(b'x') => hex(2),
        Some(b'u') => hex(4),
        Some(b'U') => hex(8),
        Some(b'N') if s.get(i + 2) == Some(&b'{') => {
            let name = s.get(i + 3..).unwrap_or_default();
            match name.iter().position(|&b| b == b'}') {
                Some(close) => i + 4 + close,
                None => s.len(),
            }
        }
        Some(b'0'..=b'7') => {
            let digits = s[i + 1..].iter().take(3);
            let octal =
                digits.take_while(|b| (b'0'..=b'7').contains(b));
            i + 1 + octal.count()
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
    use super::{
        BRACKETS_MAX, DEPTH_MAX, DEPTH_SHIFT, DOUBLE, FIELD, FMT,
        MAX_FRAMES, RAW, SPEC, STR, Stack, TRIPLE,
    };
    use crate::State;

    #[test]
    fn stacks_round_trip() {
        let frames = [
            STR,
            STR | DOUBLE | TRIPLE | RAW | FMT,
            FIELD | SPEC,
            FIELD | DEPTH_MAX << DEPTH_SHIFT,
        ];
        let mut stack = Stack::default();
        assert_eq!(stack.store(), State::default());
        for frame in frames {
            assert!(stack.push(frame));
            assert_eq!(Stack::load(&stack.store()), stack);
        }
        while stack.len > 0 {
            stack.pop();
            assert_eq!(Stack::load(&stack.store()), stack);
        }
        assert_eq!(stack.frames, 0);
    }

    #[test]
    fn line_state_round_trips() {
        let stack = Stack {
            brackets: BRACKETS_MAX,
            joined: true,
            ..Stack::default()
        };
        assert_eq!(Stack::load(&stack.store()), stack);
    }

    #[test]
    fn stack_stops_at_its_limit() {
        let mut stack = Stack::default();
        for _ in 0..MAX_FRAMES {
            assert!(stack.push(STR));
        }
        assert!(!stack.push(STR));
        assert_eq!(stack.len, MAX_FRAMES);
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 1 << 40, 0xFFFF_FFFF, 7 << 56] {
            let stack = Stack::load(&State::from_word(word));
            assert_eq!(Stack::load(&stack.store()), stack);
        }
    }
}
