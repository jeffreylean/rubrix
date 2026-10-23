//! One-shot `highlight` throughput on every fixture under
//! `bench/fixtures`, in Pierre's columns.
//!
//! `--stress` adds each large tier repeated to 100,000 lines.
//! `--json <path>` also writes the table as JSON.

use std::path::PathBuf;
use std::process::ExitCode;

use rubrix_bench::{
    Cell, Fixture, Table, fixtures, measure, mib_per_sec,
};
use rubrix_highlight::{Lang, highlight};

const STRESS_LINES: usize = 100_000;

fn main() -> ExitCode {
    let mut stress = false;
    let mut json = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stress" => stress = true,
            "--json" => match args.next() {
                Some(path) => json = Some(PathBuf::from(path)),
                None => return usage("--json needs a path"),
            },
            other => {
                return usage(&format!("unknown argument {other}"));
            }
        }
    }

    let mut cases = match fixtures() {
        Ok(cases) => cases,
        Err(err) => {
            eprintln!("cannot read bench/fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };
    if stress {
        let large: Vec<Fixture> = cases
            .iter()
            .filter(|fixture| fixture.name.contains("/large."))
            .map(|fixture| fixture.repeated(STRESS_LINES))
            .collect();
        cases.extend(large);
    }

    let mut table =
        Table::new(&["file", "lines", "us/call", "MiB/s"]);
    for fixture in &cases {
        let lang = Lang::detect(
            &fixture.path.to_string_lossy(),
            fixture.text.lines().next().unwrap_or_default(),
        );
        assert_eq!(
            lang.name(),
            fixture.lang,
            "{} must resolve to its directory's language",
            fixture.name
        );
        if let Err(err) =
            highlight(lang, &fixture.text).check(&fixture.text)
        {
            panic!("{}: {err}", fixture.name);
        }
        let timing = measure(|| highlight(lang, &fixture.text));
        table.push(vec![
            Cell::Text(fixture.name.clone()),
            Cell::Count(fixture.lines()),
            Cell::Number(timing.median * 1e6),
            Cell::Number(mib_per_sec(
                fixture.text.len(),
                timing.median,
            )),
        ]);
    }
    table.print();
    if let Some(path) = json
        && let Err(err) = table.write_json(&path)
    {
        eprintln!("cannot write {}: {err}", path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn usage(problem: &str) -> ExitCode {
    eprintln!(
        "{problem}\nusage: highlight [--stress] [--json <path>]"
    );
    ExitCode::FAILURE
}
