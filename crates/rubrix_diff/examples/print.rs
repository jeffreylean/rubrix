//! Prints the line diff of two files as numbered rows, one text line
//! per row. Both files are read as bytes and decoded with
//! `String::from_utf8_lossy`, so bytes that are not UTF-8 print as
//! U+FFFD. The diff keeps 3 lines of context.
//!
//! ```text
//! print [--split] [--no-words] [--expand <hunk>:<up|down|all>]... <old> <new>
//! print [--split] [--no-words] --patch <file>
//! ```
//!
//! Each diff goes through `realign` before its rows are derived, so
//! the most similar removed and added lines pair.
//!
//! `--split` prints the split layout instead of the stacked one.
//! `--expand` reveals `Expansion::STEP` lines of the gap above the
//! 0-based hunk, from the `up` or `down` end, or the whole gap with
//! `all`. It may repeat. The hunk count names the gap after the last
//! hunk. A larger number, or any `--expand` on identical files, is
//! a usage error.
//!
//! Output has no colour, and none for identical files. The changed
//! words of a paired removed or added line, from `Row::words`, are
//! underlined with the escapes `ESC[4m` and `ESC[24m`; `--no-words`
//! leaves them out, and the output is otherwise the same. Each hunk
//! starts with its `@@ -a,b +c,d @@` header on its own line. With
//! `w` as the larger of 4 and the digit count of the longer side's
//! line count, a line number is right-aligned to `w`, and a missing
//! one is `w` spaces. Line text has no terminator.
//!
//! Stacked rows:
//!
//! ```text
//! {old:>w} {new:>w} {sign} {text}
//! ```
//!
//! The sign is ` ` for an unchanged or revealed line, `-` for a
//! deletion and `+` for an addition.
//!
//! Split rows:
//!
//! ```text
//! {old:>w} {osign} {left} | {new:>w} {nsign} {right}
//! ```
//!
//! `left` is padded to the width in chars of the widest left text,
//! at most 60, and longer texts stay whole. The signs are ` ` and
//! ` ` for an unchanged line, `-` and a blank right cell for a
//! deletion, a blank left cell and `+` for an addition, and `-` and
//! `+` for a paired row. A blank cell is spaces for the number, a
//! space for the sign and empty text.
//!
//! A gap row, in both layouts:
//!
//! ```text
//! {blank:>w} {blank:>w}   ... {hidden} unchanged lines
//! ```
//!
//! A changed line whose split partner differs from it only in a `\r`
//! before the `\n` shows its line ending escaped after its text, as
//! in `{text}\r\n` beside `{text}\n`, in both layouts.
//!
//! A row showing a line that does not end in `\n` is followed by
//! `\ No newline at end of file`. In split rows that marker is a row
//! of blank cells whose text is the marker on each side whose line
//! lacks the `\n`.
//!
//! `--patch <file>` prints a patch instead: the file is read as
//! bytes and decoded as above, then parsed with `parse_patch`. Each
//! file in it starts a block of
//!
//! ```text
//! {name}  {kind}  +{added} -{removed}
//! ```
//!
//! where `name` is `{old} -> {new}` when the two names differ, and
//! `kind` is `modified`, `renamed`, `renamed and modified`, `added`,
//! `deleted`, `copied` or `binary`. A line `  mode {old} -> {new}`
//! in octal follows when the patch names a mode, with one side only
//! for an added or deleted file. The file's hunks follow in the
//! rows above, with line numbers taken from the hunk headers and
//! gap rows for the lines between hunks. Blocks are separated by a
//! blank line. `--expand` and the `<old> <new>` files do not go with
//! `--patch`.
//!
//! Exit status is 0 on success, whether or not the files differ, 1
//! with a message on stderr naming the line of a patch that does not
//! parse, and 2 with a message on stderr for a usage error or an
//! unreadable file.

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::ops::Range;
use std::process::ExitCode;

use rubrix_diff::{
    Expansion, FileChange, FileDiff, Layout, LineRef, Lines, Row,
    Rows, Side, diff_texts, parse_patch, realign, rows,
};

const CONTEXT: u32 = 3;
const MIN_NUMBER_WIDTH: usize = 4;
const MAX_LEFT_WIDTH: usize = 60;
const NO_NEWLINE: &str = "\\ No newline at end of file";
const UNDERLINE: &str = "\x1b[4m";
const NO_UNDERLINE: &str = "\x1b[24m";
const USAGE: &str = "usage: print [--split] [--no-words] [--expand \
                     <hunk>:<up|down|all>]... <old> <new>\n       \
                     print [--split] [--no-words] --patch <file>";

struct Args {
    layout: Layout,
    words: bool,
    expands: Vec<(u32, Side)>,
    old: String,
    new: String,
}

fn parse(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, String> {
    let mut layout = Layout::Stacked;
    let mut words = true;
    let mut expands = Vec::new();
    let mut paths = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--split" => layout = Layout::Split,
            "--no-words" => words = false,
            "--expand" => {
                let spec =
                    args.next().ok_or("--expand needs a value")?;
                expands.push(parse_expand(&spec)?);
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown flag {flag}"));
            }
            _ => paths.push(arg),
        }
    }
    let [old, new] = <[String; 2]>::try_from(paths)
        .map_err(|_| "expected an old and a new file".to_owned())?;
    Ok(Args {
        layout,
        words,
        expands,
        old,
        new,
    })
}

fn parse_expand(spec: &str) -> Result<(u32, Side), String> {
    let (hunk, side) = spec.split_once(':').ok_or_else(|| {
        format!("--expand {spec} is not <hunk>:<up|down|all>")
    })?;
    let hunk = hunk.parse().map_err(|_| {
        format!("--expand {spec} has no hunk number before the colon")
    })?;
    let side = match side {
        "up" => Side::Up,
        "down" => Side::Down,
        "all" => Side::All,
        _ => {
            return Err(format!(
                "--expand {spec} side must be up, down or all"
            ));
        }
    };
    Ok((hunk, side))
}

fn check_expands(
    expands: &[(u32, Side)],
    diff: &FileDiff,
) -> Result<(), String> {
    let hunks = diff.hunks().len();
    let Some(&(hunk, _)) = expands
        .iter()
        .find(|&&(hunk, _)| hunks == 0 || hunk as usize > hunks)
    else {
        return Ok(());
    };
    Err(if hunks == 0 {
        format!(
            "--expand names hunk {hunk}, but the files are identical"
        )
    } else {
        format!(
            "--expand names hunk {hunk}, but the diff has {hunks} \
             hunks, so it must be 0 to {hunks}"
        )
    })
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|err| format!("cannot read {path}: {err}"))
}

fn main() -> ExitCode {
    let usage_failure = ExitCode::from(2);
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|arg| arg == "--patch") {
        return print_patch(&argv);
    }
    let args = match parse(argv.into_iter()) {
        Ok(args) => args,
        Err(problem) => {
            eprintln!("{problem}\n{USAGE}");
            return usage_failure;
        }
    };
    let (old, new) = match (read(&args.old), read(&args.new)) {
        (Ok(old), Ok(new)) => (old, new),
        (Err(problem), _) | (_, Err(problem)) => {
            eprintln!("{problem}");
            return usage_failure;
        }
    };
    let mut diff = diff_texts(&old, &new, CONTEXT);
    realign(&mut diff);
    if let Err(problem) = check_expands(&args.expands, &diff) {
        eprintln!("{problem}\n{USAGE}");
        return usage_failure;
    }
    let mut expansion = Expansion::new();
    for &(hunk, side) in &args.expands {
        expansion.reveal(hunk, side, Expansion::STEP);
    }
    let rows = rows(&diff, args.layout, &expansion);

    let mut out = BufWriter::new(io::stdout().lock());
    exit_code(
        write_rows(&mut out, &diff, &rows, args.words)
            .and_then(|()| out.flush()),
    )
}

fn exit_code(written: io::Result<()>) -> ExitCode {
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("cannot write: {err}");
            ExitCode::FAILURE
        }
    }
}

struct PatchArgs<'a> {
    layout: Layout,
    words: bool,
    path: &'a str,
}

fn patch_args(args: &[String]) -> Result<PatchArgs<'_>, String> {
    let mut layout = Layout::Stacked;
    let mut words = true;
    let mut path = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--split" => layout = Layout::Split,
            "--no-words" => words = false,
            "--patch" => {
                path = Some(
                    args.next()
                        .ok_or("--patch needs a file")?
                        .as_str(),
                );
            }
            other => {
                return Err(format!(
                    "{other} does not go with --patch"
                ));
            }
        }
    }
    let path = path.ok_or("--patch needs a file")?;
    Ok(PatchArgs {
        layout,
        words,
        path,
    })
}

fn print_patch(args: &[String]) -> ExitCode {
    let usage_failure = ExitCode::from(2);
    let args = match patch_args(args) {
        Ok(args) => args,
        Err(problem) => {
            eprintln!("{problem}\n{USAGE}");
            return usage_failure;
        }
    };
    let path = args.path;
    let text = match read(path) {
        Ok(text) => text,
        Err(problem) => {
            eprintln!("{problem}");
            return usage_failure;
        }
    };
    let mut files = match parse_patch(&text) {
        Ok(files) => files,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::FAILURE;
        }
    };
    files.iter_mut().for_each(realign);
    let mut out = BufWriter::new(io::stdout().lock());
    exit_code(
        write_patch(&mut out, &files, args.layout, args.words)
            .and_then(|()| out.flush()),
    )
}

fn write_patch(
    out: &mut impl Write,
    files: &[FileDiff],
    layout: Layout,
    words: bool,
) -> io::Result<()> {
    for (at, file) in files.iter().enumerate() {
        if at > 0 {
            writeln!(out)?;
        }
        writeln!(
            out,
            "{}  {}  +{} -{}",
            file_names(file),
            change_name(file.change()),
            file.added(),
            file.removed()
        )?;
        let mode = |mode: Option<u32>| mode.map(|m| format!("{m:o}"));
        match (mode(file.old_mode()), mode(file.new_mode())) {
            (Some(old), Some(new)) => {
                writeln!(out, "  mode {old} -> {new}")?;
            }
            (Some(only), None) | (None, Some(only)) => {
                writeln!(out, "  mode {only}")?;
            }
            (None, None) => {}
        }
        let rows = rows(file, layout, &Expansion::new());
        write_rows(out, file, &rows, words)?;
    }
    Ok(())
}

fn file_names(file: &FileDiff) -> String {
    match (file.old_name(), file.new_name()) {
        (Some(old), Some(new)) if old != new => {
            format!("{old} -> {new}")
        }
        (Some(name), _) | (None, Some(name)) => name.to_owned(),
        (None, None) => "(no name)".to_owned(),
    }
}

fn change_name(change: FileChange) -> &'static str {
    match change {
        FileChange::Modified => "modified",
        FileChange::Renamed => "renamed",
        FileChange::RenamedModified => "renamed and modified",
        FileChange::Added => "added",
        FileChange::Deleted => "deleted",
        FileChange::Copied => "copied",
        FileChange::Binary => "binary",
    }
}

fn write_rows(
    out: &mut impl Write,
    diff: &FileDiff,
    rows: &Rows,
    words: bool,
) -> io::Result<()> {
    let text = Text::new(diff);
    let all: Vec<Row> = rows.range(..).collect();
    let total = if diff.is_partial() {
        let lines = all.iter().flat_map(|row| {
            [row.old_line(), row.new_line()].into_iter().flatten()
        });
        lines.map(|line| line.number.get()).max().unwrap_or(0)
    } else {
        diff.old_lines().len().max(diff.new_lines().len())
    };
    let width = total.to_string().len().max(MIN_NUMBER_WIDTH);
    let left_width = match rows.layout() {
        Layout::Stacked => 0,
        Layout::Split => all
            .iter()
            .flat_map(|row| {
                let shown = row
                    .old_line()
                    .map(|line| text.old_text(line).chars().count());
                let marker = lacks_newline(diff, row)
                    .old
                    .then_some(NO_NEWLINE.len());
                shown.into_iter().chain(marker)
            })
            .max()
            .unwrap_or(0)
            .min(MAX_LEFT_WIDTH),
    };

    let mut next_hunk = 0;
    for (at, row) in all.iter().enumerate() {
        while next_hunk < diff.hunks().len()
            && rows.hunk_rows(next_hunk as u32).start == at as u32
        {
            writeln!(out, "{}", diff.hunks()[next_hunk].header())?;
            next_hunk += 1;
        }
        let marks = words.then(|| row.words(diff)).flatten();
        let (old_marks, new_marks) = marks.unwrap_or_default();
        let marks = Marks {
            old: &old_marks,
            new: &new_marks,
        };
        let line = match (row, rows.layout()) {
            (Row::Gap { hidden, .. }, _) => format!(
                "{:width$} {:width$}   ... {hidden} unchanged lines",
                "", ""
            ),
            (_, Layout::Stacked) => stacked(&text, width, row, marks),
            (_, Layout::Split) => {
                split(&text, width, left_width, row, marks)
            }
        };
        writeln!(out, "{line}")?;
        let lacks = lacks_newline(diff, row);
        match rows.layout() {
            _ if !(lacks.old || lacks.new) => {}
            Layout::Stacked => writeln!(out, "{NO_NEWLINE}")?,
            Layout::Split => writeln!(
                out,
                "{}",
                split_marker(width, left_width, lacks)
            )?,
        }
    }
    Ok(())
}

struct Text<'a> {
    diff: &'a FileDiff,
    old_cr_only: HashSet<u32>,
    new_cr_only: HashSet<u32>,
}

impl<'a> Text<'a> {
    fn new(diff: &'a FileDiff) -> Text<'a> {
        let pairs = rows(diff, Layout::Split, &Expansion::new());
        let (old_cr_only, new_cr_only) = pairs
            .range(..)
            .filter_map(|row| match row {
                Row::Paired { old, new } => {
                    Some((old.index, new.index))
                }
                _ => None,
            })
            .filter(|&(old, new)| {
                differ_only_in_cr(
                    diff.old_lines().raw(old),
                    diff.new_lines().raw(new),
                )
            })
            .unzip();
        Text {
            diff,
            old_cr_only,
            new_cr_only,
        }
    }

    fn old_text(&self, line: LineRef) -> Cow<'a, str> {
        shown(self.diff.old_lines(), &self.old_cr_only, line.index)
    }

    fn new_text(&self, line: LineRef) -> Cow<'a, str> {
        shown(self.diff.new_lines(), &self.new_cr_only, line.index)
    }
}

fn differ_only_in_cr(old: &str, new: &str) -> bool {
    let old = old.strip_suffix('\n').unwrap_or(old);
    let new = new.strip_suffix('\n').unwrap_or(new);
    old != new
        && old.strip_suffix('\r').unwrap_or(old)
            == new.strip_suffix('\r').unwrap_or(new)
}

fn shown<'a>(
    lines: &'a Lines,
    cr_only: &HashSet<u32>,
    index: u32,
) -> Cow<'a, str> {
    let text = lines.line(index);
    if !cr_only.contains(&index) {
        return Cow::Borrowed(text);
    }
    let ending = &lines.raw(index)[text.len()..];
    Cow::Owned(format!("{text}{}", ending.escape_default()))
}

fn signs(row: &Row) -> (char, char) {
    match row {
        Row::Context { .. } | Row::Expanded { .. } => (' ', ' '),
        Row::Deletion { .. } => ('-', ' '),
        Row::Addition { .. } => (' ', '+'),
        Row::Paired { .. } => ('-', '+'),
        Row::Gap { .. } => unreachable!("a gap row has no signs"),
    }
}

fn number(line: Option<LineRef>, width: usize) -> String {
    let shown = line.map(|line| line.number.get().to_string());
    format!("{:>width$}", shown.unwrap_or_default())
}

#[derive(Clone, Copy)]
struct Marks<'a> {
    old: &'a [Range<u32>],
    new: &'a [Range<u32>],
}

fn underlined(text: &str, marks: &[Range<u32>]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for mark in marks {
        let (start, end) = (mark.start as usize, mark.end as usize);
        out.push_str(&text[at..start]);
        out.push_str(UNDERLINE);
        out.push_str(&text[start..end]);
        out.push_str(NO_UNDERLINE);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

fn stacked(
    text: &Text,
    width: usize,
    row: &Row,
    marks: Marks,
) -> String {
    let (old_sign, new_sign) = signs(row);
    let sign = if old_sign == ' ' { new_sign } else { old_sign };
    let shown = match (row.new_line(), row.old_line()) {
        (Some(line), _) => {
            underlined(&text.new_text(line), marks.new)
        }
        (None, Some(line)) => {
            underlined(&text.old_text(line), marks.old)
        }
        (None, None) => String::new(),
    };
    format!(
        "{} {} {sign} {shown}",
        number(row.old_line(), width),
        number(row.new_line(), width),
    )
}

struct Cell<'a> {
    number: Option<LineRef>,
    sign: char,
    text: &'a str,
    marks: &'a [Range<u32>],
}

fn split(
    text: &Text,
    width: usize,
    left_width: usize,
    row: &Row,
    marks: Marks,
) -> String {
    let (old_sign, new_sign) = signs(row);
    let left = row.old_line().map(|line| text.old_text(line));
    let right = row.new_line().map(|line| text.new_text(line));
    split_cells(
        width,
        left_width,
        Cell {
            number: row.old_line(),
            sign: old_sign,
            text: left.as_deref().unwrap_or(""),
            marks: marks.old,
        },
        Cell {
            number: row.new_line(),
            sign: new_sign,
            text: right.as_deref().unwrap_or(""),
            marks: marks.new,
        },
    )
}

fn split_marker(
    width: usize,
    left_width: usize,
    lacks: Lacks,
) -> String {
    let marker = |lacks: bool| Cell {
        number: None,
        sign: ' ',
        text: if lacks { NO_NEWLINE } else { "" },
        marks: &[],
    };
    split_cells(
        width,
        left_width,
        marker(lacks.old),
        marker(lacks.new),
    )
}

fn split_cells(
    width: usize,
    left_width: usize,
    old: Cell,
    new: Cell,
) -> String {
    let pad = left_width.saturating_sub(old.text.chars().count());
    format!(
        "{} {} {}{:pad$} | {} {} {}",
        number(old.number, width),
        old.sign,
        underlined(old.text, old.marks),
        "",
        number(new.number, width),
        new.sign,
        underlined(new.text, new.marks),
    )
}

struct Lacks {
    old: bool,
    new: bool,
}

fn lacks_newline(diff: &FileDiff, row: &Row) -> Lacks {
    let lacks = |lines: &Lines, line: Option<LineRef>| {
        line.is_some_and(|line| {
            !lines.raw(line.index).ends_with('\n')
        })
    };
    Lacks {
        old: lacks(diff.old_lines(), row.old_line()),
        new: lacks(diff.new_lines(), row.new_line()),
    }
}
