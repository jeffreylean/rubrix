use rubrix_edit::{
    Buffer, Command, Editor, History, Monospace, Selection,
    Selections,
};

fn editor(text: &str, selections: &[Selection]) -> Editor {
    let mut all = Selections::new(selections[0]);
    for &selection in &selections[1..] {
        all.add(selection);
    }
    Editor::from_parts(Buffer::new(text), all, History::new())
}

fn carets(text: &str, at: &[usize]) -> Editor {
    let at: Vec<Selection> =
        at.iter().map(|&at| Selection::caret(at)).collect();
    editor(text, &at)
}

fn range(anchor: usize, head: usize) -> Selection {
    Selection { anchor, head }
}

fn apply(editor: &mut Editor, command: Command<'_>) -> bool {
    editor.apply(command, &Monospace::default()).is_some()
}

fn text(editor: &Editor) -> String {
    editor.buffer().to_string()
}

fn selections(editor: &Editor) -> Vec<Selection> {
    editor.selections().iter().collect()
}

fn caret_offsets(editor: &Editor) -> Vec<usize> {
    editor
        .selections()
        .iter()
        .map(|selection| {
            assert!(selection.is_caret(), "{selection:?} is a range");
            selection.head
        })
        .collect()
}

/// Runs `command` and checks it made one undo step, which restores
/// the text and the selections. Leaves the command's result.
fn run_undoable(editor: &mut Editor, command: Command<'_>) {
    let (before_text, before) = (text(editor), selections(editor));
    let depth = editor.history().undo_depth();
    assert!(apply(editor, command), "{command:?} changed nothing");
    let (after_text, after) = (text(editor), selections(editor));
    assert_eq!(editor.history().undo_depth(), depth + 1);
    apply(editor, Command::Undo);
    assert_eq!(text(editor), before_text, "undo of {command:?}");
    assert_eq!(selections(editor), before, "undo of {command:?}");
    apply(editor, Command::Redo);
    assert_eq!(text(editor), after_text);
    assert_eq!(selections(editor), after);
}

#[test]
fn tab_at_a_caret_on_a_tab_led_line_inserts_a_tab() {
    let mut editor = carets("\tfoo", &[1]);
    run_undoable(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "\t\tfoo");
    assert_eq!(caret_offsets(&editor), [2]);
}

#[test]
fn tab_at_a_caret_on_a_space_led_line_fills_to_the_next_stop() {
    let mut editor = carets("  foo", &[2]);
    run_undoable(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "    foo");
    assert_eq!(caret_offsets(&editor), [4]);
}

#[test]
fn a_tab_before_the_caret_counts_to_its_stop() {
    let mut editor = carets("ab\tcd", &[4]);
    apply(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "ab\tc   d", "column 5 fills to 8");
    let mut editor = carets("ab\tcd", &[3]);
    apply(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "ab\t    cd", "column 4 fills to 8");
}

#[test]
fn tab_replaces_a_one_line_range() {
    let mut editor = editor("abcd", &[range(1, 3)]);
    apply(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "a   d");
    assert_eq!(caret_offsets(&editor), [4]);
}

#[test]
fn carets_on_one_line_each_get_their_own_indent() {
    let mut editor = carets("ab", &[0, 1]);
    run_undoable(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "    a   b");
    assert_eq!(caret_offsets(&editor), [4, 8]);
}

#[test]
fn tab_over_several_lines_indents_each_and_skips_blank_lines() {
    let mut editor = editor("a\n\n\tb\n  c\nd", &[range(0, 10)]);
    run_undoable(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "    a\n\n\t\tb\n      c\nd");
    assert_eq!(selections(&editor), [range(4, 19)]);
}

#[test]
fn a_line_shared_by_two_selections_indents_once() {
    let mut editor =
        editor("ab\ncd\nef", &[range(0, 4), range(5, 7)]);
    apply(&mut editor, Command::Indent);
    assert_eq!(text(&editor), "    ab\n    cd\n    ef");
}

#[test]
fn indent_lines_indents_a_caret_line_at_its_start() {
    let mut editor = carets("ab\n", &[1]);
    run_undoable(&mut editor, Command::IndentLines);
    assert_eq!(text(&editor), "    ab\n");
    assert_eq!(caret_offsets(&editor), [5]);
    let mut editor = carets("ab\n", &[3]);
    apply(&mut editor, Command::IndentLines);
    assert_eq!(text(&editor), "ab\n    ", "a caret's blank line too");
}

#[test]
fn outdent_removes_a_tab_or_up_to_four_spaces() {
    let mut editor =
        editor("\t\ta\n  b\n      c\nd", &[range(0, 16)]);
    run_undoable(&mut editor, Command::Outdent);
    assert_eq!(text(&editor), "\ta\nb\n  c\nd");
    let mut editor = carets("  x", &[3]);
    apply(&mut editor, Command::Outdent);
    assert_eq!(text(&editor), "x");
    assert_eq!(caret_offsets(&editor), [1]);
}

#[test]
fn outdent_with_nothing_to_remove_changes_nothing() {
    let mut editor = carets("a\nb", &[0, 2]);
    assert!(!apply(&mut editor, Command::Outdent));
    assert_eq!(editor.history().undo_depth(), 0);
}

#[test]
fn backspace_in_leading_spaces_goes_back_to_the_previous_stop() {
    for (line, at, left) in [
        ("      x", 6, "    x"),
        ("        x", 8, "    x"),
        ("   x", 3, "x"),
    ] {
        let mut editor = carets(line, &[at]);
        run_undoable(&mut editor, Command::Backspace);
        assert_eq!(text(&editor), left, "from column {at}");
        assert_eq!(caret_offsets(&editor), [left.len() - 1]);
    }
}

#[test]
fn backspace_past_a_tab_or_text_deletes_one_character() {
    for (line, at, left) in [
        ("  \tx", 3, "  x"),
        ("ab  x", 4, "ab x"),
        ("\t    x", 5, "\t   x"),
    ] {
        let mut editor = carets(line, &[at]);
        apply(&mut editor, Command::Backspace);
        assert_eq!(text(&editor), left, "{line:?} at {at}");
    }
}

#[test]
fn move_down_and_up_carry_the_selected_lines() {
    let mut editor = editor("a\nbb\nc\nd", &[range(0, 3)]);
    run_undoable(&mut editor, Command::MoveLines { down: true });
    assert_eq!(text(&editor), "c\na\nbb\nd");
    assert_eq!(selections(&editor), [range(2, 5)]);
    run_undoable(&mut editor, Command::MoveLines { down: false });
    assert_eq!(text(&editor), "a\nbb\nc\nd");
    assert_eq!(selections(&editor), [range(0, 3)]);
}

#[test]
fn move_down_onto_the_last_line_drops_its_missing_break() {
    let mut editor = carets("a\nb", &[1]);
    run_undoable(&mut editor, Command::MoveLines { down: true });
    assert_eq!(text(&editor), "b\na");
    assert_eq!(caret_offsets(&editor), [3]);
    run_undoable(&mut editor, Command::MoveLines { down: false });
    assert_eq!(text(&editor), "a\nb");
    assert_eq!(caret_offsets(&editor), [1]);
}

#[test]
fn a_move_with_a_block_at_the_edge_is_refused() {
    let mut editor = carets("a\nb\nc", &[0, 4]);
    assert!(!apply(&mut editor, Command::MoveLines { down: false }));
    assert!(!apply(&mut editor, Command::MoveLines { down: true }));
    assert_eq!(text(&editor), "a\nb\nc");
    assert_eq!(caret_offsets(&editor), [0, 4]);
    assert_eq!(editor.history().undo_depth(), 0);
}

#[test]
fn move_keeps_crlf_breaks() {
    let mut editor = carets("a\r\nb\r\nc", &[3]);
    apply(&mut editor, Command::MoveLines { down: false });
    assert_eq!(text(&editor), "b\r\na\r\nc");
    assert_eq!(caret_offsets(&editor), [0]);
}

#[test]
fn copy_down_selects_the_lower_copy_and_copy_up_the_upper() {
    for (down, selected) in
        [(true, range(4, 7)), (false, range(0, 3))]
    {
        let mut lines = editor("a\nb\nc", &[range(0, 3)]);
        run_undoable(&mut lines, Command::CopyLines { down });
        assert_eq!(text(&lines), "a\nb\na\nb\nc");
        assert_eq!(selections(&lines), [selected], "down {down}");
    }
}

#[test]
fn copy_of_two_blocks_shifts_the_later_one() {
    let mut editor = carets("a\nb\nc", &[0, 4]);
    apply(&mut editor, Command::CopyLines { down: true });
    assert_eq!(text(&editor), "a\na\nb\nc\nc");
    assert_eq!(caret_offsets(&editor), [2, 8]);
}

#[test]
fn blank_line_below_keeps_the_indent_and_moves_the_caret() {
    let mut editor = carets("  ab\ncd", &[3, 6]);
    run_undoable(&mut editor, Command::BlankLineBelow);
    assert_eq!(text(&editor), "  ab\n  \ncd\n");
    assert_eq!(caret_offsets(&editor), [7, 11]);
}

#[test]
fn transpose_swaps_around_the_caret_and_before_a_line_end() {
    let mut editor = carets("abc", &[1]);
    run_undoable(&mut editor, Command::Transpose);
    assert_eq!(text(&editor), "bac");
    assert_eq!(caret_offsets(&editor), [2]);
    let mut editor = carets("abc", &[3]);
    run_undoable(&mut editor, Command::Transpose);
    assert_eq!(text(&editor), "acb");
    assert_eq!(caret_offsets(&editor), [3]);
}

#[test]
fn transpose_swaps_graphemes_and_crosses_a_line_start() {
    let mut editor = carets("é🎉", &[2]);
    apply(&mut editor, Command::Transpose);
    assert_eq!(text(&editor), "🎉é");
    let mut editor = carets("ab\ncd", &[3]);
    apply(&mut editor, Command::Transpose);
    assert_eq!(text(&editor), "ac\nbd");
    assert_eq!(caret_offsets(&editor), [4]);
}

#[test]
fn transpose_skips_a_swap_that_overlaps_the_one_before() {
    let mut editor = carets("abcd", &[1, 2]);
    apply(&mut editor, Command::Transpose);
    assert_eq!(text(&editor), "bacd");
    assert_eq!(caret_offsets(&editor), [2]);
}

#[test]
fn transpose_with_nothing_to_swap_changes_nothing() {
    let mut editor = carets("a\n", &[0, 1, 2]);
    assert!(!apply(&mut editor, Command::Transpose));
}

#[test]
fn kill_line_deletes_to_the_end_then_the_break() {
    let mut editor = carets("abc\ndef", &[1]);
    run_undoable(&mut editor, Command::DeleteToLineEnd);
    assert_eq!(text(&editor), "a\ndef");
    assert_eq!(caret_offsets(&editor), [1]);
    run_undoable(&mut editor, Command::DeleteToLineEnd);
    assert_eq!(text(&editor), "adef");
    assert_eq!(caret_offsets(&editor), [1]);
}

#[test]
fn kill_line_at_two_carets_on_one_line_merges() {
    let mut editor = carets("abcdef\ng", &[1, 3]);
    run_undoable(&mut editor, Command::DeleteToLineEnd);
    assert_eq!(text(&editor), "a\ng");
    assert_eq!(caret_offsets(&editor), [1]);
}
