//! Prints the line diff of two files as numbered rows, one text line
//! per row. Both files are read as bytes and decoded with
//! `String::from_utf8_lossy`, so bytes that are not UTF-8 print as
//! U+FFFD. The diff keeps 3 lines of context.
//!
//! ```text
//! print [--split] [--expand <hunk>:<up|down|all>]... <old> <new>
//! ```
//!
//! `--split` prints the split layout instead of the stacked one.
//! `--expand` reveals `Expansion::STEP` lines of the gap above the
//! 0-based hunk, from the `up` or `down` end, or the whole gap with
//! `all`. It may repeat. The hunk count names the gap after the last
//! hunk. A larger number, or any `--expand` on identical files, is
//! a usage error.
//!
//! Output has no colour, and none for identical files. Each hunk
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
//! Exit status is 0 on success, whether or not the files differ, and
//! 2 with a message on stderr for a usage error or an unreadable
//! file.

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use rubrix_diff::{
    Expansion, FileDiff, Layout, LineRef, Lines, Row, Rows, Side,
    diff_texts, rows,
};

const CONTEXT: u32 = 3;
const MIN_NUMBER_WIDTH: usize = 4;
const MAX_LEFT_WIDTH: usize = 60;
const NO_NEWLINE: &str = "\\ No newline at end of file";
const USAGE: &str = "usage: print [--split] [--expand \
                     <hunk>:<up|down|all>]... <old> <new>";

struct Args {
    layout: Layout,
    expands: Vec<(u32, Side)>,
    old: String,
    new: String,
}

fn parse(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, String> {
    let mut layout = Layout::Stacked;
    let mut expands = Vec::new();
    let mut paths = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--split" => layout = Layout::Split,
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
    let args = match parse(std::env::args().skip(1)) {
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
    let diff = diff_texts(&old, &new, CONTEXT);
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
    match write_rows(&mut out, &diff, &rows)
        .and_then(|()| out.flush())
    {
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

fn write_rows(
    out: &mut impl Write,
    diff: &FileDiff,
    rows: &Rows,
) -> io::Result<()> {
    let text = Text::new(diff);
    let total = diff.old_lines().len().max(diff.new_lines().len());
    let width = total.to_string().len().max(MIN_NUMBER_WIDTH);
    let all: Vec<Row> = rows.range(..).collect();
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
        let line = match (row, rows.layout()) {
            (Row::Gap { hidden, .. }, _) => format!(
                "{:width$} {:width$}   ... {hidden} unchanged lines",
                "", ""
            ),
            (_, Layout::Stacked) => stacked(&text, width, row),
            (_, Layout::Split) => {
                split(&text, width, left_width, row)
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

fn stacked(text: &Text, width: usize, row: &Row) -> String {
    let (old_sign, new_sign) = signs(row);
    let sign = if old_sign == ' ' { new_sign } else { old_sign };
    let shown = match (row.new_line(), row.old_line()) {
        (Some(line), _) => text.new_text(line),
        (None, Some(line)) => text.old_text(line),
        (None, None) => Cow::Borrowed(""),
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
}

fn split(
    text: &Text,
    width: usize,
    left_width: usize,
    row: &Row,
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
        },
        Cell {
            number: row.new_line(),
            sign: new_sign,
            text: right.as_deref().unwrap_or(""),
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
    format!(
        "{} {} {:left_width$} | {} {} {}",
        number(old.number, width),
        old.sign,
        old.text,
        number(new.number, width),
        new.sign,
        new.text,
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
