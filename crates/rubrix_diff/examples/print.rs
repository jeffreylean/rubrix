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
//! hunk.
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
//! A row showing a line that does not end in `\n` is followed by
//! `\ No newline at end of file`.
//!
//! Exit status is 0 on success, whether or not the files differ, and
//! 2 with a message on stderr for a usage error or an unreadable
//! file.

use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use rubrix_diff::{
    Expansion, FileDiff, Layout, LineRef, Row, Rows, Side,
    diff_texts, rows,
};

const CONTEXT: u32 = 3;
const MIN_NUMBER_WIDTH: usize = 4;
const MAX_LEFT_WIDTH: usize = 60;
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
    let total = diff.old_lines().len().max(diff.new_lines().len());
    let width = total.to_string().len().max(MIN_NUMBER_WIDTH);
    let all: Vec<Row> = rows.range(..).collect();
    let left_width = match rows.layout() {
        Layout::Stacked => 0,
        Layout::Split => all
            .iter()
            .filter_map(Row::old_line)
            .map(|line| {
                diff.old_lines().line(line.index).chars().count()
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
            (_, Layout::Stacked) => stacked(diff, width, row),
            (_, Layout::Split) => split(diff, width, left_width, row),
        };
        writeln!(out, "{line}")?;
        if lacks_newline(diff, row) {
            writeln!(out, "\\ No newline at end of file")?;
        }
    }
    Ok(())
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

fn stacked(diff: &FileDiff, width: usize, row: &Row) -> String {
    let (old_sign, new_sign) = signs(row);
    let sign = if old_sign == ' ' { new_sign } else { old_sign };
    let text = match (row.new_line(), row.old_line()) {
        (Some(line), _) => diff.new_lines().line(line.index),
        (None, Some(line)) => diff.old_lines().line(line.index),
        (None, None) => "",
    };
    format!(
        "{} {} {sign} {text}",
        number(row.old_line(), width),
        number(row.new_line(), width),
    )
}

fn split(
    diff: &FileDiff,
    width: usize,
    left_width: usize,
    row: &Row,
) -> String {
    let (old_sign, new_sign) = signs(row);
    let left = row
        .old_line()
        .map_or("", |line| diff.old_lines().line(line.index));
    let right = row
        .new_line()
        .map_or("", |line| diff.new_lines().line(line.index));
    format!(
        "{} {old_sign} {left:left_width$} | {} {new_sign} {right}",
        number(row.old_line(), width),
        number(row.new_line(), width),
    )
}

fn lacks_newline(diff: &FileDiff, row: &Row) -> bool {
    let old =
        row.old_line().map(|line| diff.old_lines().raw(line.index));
    let new =
        row.new_line().map(|line| diff.new_lines().raw(line.index));
    old.into_iter().chain(new).any(|raw| !raw.ends_with('\n'))
}
