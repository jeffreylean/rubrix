//! Replays an editing script against a file and checks the result.
//!
//! ```text
//! cargo run --release -p rubrix_edit --example replay -- <file> <script>
//! cargo run --release -p rubrix_edit --example replay -- --model <file> <script>
//! ```
//!
//! The script runs against an [`Editor`] on the file, with
//! [`Monospace::default`] as the layout. Each check prints
//! `replay: <script line> <kind> pass` or
//! `replay: <script line> <kind> fail: got <value>`, and the run ends
//! with `replay: <passed> of <total> checks pass`. The run exits 1
//! when a check fails, and 2 on a bad script line, printing
//! `replay: line <n>: <reason>`, or on an unreadable file.
//!
//! # Script
//!
//! One step per line. Blank lines and lines starting with `#` are
//! skipped.
//!
//! Header lines, allowed only before the first step, change the file
//! before the steps run:
//!
//! - `repeat <lines>` repeats the file in whole copies until it has
//!   at least that many lines, as `rubrix_bench::Fixture::repeated`
//!   does.
//! - `crlf` turns every `\n` into `\r\n`, after any `repeat`.
//! - `lang <name>` sets the script's language, a
//!   `rubrix_highlight::Lang` name. It defaults to what
//!   `Lang::detect` reads from the file's path and first line.
//!
//! A `<pos>` is `<line>:<col>`, the line counted from 1 and the
//! column counted from 1 in bytes of the line, so `1:1` is offset 0.
//! It must lie on a char boundary of the line's content, its end
//! included.
//!
//! A `<text>` is the rest of the line after one space, with the
//! escapes `\n`, `\r`, `\t`, `\s` for a space, and `\\`. A missing
//! text is empty.
//!
//! Steps:
//!
//! - `caret <pos>` sets one caret.
//! - `add-caret <pos>` adds a caret as the new primary.
//! - `select <pos> <pos>` sets one selection, anchor then head.
//! - `type <text>` runs one `Command::Type` per char of the text.
//! - `paste <text>` runs one `Command::Paste` with the whole text.
//! - `key <name> [<times>]` runs a command `times` times, 1 by
//!   default. The motions are `left`, `right`, `word-left`,
//!   `word-right`, `home`, `end`, `up`, `down`, `page-up`,
//!   `page-down`, `doc-start` and `doc-end`, each also as
//!   `shift+<name>` to extend. The other names are `backspace`,
//!   `delete`, `delete-word-back`, `delete-word-forward`, `enter`,
//!   `cut`, `select-all`, `collapse`, `undo`, `redo`, `indent`,
//!   `outdent`, `indent-lines`, `move-up`, `move-down`, `copy-up`,
//!   `copy-down`, `blank-below`, `transpose`, `kill-line` and
//!   `next-occurrence`, and `comment` and `block-comment`, which
//!   toggle comments in the script's language.
//! - `column <pos> <pos>` sets the selections `rubrix_edit::column`
//!   gives for a drag from the first position to the second, at the
//!   lines and [`Monospace`] x of each.
//! - `toggle-caret <pos>` sets the selections
//!   `rubrix_edit::toggle_caret` gives at the position.
//! - `copy` keeps what `Editor::copy_text` gives.
//!
//! Checks:
//!
//! - `expect text <text>` compares the whole text.
//! - `expect line <n> <text>` compares the content of line `n`,
//!   without its `\n` and without a `\r` before it.
//! - `expect carets <count> primary <k> <sel>...` compares the
//!   selections in buffer order. `k` is the primary's place, counted
//!   from 1, and each `<sel>` is a `<pos>` for a caret or
//!   `<pos>..<pos>`, anchor then head, for a range.
//! - `expect undo-depth <n>` compares the undo steps.
//! - `expect hash <hex>` compares the 64-bit FNV-1a hash of the
//!   text's bytes, as 16 lowercase hex digits.
//! - `expect copied <text>` compares the text the last `copy` kept,
//!   empty before the first.
//! - `expect bracket <pos>` compares where the bracket that matches
//!   the one at the primary head is, or `expect bracket none` checks
//!   there is none. Strings and comments, as `rubrix_highlight` lexes
//!   the text in the script's language, are skipped.
//!
//! A failed check prints the value it got in the same form, escaped,
//! so it can be pasted back into the script.
//!
//! # Model
//!
//! `--model` runs the script against a naive model instead of the
//! editor: a `String` and a sorted list of caret offsets, one caret
//! at offset 0 to start. It runs the headers, `caret`, `add-caret`
//! and `type`, which inserts text with no line break at every caret,
//! last caret first, and so leaves what typing it a char at a time
//! does. At each `expect hash` it prints `model: <script line> hash
//! <hex>`. It skips the other checks, and any other step is an error.
//! A script records the hash the model prints, so its `expect hash`
//! comes from a model of the steps and not from the code under test.

use std::process::ExitCode;
use std::{env, fs};

use rubrix_edit::{
    Buffer, Command, Editor, Layout, Monospace, Motion, Selection,
    Selections, bracket_skips, column, matching_bracket,
    toggle_caret,
};
use rubrix_highlight::{Lang, Lines};

/// A line and a byte column, both counted from 0.
type Pos = (usize, usize);

struct Script {
    repeat: Option<usize>,
    crlf: bool,
    lang: Lang,
    /// Each step with its line in the script.
    steps: Vec<(usize, Step)>,
}

enum Step {
    Caret(Pos),
    AddCaret(Pos),
    Select(Pos, Pos),
    Type(String),
    Paste(String),
    Key(Command<'static>, usize),
    Expect(Check, String),
    Column(Pos, Pos),
    ToggleCaret(Pos),
    Copy,
}

#[derive(Clone, Copy)]
enum Check {
    Text,
    Line(usize),
    Carets,
    UndoDepth,
    Hash,
    Copied,
    Bracket,
}

impl Check {
    fn name(self) -> &'static str {
        match self {
            Check::Text => "text",
            Check::Line(_) => "line",
            Check::Carets => "carets",
            Check::UndoDepth => "undo-depth",
            Check::Hash => "hash",
            Check::Copied => "copied",
            Check::Bracket => "bracket",
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let (model, file, script) = match args.as_slice() {
        [flag, file, script] if flag == "--model" => {
            (true, file, script)
        }
        [file, script] => (false, file, script),
        _ => {
            eprintln!("usage: replay [--model] <file> <script>");
            return ExitCode::from(2);
        }
    };
    match run(model, file, script) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(problem) => {
            eprintln!("replay: {problem}");
            ExitCode::from(2)
        }
    }
}

/// Whether every check passed.
fn run(
    model: bool,
    file: &str,
    script: &str,
) -> Result<bool, String> {
    let read = |path: &str| {
        fs::read_to_string(path)
            .map_err(|err| format!("cannot read {path}: {err}"))
    };
    let mut text = read(file)?;
    let lang = Lang::detect(file, text.lines().next().unwrap_or(""));
    let script = parse(&read(script)?, lang)?;
    if let Some(lines) = script.repeat {
        let newlines = text.matches('\n').count().max(1);
        text = text.repeat(lines.div_ceil(newlines));
    }
    if script.crlf {
        text = text.replace('\n', "\r\n");
    }
    if model {
        run_model(text, &script.steps)?;
        Ok(true)
    } else {
        replay(&text, &script.steps, script.lang)
    }
}

fn parse(source: &str, lang: Lang) -> Result<Script, String> {
    let mut script = Script {
        repeat: None,
        crlf: false,
        lang,
        steps: Vec::new(),
    };
    for (i, line) in source.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |reason: String| format!("line {}: {reason}", i + 1);
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "repeat" | "crlf" | "lang"
                if !script.steps.is_empty() =>
            {
                return Err(at(format!(
                    "{word} after the first step"
                )));
            }
            "repeat" => {
                script.repeat = Some(rest.parse().map_err(|_| {
                    at(format!("{rest} is no count"))
                })?);
            }
            "crlf" => script.crlf = true,
            "lang" => {
                script.lang =
                    Lang::from_name(rest).ok_or_else(|| {
                        at(format!("{rest} is no language"))
                    })?;
            }
            _ => script.steps.push((
                i + 1,
                step(word, rest, script.lang).map_err(at)?,
            )),
        }
    }
    Ok(script)
}

fn step(word: &str, rest: &str, lang: Lang) -> Result<Step, String> {
    Ok(match word {
        "caret" => Step::Caret(pos(rest)?),
        "add-caret" => Step::AddCaret(pos(rest)?),
        "select" => {
            let (anchor, head) = rest
                .split_once(' ')
                .ok_or("select takes two positions")?;
            Step::Select(pos(anchor)?, pos(head)?)
        }
        "type" => Step::Type(unescape(rest)?),
        "paste" => Step::Paste(unescape(rest)?),
        "key" => {
            let (name, times) = match rest.split_once(' ') {
                Some((name, times)) => (
                    name,
                    times.parse().map_err(|_| {
                        format!("{times} is no count")
                    })?,
                ),
                None => (rest, 1),
            };
            Step::Key(key(name, lang)?, times)
        }
        "expect" => {
            let (kind, value) =
                rest.split_once(' ').unwrap_or((rest, ""));
            let (check, value) = match kind {
                "text" => (Check::Text, value),
                "line" => {
                    let (line, text) =
                        value.split_once(' ').unwrap_or((value, ""));
                    (Check::Line(from_one(line)?), text)
                }
                "carets" => (Check::Carets, value),
                "undo-depth" => (Check::UndoDepth, value),
                "hash" => (Check::Hash, value),
                "copied" => (Check::Copied, value),
                "bracket" => (Check::Bracket, value),
                _ => return Err(format!("no check {kind}")),
            };
            Step::Expect(check, unescape(value)?)
        }
        "column" => {
            let (from, to) = rest
                .split_once(' ')
                .ok_or("column takes two positions")?;
            Step::Column(pos(from)?, pos(to)?)
        }
        "toggle-caret" => Step::ToggleCaret(pos(rest)?),
        "copy" => Step::Copy,
        _ => return Err(format!("no step {word}")),
    })
}

fn key(name: &str, lang: Lang) -> Result<Command<'static>, String> {
    let (extend, base) = match name.strip_prefix("shift+") {
        Some(base) => (true, base),
        None => (false, name),
    };
    let motion = match base {
        "left" => Motion::Left,
        "right" => Motion::Right,
        "word-left" => Motion::WordLeft,
        "word-right" => Motion::WordRight,
        "home" => Motion::Home,
        "end" => Motion::End,
        "up" => Motion::Up,
        "down" => Motion::Down,
        "page-up" => Motion::PageUp,
        "page-down" => Motion::PageDown,
        "doc-start" => Motion::DocStart,
        "doc-end" => Motion::DocEnd,
        _ if extend => return Err(format!("no key {name}")),
        "backspace" => return Ok(Command::Backspace),
        "delete" => return Ok(Command::Delete),
        "delete-word-back" => return Ok(Command::DeleteWordBack),
        "delete-word-forward" => {
            return Ok(Command::DeleteWordForward);
        }
        "enter" => return Ok(Command::Enter),
        "cut" => return Ok(Command::Cut),
        "select-all" => return Ok(Command::SelectAll),
        "collapse" => return Ok(Command::CollapseToPrimary),
        "undo" => return Ok(Command::Undo),
        "redo" => return Ok(Command::Redo),
        _ => return line_key(name, lang),
    };
    Ok(Command::Move { motion, extend })
}

fn pos(text: &str) -> Result<Pos, String> {
    let (line, col) = text
        .split_once(':')
        .ok_or_else(|| format!("{text} is not <line>:<col>"))?;
    Ok((from_one(line)?, from_one(col)?))
}

/// A number counted from 1, as counted from 0.
fn from_one(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n - 1),
        _ => Err(format!("{text} is not a number from 1")),
    }
}

fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next() {
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('s') => ' ',
            Some('\\') => '\\',
            _ => return Err(format!("a bad escape in {text}")),
        });
    }
    Ok(out)
}

/// The inverse of `unescape`, writing a space as `\s` only at the
/// ends, where it would not show.
fn escape(text: &str) -> String {
    let lead = text.len() - text.trim_start_matches(' ').len();
    let trail = text.trim_end_matches(' ').len();
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.char_indices() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' ' if i < lead || i >= trail => out.push_str("\\s"),
            c => out.push(c),
        }
    }
    out
}

/// The 64-bit FNV-1a hash of the text, as 16 hex digits.
fn hash(text: &str) -> String {
    let hash =
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{hash:016x}")
}

/// The offset of `pos` in its line, found as its start and content.
fn offset(
    line: Option<(usize, &str)>,
    (row, col): Pos,
) -> Result<usize, String> {
    let (start, content) =
        line.ok_or_else(|| format!("no line {}", row + 1))?;
    if !content.is_char_boundary(col) {
        return Err(format!(
            "{}:{} is past the line's end or inside a char",
            row + 1,
            col + 1
        ));
    }
    Ok(start + col)
}

/// The start and content of a line of the buffer.
fn buffer_line(
    buffer: &Buffer,
    line: usize,
) -> Option<(usize, &str)> {
    let line = u32::try_from(line)
        .ok()
        .filter(|&line| line < buffer.line_count())?;
    let start = buffer.line_start(line);
    Some((start, &buffer.line(line)[..buffer.line_end(line) - start]))
}

/// The start and content of a line of the model's text, which ends
/// before a `\r` only when a `\n` follows, as in [`Buffer`].
fn model_line(text: &str, line: usize) -> Option<(usize, &str)> {
    let mut lines = text.split('\n');
    let mut start = 0;
    for _ in 0..line {
        start += lines.next()?.len() + 1;
    }
    let raw = lines.next()?;
    let content = match raw.strip_suffix('\r') {
        Some(content) if start + raw.len() < text.len() => content,
        _ => raw,
    };
    Some((start, content))
}

/// Whether every check passed.
fn replay(
    text: &str,
    steps: &[(usize, Step)],
    lang: Lang,
) -> Result<bool, String> {
    let layout = Monospace::default();
    let mut editor = Editor::new(Buffer::new(text));
    let mut copied = String::new();
    let (mut passed, mut total) = (0, 0);
    for (n, step) in steps {
        let at = |pos: Pos, editor: &Editor| {
            offset(buffer_line(editor.buffer(), pos.0), pos)
                .map_err(|reason| format!("line {n}: {reason}"))
        };
        match step {
            Step::Caret(pos) => {
                let caret = Selection::caret(at(*pos, &editor)?);
                editor.set_selections(Selections::new(caret));
            }
            Step::AddCaret(pos) => {
                let mut selections = editor.selections().clone();
                selections.add(Selection::caret(at(*pos, &editor)?));
                editor.set_selections(selections);
            }
            Step::Select(anchor, head) => {
                let selection = Selection {
                    anchor: at(*anchor, &editor)?,
                    head: at(*head, &editor)?,
                };
                editor.set_selections(Selections::new(selection));
            }
            Step::Type(text) => {
                for (i, c) in text.char_indices() {
                    let key =
                        Command::Type(&text[i..i + c.len_utf8()]);
                    let _ = editor.apply(key, &layout);
                }
            }
            Step::Paste(text) => {
                let _ = editor.apply(Command::Paste(text), &layout);
            }
            Step::Key(command, times) => {
                for _ in 0..*times {
                    let _ = editor.apply(*command, &layout);
                }
            }
            Step::Expect(check, want) => {
                total += 1;
                let kind = check.name();
                match observe(&editor, *check, lang, &copied) {
                    Some(got) if got == *want => {
                        passed += 1;
                        println!("replay: {n} {kind} pass");
                    }
                    got => println!(
                        "replay: {n} {kind} fail: got {}",
                        got.map_or(
                            "no such line".to_owned(),
                            |got| { escape(&got) }
                        )
                    ),
                }
            }
            Step::Column(from, to) => {
                let point = |pos: Pos| {
                    let at = at(pos, &editor)?;
                    let buffer = editor.buffer();
                    Ok::<_, String>((
                        buffer.line_of(at),
                        layout.x(buffer, at),
                    ))
                };
                let selections = column(
                    editor.buffer(),
                    &layout,
                    point(*from)?,
                    point(*to)?,
                );
                editor.set_selections(selections);
            }
            Step::ToggleCaret(pos) => {
                let at = at(*pos, &editor)?;
                editor.set_selections(toggle_caret(
                    editor.selections(),
                    at,
                ));
            }
            Step::Copy => copied = editor.copy_text(),
        }
    }
    println!("replay: {passed} of {total} checks pass");
    Ok(passed == total)
}

/// What `check` reads from the editor, or `None` for a line past the
/// end.
fn observe(
    editor: &Editor,
    check: Check,
    lang: Lang,
    copied: &str,
) -> Option<String> {
    let buffer = editor.buffer();
    Some(match check {
        Check::Text => buffer.to_string(),
        Check::Line(line) => buffer_line(buffer, line)?.1.to_owned(),
        Check::Carets => carets(editor),
        Check::UndoDepth => editor.history().undo_depth().to_string(),
        Check::Hash => hash(&buffer.to_string()),
        Check::Copied => copied.to_owned(),
        Check::Bracket => bracket(editor, lang),
    })
}

fn carets(editor: &Editor) -> String {
    let buffer = editor.buffer();
    let selections = editor.selections();
    let mut out = format!(
        "{} primary {}",
        selections.len(),
        selections.primary_index() + 1
    );
    for selection in selections.iter() {
        out.push(' ');
        out.push_str(&show(buffer, selection.anchor));
        if !selection.is_caret() {
            out.push_str("..");
            out.push_str(&show(buffer, selection.head));
        }
    }
    out
}

fn run_model(
    mut text: String,
    steps: &[(usize, Step)],
) -> Result<(), String> {
    let mut carets = vec![0];
    for (n, step) in steps {
        let at = |reason: String| format!("line {n}: {reason}");
        match step {
            Step::Caret(pos) | Step::AddCaret(pos) => {
                let caret = offset(model_line(&text, pos.0), *pos)
                    .map_err(at)?;
                if matches!(step, Step::Caret(_)) {
                    carets.clear();
                }
                if let Err(i) = carets.binary_search(&caret) {
                    carets.insert(i, caret);
                }
            }
            Step::Type(typed) if !typed.contains(['\r', '\n']) => {
                for &caret in carets.iter().rev() {
                    text.insert_str(caret, typed);
                }
                for (i, caret) in carets.iter_mut().enumerate() {
                    *caret += (i + 1) * typed.len();
                }
            }
            Step::Expect(Check::Hash, _) => {
                println!("model: {n} hash {}", hash(&text));
            }
            Step::Expect(..) => {}
            _ => {
                return Err(at("--model runs only caret, \
                               add-caret and type with no line \
                               break"
                    .to_owned()));
            }
        }
    }
    Ok(())
}

/// The keys of the line, comment and occurrence commands. `comment`
/// and `block-comment` toggle comments in `lang`.
fn line_key(
    name: &str,
    lang: Lang,
) -> Result<Command<'static>, String> {
    Ok(match name {
        "indent" => Command::Indent,
        "outdent" => Command::Outdent,
        "indent-lines" => Command::IndentLines,
        "move-up" => Command::MoveLines { down: false },
        "move-down" => Command::MoveLines { down: true },
        "copy-up" => Command::CopyLines { down: false },
        "copy-down" => Command::CopyLines { down: true },
        "blank-below" => Command::BlankLineBelow,
        "transpose" => Command::Transpose,
        "kill-line" => Command::DeleteToLineEnd,
        "comment" => Command::ToggleComment { lang, block: false },
        "block-comment" => {
            Command::ToggleComment { lang, block: true }
        }
        "next-occurrence" => Command::AddNextOccurrence,
        _ => return Err(format!("no key {name}")),
    })
}

/// `at` as a `<pos>`.
fn show(buffer: &Buffer, at: usize) -> String {
    let line = buffer.line_of(at);
    format!("{}:{}", line + 1, at - buffer.line_start(line) + 1)
}

/// Where the bracket that matches the one at the primary head is, or
/// `none`.
fn bracket(editor: &Editor, lang: Lang) -> String {
    let buffer = editor.buffer();
    let highlights =
        rubrix_highlight::highlight(lang, &buffer.to_string());
    let head = editor.selections().primary().head;
    matching_bracket(buffer, head, |line| {
        bracket_skips(highlights.stored_runs(line))
    })
    .map_or_else(
        || "none".to_owned(),
        |found| show(buffer, found.far),
    )
}
