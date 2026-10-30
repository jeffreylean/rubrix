//! Per-keystroke cost of `rubrix_edit` with a view's re-highlight, on
//! the Rust large tier and on that tier repeated to 100,000 lines, as
//! `live` builds them.
//!
//! Every row times one key or one operation per sample, after 60
//! warm-up iterations it does not record. A keyed row times what a
//! view does for a keystroke: `Editor::apply`, then
//! `Highlights::edit_many` of the change's line edits with a 60-line
//! window, then `Highlights::runs` of every window line. The window
//! is centred on the primary caret's line, or for the 100-caret rows
//! on the middle line, which holds one of the carets. `type 5000 +
//! hl` types at the start of the middle line, a comment in both
//! inputs, so the lexer reads the typed code and not only a comment.
//! `2000 scattered` times `Buffer::replace` alone, at Pierre's
//! stride, and `20000 round trips` times byte to line and column and
//! back on the text the scattered inserts left. `budget us` is the
//! p95 budget of a row at 100,000 lines, and the run ends with how
//! many p95s met theirs.
//!
//! The rows check their work. After each keyed row every key must
//! have changed the text, and the highlights, settled, must equal
//! `highlight` of the editor's text. The scattered inserts must leave
//! the text a `String` model has, and every round trip must agree
//! with the model and come back to its offset. A mismatch prints the
//! scenario and exits 1.
//!
//! `--json <path>` also writes the table as JSON.

use std::hint::black_box;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rubrix_bench::{Cell, Table, fixtures, percentiles};
use rubrix_edit::{
    Buffer, Command, Editor, Monospace, Selection, Selections,
};
use rubrix_highlight::{Highlights, Lang, highlight};

const STRESS_LINES: usize = 100_000;
const WARMUP: usize = 60;
const KEY_SAMPLES: usize = 100;
const TYPED: &str = "let value = compute(alpha, beta);";
const TYPED_KEYS: usize = 5_000;
const SCATTERED: usize = 2_000;
const ROUND_TRIPS: usize = 20_000;
const CARETS: u32 = 100;
const WINDOW: u32 = 60;
const ENTER_LINE: u32 = 5_000;

struct Row {
    scenario: &'static str,
    /// Seconds per key or operation.
    samples: Vec<f64>,
    /// The p95 budget at 100,000 lines, in microseconds.
    budget: Option<f64>,
}

/// Runs `op` for each iteration and returns the durations after the
/// warm-up, in seconds.
fn timed(
    iterations: usize,
    op: impl FnMut(usize) -> Duration,
) -> Vec<f64> {
    (0..iterations)
        .map(op)
        .skip(WARMUP)
        .map(|took| took.as_secs_f64())
        .collect()
}

/// Where a keyed row centres its window.
#[derive(Clone, Copy)]
enum Around {
    Caret,
    Middle,
}

/// An editor with highlights that follow it, as a view keeps them.
struct Session {
    editor: Editor,
    highlights: Highlights,
    lang: Lang,
    unchanged_keys: usize,
}

impl Session {
    fn place(&mut self, carets: &[usize]) {
        let mut selections =
            Selections::new(Selection::caret(carets[0]));
        for &at in &carets[1..] {
            selections.add(Selection::caret(at));
        }
        self.editor.set_selections(selections);
    }

    fn press(
        &mut self,
        command: Command<'_>,
        around: Around,
    ) -> Duration {
        let start = Instant::now();
        let Some(change) =
            self.editor.apply(command, &Monospace::default())
        else {
            self.unchanged_keys += 1;
            return start.elapsed();
        };
        let buffer = self.editor.buffer();
        let count = buffer.line_count();
        let centre = match around {
            Around::Caret => buffer
                .line_of(self.editor.selections().primary().head),
            Around::Middle => count / 2,
        };
        let first = centre.saturating_sub(WINDOW / 2);
        let window = first..(first + WINDOW).min(count);
        self.highlights.edit_many(
            change.line_edits(),
            buffer,
            window.clone(),
        );
        for line in window {
            black_box(self.highlights.runs(line, buffer));
        }
        start.elapsed()
    }

    fn check(&mut self) -> Result<(), String> {
        if self.unchanged_keys > 0 {
            return Err(format!(
                "{} keys changed no text",
                self.unchanged_keys
            ));
        }
        let buffer = self.editor.buffer();
        self.highlights.runs(buffer.line_count() - 1, buffer);
        if self.highlights
            != highlight(self.lang, &buffer.to_string())
        {
            return Err("highlights differ from highlight".to_owned());
        }
        Ok(())
    }
}

/// Times `iterations` keys on `session`, then checks it.
fn keyed(
    mut session: Session,
    scenario: &'static str,
    budget: Option<f64>,
    iterations: usize,
    mut key: impl FnMut(&mut Session, usize) -> Duration,
) -> Result<Row, String> {
    let samples = timed(iterations, |i| key(&mut session, i));
    session
        .check()
        .map_err(|problem| format!("{scenario}: {problem}"))?;
    Ok(Row {
        scenario,
        samples,
        budget,
    })
}

/// The scattered inserts, and the round trips on the text they leave,
/// both checked against a `String` model.
fn scatter(text: &str) -> Result<[Row; 2], String> {
    let mut buffer = Buffer::new(text);
    let mut model = text.to_owned();
    let mut starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let lines = starts.len();
    let inserts = timed(SCATTERED, |i| {
        let line = i * 7919 % lines;
        let end = starts.get(line + 1).map_or(model.len(), |s| s - 1);
        let at = starts[line]
            + model[starts[line]..end]
                .floor_char_boundary(9 + i % 20);
        model.insert(at, 'x');
        for start in &mut starts[line + 1..] {
            *start += 1;
        }
        let start = Instant::now();
        let change = buffer.replace([(at..at, "x")]);
        let took = start.elapsed();
        black_box(change);
        took
    });
    if buffer.to_string() != model {
        return Err("2000 scattered: the text differs from the \
                    model"
            .to_owned());
    }

    let len = buffer.len();
    let mut agree = 0;
    let trips = timed(ROUND_TRIPS, |i| {
        let at = model.floor_char_boundary(i * 104_729 % len);
        let start = Instant::now();
        let line = buffer.line_of(at);
        let col = at - buffer.line_start(line);
        let back = black_box(buffer.line_start(line) + col);
        let took = start.elapsed();
        let want = starts.partition_point(|&s| s <= at) - 1;
        if back == at
            && line as usize == want
            && col == at - starts[want]
        {
            agree += 1;
        }
        took
    });
    if agree != ROUND_TRIPS {
        return Err(format!(
            "20000 round trips: {agree} of {ROUND_TRIPS} agree with \
             the model"
        ));
    }
    Ok([
        Row {
            scenario: "2000 scattered",
            samples: inserts,
            budget: None,
        },
        Row {
            scenario: "20000 round trips",
            samples: trips,
            budget: None,
        },
    ])
}

fn run(lang: Lang, text: &str) -> Result<Vec<Row>, String> {
    let original = highlight(lang, text);
    let buffer = Buffer::new(text);
    let lines = buffer.line_count();
    let session = |carets: &[usize]| {
        let mut session = Session {
            editor: Editor::new(Buffer::new(text)),
            highlights: original.clone(),
            lang,
            unchanged_keys: 0,
        };
        session.place(carets);
        session
    };
    let carets: Vec<usize> = (0..CARETS)
        .map(|i| buffer.line_start(i * lines / CARETS))
        .collect();
    let keys = WARMUP + KEY_SAMPLES;

    let mut rows = vec![keyed(
        session(&[buffer.line_start(lines / 2)]),
        "type 5000 + hl",
        Some(500.0),
        TYPED_KEYS,
        |session, i| {
            let at = i % TYPED.len();
            session
                .press(Command::Type(&TYPED[at..=at]), Around::Caret)
        },
    )?];
    rows.extend(scatter(text)?);
    rows.push(keyed(
        session(&carets),
        "100 carets",
        Some(1000.0),
        keys,
        |session, _| {
            session.press(Command::Type("x"), Around::Middle)
        },
    )?);
    rows.push(keyed(
        session(&[buffer.line_end(ENTER_LINE)]),
        "enter at 5000",
        Some(500.0),
        keys,
        |session, _| {
            session.place(&[session
                .editor
                .buffer()
                .line_end(ENTER_LINE)]);
            session.press(Command::Enter, Around::Caret)
        },
    )?);
    rows.push(keyed(
        session(&[buffer.line_start(ENTER_LINE + 1)]),
        "join at 5000",
        Some(500.0),
        keys,
        |session, _| {
            session.place(&[session
                .editor
                .buffer()
                .line_start(ENTER_LINE + 1)]);
            session.press(Command::Backspace, Around::Caret)
        },
    )?);
    rows.push(Row {
        scenario: "snapshot",
        samples: timed(keys, |_| {
            let start = Instant::now();
            let snapshot = black_box(buffer.snapshot());
            let took = start.elapsed();
            drop(snapshot);
            took
        }),
        budget: Some(10.0),
    });
    rows.push(keyed(
        session(&carets),
        "100 carets /*",
        None,
        keys,
        |session, i| {
            let typed = if i % 2 == 0 { "/*" } else { "*/" };
            session.press(Command::Type(typed), Around::Middle)
        },
    )?);
    rows.push(keyed(
        session(&carets),
        "100 carets after snapshot",
        None,
        keys,
        |session, _| {
            let snapshot = session.editor.buffer().snapshot();
            let took =
                session.press(Command::Type("x"), Around::Middle);
            drop(snapshot);
            took
        },
    )?);
    Ok(rows)
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
        "samples",
        "budget us",
    ]);
    let (mut met, mut budgeted) = (0, 0);
    let stress = large.repeated(STRESS_LINES);
    for (fixture, budgets) in [(large, false), (&stress, true)] {
        let name = &fixture.name;
        let lang = Lang::detect(
            &fixture.path.to_string_lossy(),
            fixture.text.lines().next().unwrap_or_default(),
        );
        let rows = match run(lang, &fixture.text) {
            Ok(rows) => rows,
            Err(problem) => {
                eprintln!("{name}: {problem}");
                return ExitCode::FAILURE;
            }
        };
        for row in rows {
            let samples = row.samples.len();
            let timing = percentiles(row.samples);
            let budget = row.budget.filter(|_| budgets);
            if let Some(budget) = budget {
                budgeted += 1;
                met += usize::from(timing.p95 * 1e6 <= budget);
            }
            table.push(vec![
                Cell::Text(name.clone()),
                Cell::Text(row.scenario.to_owned()),
                Cell::Number(timing.median * 1e6),
                Cell::Number(timing.p95 * 1e6),
                Cell::Count(samples),
                budget.map_or(Cell::Missing, Cell::Number),
            ]);
        }
        table.note(format!(
            "checks ok: {name}: every key changed the text and left \
             highlights equal to highlight(), the scattered inserts \
             left the model's text, {ROUND_TRIPS} of {ROUND_TRIPS} \
             round trips agree"
        ));
    }
    table.print();
    println!("edit: {met} of {budgeted} budgets met");
    if let Some(path) = json
        && let Err(err) = table.write_json(&path)
    {
        eprintln!("cannot write {}: {err}", path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn usage(problem: &str) -> ExitCode {
    eprintln!("{problem}\nusage: edit [--json <path>]");
    ExitCode::FAILURE
}
