//! `diff_texts` and `rows` timings on the old/new pairs that
//! `bench/gen-edits.sh` writes under `target/edits`.
//!
//! `--algorithm myers|histogram` picks the algorithm; the default
//! is `Algorithm::default()`. `--json <path>` also writes the table
//! as JSON.

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use rubrix_bench::{Cell, Table, measure};
use rubrix_diff::{
    Algorithm, Expansion, Layout, diff_texts_with, rows,
};

const CONTEXT: u32 = 3;
const CASES: [&str; 10] = [
    "tiny-1", "tiny-10", "tiny-50", "small-1", "small-10",
    "small-50", "large-1", "large-10", "large-50", "stress-1",
];
const ALGORITHMS: [(&str, Algorithm); 2] = [
    ("myers", Algorithm::Myers),
    ("histogram", Algorithm::Histogram),
];

fn main() -> ExitCode {
    let mut algorithm = Algorithm::default();
    let mut json = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--algorithm" => {
                let Some(name) = args.next() else {
                    return usage("--algorithm needs a name");
                };
                match ALGORITHMS.iter().find(|(n, _)| *n == name) {
                    Some(&(_, chosen)) => algorithm = chosen,
                    None => {
                        return usage(&format!(
                            "unknown algorithm {name}"
                        ));
                    }
                }
            }
            "--json" => match args.next() {
                Some(path) => json = Some(PathBuf::from(path)),
                None => return usage("--json needs a path"),
            },
            other => {
                return usage(&format!("unknown argument {other}"));
            }
        }
    }

    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let edits = repo.join("target/edits");
    match Command::new("bash")
        .arg(repo.join("bench/gen-edits.sh"))
        .arg(&edits)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!("bench/gen-edits.sh failed: {status}");
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("cannot run bench/gen-edits.sh: {err}");
            return ExitCode::FAILURE;
        }
    }

    let algorithm_name = ALGORITHMS
        .iter()
        .find(|(_, a)| *a == algorithm)
        .map_or("", |(name, _)| name);
    let mut table = Table::new(&[
        "pair",
        "lines",
        "algorithm",
        "ms",
        "p95 ms",
        "rows",
        "added",
        "removed",
    ]);
    for case in CASES {
        let read = |side: &str| {
            let path = edits.join(case).join(side);
            std::fs::read_to_string(&path).map_err(|err| {
                format!("cannot read {}: {err}", path.display())
            })
        };
        let (old, new) =
            match (read("old.rs.txt"), read("new.rs.txt")) {
                (Ok(old), Ok(new)) => (old, new),
                (Err(problem), _) | (_, Err(problem)) => {
                    eprintln!("{problem}");
                    return ExitCode::FAILURE;
                }
            };
        let expansion = Expansion::new();
        let timing = measure(|| {
            let diff =
                diff_texts_with(&old, &new, CONTEXT, algorithm);
            let rows = rows(&diff, Layout::Stacked, &expansion);
            rows.range(..).map(black_box).count()
        });
        let diff = diff_texts_with(&old, &new, CONTEXT, algorithm);
        let rows = rows(&diff, Layout::Stacked, &expansion);
        table.push(vec![
            Cell::Text(case.to_owned()),
            Cell::Count(diff.old_lines().len() as usize),
            Cell::Text(algorithm_name.to_owned()),
            Cell::Number(timing.median * 1e3),
            Cell::Number(timing.p95 * 1e3),
            Cell::Count(rows.len() as usize),
            Cell::Count(diff.added() as usize),
            Cell::Count(diff.removed() as usize),
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
        "{problem}\nusage: diff [--algorithm myers|histogram] \
         [--json <path>]"
    );
    ExitCode::FAILURE
}
