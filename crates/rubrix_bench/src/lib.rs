use std::fmt::Write as _;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io};

const WARMUP: Duration = Duration::from_millis(200);
const WARMUP_ITERATIONS: u64 = 3;
const BUDGET: Duration = Duration::from_millis(1500);
const MIN_SAMPLES: usize = 20;
const BATCH_TARGET: Duration = Duration::from_millis(5);

/// Per-call timings of one benchmark case, in seconds.
pub struct Measurement {
    pub median: f64,
    pub p95: f64,
}

/// Times `run` the way Pierre's `benchmark/measure.ts` does: a
/// 200 ms warm-up that sizes batches to about 5 ms, then a 1.5 s
/// budget of at least 20 batches. Each sample is one batch's
/// average per-call time.
pub fn measure<R>(mut run: impl FnMut() -> R) -> Measurement {
    let mut batch = 1;
    let mut elapsed = Duration::ZERO;
    let mut iterations = 0;
    while elapsed < WARMUP || iterations < WARMUP_ITERATIONS {
        let took = time_batch(&mut run, batch);
        elapsed += took;
        iterations += batch;
        let fit = (batch as f64 * BATCH_TARGET.as_secs_f64()
            / took.as_secs_f64().max(1e-6))
        .ceil() as u64;
        batch = fit.clamp(1, batch * 2);
    }

    let mut samples = Vec::new();
    let mut elapsed = Duration::ZERO;
    while elapsed < BUDGET || samples.len() < MIN_SAMPLES {
        let took = time_batch(&mut run, batch);
        elapsed += took;
        samples.push(took.as_secs_f64() / batch as f64);
    }
    samples.sort_by(f64::total_cmp);
    let middle = samples.len() / 2;
    let median = if samples.len() % 2 == 0 {
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

fn time_batch<R>(
    run: &mut impl FnMut() -> R,
    batch: u64,
) -> Duration {
    let start = Instant::now();
    for _ in 0..batch {
        black_box(run());
    }
    start.elapsed()
}

pub struct Fixture {
    /// The directory name, which is the language's `Lang::name`.
    pub lang: String,
    /// `<lang>/<file name>`, as the tables print it.
    pub name: String,
    pub path: PathBuf,
    pub text: String,
}

impl Fixture {
    /// Lines the way `rubrix_highlight` counts them, one more than
    /// the number of `\n` bytes.
    pub fn lines(&self) -> usize {
        line_count(&self.text)
    }

    /// The text repeated until it holds at least `lines` lines, so
    /// every copy stays whole.
    pub fn repeated(&self, lines: usize) -> Fixture {
        let newlines = (line_count(&self.text) - 1).max(1);
        let copies = lines.div_ceil(newlines);
        Fixture {
            lang: self.lang.clone(),
            name: format!("{} x{copies}", self.name),
            path: self.path.clone(),
            text: self.text.repeat(copies),
        }
    }
}

fn line_count(text: &str) -> usize {
    text.bytes().filter(|&b| b == b'\n').count() + 1
}

const TIERS: [&str; 3] = ["tiny", "small", "large"];

/// The fixtures of every language directory under `bench/fixtures`,
/// sorted by language and then by tier.
pub fn fixtures() -> io::Result<Vec<Fixture>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/fixtures");
    let mut langs = fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    langs.retain(|path| path.is_dir());
    langs.sort();

    let mut fixtures = Vec::new();
    for dir in langs {
        let mut files = fs::read_dir(&dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        files.sort();
        for tier in TIERS {
            let prefix = format!("{tier}.");
            let Some(path) = files
                .iter()
                .find(|path| file_name(path).starts_with(&prefix))
            else {
                continue;
            };
            let lang = file_name(&dir).to_owned();
            fixtures.push(Fixture {
                name: format!("{lang}/{}", file_name(path)),
                lang,
                text: fs::read_to_string(path)?,
                path: path.clone(),
            });
        }
    }
    Ok(fixtures)
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

pub fn mib_per_sec(bytes: usize, seconds: f64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / seconds
}

/// One table cell. Numbers stay numbers in `--json` output.
pub enum Cell {
    Text(String),
    Count(usize),
    Number(f64),
}

impl Cell {
    fn display(&self) -> String {
        match self {
            Cell::Text(text) => text.clone(),
            Cell::Count(n) => n.to_string(),
            Cell::Number(n) => round(*n),
        }
    }

    fn json(&self) -> String {
        match self {
            Cell::Text(text) => json_string(text),
            Cell::Count(n) => n.to_string(),
            Cell::Number(n) => n.to_string(),
        }
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                write!(out, "\\u{:04x}", c as u32)
                    .expect("write to String");
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn round(n: f64) -> String {
    match n {
        n if n >= 100.0 => format!("{n:.0}"),
        n if n >= 10.0 => format!("{n:.1}"),
        n => format!("{n:.2}"),
    }
}

/// A results table with named columns, printed aligned or written
/// as a JSON array of objects keyed by column name.
pub struct Table {
    columns: Vec<&'static str>,
    rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn new(columns: &[&'static str]) -> Table {
        Table {
            columns: columns.to_vec(),
            rows: Vec::new(),
        }
    }

    pub fn push(&mut self, row: Vec<Cell>) {
        assert_eq!(row.len(), self.columns.len(), "row width");
        self.rows.push(row);
    }

    pub fn print(&self) {
        let cells: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| row.iter().map(Cell::display).collect())
            .collect();
        let widths: Vec<usize> = (0..self.columns.len())
            .map(|i| {
                cells
                    .iter()
                    .map(|row| row[i].chars().count())
                    .chain([self.columns[i].len()])
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let line = |row: &[&str]| {
            let mut out = String::new();
            for (i, cell) in row.iter().enumerate() {
                let pad = widths[i] - cell.chars().count();
                if i == 0 {
                    write!(out, "{cell}{}", " ".repeat(pad))
                } else {
                    write!(out, "  {}{cell}", " ".repeat(pad))
                }
                .expect("write to String");
            }
            println!("{out}");
        };
        line(&self.columns);
        for row in &cells {
            line(&row.iter().map(String::as_str).collect::<Vec<_>>());
        }
    }

    pub fn write_json(&self, path: &Path) -> io::Result<()> {
        let objects: Vec<String> = self
            .rows
            .iter()
            .map(|row| {
                let fields: Vec<String> = self
                    .columns
                    .iter()
                    .zip(row)
                    .map(|(name, cell)| {
                        format!(
                            "{}: {}",
                            json_string(name),
                            cell.json()
                        )
                    })
                    .collect();
                format!("  {{{}}}", fields.join(", "))
            })
            .collect();
        fs::write(path, format!("[\n{}\n]\n", objects.join(",\n")))
    }
}
