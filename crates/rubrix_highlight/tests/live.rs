use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use rubrix_highlight::{Highlights, Kind, Lang, LineEdit, highlight};

const EDITS: u32 = 1000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n)) as u32
    }

    fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

fn parse_seed(var: Option<&str>) -> u64 {
    var.map_or(1, |seed| {
        seed.parse().unwrap_or_else(|_| {
            panic!(
                "RUBRIX_SEED must be an unsigned integer, got \
                 {seed:?}"
            )
        })
    })
}

fn seed() -> u64 {
    let var = std::env::var_os("RUBRIX_SEED");
    parse_seed(
        var.as_ref().map(|seed| seed.to_string_lossy()).as_deref(),
    )
}

#[test]
fn seed_defaults_to_one() {
    assert_eq!(parse_seed(None), 1);
}

#[test]
fn seed_reads_the_variable() {
    assert_eq!(parse_seed(Some("7")), 7);
}

#[test]
#[should_panic(expected = "RUBRIX_SEED")]
fn unparsable_seed_fails() {
    parse_seed(Some("seven"));
}

fn golden_texts() -> Vec<(Lang, PathBuf, String)> {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut texts = Vec::new();
    for dir in sorted_entries(&root) {
        let name = dir.file_name().unwrap().to_str().unwrap();
        let lang = Lang::from_name(name).unwrap();
        for source in sorted_entries(&dir) {
            if source.extension().is_some_and(|ext| ext == "golden") {
                continue;
            }
            let text = fs::read_to_string(&source).unwrap();
            texts.push((lang, source, text));
        }
    }
    assert!(!texts.is_empty(), "no golden sources under {root:?}");
    texts
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    entries
}

fn split(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_owned).collect()
}

fn join(lines: &[String]) -> String {
    lines.join("\n")
}

const PIECES: [&str; 18] = [
    "/*", "*/", "\"", "'", "`", "//", "/// ", "#[", "r#\"", "\"#",
    "\\", "{", "}", "\r", "é🎉", " ", "'a", "0x1f",
];

fn fragment(rng: &mut Rng, lines: &[String]) -> String {
    match rng.below(4) {
        0 => {
            PIECES[rng.below(PIECES.len() as u32) as usize].to_owned()
        }
        1 => lines[rng.below(lines.len() as u32) as usize].clone(),
        2 => "let x = 1;".to_owned(),
        _ => String::new(),
    }
}

fn mutate(rng: &mut Rng, line: &str, lines: &[String]) -> String {
    let bounds: Vec<usize> = line
        .char_indices()
        .map(|(at, _)| at)
        .chain([line.len()])
        .collect();
    let start = bounds[rng.below(bounds.len() as u32) as usize];
    let end = if rng.chance(50) {
        start
    } else {
        let after: Vec<usize> = bounds
            .iter()
            .copied()
            .filter(|&at| at >= start)
            .collect();
        after[rng.below(after.len() as u32) as usize]
    };
    let mut out = line.to_owned();
    out.replace_range(start..end, &fragment(rng, lines));
    out
}

fn random_edit(
    rng: &mut Rng,
    lines: &mut Vec<String>,
) -> (LineEdit, Range<u32>) {
    let count = lines.len() as u32;
    let (first, removed, inserted) = if rng.chance(60) {
        (rng.below(count), 1, 1)
    } else {
        let first = rng.below(count + 1);
        let removed = rng.below(4).min(count - first);
        let mut inserted = rng.below(4);
        if count - removed + inserted == 0 {
            inserted = 1;
        }
        (first, removed, inserted)
    };
    let old =
        lines[first as usize..(first + removed) as usize].to_vec();
    let new: Vec<String> = (0..inserted)
        .map(|n| match old.get(n as usize) {
            Some(line) if rng.chance(80) => mutate(rng, line, lines),
            _ => fragment(rng, lines),
        })
        .collect();
    lines.splice(first as usize..(first + removed) as usize, new);

    let count = lines.len() as u32;
    let window = match rng.below(5) {
        0 => 0..u32::MAX,
        1 => 0..0,
        2 => {
            let start = rng.below(count + 1);
            start..start + rng.below(30)
        }
        3 => {
            first.saturating_sub(rng.below(10))
                ..first + inserted + rng.below(40)
        }
        _ => count.saturating_sub(rng.below(50))..count,
    };
    (
        LineEdit {
            first,
            removed,
            inserted,
        },
        window,
    )
}

#[track_caller]
fn assert_current_prefix(
    got: &Highlights,
    want: &Highlights,
    text: &str,
    context: &str,
) {
    assert_eq!(got.line_count(), want.line_count(), "{context}");
    got.check(text).unwrap_or_else(|err| {
        panic!("{context}: check failed: {err}");
    });
    let current = got.stale_from().unwrap_or(got.line_count());
    for n in 0..got.line_count() {
        assert_eq!(
            got.line_range(n),
            want.line_range(n),
            "{context}: line {n} range"
        );
        if n < current {
            assert_eq!(
                got.stored_runs(n),
                want.stored_runs(n),
                "{context}: line {n} runs (current up to {current})"
            );
            assert_eq!(
                got.start_state(n + 1),
                want.start_state(n + 1),
                "{context}: line {n} end state"
            );
        }
    }
}

fn changed_hull(
    old: &Highlights,
    new: &Highlights,
    edit: LineEdit,
    window: &Range<u32>,
) -> Range<u32> {
    let LineEdit {
        first,
        removed,
        inserted,
    } = edit;
    if first < window.start {
        return 0..0;
    }
    let mut changed = (window.start
        ..window.end.min(new.line_count()))
        .filter(|&n| {
            n >= first
                && (n < first + inserted
                    || new.stored_runs(n)
                        != old.stored_runs(n - inserted + removed))
        });
    let Some(start) = changed.next() else {
        return 0..0;
    };
    start..changed.next_back().unwrap_or(start) + 1
}

#[track_caller]
fn assert_nothing_lexed(
    before: &Highlights,
    after: &Highlights,
    edit: LineEdit,
    context: &str,
) {
    let LineEdit {
        first,
        removed,
        inserted,
    } = edit;
    for n in (0..first).chain(first + inserted..after.line_count()) {
        let old = if n < first { n } else { n - inserted + removed };
        assert_eq!(
            after.stored_runs(n),
            before.stored_runs(old),
            "{context}: line {n} was lexed"
        );
    }
    assert!(
        after.stale_from().unwrap_or(after.line_count()) <= first,
        "{context}: stale from {:?}, not from the edit",
        after.stale_from()
    );
}

fn settle(got: &mut Highlights, lines: &[String]) {
    let last = got.line_count() - 1;
    got.runs(last, lines);
    assert_eq!(got.stale_from(), None);
}

#[test]
fn random_edits_match_highlight() {
    let seed = seed();
    for (index, (lang, path, text)) in
        golden_texts().iter().enumerate()
    {
        let name = path.file_name().unwrap().to_str().unwrap();
        let mut rng = Rng(seed ^ ((index as u64 + 1) << 32));
        let mut lines = split(text);
        let mut got = highlight(*lang, text);
        for step in 0..EDITS {
            let before = got.clone();
            let (edit, window) = random_edit(&mut rng, &mut lines);
            let changed = got.edit(edit, &lines, window.clone());
            let context = format!(
                "{name} seed {seed} edit {step} {edit:?} window \
                 {window:?}"
            );
            assert!(
                changed.is_empty()
                    || (window.start <= changed.start
                        && changed.end <= window.end),
                "{context}: changed {changed:?} leaves the window"
            );
            assert!(changed.end <= got.line_count(), "{context}");
            let text = join(&lines);
            let want = highlight(*lang, &text);
            assert_current_prefix(&got, &want, &text, &context);
            if edit.first < window.start {
                assert!(
                    changed.is_empty(),
                    "{context}: changed {changed:?} above the window"
                );
                assert_nothing_lexed(&before, &got, edit, &context);
            }
            if before.stale_from().is_none() {
                assert_eq!(
                    changed,
                    changed_hull(&before, &want, edit, &window),
                    "{context}: changed is not the hull of the \
                     changed lines"
                );
            }
            if rng.chance(30) {
                let line = rng.below(got.line_count());
                assert_eq!(
                    got.runs(line, &lines),
                    want.stored_runs(line),
                    "{context}: read of line {line}"
                );
                assert_current_prefix(&got, &want, &text, &context);
            }
        }
        settle(&mut got, &lines);
        let text = join(&lines);
        let want = highlight(*lang, &text);
        assert!(
            got == want,
            "{name} seed {seed}: final text differs"
        );
        got.check(&text).unwrap();
        println!("{name}: {EDITS} edits ok (seed {seed})");
    }
}

fn numbered(count: usize) -> Vec<String> {
    (0..count)
        .map(|n| format!("let x{n} = {n}; // c"))
        .collect()
}

#[test]
fn stale_lines_are_lexed_on_read() {
    let mut lines = numbered(10_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[1].insert_str(0, "/*");
    let edit = LineEdit {
        first: 1,
        removed: 1,
        inserted: 1,
    };
    let changed = got.edit(edit, &lines, 0..120);
    assert!(
        !changed.is_empty() && changed.len() <= 120,
        "changed {changed:?}"
    );
    assert_eq!(got.stale_from(), Some(120));
    assert!(
        got.stored_runs(9_000)
            .iter()
            .any(|run| run.kind != Kind::Comment),
        "a line past the window was lexed early"
    );

    let runs = got.runs(9_000, &lines).to_vec();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| run.kind == Kind::Comment));
    assert_eq!(got.stale_from(), Some(9_001));

    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn stale_edit_above_a_distant_window_returns_nothing() {
    let mut lines = numbered(10_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[2].insert_str(0, "/*");
    let edit = LineEdit {
        first: 2,
        removed: 1,
        inserted: 1,
    };
    let changed = got.edit(edit, &lines, 9_880..10_000);
    assert!(changed.is_empty(), "changed {changed:?}");
    assert_eq!(got.stale_from(), Some(2));

    let last = got.line_count() - 1;
    assert!(
        got.runs(last, &lines)
            .iter()
            .all(|run| run.kind == Kind::Comment)
    );
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn edit_above_the_window_lexes_nothing() {
    let mut lines = numbered(1_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[50].insert_str(0, "/*");
    let edit = LineEdit {
        first: 50,
        removed: 1,
        inserted: 1,
    };
    let changed = got.edit(edit, &lines, 51..171);
    assert!(changed.is_empty(), "changed {changed:?}");
    assert_eq!(got.stale_from(), Some(50));
    assert!(
        got.stored_runs(100)
            .iter()
            .any(|run| run.kind != Kind::Comment),
        "a line in the window was lexed"
    );

    assert!(
        got.runs(100, &lines)
            .iter()
            .all(|run| run.kind == Kind::Comment)
    );
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn edit_above_the_window_does_not_lex_up_to_older_stale_lines() {
    let mut lines = numbered(1_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[0].insert_str(0, "/*");
    let open = LineEdit {
        first: 0,
        removed: 1,
        inserted: 1,
    };
    got.edit(open, &lines, 0..300);
    assert_eq!(got.stale_from(), Some(300));

    lines[10].insert_str(0, "*/");
    let close = LineEdit {
        first: 10,
        removed: 1,
        inserted: 1,
    };
    let changed = got.edit(close, &lines, 100..220);
    assert!(changed.is_empty(), "changed {changed:?}");
    assert_eq!(got.stale_from(), Some(10));
    assert!(
        got.stored_runs(150)
            .iter()
            .all(|run| run.kind == Kind::Comment),
        "a line in the window was lexed"
    );

    assert!(
        got.runs(150, &lines)
            .iter()
            .any(|run| run.kind != Kind::Comment)
    );
    assert_eq!(got.stale_from(), Some(151));
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn deferred_edit_does_not_converge_on_a_line_it_has_not_lexed() {
    let mut lines: Vec<String> =
        ["/*", "x", "*/", "let y = 1;"].map(str::to_owned).to_vec();
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[0] = "let a = 1;".to_owned();
    let edit = LineEdit {
        first: 0,
        removed: 1,
        inserted: 1,
    };
    let changed = got.edit(edit, &lines, 3..4);
    assert!(changed.is_empty(), "changed {changed:?}");
    assert!(
        got.runs(1, &lines)
            .iter()
            .all(|run| run.kind != Kind::Comment)
    );
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn changed_lines_leave_out_lexed_lines_that_did_not_change() {
    let mut lines: Vec<String> =
        ["/* open", "b", "c", "d", "e"].map(str::to_owned).to_vec();
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines.insert(1, "/*".to_owned());
    let edit = LineEdit {
        first: 1,
        removed: 0,
        inserted: 1,
    };
    assert_eq!(got.edit(edit, &lines, 0..u32::MAX), 1..2);
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn changed_lines_stop_at_the_last_line_that_changed() {
    let mut lines: Vec<String> =
        ["/* a", "b", "c", "d"].map(str::to_owned).to_vec();
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines.splice(0..0, ["/*".to_owned(), "/*".to_owned()]);
    let edit = LineEdit {
        first: 0,
        removed: 0,
        inserted: 2,
    };
    assert_eq!(got.edit(edit, &lines, 0..u32::MAX), 0..2);
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn local_edit_changes_one_line() {
    let mut lines = vec![
        "fn main() {".to_owned(),
        "    let s = \"hello world\";".to_owned(),
        "    let t = 'a';".to_owned(),
        "}".to_owned(),
    ];
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[1] = "    let s = \"jello world\";".to_owned();
    let edit = LineEdit {
        first: 1,
        removed: 1,
        inserted: 1,
    };
    assert_eq!(got.edit(edit, &lines, 0..u32::MAX), 1..2);
    assert_eq!(got.stale_from(), None);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn local_edit_in_a_big_file_does_not_touch_the_rest() {
    let mut lines = numbered(10_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    lines[5_000].push_str(" // more");
    let edit = LineEdit {
        first: 5_000,
        removed: 1,
        inserted: 1,
    };
    assert_eq!(got.edit(edit, &lines, 0..u32::MAX), 5_000..5_001);
    assert_eq!(got.stale_from(), None);
}

#[test]
fn many_edits_to_one_line_keep_the_runs() {
    let mut lines = numbered(50);
    let mut got = highlight(Lang::Rust, &join(&lines));
    for n in 0..5_000 {
        lines[10] = format!("let y = {n}; /* a */ \"b\" // c");
        let edit = LineEdit {
            first: 10,
            removed: 1,
            inserted: 1,
        };
        got.edit(edit, &lines, 0..u32::MAX);
    }
    assert!(got == highlight(Lang::Rust, &join(&lines)));
    got.check(&join(&lines)).unwrap();
}

#[test]
#[should_panic(expected = "past the end")]
fn edit_past_the_end_panics() {
    let lines = vec!["a".to_owned()];
    let mut got = highlight(Lang::Rust, "a");
    let edit = LineEdit {
        first: 1,
        removed: 1,
        inserted: 0,
    };
    got.edit(edit, &lines, 0..1);
}
