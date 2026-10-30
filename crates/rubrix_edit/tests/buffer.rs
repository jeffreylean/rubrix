use std::fs;
use std::ops::Range;
use std::path::Path;

use rubrix_edit::{
    Assoc, Buffer, Change, Command, Editor, Eol, Monospace, Motion,
    Selection, Selections,
};
use rubrix_highlight::{Lang, Lines, highlight};

mod common;

use common::{Rng, seed};

const TEXTS: [&str; 12] = [
    "",
    "a",
    "\n",
    "\r\n",
    "é",
    "🎉",
    "你好",
    "ab\ncd",
    "x\r\ny\r\n",
    "\n\n\n",
    "\r",
    "fn main() {}",
];

fn offset(rng: &mut Rng, text: &str) -> usize {
    text.floor_char_boundary(rng.below(text.len() as u32 + 1) as usize)
}

fn insert_text(rng: &mut Rng) -> String {
    if rng.chance(2) {
        "line\n".repeat(600)
    } else {
        TEXTS[rng.below(TEXTS.len() as u32) as usize].to_owned()
    }
}

/// One to four sorted, non-overlapping replacements of `text`.
fn batch(rng: &mut Rng, text: &str) -> Vec<(Range<usize>, String)> {
    let mut cuts: Vec<usize> = (0..2 * (1 + rng.below(4)))
        .map(|_| offset(rng, text))
        .collect();
    cuts.sort_unstable();
    cuts.chunks(2)
        .map(|pair| {
            let start = pair[0];
            let end = if rng.chance(3) {
                pair[1]
            } else {
                text.floor_char_boundary(
                    pair[1].min(start + rng.below(8) as usize),
                )
            };
            (start..end.max(start), insert_text(rng))
        })
        .collect()
}

fn apply_model(
    text: &str,
    edits: &[(Range<usize>, String)],
) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (range, new) in edits {
        out.push_str(&text[at..range.start]);
        out.push_str(new);
        at = range.end;
    }
    out.push_str(&text[at..]);
    out
}

fn line_starts(text: &str) -> Vec<usize> {
    let breaks = text.match_indices('\n').map(|(i, _)| i + 1);
    std::iter::once(0).chain(breaks).collect()
}

fn assert_agrees(
    rng: &mut Rng,
    buffer: &Buffer,
    model: &str,
    context: &str,
) {
    assert_eq!(buffer.to_string(), model, "{context}: text");
    assert_eq!(buffer.len(), model.len(), "{context}: len");
    let starts = line_starts(model);
    assert_eq!(
        buffer.line_count() as usize,
        starts.len(),
        "{context}: line count"
    );
    for _ in 0..4 {
        let at = offset(rng, model);
        let line = starts.partition_point(|&s| s <= at) - 1;
        assert_eq!(
            buffer.line_of(at) as usize,
            line,
            "{context}: line_of({at})"
        );
        let line = rng.below(starts.len() as u32);
        assert_eq!(
            buffer.line_start(line),
            starts[line as usize],
            "{context}: line_start({line})"
        );
        let (a, b) = (offset(rng, model), offset(rng, model));
        let range = a.min(b)..a.max(b);
        assert_eq!(
            buffer.slice(range.clone()),
            &model[range.clone()],
            "{context}: slice({range:?})"
        );
    }
    assert_eq!(
        buffer.line_start(buffer.line_count()),
        model.len() + 1,
        "{context}: start past the last line"
    );
}

#[test]
fn random_replacements_match_a_string() {
    let seed = seed();
    let mut rng = Rng(seed ^ (3 << 32));
    let mut model: String =
        (0..3_000).map(|n| format!("line {n}\n")).collect();
    let mut buffer = Buffer::new(&model);
    for step in 0..10_000 {
        let mut edits = batch(&mut rng, &model);
        if rng.chance(1) {
            let start = offset(&mut rng, &model);
            let end = model.floor_char_boundary(start + 20_000);
            edits = vec![(start..end, String::new())];
        }
        let lines = i64::from(buffer.line_count());
        let change = buffer.replace(edits.clone());
        model = apply_model(&model, &edits);
        let context = format!("seed {seed} step {step}");
        assert_eq!(
            lines + change.line_delta(),
            line_starts(&model).len() as i64,
            "{context}: line delta"
        );
        assert_agrees(&mut rng, &buffer, &model, &context);
    }
}

#[test]
fn crlf_text_round_trips() {
    let text = "fn main() {\r\n    x();\r\n}\r\n";
    let buffer = Buffer::new(text);
    assert_eq!(buffer.eol(), Eol::Crlf);
    assert_eq!(buffer.to_string(), text);
    assert_eq!(buffer.line(1), "    x();\r");
    assert_eq!(buffer.line_end(1), 21);
}

#[test]
fn mixed_line_endings_round_trip() {
    let text = "a\nb\r\nc\rd\n\r\ne";
    let buffer = Buffer::new(text);
    assert_eq!(buffer.eol(), Eol::Lf);
    assert_eq!(buffer.to_string(), text);
    assert_eq!(buffer.line_count(), 5);
}

#[test]
fn normalize_writes_the_buffer_ending() {
    assert_eq!(
        Eol::Crlf.normalize("a\nb\r\nc\rd"),
        "a\r\nb\r\nc\r\nd"
    );
    assert_eq!(Eol::Lf.normalize("a\nb\r\nc\rd"), "a\nb\nc\nd");
    assert!(matches!(
        Eol::Crlf.normalize("a\r\nb"),
        std::borrow::Cow::Borrowed(_)
    ));
}

fn lines_of(buffer: &Buffer) -> Vec<String> {
    (0..buffer.line_count())
        .map(|n| buffer.line(n).to_owned())
        .collect()
}

#[test]
fn line_edits_rebuild_the_lines() {
    let seed = seed();
    let mut rng = Rng(seed ^ (5 << 32));
    let mut buffer = Buffer::new(
        &(0..300).map(|n| format!("row {n}\n")).collect::<String>(),
    );
    for step in 0..2_000 {
        let mut lines = lines_of(&buffer);
        let edits = batch(&mut rng, &buffer.to_string());
        let change = buffer.replace(edits.clone());
        let mut floor = 0;
        for edit in change.line_edits() {
            assert!(edit.first >= floor, "step {step}: {edit:?}");
            assert!(edit.removed > 0 && edit.inserted > 0);
            floor = edit.first + edit.inserted;
            let (first, removed) =
                (edit.first as usize, edit.removed as usize);
            let new = (edit.first..edit.first + edit.inserted)
                .map(|n| buffer.line(n).to_owned());
            lines.splice(first..first + removed, new);
        }
        assert_eq!(
            lines,
            lines_of(&buffer),
            "seed {seed} step {step} {edits:?}"
        );
    }
}

#[test]
fn replacements_on_one_line_share_a_line_edit() {
    let mut buffer = Buffer::new("ab\ncd\nef\ngh");
    let change = buffer.replace([
        (0..0, "x"),
        (1..4, "\n\n"),
        (7..7, "y"),
        (10..10, "z"),
    ]);
    let edits: Vec<_> = change
        .line_edits()
        .map(|e| (e.first, e.removed, e.inserted))
        .collect();
    assert_eq!(edits, [(0, 2, 3), (3, 1, 1), (4, 1, 1)]);
}

/// Pierre's `remapOffsetThroughEdits`, the walk `Change::map`
/// documents, with the `Before` stop added.
fn walk(change: &Change, offset: usize, assoc: Assoc) -> usize {
    let mut delta = 0isize;
    for rep in change.replacements() {
        let shifted = (rep.range.start as isize + delta) as usize;
        if offset < rep.range.start {
            break;
        }
        let holds = offset < rep.range.end
            || (assoc == Assoc::Before && offset == rep.range.start);
        if holds {
            return match assoc {
                Assoc::After => shifted + rep.new.len(),
                Assoc::Before => shifted,
            };
        }
        delta += rep.new.len() as isize - rep.range.len() as isize;
    }
    (offset as isize + delta) as usize
}

#[test]
fn map_moves_offsets_like_a_walk() {
    let seed = seed();
    let mut rng = Rng(seed ^ (9 << 32));
    for step in 0..2_000 {
        let text: String =
            (0..20).map(|n| format!("{n} é\n")).collect();
        let mut buffer = Buffer::new(&text);
        let change = buffer.replace(batch(&mut rng, &text));
        let mut probes: Vec<usize> = Vec::new();
        for rep in change.replacements() {
            let Range { start, end } = rep.range;
            probes.extend([
                start.saturating_sub(1),
                start,
                end,
                end + 1,
            ]);
            probes.push(text.floor_char_boundary((start + end) / 2));
        }
        for at in probes.into_iter().filter(|&at| at <= text.len()) {
            for assoc in [Assoc::Before, Assoc::After] {
                assert_eq!(
                    change.map(at, assoc),
                    walk(&change, at, assoc),
                    "seed {seed} step {step} {change:?} at {at} \
                     {assoc:?}"
                );
            }
        }
    }
}

#[test]
fn map_follows_the_documented_cases() {
    let mut buffer = Buffer::new("hello world");
    let change = buffer.replace([(0..0, "X"), (6..11, "there")]);
    assert_eq!(buffer.to_string(), "Xhello there");
    assert_eq!(change.map(0, Assoc::After), 1);
    assert_eq!(change.map(0, Assoc::Before), 0);
    assert_eq!(change.map(3, Assoc::Before), 4);
    assert_eq!(change.map(6, Assoc::After), 12);
    assert_eq!(change.map(6, Assoc::Before), 7);
    assert_eq!(change.map(8, Assoc::After), 12);
    assert_eq!(change.map(8, Assoc::Before), 7);
    assert_eq!(change.map(11, Assoc::Before), 12);
}

#[test]
fn empty_ranges_at_one_offset_go_in_in_order() {
    let mut buffer = Buffer::new("ac");
    let change =
        buffer.replace([(1..1, "b"), (1..1, "B"), (1..2, "")]);
    assert_eq!(buffer.to_string(), "abB");
    assert_eq!(change.len(), 3);
}

#[test]
fn a_snapshot_keeps_its_text() {
    let text: String = (0..2_000).map(|n| format!("{n}\n")).collect();
    let mut buffer = Buffer::new(&text);
    let mut snapshot = buffer.snapshot();
    let _ = buffer.replace([(0..0, "edit\n"), (5_000..5_100, "")]);
    assert_eq!(snapshot.to_string(), text);
    let _ = snapshot.replace([(10..10, "other")]);
    assert_ne!(buffer.to_string(), snapshot.to_string());
    let edited = buffer.to_string();
    let _ = snapshot.replace([(0..snapshot.len(), "")]);
    assert_eq!(buffer.to_string(), edited);
    assert_eq!(snapshot.to_string(), "");
}

fn large_rust() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/fixtures/rust/large.rs.txt");
    fs::read_to_string(path).unwrap()
}

/// One to 20 carets, fewer where two land on one offset.
fn random_carets(rng: &mut Rng, buffer: &Buffer) -> Selections {
    let count = 1 + rng.below(20);
    let mut at = || rng.below(buffer.len() as u32 + 1) as usize;
    let mut selections = Selections::new(Selection::caret(at()));
    for _ in 1..count {
        selections.add(Selection::caret(at()));
    }
    selections
}

fn random_command(rng: &mut Rng) -> Command<'static> {
    const TYPED: [&str; 8] =
        ["x", "/*", "*/", "\"", "é🎉", " ", "{", "}"];
    match rng.below(12) {
        0..=3 => Command::Type(TYPED[rng.below(8) as usize]),
        4 => Command::Enter,
        5 => Command::Backspace,
        6 => Command::Delete,
        7 => Command::DeleteWordBack,
        8 => Command::Paste("a\nb /* c\n"),
        9 => Command::Undo,
        10 => Command::Redo,
        _ => Command::Move {
            motion: [Motion::Left, Motion::Down, Motion::WordRight]
                [rng.below(3) as usize],
            extend: rng.chance(30),
        },
    }
}

#[test]
fn editor_changes_keep_highlights_current() {
    let seed = seed();
    let mut rng = Rng(seed ^ (11 << 32));
    let text = large_rust();
    let mut highlights = highlight(Lang::Rust, &text);
    let mut editor = Editor::new(Buffer::new(&text));
    let layout = Monospace::default();
    for step in 0..400 {
        if step % 50 == 0 {
            let carets = random_carets(&mut rng, editor.buffer());
            editor.set_selections(carets);
        }
        let command = random_command(&mut rng);
        let Some(change) = editor.apply(command, &layout) else {
            continue;
        };
        let count = editor.buffer().line_count();
        let start = rng.below(count.saturating_sub(60).max(1));
        highlights.edit_many(
            change.line_edits(),
            editor.buffer(),
            start..start + 60,
        );
    }
    let last = editor.buffer().line_count() - 1;
    highlights.runs(last, editor.buffer());
    assert!(
        highlights
            == highlight(Lang::Rust, &editor.buffer().to_string()),
        "seed {seed}"
    );
}
