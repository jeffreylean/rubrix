//! Streaming throughput of `Stream` on every fixture under
//! `bench/fixtures`, in Pierre's streaming columns, next to the
//! one-shot `highlight` time of the same text.
//!
//! Each run feeds the fixture to a new `Stream` in chunks of 4,096
//! bytes, each extended to the next char boundary. Pierre's chunks
//! hold 4,096 UTF-16 code units, which are the same bytes for ASCII
//! text, so the chunk counts match its table: 1 for a tiny fixture
//! of 2 KiB, about 8 for a small one, and about 130 for a large one.
//! `ratio` is the streaming time over the one-shot time.

use std::process::ExitCode;

use rubrix_bench::{Cell, Table, fixtures, measure, mib_per_sec};
use rubrix_highlight::{Highlights, Lang, Stream, highlight};

const CHUNK_BYTES: usize = 4096;

fn chunks(text: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start == text.len() {
            return None;
        }
        let end = text.ceil_char_boundary(
            (start + CHUNK_BYTES).min(text.len()),
        );
        let chunk = &text[start..end];
        start = end;
        Some(chunk)
    })
}

fn stream(lang: Lang, text: &str) -> Highlights {
    let mut stream = Stream::new(lang);
    for chunk in chunks(text) {
        stream.push(chunk);
    }
    stream.finish()
}

fn main() -> ExitCode {
    if let Some(arg) = std::env::args().nth(1) {
        eprintln!("unknown argument {arg}\nusage: stream");
        return ExitCode::FAILURE;
    }
    let cases = match fixtures() {
        Ok(cases) => cases,
        Err(err) => {
            eprintln!("cannot read bench/fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut table = Table::new(&[
        "file",
        "lines",
        "chunks",
        "us/call",
        "MiB/s",
        "one-shot us/call",
        "ratio",
    ]);
    for fixture in &cases {
        let lang = Lang::detect(
            &fixture.path.to_string_lossy(),
            fixture.text.lines().next().unwrap_or_default(),
        );
        let streamed = stream(lang, &fixture.text);
        if let Err(err) = streamed.check(&fixture.text) {
            panic!("{}: {err}", fixture.name);
        }
        assert!(
            streamed == highlight(lang, &fixture.text),
            "{}: streaming differs from one-shot",
            fixture.name
        );
        let timing = measure(|| stream(lang, &fixture.text));
        let one_shot = measure(|| highlight(lang, &fixture.text));
        table.push(vec![
            Cell::Text(fixture.name.clone()),
            Cell::Count(fixture.lines()),
            Cell::Count(chunks(&fixture.text).count()),
            Cell::Number(timing.median * 1e6),
            Cell::Number(mib_per_sec(
                fixture.text.len(),
                timing.median,
            )),
            Cell::Number(one_shot.median * 1e6),
            Cell::Number(timing.median / one_shot.median),
        ]);
    }
    table.print();
    ExitCode::SUCCESS
}
