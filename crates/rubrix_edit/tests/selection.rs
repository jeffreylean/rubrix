use rubrix_edit::{
    Affinity, Assoc, Buffer, Command, Editor, History, Monospace,
    Selection, Selections,
};

mod common;

use common::{Rng, seed};

fn range(anchor: usize, head: usize) -> Selection {
    Selection { anchor, head }
}

fn all(selections: &Selections) -> Vec<Selection> {
    selections.iter().collect()
}

fn editor(text: &str, selections: Selections) -> Editor {
    Editor::from_parts(Buffer::new(text), selections, History::new())
}

#[test]
fn overlapping_selections_merge_into_the_newest_slot() {
    let mut selections = Selections::new(range(0, 2));
    selections.add(range(10, 14));
    selections.add(range(20, 12));
    assert_eq!(all(&selections), [range(0, 2), range(20, 10)]);
    assert_eq!(selections.primary(), range(20, 10));

    let mut selections = Selections::new(range(10, 14));
    selections.add(range(20, 12));
    selections.add(range(30, 31));
    selections.add(Selection::caret(16));
    assert_eq!(all(&selections), [range(10, 20), range(30, 31)]);
    assert_eq!(selections.primary(), range(10, 20));
}

#[test]
fn an_added_head_keeps_its_affinity_unless_a_merge_moves_it() {
    let mut selections = Selections::new(Selection::caret(2));
    selections
        .add_with_affinity(Selection::caret(12), Affinity::Upstream);
    assert_eq!(selections.affinity(0), Affinity::Downstream);
    assert_eq!(selections.affinity(1), Affinity::Upstream);
    selections.add_with_affinity(range(5, 12), Affinity::Upstream);
    assert_eq!(all(&selections), [Selection::caret(2), range(5, 12)]);
    assert_eq!(
        selections.affinity(1),
        Affinity::Upstream,
        "the union's head is the added head"
    );
    let mut selections = Selections::new(range(10, 14));
    selections
        .add_with_affinity(Selection::caret(12), Affinity::Upstream);
    assert_eq!(all(&selections), [range(10, 14)]);
    assert_eq!(
        selections.affinity(0),
        Affinity::Downstream,
        "the union's head is not the added head"
    );
}

#[test]
fn a_merged_caret_points_away_from_the_union_start() {
    let mut selections = Selections::new(range(4, 8));
    selections.add(Selection::caret(4));
    assert_eq!(all(&selections), [range(8, 4)]);
    let mut selections = Selections::new(range(4, 8));
    selections.add(Selection::caret(8));
    assert_eq!(all(&selections), [range(4, 8)]);
}

#[test]
fn a_merge_of_older_selections_keeps_the_primary() {
    let mut selections = Selections::new(Selection::caret(1));
    selections.add(Selection::caret(2));
    selections.add(Selection::caret(10));
    let mut editor = editor("abcdefghijkl", selections);
    let _ = editor.apply(Command::Backspace, &Monospace::default());
    assert_eq!(editor.buffer().to_string(), "cdefghikl");
    let selections = editor.selections();
    assert_eq!(
        all(selections),
        [Selection::caret(0), Selection::caret(7)]
    );
    assert_eq!(selections.primary(), Selection::caret(7));
    assert_eq!(selections.primary_index(), 1);
}

#[test]
fn touching_ranges_stay_apart_and_typing_replaces_both() {
    let mut selections = Selections::new(range(0, 3));
    selections.add(range(3, 6));
    assert_eq!(selections.len(), 2);
    let mut editor = editor("abcdefgh", selections);
    let change =
        editor.apply(Command::Type("x"), &Monospace::default());
    assert_eq!(change.map(|change| change.len()), Some(2));
    assert_eq!(editor.buffer().to_string(), "xxgh");
    assert_eq!(
        all(editor.selections()),
        [Selection::caret(1), Selection::caret(2)]
    );
}

#[test]
fn the_primary_is_the_last_added() {
    let mut selections = Selections::new(Selection::caret(5));
    for at in [9, 1, 7] {
        selections.add(Selection::caret(at));
        assert_eq!(selections.primary(), Selection::caret(at));
    }
    assert_eq!(selections.primary_index(), 2);
    assert_eq!(
        all(&selections).iter().map(|s| s.head).collect::<Vec<_>>(),
        [1, 5, 7, 9]
    );
}

#[test]
fn carets_that_come_to_share_an_offset_merge() {
    let mut selections = Selections::new(Selection::caret(1));
    selections.add(Selection::caret(2));
    let mut editor = editor("abc", selections);
    let layout = Monospace::default();
    let _ = editor.apply(Command::Backspace, &layout);
    assert_eq!(editor.buffer().to_string(), "c");
    assert_eq!(all(editor.selections()), [Selection::caret(0)]);
    let _ = editor.apply(Command::Type("y"), &layout);
    assert_eq!(editor.buffer().to_string(), "yc");
}

#[test]
fn edits_move_the_selections() {
    let mut selections = Selections::new(Selection::caret(1));
    selections.add(range(4, 6));
    let mut editor = editor("ab\ncdef", selections);
    let change = editor
        .apply(Command::Type("XY"), &Monospace::default())
        .unwrap();
    assert_eq!(editor.buffer().to_string(), "aXYb\ncXYf");
    assert_eq!(
        all(editor.selections()),
        [Selection::caret(3), Selection::caret(8)]
    );
    assert_eq!(change.map(0, Assoc::After), 0);
    assert_eq!(change.map(2, Assoc::After), 4);
    assert_eq!(change.map(7, Assoc::After), 9);
}

#[test]
fn an_insert_at_an_offset_pushes_it_right_only_with_after() {
    let mut buffer = Buffer::new("hello");
    let change = buffer.replace([(2..2, "XY")]);
    assert_eq!(change.map(2, Assoc::After), 4);
    assert_eq!(change.map(2, Assoc::Before), 2);
    assert_eq!(change.map(1, Assoc::After), 1);
    assert_eq!(change.map(3, Assoc::Before), 5);
}

fn overlaps(a: Selection, b: Selection) -> bool {
    if a.is_caret() || b.is_caret() {
        a.start() <= b.end() && b.start() <= a.end()
    } else {
        a.start() < b.end() && b.start() < a.end()
    }
}

#[test]
fn random_additions_stay_sorted_and_apart() {
    let seed = seed();
    let mut rng = Rng(seed ^ (17 << 32));
    for case in 0..200 {
        let mut selections = Selections::new(Selection::caret(0));
        for _ in 0..1 + rng.below(30) {
            let anchor = rng.below(100) as usize;
            let head = if rng.chance(50) {
                anchor
            } else {
                rng.below(100) as usize
            };
            let added = range(anchor, head);
            selections.add(added);
            let context = format!("seed {seed} case {case}");
            let primary = selections.primary();
            assert!(
                primary.start() <= added.start()
                    && added.end() <= primary.end(),
                "{context}: the primary holds the newest"
            );
            let list = all(&selections);
            for pair in list.windows(2) {
                assert!(
                    pair[0].start() <= pair[1].start()
                        && !overlaps(pair[0], pair[1]),
                    "{context}: {list:?}"
                );
            }
        }
    }
}
