//! Prints a file with ANSI colours from a fixed kind-to-colour
//! table. With the colour codes stripped, the output equals the file
//! when it is UTF-8. Bytes that are not UTF-8 print as U+FFFD.
//!
//! ```text
//! ansi [--lang <name>] [--stream <bytes>] [--runs | --check] <file>
//! ```
//!
//! `--lang` overrides `Lang::detect`. `--stream` feeds the file to
//! `Stream` in chunks of that many bytes, each extended to the next
//! char boundary, and prints what the one-shot path prints.
//! `--runs` prints one `line start..end kind text` row per run
//! instead. `--check` checks that the runs cover every line exactly
//! and end on char boundaries, then prints `ok`.

use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::process::ExitCode;

use rubrix_highlight::{
    Highlights, Kind, Lang, Stream, highlight, spans,
};

enum Output {
    Colour,
    Runs,
    Check,
}

struct Args {
    lang: Option<Lang>,
    stream: Option<NonZeroUsize>,
    output: Output,
    path: String,
}

fn parse(
    mut args: impl Iterator<Item = String>,
) -> Result<Args, String> {
    let mut lang = None;
    let mut stream = None;
    let mut output = Output::Colour;
    let mut path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => {
                let name =
                    args.next().ok_or("--lang needs a name")?;
                lang = Some(Lang::from_name(&name).ok_or_else(
                    || {
                        let names: Vec<&str> = Lang::ALL
                            .iter()
                            .map(|lang| lang.name())
                            .collect();
                        format!(
                            "unknown language {name}, expected one \
                             of {}",
                            names.join(", ")
                        )
                    },
                )?);
            }
            "--stream" => {
                stream = Some(
                    args.next()
                        .and_then(|size| size.parse().ok())
                        .ok_or(
                            "--stream needs a byte count above 0",
                        )?,
                );
            }
            "--runs" => output = Output::Runs,
            "--check" => output = Output::Check,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown flag {flag}"));
            }
            _ if path.is_some() => {
                return Err("more than one file".into());
            }
            _ => path = Some(arg),
        }
    }
    let path = path.ok_or("no file given")?;
    Ok(Args {
        lang,
        stream,
        output,
        path,
    })
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(problem) => {
            eprintln!(
                "{problem}\nusage: ansi [--lang <name>] [--stream \
                 <bytes>] [--runs | --check] <file>"
            );
            return ExitCode::FAILURE;
        }
    };
    let text = match std::fs::read(&args.path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(err) => {
            eprintln!("cannot read {}: {err}", args.path);
            return ExitCode::FAILURE;
        }
    };
    let lang = args.lang.unwrap_or_else(|| {
        let first_line = text.lines().next().unwrap_or_default();
        Lang::detect(&args.path, first_line)
    });
    let highlights = match args.stream {
        Some(size) => stream(lang, &text, size),
        None => highlight(lang, &text),
    };
    let mut out = io::stdout().lock();
    let written = match args.output {
        Output::Colour => write_colour(&mut out, &highlights, &text),
        Output::Runs => write!(out, "{}", highlights.display(&text)),
        Output::Check => match highlights.check(&text) {
            Ok(()) => writeln!(out, "ok"),
            Err(err) => {
                eprintln!("{}: {err}", args.path);
                return ExitCode::FAILURE;
            }
        },
    };
    match written.and_then(|()| out.flush()) {
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

fn stream(lang: Lang, text: &str, size: NonZeroUsize) -> Highlights {
    let mut stream = Stream::new(lang);
    let mut start = 0;
    while start < text.len() {
        let end = text.ceil_char_boundary(
            start.saturating_add(size.get()).min(text.len()),
        );
        stream.push(&text[start..end]);
        start = end;
    }
    stream.finish()
}

fn write_colour(
    out: &mut impl Write,
    highlights: &Highlights,
    text: &str,
) -> io::Result<()> {
    for n in 0..highlights.line_count() {
        let range = highlights.line_range(n);
        let line = &text[range.clone()];
        for (piece, kind) in spans(highlights.stored_runs(n)) {
            let piece = &line[piece];
            match colour(kind) {
                Some(code) => {
                    write!(out, "\x1b[{code}m{piece}\x1b[0m")?
                }
                None => out.write_all(piece.as_bytes())?,
            }
        }
        let next = if n + 1 < highlights.line_count() {
            highlights.line_range(n + 1).start
        } else {
            text.len()
        };
        out.write_all(&text.as_bytes()[range.end..next])?;
    }
    Ok(())
}

fn colour(kind: Kind) -> Option<&'static str> {
    let mut next = Some(kind);
    while let Some(kind) = next {
        let code = match kind {
            Kind::Attribute => Some("93"),
            Kind::Boolean | Kind::Constant | Kind::Number => {
                Some("33")
            }
            Kind::Comment => Some("90"),
            Kind::Constructor | Kind::Type => Some("36"),
            Kind::DiffMinus => Some("31"),
            Kind::DiffPlus => Some("32"),
            Kind::Function => Some("34"),
            Kind::FunctionDecorator => Some("93"),
            Kind::FunctionMacro => Some("1;34"),
            Kind::Keyword => Some("35"),
            Kind::Label => Some("31"),
            Kind::Namespace => Some("96"),
            Kind::Property => Some("94"),
            Kind::String => Some("32"),
            Kind::StringEscape => Some("36"),
            Kind::StringRegex => Some("31"),
            Kind::StringSpecial => Some("95"),
            Kind::Title => Some("1;35"),
            Kind::VariableSpecial => Some("91"),
            _ => None,
        };
        if code.is_some() {
            return code;
        }
        next = kind.parent();
    }
    None
}
