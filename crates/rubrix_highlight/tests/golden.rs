//! Lexes each source under `tests/golden/<lang>/` with the language
//! its directory names and compares the rows with `<name>.golden`.
//! `UPDATE_GOLDEN=1 cargo test -p rubrix_highlight` rewrites them.

use std::fs;
use std::path::{Path, PathBuf};

use rubrix_highlight::{Lang, highlight, lex_line};

struct Case {
    lang: Lang,
    source: PathBuf,
    text: String,
}

impl Case {
    fn name(&self) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path =
            self.source.strip_prefix(root).unwrap_or(&self.source);
        path.display().to_string()
    }

    /// Each line's content, split here rather than taken from the
    /// highlights under test.
    fn lines(&self) -> impl Iterator<Item = &str> {
        self.text
            .split('\n')
            .map(|raw| raw.strip_suffix('\r').unwrap_or(raw))
    }
}

fn cases() -> Vec<Case> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut cases = Vec::new();
    for dir in sorted_entries(&root) {
        let name = dir.file_name().unwrap().to_str().unwrap();
        let lang = Lang::from_name(name).unwrap_or_else(|| {
            panic!("tests/golden/{name} names no language")
        });
        for source in sorted_entries(&dir) {
            if source.extension().is_some_and(|ext| ext == "golden") {
                continue;
            }
            let text = fs::read_to_string(&source).unwrap();
            cases.push(Case { lang, source, text });
        }
    }
    assert!(!cases.is_empty(), "no golden sources under {root:?}");
    cases
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    entries
}

#[test]
fn golden() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut failures = Vec::new();
    for case in cases() {
        let highlights = highlight(case.lang, &case.text);
        if let Err(err) = highlights.check(&case.text) {
            failures.push(format!("{}: {err}", case.name()));
            continue;
        }
        let rows = highlights.display(&case.text).to_string();
        let mut golden = case.source.clone().into_os_string();
        golden.push(".golden");
        if update {
            fs::write(&golden, rows).unwrap();
            continue;
        }
        let expected =
            fs::read_to_string(&golden).unwrap_or_default();
        if let Some((want, got)) = first_difference(&expected, &rows)
        {
            failures.push(format!(
                "{}\n  want: {want}\n  got:  {got}",
                case.name()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} golden files differ (rerun with UPDATE_GOLDEN=1 to \
         rewrite them):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn first_difference<'a>(
    want: &'a str,
    got: &'a str,
) -> Option<(&'a str, &'a str)> {
    let mut want_rows = want.lines();
    let mut got_rows = got.lines();
    loop {
        match (want_rows.next(), got_rows.next()) {
            (None, None) => return None,
            (a, b) if a == b => continue,
            (a, b) => {
                return Some((
                    a.unwrap_or("<end>"),
                    b.unwrap_or("<end>"),
                ));
            }
        }
    }
}

/// Lexing from any line, with the state saved for it, gives the same
/// runs and states as lexing the whole text.
#[test]
fn resume() {
    for case in cases() {
        let whole = highlight(case.lang, &case.text);
        let lines: Vec<&str> = case.lines().collect();
        for first in 0..lines.len() {
            let mut state = whole.start_state(first as u32).clone();
            for (n, line) in (first..).zip(&lines[first..]) {
                let mut runs = Vec::new();
                state = lex_line(case.lang, line, &state, &mut runs);
                let n = n as u32;
                assert_eq!(
                    runs,
                    whole.runs(n),
                    "{} line {n}, resumed at line {first}",
                    case.name()
                );
                assert_eq!(
                    &state,
                    whole.start_state(n + 1),
                    "{} state after line {n}, resumed at line \
                     {first}",
                    case.name()
                );
            }
        }
    }
}

/// `\r\n` endings give the runs and states that `\n` endings give.
#[test]
fn crlf() {
    for case in cases() {
        let lf = case.text.replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        let want = highlight(case.lang, &lf);
        let got = highlight(case.lang, &crlf);
        got.check(&crlf)
            .unwrap_or_else(|err| panic!("{}: {err}", case.name()));
        assert_eq!(
            got.line_count(),
            want.line_count(),
            "{}",
            case.name()
        );
        for n in 0..want.line_count() {
            assert_eq!(
                got.runs(n),
                want.runs(n),
                "{} line {n}",
                case.name()
            );
            assert_eq!(
                got.start_state(n + 1),
                want.start_state(n + 1),
                "{} state after line {n}",
                case.name()
            );
        }
    }
}
