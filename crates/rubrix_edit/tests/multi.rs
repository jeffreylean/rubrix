use rubrix_edit::{
    Buffer, Command, Editor, History, Monospace, Selection,
    Selections, column, toggle_caret,
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
    let selections = column(
        &buffer,
        &Monospace::default(),
        (0, 1.0),
        (buffer.line_start(4), 4.0),
    );
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
    let selections = column(
        &buffer,
        &Monospace::default(),
        (buffer.line_start(3), 5.0),
        (buffer.line_start(1), 2.0),
    );
    assert_eq!(
        listed(&selections),
        [range(12, 9), range(16, 16), range(22, 19)]
    );
    assert_eq!(selections.primary(), range(12, 9));
}

#[test]
fn column_snaps_to_grapheme_boundaries() {
    let buffer = Buffer::new("e\u{301}xyz\nabcd");
    let selections = column(
        &buffer,
        &Monospace::default(),
        (0, 1.0),
        (buffer.line_start(1), 2.0),
    );
    assert_eq!(listed(&selections), [range(3, 4), range(8, 9)]);
}

fn ranges(list: &[(usize, usize)]) -> Selections {
    let mut selections = Selections::new(range(list[0].0, list[0].1));
    for &(anchor, head) in &list[1..] {
        selections.add(range(anchor, head));
    }
    selections
}

/// The selections after one mod+D on `selections` in `text`.
fn next_occurrence(text: &str, selections: Selections) -> Selections {
    let mut editor = Editor::from_parts(
        Buffer::new(text),
        selections,
        History::new(),
    );
    let change = editor
        .apply(Command::AddNextOccurrence, &Monospace::default());
    assert!(change.is_none(), "mod+D changed the text");
    editor.selections().clone()
}

#[test]
fn next_occurrence_first_selects_the_word_at_each_caret() {
    let text = "let a; let b;\nfoo(bar)";
    let selections = next_occurrence(text, carets(&[1, 17, 19]));
    assert_eq!(
        listed(&selections),
        [range(0, 3), range(14, 17), range(18, 21)]
    );
    assert_eq!(selections.primary(), range(18, 21));
}

#[test]
fn next_occurrence_leaves_a_caret_away_from_words() {
    let selections = carets(&[3]);
    assert_eq!(
        next_occurrence("a  ;  b", selections.clone()),
        selections
    );
}

#[test]
fn next_occurrence_adds_the_next_match_as_the_primary() {
    let selections =
        next_occurrence("let x; let y; let z;", ranges(&[(0, 3)]));
    assert_eq!(listed(&selections), [range(0, 3), range(7, 10)]);
    assert_eq!(selections.primary(), range(7, 10));
}

#[test]
fn next_occurrence_skips_a_match_already_selected() {
    let selections =
        next_occurrence("ab ab ab", ranges(&[(3, 5), (0, 2)]));
    assert_eq!(
        listed(&selections),
        [range(0, 2), range(3, 5), range(6, 8)]
    );
    assert_eq!(selections.primary(), range(6, 8));
}

#[test]
fn next_occurrence_wraps_past_the_end_and_stops_when_all_are_taken() {
    let text = "ab ab\nab";
    let wrapped = next_occurrence(text, ranges(&[(3, 5), (6, 8)]));
    assert_eq!(
        listed(&wrapped),
        [range(0, 2), range(3, 5), range(6, 8)]
    );
    assert_eq!(wrapped.primary(), range(0, 2));
    assert_eq!(next_occurrence(text, wrapped.clone()), wrapped);
}

#[test]
fn next_occurrence_finds_a_selection_that_spans_lines() {
    let selections =
        next_occurrence("a\r\nb a\r\nb", ranges(&[(0, 4)]));
    assert_eq!(listed(&selections), [range(0, 4), range(5, 9)]);
}

#[test]
fn next_occurrence_does_nothing_when_the_texts_differ() {
    let selections = ranges(&[(0, 2), (3, 5)]);
    assert_eq!(
        next_occurrence("ab cd ab cd", selections.clone()),
        selections
    );
}

fn escape(selections: Selections) -> Vec<Selection> {
    let mut editor = Editor::from_parts(
        Buffer::new("abcd efgh"),
        selections,
        History::new(),
    );
    let _ = editor
        .apply(Command::CollapseToPrimary, &Monospace::default());
    listed(editor.selections())
}

#[test]
fn escape_keeps_only_the_primary_of_several_carets() {
    assert_eq!(escape(carets(&[1, 7, 3])), [Selection::caret(3)]);
}

#[test]
fn escape_keeps_the_primary_range_of_several_selections() {
    let mut selections = Selections::new(Selection::caret(0));
    selections.add(range(8, 5));
    assert_eq!(escape(selections), [range(8, 5)]);
}

#[test]
fn escape_turns_one_range_into_a_caret_at_its_head() {
    assert_eq!(
        escape(Selections::new(range(8, 5))),
        [Selection::caret(5)]
    );
}
