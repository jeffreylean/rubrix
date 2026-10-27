//! `parse_patch` plus `rows` for every file, on the patches under
//! `bench/patch`.
//!
//! `--json <path>` also writes the table as JSON.

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rubrix_bench::{Cell, Table, measure};
use rubrix_diff::{Expansion, FileDiff, Layout, parse_patch, rows};

const PATCHES: [&str; 2] = ["large.patch.txt", "stress.patch.txt"];

fn main() -> ExitCode {
    let mut json = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => match args.next() {
                Some(path) => json = Some(PathBuf::from(path)),
                None => return usage("--json needs a path"),
            },
            other => {
                return usage(&format!("unknown argument {other}"));
            }
        }
    }

    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/patch");
    let mut table = Table::new(&[
        "patch", "lines", "files", "ms", "p95 ms", "rows", "added",
        "removed",
    ]);
    for name in PATCHES {
        let path = dir.join(name);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("cannot read {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        };
        let expansion = Expansion::new();
        let parse_and_row = || {
            let files = parse_patch(&text).map_err(|err| {
                format!("{}: {err}", path.display())
            })?;
            let rows = files
                .iter()
                .map(|file| {
                    rows(file, Layout::Stacked, &expansion)
                        .range(..)
                        .map(black_box)
                        .count()
                })
                .sum::<usize>();
            Ok::<_, String>((files, rows))
        };
        let (files, row_count) = match parse_and_row() {
            Ok(parsed) => parsed,
            Err(problem) => {
                eprintln!("{problem}");
                return ExitCode::FAILURE;
            }
        };
        let timing = measure(|| black_box(parse_and_row()));
        let sum = |count: fn(&FileDiff) -> u32| {
            files.iter().map(count).sum::<u32>() as usize
        };
        table.push(vec![
            Cell::Text(name.to_owned()),
            Cell::Count(text.lines().count()),
            Cell::Count(files.len()),
            Cell::Number(timing.median * 1e3),
            Cell::Number(timing.p95 * 1e3),
            Cell::Count(row_count),
            Cell::Count(sum(FileDiff::added)),
            Cell::Count(sum(FileDiff::removed)),
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
    eprintln!("{problem}\nusage: patch [--json <path>]");
    ExitCode::FAILURE
}
