//! `realign` plus `Row::words` for every paired line, on
//! `bench/patch/large.patch.txt` and on the large 50% pair that
//! `bench/gen-edits.sh` writes into a scratch directory under the
//! system temp directory, removed before exit.
//!
//! `realign ms` times `realign` alone; `ms` and `p95 ms` time it
//! with the word ranges. `--json <path>` also writes the table as
//! JSON.

use std::hint::black_box;
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use rubrix_bench::{Cell, Table, measure_each};
use rubrix_diff::{
    Expansion, FileDiff, Layout, Row, diff_texts, parse_patch,
    realign, rows,
};

const CONTEXT: u32 = 3;
const PAIR: &str = "large-50";

struct Case {
    name: String,
    lines: usize,
    files: Vec<FileDiff>,
}

#[derive(Clone, Copy, Default)]
struct Counts {
    pairs: usize,
    marked: usize,
}

impl Add for Counts {
    type Output = Counts;

    fn add(self, other: Counts) -> Counts {
        Counts {
            pairs: self.pairs + other.pairs,
            marked: self.marked + other.marked,
        }
    }
}

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
    let cases = match cases() {
        Ok(cases) => cases,
        Err(problem) => {
            eprintln!("{problem}");
            return ExitCode::FAILURE;
        }
    };

    let mut table = Table::new(&[
        "case",
        "lines",
        "pairs",
        "marked",
        "realign ms",
        "ms",
        "p95 ms",
    ]);
    for Case { name, lines, files } in cases {
        let realign_only = time(&files, |files| {
            files.iter_mut().for_each(realign);
        });
        let timing = time(&files, |files| {
            files.iter_mut().for_each(realign);
            files
                .iter()
                .map(marked_pairs)
                .fold(Counts::default(), Counts::add)
        });
        let mut aligned = files.clone();
        aligned.iter_mut().for_each(realign);
        let counts = aligned
            .iter()
            .map(marked_pairs)
            .fold(Counts::default(), Counts::add);
        table.push(vec![
            Cell::Text(name),
            Cell::Count(lines),
            Cell::Count(counts.pairs),
            Cell::Count(counts.marked),
            Cell::Number(realign_only.median * 1e3),
            Cell::Number(timing.median * 1e3),
            Cell::Number(timing.p95 * 1e3),
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

fn cases() -> Result<Vec<Case>, String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let patch_path = repo.join("bench/patch/large.patch.txt");
    let patch = read(&patch_path)?;
    let files = parse_patch(&patch)
        .map_err(|err| format!("{}: {err}", patch_path.display()))?;

    let scratch = std::env::temp_dir()
        .join(format!("rubrix-words-{}", std::process::id()));
    let pair = generate_pair(&repo, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    let (old, new) = pair?;
    Ok(vec![
        Case {
            name: "large.patch.txt".to_owned(),
            lines: patch.lines().count(),
            files,
        },
        Case {
            name: PAIR.to_owned(),
            lines: old.lines().count(),
            files: vec![diff_texts(&old, &new, CONTEXT)],
        },
    ])
}

fn generate_pair(
    repo: &Path,
    scratch: &Path,
) -> Result<(String, String), String> {
    let status = Command::new("bash")
        .arg(repo.join("bench/gen-edits.sh"))
        .arg(scratch)
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|err| {
            format!("cannot run bench/gen-edits.sh: {err}")
        })?;
    if !status.success() {
        return Err(format!("bench/gen-edits.sh failed: {status}"));
    }
    let dir = scratch.join(PAIR);
    Ok((
        read(&dir.join("old.rs.txt"))?,
        read(&dir.join("new.rs.txt"))?,
    ))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|err| {
        format!("cannot read {}: {err}", path.display())
    })
}

fn time<R>(
    files: &[FileDiff],
    mut run: impl FnMut(&mut Vec<FileDiff>) -> R,
) -> rubrix_bench::Measurement {
    let mut state = files.to_vec();
    measure_each(
        &mut state,
        |state| black_box(run(state)),
        |state| {
            state.clone_from_slice(files);
        },
    )
}

fn marked_pairs(file: &FileDiff) -> Counts {
    let table = rows(file, Layout::Split, &Expansion::new());
    table
        .range(..)
        .filter(|row| matches!(row, Row::Paired { .. }))
        .map(|row| Counts {
            pairs: 1,
            marked: usize::from(black_box(row.words(file)).is_some()),
        })
        .fold(Counts::default(), Counts::add)
}

fn usage(problem: &str) -> ExitCode {
    eprintln!("{problem}\nusage: words [--json <path>]");
    ExitCode::FAILURE
}
