mod support;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ui::{Display, Node};
use bevy::ui_widgets::Button;
use bevy_rubrix::{DiffRow, DiffSource, Layout, Row, Side};
use rubrix_diff::Expansion;
use support::diff::*;
use support::{copy, point_in, press_on, shift};

fn gap_pair() -> DiffSource {
    let old = numbered(400);
    let new = old
        .replace("line 10\n", "line ten\n")
        .replace("line 320\n", "line three hundred twenty\n");
    DiffSource::texts("numbers.txt", &old, &new)
}

fn assert_slots_show(app: &mut App, want: &[DiffRow]) {
    let source = doc(app).source().clone();
    let files = source.files().collect::<Vec<_>>();
    let built = slots(app);
    assert!(!built.is_empty(), "rows are built");
    for (index, root) in built {
        let row = doc(app).row(index);
        assert_eq!(row, want.get(index).copied(), "row {index}");
        let Some(DiffRow::Body { file, row }) = row else {
            continue;
        };
        if matches!(row, Row::Gap { .. }) {
            continue;
        }
        let shown = texts(app, root);
        let diff = files[file];
        let number = |n: Option<u32>| n.map(|n| n.to_string());
        let numbers = shown[0].split_whitespace().map(str::to_owned);
        let want_numbers = [
            number(row.old_line().map(|l| l.number.get())),
            number(row.new_line().map(|l| l.number.get())),
        ]
        .into_iter()
        .flatten();
        assert!(numbers.eq(want_numbers), "row {index}: {shown:?}");
        let text = match row {
            Row::Deletion { .. } => old_text(diff, row),
            _ => new_text(diff, row),
        };
        assert_eq!(Some(&shown[1]), text.as_ref(), "row {index}");
    }
}

#[test]
fn spawned_rows_match_the_rows_of_each_file() {
    let source =
        DiffSource::patch(LARGE_PATCH).expect("the patch parses");
    let want = expected(&source, Layout::Stacked, &[]);
    let mut app = showing(source);
    assert_eq!(doc(&mut app).len(), want.len());
    assert_slots_show(&mut app, &want);

    scroll_to(&mut app, 5_000);
    assert_eq!(first_visible(&mut app), 5_000);
    let built = slots(&mut app);
    assert!(built.iter().any(|&(index, _)| index == 5_000));
    assert!(built.len() < 200, "{} slots", built.len());
    assert_slots_show(&mut app, &want);
}

fn gap_slot(app: &mut App, hunk: u32) -> Entity {
    let index = (0..doc(app).len())
        .find(|&index| {
            matches!(
                doc(app).row(index),
                Some(DiffRow::Body { row: Row::Gap { hunk: h, .. }, .. })
                    if h == hunk
            )
        })
        .expect("the gap shows");
    slot(app, index)
}

#[test]
fn expanding_a_gap_adds_exactly_the_revealed_rows() {
    let source = gap_pair();
    let mut app = showing(source.clone());
    let before = doc(&mut app).len();
    assert_eq!(before, expected(&source, Layout::Stacked, &[]).len());

    let gap = gap_slot(&mut app, 1);
    press(&mut app, gap, "↑ up");
    let mut expansion = Expansion::new();
    expansion.reveal(1, Side::Up, Expansion::STEP);
    let want =
        expected(&source, Layout::Stacked, &[expansion.clone()]);
    assert_eq!(doc(&mut app).len(), before + 100);
    let rows = (0..want.len())
        .map(|index| doc(&mut app).row(index))
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        want.iter().copied().map(Some).collect::<Vec<_>>()
    );
    assert_slots_show(&mut app, &want);

    let gap = gap_slot(&mut app, 1);
    press(&mut app, gap, "↕ all");
    expansion.reveal(1, Side::All, 0);
    let want = expected(&source, Layout::Stacked, &[expansion]);
    assert_eq!(doc(&mut app).len(), want.len());
    let gaps = (0..want.len())
        .filter_map(|index| doc(&mut app).row(index))
        .filter(|row| {
            matches!(
                row,
                DiffRow::Body {
                    row: Row::Gap { hunk: 1, .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(gaps, 0, "the middle gap shows whole");
    assert_slots_show(&mut app, &want);
}

#[test]
fn collapsing_a_file_hides_its_rows() {
    let source =
        DiffSource::patch(LARGE_PATCH).expect("the patch parses");
    let mut app = showing(source.clone());
    let total = doc(&mut app).len();
    let first = source.files().next().expect("a file");
    let own =
        rubrix_diff::rows(first, Layout::Stacked, &Expansion::new())
            .len() as usize;

    let header = slot(&mut app, 0);
    let name = texts(&app, header)[0].clone();
    assert!(name.starts_with("▾ "), "{name}");
    let label = name.clone();
    press(&mut app, header, &label);
    assert!(doc(&mut app).is_collapsed(0));
    assert_eq!(doc(&mut app).len(), total - own);
    assert_eq!(
        doc(&mut app).row(1),
        Some(DiffRow::Header { file: 1 })
    );
    let header = slot(&mut app, 0);
    assert!(texts(&app, header)[0].starts_with("▸ "));

    let label = texts(&app, header)[0].clone();
    press(&mut app, header, &label);
    assert_eq!(doc(&mut app).len(), total);
}

#[test]
fn a_patch_source_has_no_expand_buttons() {
    let source =
        DiffSource::patch(LARGE_PATCH).expect("the patch parses");
    let mut app = showing(source);
    let gaps = slots(&mut app)
        .into_iter()
        .filter(|&(index, _)| {
            matches!(
                doc(&mut app).row(index),
                Some(DiffRow::Body {
                    row: Row::Gap { .. },
                    ..
                })
            )
        })
        .map(|(_, root)| root)
        .collect::<Vec<_>>();
    assert!(!gaps.is_empty(), "a gap row is built");
    for gap in gaps {
        let buttons = buttons(&mut app, gap);
        assert_eq!(buttons.len(), 3);
        assert!(
            buttons.iter().all(|&display| display == Display::None)
        );
    }

    let mut app = showing(gap_pair());
    let gap = gap_slot(&mut app, 1);
    assert_eq!(buttons(&mut app, gap), [Display::Flex; 3]);
}

fn buttons(app: &mut App, root: Entity) -> Vec<Display> {
    let world = app.world_mut();
    let mut found =
        world.query_filtered::<(Entity, &Node), With<Button>>();
    found
        .iter(world)
        .filter(|&(button, _)| {
            let mut at = button;
            loop {
                if at == root {
                    return true;
                }
                match world.get::<ChildOf>(at) {
                    Some(parent) => at = parent.parent(),
                    None => return false,
                }
            }
        })
        .map(|(_, node)| node.display)
        .collect()
}

#[test]
fn a_limit_shows_n_rows_and_one_button_row() {
    let old =
        (1..=20).map(|n| format!("old {n}\n")).collect::<String>();
    let new =
        (1..=20).map(|n| format!("new {n}\n")).collect::<String>();
    let source = DiffSource::texts("forty.txt", &old, &new);
    let mut app = showing(source);
    set(&mut app, |showing| {
        showing.compact = true;
        showing.limit = Some(16);
    });
    assert_eq!(doc(&mut app).len(), 1 + 16 + 1);
    assert_eq!(
        doc(&mut app).row(17),
        Some(DiffRow::More { lines: 24 })
    );
    assert_eq!(doc(&mut app).row(18), None);
    let more = slot(&mut app, 17);
    assert_eq!(texts(&app, more), ["... 24 more lines"]);
    let first = slot(&mut app, 1);
    assert_eq!(texts(&app, first), ["- old 1"]);

    let more = slot(&mut app, 17);
    press(&mut app, more, "... 24 more lines");
    assert_eq!(doc(&mut app).len(), 1 + 40);
    let added = slot(&mut app, 21);
    assert_eq!(texts(&app, added), ["+ new 1"]);
}

#[test]
fn each_patch_hunk_is_highlighted_from_the_start() {
    let patch = "\
diff --git a/lib.rs b/lib.rs
--- a/lib.rs
+++ b/lib.rs
@@ -1,2 +1,2 @@
 /* a comment the omitted lines close
-let old = 1;
+let new = 1;
@@ -20 +20 @@
-fn old() {}
+fn new() {}
";
    let mut app =
        showing(DiffSource::patch(patch).expect("the patch parses"));
    assert_eq!(
        doc(&mut app).row(4),
        Some(DiffRow::Body {
            file: 0,
            row: Row::Gap {
                hunk: 1,
                hidden: 17,
                up: false,
                down: false
            }
        })
    );
    let comment = slot(&mut app, 2);
    assert_eq!(colour_of(&app, comment, "let"), FAINT);
    let code = slot(&mut app, 5);
    assert_eq!(colour_of(&app, code, "fn"), ACCENT);
}

fn one_change() -> DiffSource {
    DiffSource::texts("letters.txt", "a\nb\nc\n", "a\nB\nc\n")
}

fn select_lines(app: &mut App, from: usize, to: usize) {
    let first = line_text(app, from);
    let at = point_in(app, first, 0);
    press_on(app, first, at, 3);
    let last = line_text(app, to);
    let at = point_in(app, last, 0);
    shift(app, true);
    press_on(app, last, at, 1);
    shift(app, false);
}

#[test]
fn a_compact_copy_takes_each_selected_line_with_its_prefix() {
    let mut app = laid_out(one_change(), 600.0, 400.0);
    set(&mut app, |showing| showing.compact = true);
    select_lines(&mut app, 1, 3);
    assert_eq!(copy(&mut app), "  a\n- b\n+ B");
}

#[test]
fn a_stacked_copy_takes_each_selected_line_without_a_prefix() {
    let mut app = laid_out(one_change(), 600.0, 400.0);
    select_lines(&mut app, 1, 4);
    assert_eq!(copy(&mut app), "a\nb\nB\nc");
}

#[test]
fn a_copy_across_a_header_takes_no_header_text() {
    let mut app =
        laid_out(DiffSource::patch(TWO_FILES).unwrap(), 600.0, 400.0);
    select_lines(&mut app, 2, 4);
    assert_eq!(copy(&mut app), "X\ny");
}

#[test]
fn rows_that_change_drop_the_selection() {
    let mut app = laid_out(one_change(), 600.0, 400.0);
    select_lines(&mut app, 1, 3);
    set(&mut app, |showing| showing.compact = true);
    assert_eq!(copy(&mut app), "");
}

const TWO_FILES: &str = "\
diff --git a/one.txt b/one.txt
--- a/one.txt
+++ b/one.txt
@@ -1 +1 @@
-x
+X
diff --git a/two.txt b/two.txt
--- a/two.txt
+++ b/two.txt
@@ -1 +1 @@
-y
+Y
";
