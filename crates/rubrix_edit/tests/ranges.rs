use core::ops::Range;

use rubrix_edit::{
    Assoc, Buffer, Change, Command, Editor, History, LineAnchors,
    Monospace, Ranges, Selection, Selections,
};

const TEXT: &str = "zero\none\ntwo two\nthree\nfour\n";

fn at(selection: Selection) -> Editor {
    Editor::from_parts(
        Buffer::new(TEXT),
        Selections::new(selection),
        History::new(),
    )
}

fn caret(offset: usize) -> Editor {
    at(Selection::caret(offset))
}

fn apply(editor: &mut Editor, command: Command<'_>) -> Change {
    editor
        .apply(command, &Monospace::default())
        .expect("the command changes the text")
}

/// `two two` on line 2, as the bytes of its second word.
fn second_two() -> Range<usize> {
    let start = TEXT.find("two two").expect("line 2") + 4;
    start..start + 3
}

fn ranged(range: Range<usize>) -> Ranges<&'static str> {
    Ranges::new([(range, "marker")])
}

fn only(ranges: &Ranges<&str>) -> Option<Range<usize>> {
    assert!(ranges.len() <= 1);
    ranges.iter().next().map(|(range, _)| range)
}

fn on_line(line: u32) -> (Editor, LineAnchors<&'static str>) {
    let editor = caret(0);
    let anchors = LineAnchors::new(editor.buffer(), [(line, "note")]);
    (editor, anchors)
}

fn lines<T>(editor: &Editor, anchors: &LineAnchors<T>) -> Vec<u32> {
    anchors
        .lines(editor.buffer())
        .map(|(line, _)| line)
        .collect()
}

#[test]
fn a_range_moves_through_an_insert_before_it() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let change =
        apply(&mut caret(word.start - 2), Command::Type("ab"));
    ranges.map(&change);
    assert_eq!(only(&ranges), Some(word.start + 2..word.end + 2));
}

#[test]
fn a_range_keeps_text_typed_at_its_start_outside() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let change = apply(&mut caret(word.start), Command::Type("ab"));
    ranges.map(&change);
    assert_eq!(only(&ranges), Some(word.start + 2..word.end + 2));
}

#[test]
fn a_range_widens_for_text_typed_inside_it() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let change =
        apply(&mut caret(word.start + 1), Command::Type("ab"));
    ranges.map(&change);
    assert_eq!(only(&ranges), Some(word.start..word.end + 2));
}

#[test]
fn a_range_keeps_text_typed_at_its_end_and_after_it_outside() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let mut editor = caret(word.end);
    ranges.map(&apply(&mut editor, Command::Type("ab")));
    ranges.map(&apply(&mut editor, Command::Enter));
    assert_eq!(only(&ranges), Some(word));
}

#[test]
fn a_range_drops_when_a_delete_covers_it() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let mut editor = at(Selection {
        anchor: word.start - 1,
        head: word.end + 2,
    });
    ranges.map(&apply(&mut editor, Command::Backspace));
    assert_eq!(only(&ranges), None);
}

#[test]
fn a_range_shrinks_when_a_delete_covers_part_of_it() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let mut editor = at(Selection {
        anchor: word.start + 1,
        head: word.end + 3,
    });
    ranges.map(&apply(&mut editor, Command::Delete));
    assert_eq!(only(&ranges), Some(word.start..word.start + 1));
}

#[test]
fn a_range_comes_back_to_its_bytes_through_an_undo() {
    let word = second_two();
    let mut ranges = ranged(word.clone());
    let mut editor = caret(TEXT.find("one").expect("line 1"));
    ranges.map(&apply(&mut editor, Command::Enter));
    ranges.map(&apply(&mut editor, Command::Type("abc")));
    assert_eq!(only(&ranges), Some(word.start + 4..word.end + 4));
    ranges.map(&apply(&mut editor, Command::Undo));
    ranges.map(&apply(&mut editor, Command::Undo));
    assert_eq!(only(&ranges), Some(word));
}

#[test]
fn a_line_moves_down_through_an_enter_before_it() {
    let (mut editor, mut anchors) = on_line(3);
    editor.set_selections(Selections::new(Selection::caret(2)));
    anchors.map(&apply(&mut editor, Command::Enter));
    assert_eq!(lines(&editor, &anchors), [4]);
}

#[test]
fn a_line_moves_down_with_its_text_through_an_enter_at_its_start() {
    let (mut editor, mut anchors) = on_line(3);
    let start = editor.buffer().line_start(3);
    editor.set_selections(Selections::new(Selection::caret(start)));
    anchors.map(&apply(&mut editor, Command::Enter));
    assert_eq!(lines(&editor, &anchors), [4]);
    assert_eq!(
        editor.buffer().slice(
            editor.buffer().line_start(4)
                ..editor.buffer().line_end(4)
        ),
        "three"
    );
}

#[test]
fn a_line_stays_through_typing_at_its_start_inside_it_and_an_enter_after_it()
 {
    let (mut editor, mut anchors) = on_line(3);
    let start = editor.buffer().line_start(3);
    editor.set_selections(Selections::new(Selection::caret(start)));
    anchors.map(&apply(&mut editor, Command::Type("ab")));
    anchors.map(&apply(&mut editor, Command::Type("\tc")));
    assert_eq!(lines(&editor, &anchors), [3]);
    let end = editor.buffer().line_end(3);
    editor.set_selections(Selections::new(Selection::caret(end)));
    anchors.map(&apply(&mut editor, Command::Enter));
    anchors.map(&apply(&mut editor, Command::Type("after")));
    assert_eq!(lines(&editor, &anchors), [3]);
}

#[test]
fn a_line_moves_up_onto_the_line_it_joins() {
    let (mut editor, mut anchors) = on_line(3);
    let start = editor.buffer().line_start(3);
    editor.set_selections(Selections::new(Selection::caret(start)));
    anchors.map(&apply(&mut editor, Command::Backspace));
    assert_eq!(lines(&editor, &anchors), [2]);
}

#[test]
fn a_line_drops_when_a_delete_covers_it_and_an_undo_brings_it_back() {
    let (mut editor, mut anchors) = on_line(3);
    let buffer = editor.buffer();
    editor.set_selections(Selections::new(Selection {
        anchor: buffer.line_start(2),
        head: buffer.line_start(4),
    }));
    anchors.map(&apply(&mut editor, Command::Backspace));
    assert!(anchors.is_empty(), "a deleted line drops its payload");
    anchors.map(&apply(&mut editor, Command::Undo));
    assert_eq!(lines(&editor, &anchors), [3]);
    anchors.map(&apply(&mut editor, Command::Redo));
    assert!(anchors.is_empty(), "the redo deletes it again");
    anchors.map(&apply(&mut editor, Command::Undo));
    assert_eq!(lines(&editor, &anchors), [3]);
}

#[test]
fn a_line_comes_back_through_the_undo_of_a_joined_backspace_run() {
    let (mut editor, mut anchors) = on_line(3);
    let buffer = editor.buffer();
    editor.set_selections(Selections::new(Selection {
        anchor: buffer.line_start(2) + 3,
        head: buffer.line_start(4),
    }));
    anchors.map(&apply(&mut editor, Command::Backspace));
    anchors.map(&apply(&mut editor, Command::Backspace));
    assert!(anchors.is_empty());
    assert_eq!(
        editor.history().undo_depth(),
        1,
        "the run is one step"
    );
    anchors.map(&apply(&mut editor, Command::Undo));
    assert_eq!(lines(&editor, &anchors), [3]);
    assert_eq!(editor.buffer().to_string(), TEXT);
}

#[test]
fn a_line_comes_back_after_the_undo_of_a_later_edit() {
    let (mut editor, mut anchors) = on_line(3);
    let buffer = editor.buffer();
    editor.set_selections(Selections::new(Selection {
        anchor: buffer.line_start(3),
        head: buffer.line_start(5),
    }));
    anchors.map(&apply(&mut editor, Command::Cut));
    anchors.map(&apply(&mut editor, Command::Type("z")));
    anchors.map(&apply(&mut editor, Command::Undo));
    anchors.map(&apply(&mut editor, Command::Undo));
    assert_eq!(lines(&editor, &anchors), [3]);
}

#[test]
fn every_line_of_one_change_comes_back_past_the_drop_limit() {
    let count = 2 * (LineAnchors::<()>::MAX_DROPPED + 20);
    let text: String =
        (0..count).map(|n| format!("line {n}\n")).collect();
    let buffer = Buffer::new(&text);
    let even: Vec<u32> = (0..count as u32).step_by(2).collect();
    let mut selections = Selections::new(Selection {
        anchor: buffer.line_start(0),
        head: buffer.line_start(1),
    });
    for &line in &even[1..] {
        selections.add(Selection {
            anchor: buffer.line_start(line),
            head: buffer.line_start(line + 1),
        });
    }
    let mut anchors = LineAnchors::new(
        &buffer,
        even.iter().map(|&line| (line, line)),
    );
    let mut editor =
        Editor::from_parts(buffer, selections, History::new());
    anchors.map(&apply(&mut editor, Command::Backspace));
    assert!(anchors.is_empty(), "each selected line is deleted");
    anchors.map(&apply(&mut editor, Command::Undo));
    assert_eq!(editor.buffer().to_string(), text);
    assert_eq!(lines(&editor, &anchors), even);
}

fn swapped(line: u32, down: bool) -> [(u32, u32); 2] {
    let other = if down { line + 1 } else { line - 1 };
    [(line, other), (other, line)]
}

fn moved(
    text: &str,
    line: u32,
    down: bool,
) -> Option<(Editor, Change)> {
    let mut editor = Editor::from_parts(
        Buffer::new(text),
        Selections::new(Selection::caret(
            Buffer::new(text).line_start(line),
        )),
        History::new(),
    );
    let change = editor
        .apply(Command::MoveLines { down }, &Monospace::default())?;
    Some((editor, change))
}

#[test]
fn a_line_move_takes_every_place_on_both_lines_with_its_line() {
    for text in [TEXT, "zero\none\n\ntwo", "a\r\nbc\r\n"] {
        let before = Buffer::new(text);
        for line in 0..before.line_count() {
            for down in [true, false] {
                let Some((mut editor, change)) =
                    moved(text, line, down)
                else {
                    continue;
                };
                let after = editor.buffer().snapshot();
                let undo = apply(&mut editor, Command::Undo);
                let redo = apply(&mut editor, Command::Redo);
                for (from, to) in swapped(line, down) {
                    let (start, end) = (
                        before.line_start(from),
                        before.line_end(from),
                    );
                    for offset in start..=end {
                        let landed =
                            after.line_start(to) + offset - start;
                        let case = format!(
                            "{text:?} line {line} down {down} \
                             offset {offset}"
                        );
                        assert_eq!(
                            change.map(offset, Assoc::After),
                            landed,
                            "{case} after"
                        );
                        if offset > start {
                            assert_eq!(
                                change.map(offset, Assoc::Before),
                                landed,
                                "{case} before"
                            );
                        }
                        assert_eq!(
                            undo.map(landed, Assoc::After),
                            offset,
                            "{case} undo"
                        );
                        assert_eq!(
                            redo.map(offset, Assoc::After),
                            landed,
                            "{case} redo"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_line_move_keeps_markers_on_both_lines_and_undo_restores_them() {
    let (one, two) = (TEXT.find("one").unwrap(), second_two());
    let markers =
        || Ranges::new([(one..one + 3, "one"), (two.clone(), "two")]);
    let (mut editor, change) = moved(TEXT, 1, true).unwrap();
    let mut ranges = markers();
    ranges.map(&change);
    let after: Vec<_> =
        ranges.iter().map(|(range, &name)| (name, range)).collect();
    let moved_two = TEXT.find("two two").unwrap() - 4;
    assert_eq!(
        after,
        [
            ("two", moved_two + 4..moved_two + 7),
            ("one", moved_two + 8..moved_two + 11)
        ]
    );
    ranges.map(&apply(&mut editor, Command::Undo));
    assert_eq!(ranges, markers());
}

#[test]
fn a_line_move_takes_the_annotations_of_both_lines_and_undo_restores_them()
 {
    for (line, down) in [(1, true), (2, false), (4, true)] {
        let mut editor = moved(TEXT, line, down).unwrap().0;
        apply(&mut editor, Command::Undo);
        let mut anchors = LineAnchors::new(
            editor.buffer(),
            swapped(line, down).map(|(from, _)| (from, from)),
        );
        let change = apply(&mut editor, Command::Redo);
        anchors.map(&change);
        let mut landed: Vec<(u32, u32)> = anchors
            .lines(editor.buffer())
            .map(|(at, &from)| (from, at))
            .collect();
        landed.sort();
        let mut expected = swapped(line, down).to_vec();
        expected.sort();
        assert_eq!(landed, expected, "line {line} down {down}");
        anchors.map(&apply(&mut editor, Command::Undo));
        let mut back: Vec<(u32, u32)> = anchors
            .lines(editor.buffer())
            .map(|(at, &from)| (from, at))
            .collect();
        back.sort();
        assert_eq!(
            back,
            expected
                .iter()
                .map(|&(from, _)| (from, from))
                .collect::<Vec<_>>(),
            "undo of line {line} down {down}"
        );
    }
}
