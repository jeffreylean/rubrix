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

struct Batch {
    edits: Vec<LineEdit>,
    /// For each line after the batch, the line it was before, or
    /// `None` for a line an edit wrote.
    origin: Vec<Option<u32>>,
}

/// Applies up to `most` random line edits at once, ascending and in
/// pre-edit lines, and returns them in sequential coordinates.
fn random_batch(
    rng: &mut Rng,
    lines: &mut Vec<String>,
    most: u32,
) -> Batch {
    let count = lines.len() as u32;
    let mut picked = Vec::new();
    let mut at = 0;
    for _ in 0..1 + rng.below(most) {
        let first = at + rng.below(count / most + 3);
        if first >= count {
            break;
        }
        let removed = (1 + rng.below(3)).min(count - first);
        let inserted = rng.below(4);
        picked.push((first, removed, inserted));
        at = first + removed;
    }
    if picked.is_empty() {
        picked.push((0, 1, 1));
    }
    let survivors: u32 =
        picked.iter().map(|&(_, removed, _)| removed).sum();
    if survivors == count && picked.iter().all(|edit| edit.2 == 0) {
        picked.last_mut().unwrap().2 = 1;
    }

    let mut out = Vec::new();
    let mut origin = Vec::new();
    let mut edits = Vec::new();
    let mut kept = 0;
    for (first, removed, inserted) in picked {
        for old in kept..first {
            out.push(lines[old as usize].clone());
            origin.push(Some(old));
        }
        edits.push(LineEdit {
            first: out.len() as u32,
            removed,
            inserted,
        });
        for n in 0..inserted {
            let line = match lines.get((first + n) as usize) {
                Some(line) if n < removed && rng.chance(80) => {
                    mutate(rng, line, lines)
                }
                _ => fragment(rng, lines),
            };
            out.push(line);
            origin.push(None);
        }
        kept = first + removed;
    }
    for old in kept..count {
        out.push(lines[old as usize].clone());
        origin.push(Some(old));
    }
    *lines = out;
    Batch { edits, origin }
}

fn random_window(rng: &mut Rng, count: u32) -> Range<u32> {
    match rng.below(4) {
        0 => 0..u32::MAX,
        1 => 0..0,
        _ => {
            let start = rng.below(count + 1);
            start..start + rng.below(60)
        }
    }
}

fn batch_hull(
    old: &Highlights,
    new: &Highlights,
    origin: &[Option<u32>],
    window: &Range<u32>,
) -> Range<u32> {
    let mut changed = (window.start
        ..window.end.min(new.line_count()))
        .filter(|&n| match origin[n as usize] {
            None => true,
            Some(was) => new.stored_runs(n) != old.stored_runs(was),
        });
    let Some(start) = changed.next() else {
        return 0..0;
    };
    start..changed.next_back().unwrap_or(start) + 1
}

#[track_caller]
fn assert_window_current(
    got: &Highlights,
    window: &Range<u32>,
    context: &str,
) {
    let upto = window.end.min(got.line_count());
    assert!(
        got.stale_from().is_none_or(|stale| stale >= upto),
        "{context}: stale from {:?} inside the window",
        got.stale_from()
    );
}

#[test]
fn random_batches_match_highlight() {
    let seed = seed();
    for (index, (lang, path, text)) in
        golden_texts().iter().enumerate()
    {
        let name = path.file_name().unwrap().to_str().unwrap();
        let mut rng = Rng(seed ^ ((index as u64 + 7) << 32));
        let mut lines = split(text);
        let mut got = highlight(*lang, text);
        for step in 0..EDITS {
            if rng.chance(20) {
                let (edit, window) =
                    random_edit(&mut rng, &mut lines);
                got.edit(edit, &lines, window);
                continue;
            }
            let before = got.clone();
            let batch = random_batch(&mut rng, &mut lines, 6);
            let window = random_window(&mut rng, lines.len() as u32);
            let context = format!(
                "{name} seed {seed} batch {step} {:?} window \
                 {window:?}",
                batch.edits
            );
            let changed = got.edit_many(
                batch.edits.iter().copied(),
                &lines,
                window.clone(),
            );
            assert!(
                changed.is_empty()
                    || (window.start <= changed.start
                        && changed.end <= window.end),
                "{context}: changed {changed:?} leaves the window"
            );
            assert_window_current(&got, &window, &context);
            let text = join(&lines);
            let want = highlight(*lang, &text);
            assert_current_prefix(&got, &want, &text, &context);
            if before.stale_from().is_none() {
                assert_eq!(
                    changed,
                    batch_hull(
                        &before,
                        &want,
                        &batch.origin,
                        &window
                    ),
                    "{context}: changed is not the hull of the \
                     changed lines"
                );
            }
        }
        settle(&mut got, &lines);
        let text = join(&lines);
        assert!(
            got == highlight(*lang, &text),
            "{name} seed {seed}: final text differs"
        );
        got.check(&text).unwrap();
    }
}

#[test]
fn one_edit_in_the_window_lexes_as_edit_does() {
    let seed = seed();
    for (index, (lang, _, text)) in golden_texts().iter().enumerate()
    {
        let mut rng = Rng(seed ^ ((index as u64 + 11) << 32));
        let mut lines = split(text);
        let mut single = highlight(*lang, text);
        let mut many = single.clone();
        for _ in 0..EDITS {
            if single.stale_from().is_some() {
                settle(&mut single, &lines);
                many = single.clone();
            }
            let (edit, window) = random_edit(&mut rng, &mut lines);
            if edit.removed == 0 || edit.first < window.start {
                single.edit(edit, &lines, 0..u32::MAX);
                settle(&mut single, &lines);
                many = single.clone();
                continue;
            }
            let want = single.edit(edit, &lines, window.clone());
            let got = many.edit_many([edit], &lines, window);
            assert_eq!(got, want);
            assert_eq!(many.stale_from(), single.stale_from());
            assert!(many == single);
        }
    }
}

fn large_rust() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/fixtures/rust/large.rs.txt");
    split(&fs::read_to_string(path).unwrap())
}

/// One keystroke at every caret: the carets as (line, byte column)
/// in buffer order, one per line. Returns the keystroke's line edits.
fn keystroke(
    rng: &mut Rng,
    lines: &mut Vec<String>,
    carets: &mut [(u32, usize)],
) -> Vec<LineEdit> {
    const TYPED: [&str; 8] =
        ["x", "x", "x", " ", "/*", "*/", "\"", "{"];
    let kind = rng.below(10);
    let typed = TYPED[rng.below(TYPED.len() as u32) as usize];
    let mut edits = Vec::new();
    let mut delta: i64 = 0;
    let mut previous_line = None;
    let mut out = Vec::new();
    let mut kept = 0;
    for caret in carets.iter_mut() {
        let (line, col) = *caret;
        let first = (line as i64 + delta) as u32;
        let text = &lines[line as usize];
        let (removed, new): (u32, Vec<String>) = match kind {
            0 | 1 => {
                let (head, tail) = text.split_at(col);
                *caret = (first + 1, 0);
                (1, vec![head.to_owned(), tail.to_owned()])
            }
            2 | 3 if col == 0 => {
                let above = line.checked_sub(1);
                if above.is_none() || above == previous_line {
                    *caret = (first, col);
                    (1, vec![text.clone()])
                } else {
                    let up = &lines[line as usize - 1];
                    *caret = (first, up.len());
                    (2, vec![format!("{up}{text}")])
                }
            }
            2 | 3 => {
                let back = text[..col].chars().next_back().unwrap();
                let at = col - back.len_utf8();
                let mut text = text.clone();
                text.remove(at);
                *caret = (first, at);
                (1, vec![text])
            }
            _ => {
                let mut text = text.clone();
                text.insert_str(col, typed);
                *caret = (first, col + typed.len());
                (1, vec![text])
            }
        };
        let start = line + 1 - removed;
        for old in kept..start {
            out.push(lines[old as usize].clone());
        }
        let first = out.len() as u32;
        if removed == 2 {
            caret.0 = first;
        }
        edits.push(LineEdit {
            first,
            removed,
            inserted: new.len() as u32,
        });
        delta += new.len() as i64 - removed as i64;
        out.extend(new);
        kept = line + 1;
        previous_line = Some(line);
    }
    for old in kept..lines.len() as u32 {
        out.push(lines[old as usize].clone());
    }
    *lines = out;
    edits
}

#[test]
fn keystrokes_at_100_carets_match_highlight() {
    let seed = seed();
    let mut rng = Rng(seed ^ (13 << 32));
    let mut lines = large_rust();
    let mut got = highlight(Lang::Rust, &join(&lines));
    let step = lines.len() as u32 / 100;
    let mut carets: Vec<(u32, usize)> =
        (0..100).map(|k| (k * step + 3, 0)).collect();
    for key in 0..200 {
        let edits = keystroke(&mut rng, &mut lines, &mut carets);
        let count = lines.len() as u32;
        let start = rng.below(count.saturating_sub(60));
        let window = start..start + 60;
        let context =
            format!("seed {seed} key {key} window {window:?}");
        got.edit_many(edits, &lines, window.clone());
        assert_window_current(&got, &window, &context);
        let text = join(&lines);
        let want = highlight(Lang::Rust, &text);
        assert_current_prefix(&got, &want, &text, &context);
        for line in window {
            assert_eq!(
                got.stored_runs(line),
                want.stored_runs(line),
                "{context}: line {line}"
            );
        }
    }
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn comments_opened_at_100_carets_stop_at_the_window() {
    let mut lines = numbered(10_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    let edits: Vec<LineEdit> = (0..100)
        .map(|k| {
            lines[k * 100].insert_str(0, "/*");
            LineEdit {
                first: k as u32 * 100,
                removed: 1,
                inserted: 1,
            }
        })
        .collect();
    let changed = got.edit_many(edits, &lines, 5_000..5_060);
    assert_eq!(changed, 5_000..5_060);
    assert_eq!(got.stale_from(), Some(5_060));
    assert!(
        got.stored_runs(9_000)
            .iter()
            .any(|run| run.kind != Kind::Comment),
        "a line past the window was lexed early"
    );
    settle(&mut got, &lines);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
fn typing_at_100_carets_leaves_nothing_stale() {
    let mut lines = numbered(100_000);
    let mut got = highlight(Lang::Rust, &join(&lines));
    let edits: Vec<LineEdit> = (0..100)
        .map(|k| {
            lines[k * 1_000].push('x');
            LineEdit {
                first: k as u32 * 1_000,
                removed: 1,
                inserted: 1,
            }
        })
        .collect();
    got.edit_many(edits, &lines, 50_000..50_060);
    assert_eq!(got.stale_from(), None);
    assert!(got == highlight(Lang::Rust, &join(&lines)));
}

#[test]
#[should_panic(expected = "out of order")]
fn line_edits_out_of_order_panic() {
    let lines = numbered(10);
    let mut got = highlight(Lang::Rust, &join(&lines));
    let edit = |first| LineEdit {
        first,
        removed: 1,
        inserted: 1,
    };
    got.edit_many([edit(5), edit(2)], &lines, 0..10);
}

#[test]
fn a_deferred_edit_in_the_window_counts_as_changed() {
    let mut lines = numbered(100);
    let mut got = highlight(Lang::Plain, &join(&lines));
    let edit = |first| LineEdit {
        first,
        removed: 1,
        inserted: 1,
    };
    lines[10].push('x');
    got.edit(edit(10), &lines, 50..60);
    assert_eq!(got.stale_from(), Some(10));
    lines[55].push('y');
    assert_eq!(got.edit_many([edit(55)], &lines, 50..60), 55..56);
    assert_window_current(&got, &(50..60), "plain");
}
