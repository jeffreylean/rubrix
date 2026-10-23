//! Streaming throughput of `Stream` on every fixture under
//! `bench/fixtures`, in Pierre's streaming columns, next to the
//! one-shot `highlight` time of the same text.
//!
//! Each run feeds the fixture to a new `Stream` in the number of
//! chunks that Pierre's streaming table gives its TypeScript tier
//! of the same name: 1 for `tiny`, 8 for `small`, and 130 for
//! `large`. A chunk ends at the next char boundary at or after its
//! share of the bytes, so the printed count can differ from that
//! number for text with multi-byte chars. `ratio` is the streaming
//! time over the one-shot time.

use std::process::ExitCode;

use rubrix_bench::{
    Cell, Fixture, Table, fixtures, measure, mib_per_sec,
};
use rubrix_highlight::{Highlights, Lang, Stream, highlight};

fn pierre_chunks(fixture: &Fixture) -> usize {
    let name = fixture
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match name.split('.').next() {
        Some("tiny") => 1,
        Some("small") => 8,
        Some("large") => 130,
        _ => panic!(
            "{} is not a tiny, small, or large tier",
            fixture.name
        ),
    }
}

fn chunks(text: &str, count: usize) -> impl Iterator<Item = &str> {
    let size = text.len().div_ceil(count);
    let mut start = 0;
    std::iter::from_fn(move || {
        if start == text.len() {
            return None;
        }
        let end =
            text.ceil_char_boundary((start + size).min(text.len()));
        let chunk = &text[start..end];
        start = end;
        Some(chunk)
    })
}

fn stream(lang: Lang, text: &str, count: usize) -> Highlights {
    let mut stream = Stream::new(lang);
    for chunk in chunks(text, count) {
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
        let count = pierre_chunks(fixture);
        let streamed = stream(lang, &fixture.text, count);
        if let Err(err) = streamed.check(&fixture.text) {
            panic!("{}: {err}", fixture.name);
        }
        assert!(
            streamed == highlight(lang, &fixture.text),
            "{}: streaming differs from one-shot",
            fixture.name
        );
        let timing = measure(|| stream(lang, &fixture.text, count));
        let one_shot = measure(|| highlight(lang, &fixture.text));
        table.push(vec![
            Cell::Text(fixture.name.clone()),
            Cell::Count(fixture.lines()),
            Cell::Count(chunks(&fixture.text, count).count()),
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
