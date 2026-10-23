//! Live-editing speed of `Highlights::edit`, in the columns of
//! Pierre's live-editing table, on the Rust large tier and on that
//! tier repeated to 100,000 lines.
//!
//! Each scenario applies one edit to highlights that are current,
//! times only the `edit` call, and then reads the last line and
//! undoes the edit outside the timer. `changed lines` is the length
//! of the range `edit` returns for the window of the scenario. The
//! template scenarios open a block comment on line 1, which changes
//! the state of every line below it. The viewport is the first 120
//! lines and the distant viewport the last 120.
//!
//! `--json <path>` also writes the table as JSON.

use std::hint::black_box;
use std::ops::Range;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rubrix_bench::{
    Cell, Fixture, Measurement, Table, fixtures, measure,
};
use rubrix_highlight::{Highlights, Kind, Lang, LineEdit, highlight};

const STRESS_LINES: usize = 100_000;
const VIEWPORT: u32 = 120;
const TEMPLATE: &str = "/*";
const WARMUP: Duration = Duration::from_millis(200);
const WARMUP_ITERATIONS: u32 = 3;
const BUDGET: Duration = Duration::from_millis(1500);
const MIN_SAMPLES: usize = 20;

#[derive(Clone, Copy)]
enum Window {
    All,
    Viewport,
    Distant,
}

impl Window {
    fn range(self, count: u32) -> Range<u32> {
        match self {
            Window::All => 0..u32::MAX,
            Window::Viewport => 0..VIEWPORT,
            Window::Distant => count.saturating_sub(VIEWPORT)..count,
        }
    }
}

struct Change {
    first: u32,
    before: Vec<String>,
    after: Vec<String>,
}

impl Change {
    fn edit(&self) -> LineEdit {
        LineEdit {
            first: self.first,
            removed: self.before.len() as u32,
            inserted: self.after.len() as u32,
        }
    }

    fn undo(&self) -> LineEdit {
        LineEdit {
            first: self.first,
            removed: self.after.len() as u32,
            inserted: self.before.len() as u32,
        }
    }
}

struct Scenario {
    name: &'static str,
    change: Change,
    window: Window,
}

fn toggled(line: &str) -> String {
    let mut line = line.to_owned();
    let last = line.pop();
    line.push(if last == Some('1') { '2' } else { '1' });
    line
}

fn edit_line(lines: &[String], target: usize) -> Change {
    let at = (target..lines.len())
        .find(|&at| lines[at].ends_with(';'))
        .unwrap_or(target);
    Change {
        first: at as u32,
        before: vec![lines[at].clone()],
        after: vec![toggled(&lines[at])],
    }
}

fn scenarios(lines: &[String]) -> Vec<Scenario> {
    let middle = lines.len() / 2;
    let template = |name, window| Scenario {
        name,
        change: Change {
            first: 1,
            before: vec![lines[1].clone()],
            after: vec![format!("{TEMPLATE}{}", lines[1])],
        },
        window,
    };
    vec![
        Scenario {
            name: "edit top",
            change: edit_line(lines, 0),
            window: Window::All,
        },
        Scenario {
            name: "edit middle",
            change: edit_line(lines, middle),
            window: Window::All,
        },
        Scenario {
            name: "edit end",
            change: edit_line(lines, lines.len() - 2),
            window: Window::All,
        },
        Scenario {
            name: "insert line",
            change: Change {
                first: middle as u32,
                before: vec![lines[middle].clone()],
                after: vec![
                    "let inserted = 1;".to_owned(),
                    lines[middle].clone(),
                ],
            },
            window: Window::All,
        },
        Scenario {
            name: "delete line",
            change: Change {
                first: middle as u32,
                before: lines[middle..=middle + 1].to_vec(),
                after: vec![lines[middle + 1].clone()],
            },
            window: Window::All,
        },
        template("template propagation", Window::All),
        template("template + viewport", Window::Viewport),
        template("template + distant viewport", Window::Distant),
    ]
}

struct Case {
    highlights: Highlights,
    lines: Vec<String>,
    scenario: Scenario,
}

impl Case {
    fn stage(&mut self) {
        let Change {
            first,
            before,
            after,
        } = &self.scenario.change;
        let first = *first as usize;
        self.lines.splice(
            first..first + before.len(),
            after.iter().cloned(),
        );
    }

    fn edit(&mut self) -> Range<u32> {
        let window =
            self.scenario.window.range(self.lines.len() as u32);
        self.highlights.edit(
            self.scenario.change.edit(),
            &self.lines,
            window,
        )
    }

    fn flush(&mut self) {
        let last = self.lines.len() as u32 - 1;
        self.highlights.runs(last, &self.lines);
    }

    fn restore(&mut self) {
        self.flush();
        let Change {
            first,
            before,
            after,
        } = &self.scenario.change;
        let first = *first as usize;
        self.lines.splice(
            first..first + after.len(),
            before.iter().cloned(),
        );
        self.highlights.edit(
            self.scenario.change.undo(),
            &self.lines,
            0..u32::MAX,
        );
        self.flush();
    }

    fn last_text_line(&self) -> u32 {
        self.lines
            .iter()
            .rposition(|line| !line.is_empty())
            .unwrap_or(0) as u32
    }
}

fn percentiles(mut samples: Vec<f64>) -> Measurement {
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    let median = if samples.len().is_multiple_of(2) {
        (samples[middle - 1] + samples[middle]) / 2.0
    } else {
        samples[middle]
    };
    let p95_index = (samples.len() as f64 * 0.95).ceil() as usize - 1;
    Measurement {
        median,
        p95: samples[p95_index],
    }
}

fn sample(case: &mut Case) -> (Measurement, usize) {
    let mut changed = 0;
    let begin = Instant::now();
    let mut warmed = 0;
    while begin.elapsed() < WARMUP || warmed < WARMUP_ITERATIONS {
        case.stage();
        changed = black_box(case.edit()).len();
        case.restore();
        warmed += 1;
    }
    let mut times = Vec::new();
    let begin = Instant::now();
    while begin.elapsed() < BUDGET || times.len() < MIN_SAMPLES {
        case.stage();
        let start = Instant::now();
        let range = black_box(case.edit());
        times.push(start.elapsed().as_secs_f64());
        changed = range.len();
        case.restore();
    }
    (percentiles(times), changed)
}

fn dash() -> Cell {
    Cell::Text("-".to_owned())
}

fn run(
    lang: Lang,
    fixture: &Fixture,
    table: &mut Table,
) -> Result<(), String> {
    let name = &fixture.name;
    let lines: Vec<String> =
        fixture.text.split('\n').map(str::to_owned).collect();
    let init = measure(|| highlight(lang, &fixture.text));
    table.push(vec![
        Cell::Text(name.clone()),
        Cell::Text("eager init".to_owned()),
        Cell::Number(init.median * 1e6),
        Cell::Number(init.p95 * 1e6),
        dash(),
        dash(),
    ]);

    let original = highlight(lang, &fixture.text);
    for scenario in scenarios(&lines) {
        let label = scenario.name;
        let mut case = Case {
            highlights: original.clone(),
            lines: lines.clone(),
            scenario,
        };
        case.stage();
        let edited = case.lines.join("\n");
        case.edit();
        case.flush();
        if case.highlights != highlight(lang, &edited) {
            return Err(format!(
                "{name}: {label}: edit differs from highlight"
            ));
        }
        case.restore();
        if case.highlights != original {
            return Err(format!(
                "{name}: {label}: undo differs from the original"
            ));
        }

        let (timing, changed) = sample(&mut case);
        let rebuild = measure(|| highlight(lang, &edited));
        table.push(vec![
            Cell::Text(name.clone()),
            Cell::Text(label.to_owned()),
            Cell::Number(timing.median * 1e6),
            Cell::Number(timing.p95 * 1e6),
            Cell::Number(rebuild.median * 1e6),
            Cell::Count(changed),
        ]);

        if matches!(case.scenario.window, Window::Distant) {
            case.stage();
            case.edit();
            let last = case.last_text_line();
            let all_comment = case
                .highlights
                .runs(last, &case.lines)
                .iter()
                .all(|run| run.kind == Kind::Comment);
            if !all_comment {
                return Err(format!(
                    "{name}: distant read is not comment"
                ));
            }
            println!("distant read ok: {name}");
            case.restore();
        }
    }
    Ok(())
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

    let cases = match fixtures() {
        Ok(cases) => cases,
        Err(err) => {
            eprintln!("cannot read bench/fixtures: {err}");
            return ExitCode::FAILURE;
        }
    };
    let Some(large) = cases
        .iter()
        .find(|fixture| fixture.name == "rust/large.rs.txt")
    else {
        eprintln!("bench/fixtures/rust/large.rs.txt is missing");
        return ExitCode::FAILURE;
    };

    let mut table = Table::new(&[
        "file",
        "scenario",
        "median us",
        "p95 us",
        "rebuild median us",
        "changed lines",
    ]);
    let stress = large.repeated(STRESS_LINES);
    for fixture in [large, &stress] {
        let lang = Lang::detect(
            &fixture.path.to_string_lossy(),
            fixture.text.lines().next().unwrap_or_default(),
        );
        if let Err(problem) = run(lang, fixture, &mut table) {
            eprintln!("{problem}");
            return ExitCode::FAILURE;
        }
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
    eprintln!("{problem}\nusage: live [--json <path>]");
    ExitCode::FAILURE
}
