use rubrix_edit::{
    Buffer, Monospace, Selection, Selections, column, toggle_caret,
};

fn range(anchor: usize, head: usize) -> Selection {
    Selection { anchor, head }
}

fn carets(offsets: &[usize]) -> Selections {
    let mut selections =
        Selections::new(Selection::caret(offsets[0]));
    for &at in &offsets[1..] {
        selections.add(Selection::caret(at));
    }
    selections
}

fn listed(selections: &Selections) -> Vec<Selection> {
    selections.iter().collect()
}

#[test]
fn toggle_caret_adds_a_caret_as_the_primary() {
    let toggled = toggle_caret(&carets(&[2]), 9);
    assert_eq!(
        listed(&toggled),
        [Selection::caret(2), Selection::caret(9)]
    );
    assert_eq!(toggled.primary(), Selection::caret(9));
}

#[test]
fn toggle_caret_removes_the_caret_it_lands_on() {
    let toggled = toggle_caret(&carets(&[2, 9, 14]), 9);
    assert_eq!(
        listed(&toggled),
        [Selection::caret(2), Selection::caret(14)]
    );
    assert_eq!(toggled.primary(), Selection::caret(14));
}

#[test]
fn toggle_caret_removes_a_range_that_holds_the_click() {
    let mut selections = Selections::new(range(4, 8));
    selections.add(Selection::caret(20));
    for at in [4, 6, 8] {
        let toggled = toggle_caret(&selections, at);
        assert_eq!(
            listed(&toggled),
            [Selection::caret(20)],
            "at {at}"
        );
    }
}

#[test]
fn toggle_caret_keeps_the_last_selection() {
    let one = carets(&[5]);
    assert_eq!(toggle_caret(&one, 5), one);
    let range = Selections::new(range(2, 7));
    assert_eq!(toggle_caret(&range, 4), range);
}

const FIVE: &str = "abcdef\nabcdef\nab\nabcdef\nabcdef";

#[test]
fn column_selects_one_range_per_line_clamped_at_a_short_line() {
    let buffer = Buffer::new(FIVE);
    let selections =
        column(&buffer, &Monospace::default(), (0, 1.0), (4, 4.0));
    assert_eq!(
        listed(&selections),
        [
            range(1, 4),
            range(8, 11),
            range(15, 16),
            range(18, 21),
            range(25, 28),
        ]
    );
    assert_eq!(selections.primary(), range(25, 28));
}

#[test]
fn column_dragged_up_and_left_puts_the_primary_on_the_top_line() {
    let buffer = Buffer::new(FIVE);
    let selections =
        column(&buffer, &Monospace::default(), (3, 5.0), (1, 2.0));
    assert_eq!(
        listed(&selections),
        [range(12, 9), range(16, 16), range(22, 19)]
    );
    assert_eq!(selections.primary(), range(12, 9));
}

#[test]
fn column_snaps_to_grapheme_boundaries() {
    let buffer = Buffer::new("e\u{301}xyz\nabcd");
    let selections =
        column(&buffer, &Monospace::default(), (0, 1.0), (1, 2.0));
    assert_eq!(listed(&selections), [range(3, 4), range(8, 9)]);
}
