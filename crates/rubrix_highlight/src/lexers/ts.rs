//! The TypeScript and JavaScript lexer. One lexer serves `.ts`,
//! `.mts`, `.cts`, `.js`, `.mjs`, and `.cjs`, and also `.tsx` and
//! `.jsx`.
//!
//! JSX tags get no rules, so `<div className="a">` lexes as
//! operators, names, and a string. A `/` straight after a `<` never
//! starts a regex, which keeps `</div>` from opening one.
//!
//! Template literals nest to any depth and run across lines. The
//! state keeps an open template and one brace counter per open
//! `${`. A `/` is a regex unless the last significant token ends a
//! value: a name, a number, a string, a closing `)`, `]`, or `}`, or
//! `this` and `super`. A regex never spans lines.
//!
//! Classes, interfaces, enums, and type aliases bind
//! `type.definition`. Pierre's TypeScript lexer calls such a name
//! `type.class`, which `Kind` does not have. Names are not checked
//! for being parameters, so `variable.parameter` is never emitted.

use alloc::vec::Vec;

use memchr::{memchr, memchr2, memchr3};

use super::{is_ident_continue, is_ident_start, space_end};
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    Code,
    Block,
    Doc,
    Template,
    Quoted(u8),
}

/// The last significant token of the text so far, which decides how
/// the next token reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Prev {
    /// An operator, an opening bracket, or the text's start. A `/`
    /// here opens a regex.
    Other,
    /// Ends a value, so a `/` here divides.
    Value,
    /// A `.` or `?.`, so a name here is a property.
    Dot,
    /// `function`.
    Fn,
    /// `class`, `interface`, `enum`, `type`, `namespace`.
    Type,
    /// `new`.
    New,
    /// `import` or `export`.
    Module,
}

const OPEN_MASK: u64 = 0xF;
const PREV_SHIFT: u32 = 4;
const FRAME: usize = 4;

fn load(state: &State) -> (Open, Prev, Vec<u32>) {
    let word = state.word();
    let open = match word & OPEN_MASK {
        1 => Open::Block,
        2 => Open::Doc,
        3 => Open::Template,
        4 => Open::Quoted(b'\''),
        5 => Open::Quoted(b'"'),
        _ => Open::Code,
    };
    let prev = match (word >> PREV_SHIFT) & OPEN_MASK {
        1 => Prev::Value,
        2 => Prev::Dot,
        3 => Prev::Fn,
        4 => Prev::Type,
        5 => Prev::New,
        6 => Prev::Module,
        _ => Prev::Other,
    };
    let frames = state
        .tail()
        .chunks_exact(FRAME)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    (open, prev, frames)
}

fn store(open: Open, prev: Prev, frames: &[u32]) -> State {
    let open = match open {
        Open::Code => 0,
        Open::Block => 1,
        Open::Doc => 2,
        Open::Template => 3,
        Open::Quoted(b'\'') => 4,
        Open::Quoted(_) => 5,
    };
    let word = open | (prev as u64) << PREV_SHIFT;
    if frames.is_empty() {
        return State::from_word(word);
    }
    let tail: Vec<u8> =
        frames.iter().flat_map(|f| f.to_le_bytes()).collect();
    State::with_tail(word, &tail)
}

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let (mut open, prev, frames) = load(state);
    let mut lexer = Lexer {
        s: line.as_bytes(),
        i: 0,
        out,
        prev,
        frames,
    };
    while lexer.i < lexer.s.len() {
        open = match open {
            Open::Code => lexer.code(),
            Open::Block => lexer.block(false),
            Open::Doc => lexer.block(true),
            Open::Template => lexer.template(),
            Open::Quoted(quote) => lexer.string(quote),
        };
    }
    store(open, lexer.prev, &lexer.frames)
}

struct Lexer<'s, 'o, 'r> {
    s: &'s [u8],
    i: usize,
    out: &'o mut Runs<'r>,
    prev: Prev,
    /// The count of unclosed `{` inside each open `${`, innermost
    /// last.
    frames: Vec<u32>,
}

impl Lexer<'_, '_, '_> {
    fn emit(&mut self, end: usize, kind: Kind) {
        self.out.push(end, kind);
        self.i = end;
    }

    fn at(&self, offset: usize) -> Option<u8> {
        self.s.get(self.i + offset).copied()
    }

    fn peek(&self, mut i: usize) -> Option<u8> {
        while matches!(self.s.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        self.s.get(i).copied()
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
            let prev =
                core::mem::replace(&mut self.prev, Prev::Other);
            let open = match b {
                b'/' => self.slash(prev),
                b'"' | b'\'' => {
                    self.emit(self.i + 1, Kind::String);
                    self.string(b)
                }
                b'`' => {
                    self.emit(self.i + 1, Kind::String);
                    self.template()
                }
                b'}' => self.close_brace(),
                _ => {
                    self.token(b, prev);
                    Open::Code
                }
            };
            if open != Open::Code {
                return open;
            }
        }
        Open::Code
    }

    fn token(&mut self, b: u8, prev: Prev) {
        match b {
            b'0'..=b'9' => self.number(),
            b'.' => self.dot(),
            b'#' => self.hash(prev),
            b'@' => self.decorator(),
            b'(' | b'[' => {
                self.emit(self.i + 1, Kind::PunctuationBracket);
            }
            b'{' => {
                if let Some(depth) = self.frames.last_mut() {
                    *depth = depth.saturating_add(1);
                }
                self.emit(self.i + 1, Kind::PunctuationBracket);
            }
            b')' | b']' => {
                self.emit(self.i + 1, Kind::PunctuationBracket);
                self.prev = Prev::Value;
            }
            b',' | b';' | b':' => {
                self.emit(self.i + 1, Kind::PunctuationDelimiter);
            }
            b'?' => self.question(),
            b'!' => self.bang(prev),
            b'*' if matches!(prev, Prev::Fn | Prev::Module) => {
                self.emit(self.i + 1, Kind::Operator);
                self.prev = if prev == Prev::Fn {
                    Prev::Fn
                } else {
                    Prev::Value
                };
            }
            _ if is_operator(b) => self.operators(prev),
            _ if is_word_start(b) => self.word(prev),
            _ => self.emit(self.i + 1, Kind::Plain),
        }
    }

    fn close_brace(&mut self) -> Open {
        match self.frames.last_mut() {
            Some(0) => {
                self.frames.pop();
                self.emit(self.i + 1, Kind::PunctuationSpecial);
                self.template()
            }
            top => {
                if let Some(depth) = top {
                    *depth -= 1;
                }
                self.emit(self.i + 1, Kind::PunctuationBracket);
                self.prev = Prev::Value;
                Open::Code
            }
        }
    }

    fn slash(&mut self, prev: Prev) -> Open {
        match self.at(1) {
            Some(b'/') => {
                self.emit(self.s.len(), Kind::Comment);
                self.prev = prev;
                Open::Code
            }
            Some(b'*') => {
                let doc = self.at(2) == Some(b'*')
                    && self.at(3) != Some(b'/');
                self.prev = prev;
                self.emit(self.i + 2, comment_kind(doc));
                self.block(doc)
            }
            _ => {
                let after_angle =
                    self.i > 0 && self.s[self.i - 1] == b'<';
                let regex = (prev != Prev::Value && !after_angle)
                    .then(|| regex_end(self.s, self.i))
                    .flatten();
                if let Some(end) = regex {
                    self.emit(end, Kind::StringRegex);
                    self.prev = Prev::Value;
                } else {
                    self.emit(self.i + 1, Kind::Operator);
                }
                Open::Code
            }
        }
    }

    fn block(&mut self, doc: bool) -> Open {
        let s = self.s;
        let mut j = self.i;
        let close = loop {
            match memchr(b'*', &s[j..]) {
                Some(k) if s.get(j + k + 1) == Some(&b'/') => {
                    break Some(j + k + 2);
                }
                Some(k) => j += k + 1,
                None => break None,
            }
        };
        if doc {
            self.doc_tags(close.map_or(s.len(), |end| end - 2));
        }
        self.emit(close.unwrap_or(s.len()), comment_kind(doc));
        match (close, doc) {
            (Some(_), _) => Open::Code,
            (None, true) => Open::Doc,
            (None, false) => Open::Block,
        }
    }

    /// Marks the `@tag`, `{type}`, and name parts of the JSDoc text
    /// from the cursor to `end`.
    fn doc_tags(&mut self, end: usize) {
        let s = self.s;
        let mut from = self.i;
        while let Some(at) =
            memchr(b'@', &s[from..end]).map(|k| from + k)
        {
            from = at + 1;
            let starts_word = at == 0
                || matches!(
                    s[at - 1],
                    b'*' | b'{' | b'(' | b' ' | b'\t'
                );
            if !starts_word
                || !s.get(from).is_some_and(u8::is_ascii_alphabetic)
            {
                continue;
            }
            let tag_end = s[from..end]
                .iter()
                .position(|b| !b.is_ascii_alphanumeric())
                .map_or(end, |k| from + k);
            self.emit(at, Kind::CommentDoc);
            self.emit(tag_end, Kind::KeywordJsdoc);
            let named = matches!(
                &s[from..tag_end],
                b"param"
                    | b"arg"
                    | b"argument"
                    | b"prop"
                    | b"property"
                    | b"template"
                    | b"typedef"
                    | b"callback"
            );
            let mut after = tag_end;
            let open = blanks_end(s, after, end);
            if let Some(close) = brace_end(s, open, end) {
                self.emit(open, Kind::CommentDoc);
                self.emit(open + 1, Kind::PunctuationBracket);
                self.emit(close - 1, Kind::TypeJsdoc);
                self.emit(close, Kind::PunctuationBracket);
                after = close;
            }
            if named {
                let name = blanks_end(s, after, end);
                let name_end = s[name..end]
                    .iter()
                    .position(|&b| !is_doc_name(b))
                    .map_or(end, |k| name + k);
                if name_end > name {
                    self.emit(name, Kind::CommentDoc);
                    self.emit(name_end, Kind::VariableJsdoc);
                    after = name_end;
                }
            }
            from = after;
        }
    }

    fn string(&mut self, quote: u8) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) =
            memchr2(quote, b'\\', &s[j..]).map(|k| j + k)
        {
            if s[k] == quote {
                self.emit(k + 1, Kind::String);
                self.prev = Prev::Value;
                return Open::Code;
            }
            self.emit(k, Kind::String);
            j = escape_end(s, k);
            self.emit(j, Kind::StringEscape);
            if k + 1 == s.len() {
                return Open::Quoted(quote);
            }
        }
        self.emit(s.len(), Kind::String);
        self.prev = Prev::Value;
        Open::Code
    }

    fn template(&mut self) -> Open {
        let s = self.s;
        let mut j = self.i;
        while let Some(k) =
            memchr3(b'`', b'\\', b'$', &s[j..]).map(|k| j + k)
        {
            match s[k] {
                b'`' => {
                    self.emit(k + 1, Kind::String);
                    self.prev = Prev::Value;
                    return Open::Code;
                }
                b'\\' => {
                    self.emit(k, Kind::String);
                    j = escape_end(s, k);
                    self.emit(j, Kind::StringEscape);
                }
                _ if s.get(k + 1) == Some(&b'{') => {
                    self.emit(k, Kind::String);
                    self.emit(k + 2, Kind::PunctuationSpecial);
                    self.frames.push(0);
                    self.prev = Prev::Other;
                    return Open::Code;
                }
                _ => j = k + 1,
            }
        }
        self.emit(s.len(), Kind::String);
        Open::Template
    }

    fn number(&mut self) {
        let s = self.s;
        let mut j = self.i;
        if s[j] == b'0'
            && matches!(
                s.get(j + 1),
                Some(b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            )
        {
            j += 2;
        } else {
            j = digits_end(s, j);
            if s.get(j) == Some(&b'.') {
                match s.get(j + 1) {
                    Some(b) if b.is_ascii_digit() => {
                        j = digits_end(s, j + 1);
                    }
                    Some(&b) if b == b'.' || is_word_start(b) => {}
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
        }
        self.emit(word_end(s, j), Kind::Number);
        self.prev = Prev::Value;
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

    fn hash(&mut self, prev: Prev) {
        if self.i == 0 && self.at(1) == Some(b'!') {
            self.emit(self.s.len(), Kind::Comment);
            self.prev = prev;
        } else if self.at(1).is_some_and(is_word_start) {
            let end = word_end(self.s, self.i + 1);
            let call = self.s.get(end) == Some(&b'(');
            let kind = if call {
                Kind::FunctionMethod
            } else {
                Kind::Property
            };
            self.emit(end, kind);
            self.prev = Prev::Value;
        } else {
            self.emit(self.i + 1, Kind::Punctuation);
        }
    }

    fn decorator(&mut self) {
        if !self.at(1).is_some_and(is_word_start) {
            self.emit(self.i + 1, Kind::Operator);
            return;
        }
        let s = self.s;
        let mut end = word_end(s, self.i + 1);
        while s.get(end) == Some(&b'.')
            && s.get(end + 1).copied().is_some_and(is_word_start)
        {
            end = word_end(s, end + 1);
        }
        self.emit(end, Kind::FunctionDecorator);
    }

    fn question(&mut self) {
        let len = match (self.at(1), self.at(2)) {
            (Some(b'.'), after)
                if !after.is_some_and(|b| b.is_ascii_digit()) =>
            {
                self.emit(self.i + 2, Kind::PunctuationDelimiter);
                self.prev = Prev::Dot;
                return;
            }
            (Some(b'?'), Some(b'=')) => 3,
            (Some(b'?'), _) => 2,
            _ => 1,
        };
        self.emit(self.i + len, Kind::Operator);
    }

    fn bang(&mut self, prev: Prev) {
        let adjacent =
            self.i > 0 && !self.s[self.i - 1].is_ascii_whitespace();
        if prev == Prev::Value && adjacent && self.at(1) != Some(b'=')
        {
            self.emit(self.i + 1, Kind::Operator);
            self.prev = Prev::Value;
        } else {
            self.operators(prev);
        }
    }

    fn operators(&mut self, prev: Prev) {
        let s = self.s;
        let mut end = self.i + 1;
        while end < s.len() && is_operator(s[end]) {
            end += 1;
        }
        let postfix = prev == Prev::Value
            && matches!(&s[self.i..end], b"++" | b"--");
        self.emit(end, Kind::Operator);
        if postfix {
            self.prev = Prev::Value;
        }
    }

    fn word(&mut self, prev: Prev) {
        let s = self.s;
        let end = word_end(s, self.i);
        let word = &s[self.i..end];
        if prev == Prev::Dot {
            let call = s.get(end) == Some(&b'(');
            let kind = if call {
                Kind::FunctionMethod
            } else {
                Kind::Property
            };
            self.emit(end, kind);
            self.prev = Prev::Value;
            return;
        }
        let (kind, after) = match keyword(word) {
            Some(Word::Reserved(kind, after)) => (kind, after),
            Some(Word::Contextual(kind, after))
                if is_keyword_here(word, prev, self.peek(end)) =>
            {
                (kind, after)
            }
            _ => (self.name_kind(word, end, prev), Prev::Value),
        };
        self.emit(end, kind);
        self.prev = after;
    }

    fn name_kind(&self, word: &[u8], end: usize, prev: Prev) -> Kind {
        match prev {
            Prev::Fn => Kind::FunctionDefinition,
            Prev::Type => Kind::TypeDefinition,
            Prev::New => Kind::Constructor,
            _ if self.s.get(end) == Some(&b'(') => Kind::Function,
            _ if is_builtin_type(word)
                && !matches!(self.peek(end), Some(b':' | b'.')) =>
            {
                Kind::TypeBuiltin
            }
            _ if word[0].is_ascii_uppercase() => {
                if is_screaming_case(word) {
                    Kind::Constant
                } else {
                    Kind::Type
                }
            }
            _ => Kind::Variable,
        }
    }
}

enum Word {
    Reserved(Kind, Prev),
    Contextual(Kind, Prev),
}

fn keyword(word: &[u8]) -> Option<Word> {
    use Word::{Contextual, Reserved};
    Some(match word {
        b"if" | b"else" | b"switch" | b"case" | b"default"
        | b"for" | b"while" | b"do" | b"return" | b"try"
        | b"catch" | b"finally" | b"throw" | b"break"
        | b"continue" | b"yield" => {
            Reserved(Kind::KeywordControl, Prev::Other)
        }
        b"const" | b"let" | b"var" => {
            Reserved(Kind::KeywordDeclaration, Prev::Other)
        }
        b"function" => Reserved(Kind::KeywordDeclaration, Prev::Fn),
        b"class" | b"enum" | b"interface" => {
            Reserved(Kind::KeywordDeclaration, Prev::Type)
        }
        b"import" | b"export" => {
            Reserved(Kind::KeywordImport, Prev::Module)
        }
        b"true" | b"false" => Reserved(Kind::Boolean, Prev::Value),
        b"null" | b"undefined" | b"NaN" | b"Infinity" => {
            Reserved(Kind::ConstantBuiltin, Prev::Value)
        }
        b"this" | b"super" => {
            Reserved(Kind::VariableSpecial, Prev::Value)
        }
        b"new" => Reserved(Kind::Keyword, Prev::New),
        b"debugger" | b"delete" | b"extends" | b"implements"
        | b"in" | b"instanceof" | b"package" | b"private"
        | b"protected" | b"public" | b"static" | b"typeof"
        | b"void" | b"with" => Reserved(Kind::Keyword, Prev::Other),
        b"type" | b"namespace" => {
            Contextual(Kind::Keyword, Prev::Type)
        }
        b"from" => Contextual(Kind::KeywordImport, Prev::Other),
        b"abstract" | b"as" | b"async" | b"await" | b"declare"
        | b"get" | b"infer" | b"is" | b"keyof" | b"of"
        | b"override" | b"readonly" | b"satisfies" | b"set" => {
            Contextual(Kind::Keyword, Prev::Other)
        }
        _ => return None,
    })
}

/// Whether a word that is a keyword only in some places is one here,
/// from the token before it and the next byte after blanks.
fn is_keyword_here(
    word: &[u8],
    prev: Prev,
    next: Option<u8>,
) -> bool {
    let name = next.is_some_and(is_word_start);
    let value = prev == Prev::Value;
    match word {
        b"as" | b"satisfies" => {
            value
                && (name || matches!(next, Some(b'{' | b'[' | b'(')))
        }
        b"is" => value && name,
        b"of" => {
            value
                && (name
                    || matches!(
                        next,
                        Some(
                            b'{' | b'['
                                | b'('
                                | b'"'
                                | b'\''
                                | b'`'
                                | b'0'
                                ..=b'9'
                        )
                    ))
        }
        b"type" => {
            name || (prev == Prev::Module
                && matches!(next, Some(b'{' | b'*')))
        }
        b"from" => matches!(next, Some(b'"' | b'\'')),
        b"async" => name || next == Some(b'('),
        b"await" => {
            !matches!(next, Some(b'=' | b':' | b',' | b';' | b')'))
        }
        b"get" | b"set" => {
            name || matches!(
                next,
                Some(b'[' | b'"' | b'\'' | b'0'..=b'9')
            )
        }
        b"readonly" => name || next == Some(b'['),
        _ => name,
    }
}

fn is_builtin_type(word: &[u8]) -> bool {
    matches!(
        word,
        b"any"
            | b"bigint"
            | b"boolean"
            | b"never"
            | b"number"
            | b"object"
            | b"string"
            | b"symbol"
            | b"unknown"
    )
}

fn is_screaming_case(word: &[u8]) -> bool {
    let upper =
        word.iter().filter(|b| b.is_ascii_uppercase()).count();
    upper > 1
        && word.iter().all(|&b| {
            b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
        })
}

fn comment_kind(doc: bool) -> Kind {
    if doc { Kind::CommentDoc } else { Kind::Comment }
}

const fn is_word_start(b: u8) -> bool {
    b == b'$' || is_ident_start(b)
}

const fn is_word(b: u8) -> bool {
    b == b'$' || is_ident_continue(b)
}

fn word_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && is_word(s[i]) {
        i += 1;
    }
    i
}

const fn is_operator(b: u8) -> bool {
    matches!(
        b,
        b'+' | b'-'
            | b'*'
            | b'%'
            | b'^'
            | b'&'
            | b'|'
            | b'='
            | b'<'
            | b'>'
            | b'~'
            | b'!'
    )
}

fn digits_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'_') {
        i += 1;
    }
    i
}

fn blanks_end(s: &[u8], mut i: usize, end: usize) -> usize {
    while i < end && matches!(s[i], b' ' | b'\t') {
        i += 1;
    }
    i
}

/// The index after the `}` that closes the `{` at `open`, when it
/// closes before `end`.
fn brace_end(s: &[u8], open: usize, end: usize) -> Option<usize> {
    if open >= end || s[open] != b'{' {
        return None;
    }
    let mut depth = 0u32;
    for (k, &b) in s[open..end].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + k + 1);
                }
            }
            _ => {}
        }
    }
    None
}

const fn is_doc_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.')
}

/// The end of the regex literal that starts with the `/` at `i`,
/// flags included, when it closes on the line.
fn regex_end(s: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let mut class = false;
    while j < s.len() {
        match s[j] {
            b'\\' => j += 1,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => return Some(word_end(s, j + 1)),
            _ => {}
        }
        j += 1;
    }
    None
}

fn escape_end(s: &[u8], i: usize) -> usize {
    let hex = |from: usize, most: usize| {
        s[from..]
            .iter()
            .take(most)
            .take_while(|b| b.is_ascii_hexdigit())
            .count()
    };
    match s.get(i + 1) {
        None => i + 1,
        Some(b'x') => i + 2 + hex(i + 2, 2),
        Some(b'u') if s.get(i + 2) == Some(&b'{') => {
            let close = i + 3 + hex(i + 3, usize::MAX);
            close + usize::from(s.get(close) == Some(&b'}'))
        }
        Some(b'u') => i + 2 + hex(i + 2, 4),
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
    use alloc::vec;

    use super::{Open, Prev, load, store};
    use crate::State;

    #[test]
    fn modes_round_trip() {
        let opens = [
            Open::Code,
            Open::Block,
            Open::Doc,
            Open::Template,
            Open::Quoted(b'\''),
            Open::Quoted(b'"'),
        ];
        let prevs = [
            Prev::Other,
            Prev::Value,
            Prev::Dot,
            Prev::Fn,
            Prev::Type,
            Prev::New,
            Prev::Module,
        ];
        for open in opens {
            for prev in prevs {
                for frames in [vec![], vec![0], vec![3, 0, u32::MAX]]
                {
                    let state = store(open, prev, &frames);
                    assert_eq!(load(&state), (open, prev, frames));
                }
            }
        }
        assert_eq!(
            store(Open::Code, Prev::Other, &[]),
            State::default()
        );
    }

    #[test]
    fn any_state_loads() {
        for word in [u64::MAX, 7, 15 << 4, 1 << 40] {
            for tail in [&[][..], &[1, 2, 3], &[1, 0, 0, 0, 9]] {
                let state = State::with_tail(word, tail);
                let (open, prev, frames) = load(&state);
                assert_eq!(
                    load(&store(open, prev, &frames)),
                    (open, prev, frames)
                );
            }
        }
    }
}
