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

#[test]
fn copy_text_joins_the_selections_in_buffer_order() {
    let text = "alpha beta\ngamma\n";
    let editor = editor(
        text,
        &[
            Selection {
                anchor: 13,
                head: 11,
            },
            Selection::caret(3),
            Selection { anchor: 0, head: 5 },
        ],
    );
    assert_eq!(editor.copy_text(), "alpha\nga");
}

#[test]
fn copy_text_keeps_crlf_and_copies_the_line_at_carets() {
    let crlf = editor(
        "a\r\nb\r\nc",
        &[
            Selection { anchor: 0, head: 4 },
            Selection { anchor: 6, head: 7 },
        ],
    );
    assert_eq!(crlf.copy_text(), "a\r\nb\r\nc");
    let carets =
        editor("abc", &[Selection::caret(1), Selection::caret(2)]);
    assert_eq!(carets.copy_text(), "abc");
}

fn carets(text: &str, at: &[usize]) -> Editor {
    let at: Vec<Selection> =
        at.iter().map(|&at| Selection::caret(at)).collect();
    editor(text, &at)
}

fn apply(editor: &mut Editor, command: Command<'_>) {
    let _ = editor.apply(command, &Monospace::default());
}

fn text(editor: &Editor) -> String {
    editor.buffer().to_string()
}

fn caret_offsets(editor: &Editor) -> Vec<usize> {
    editor.selections().iter().map(|caret| caret.head).collect()
}

#[test]
fn a_copy_at_a_caret_takes_its_line_with_the_break() {
    assert_eq!(carets("ab\ncd", &[1]).copy_text(), "ab\n");
    assert_eq!(carets("ab\ncd", &[4]).copy_text(), "cd");
    assert_eq!(carets("a\r\nb", &[0]).copy_text(), "a\r\n");
}

#[test]
fn carets_on_one_line_copy_it_once() {
    assert_eq!(carets("ab\ncd", &[0, 1, 2]).copy_text(), "ab\n");
}

#[test]
fn carets_on_several_lines_copy_each_in_order() {
    assert_eq!(carets("ab\ncd\nef", &[7, 1]).copy_text(), "ab\nef");
    assert_eq!(carets("ab\ncd\nef", &[4, 1]).copy_text(), "ab\ncd\n");
}

#[test]
fn a_range_and_a_bare_caret_copy_the_range_and_the_caret_line() {
    let source = "ab cd\nef\ngh";
    let mut editor = editor(
        source,
        &[Selection { anchor: 0, head: 2 }, Selection::caret(7)],
    );
    assert_eq!(editor.copy_text(), "ab\nef\n");
    apply(&mut editor, Command::Cut);
    assert_eq!(text(&editor), " cd\ngh");
    assert_eq!(editor.history().undo_depth(), 1);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), source);
}

#[test]
fn a_bare_caret_beside_a_range_on_its_line_takes_the_line_once() {
    let mut editor = editor(
        "ab cd\nef",
        &[Selection { anchor: 0, head: 2 }, Selection::caret(4)],
    );
    assert_eq!(editor.copy_text(), "ab cd\n");
    apply(&mut editor, Command::Cut);
    assert_eq!(text(&editor), "ef");
}

#[test]
fn a_range_that_ends_in_a_break_takes_no_second_break() {
    let editor = editor(
        "ab\ncd\nef",
        &[
            Selection { anchor: 0, head: 3 },
            Selection { anchor: 6, head: 7 },
        ],
    );
    assert_eq!(editor.copy_text(), "ab\ne");
}

#[test]
fn a_cut_at_a_caret_removes_its_line() {
    let mut editor = carets("ab\ncd\nef", &[4]);
    assert_eq!(editor.copy_text(), "cd\n");
    apply(&mut editor, Command::Cut);
    assert_eq!(text(&editor), "ab\nef");
    assert_eq!(caret_offsets(&editor), [3]);
    assert_eq!(editor.history().undo_depth(), 1);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "ab\ncd\nef");
    assert_eq!(caret_offsets(&editor), [4]);
}

#[test]
fn a_cut_on_the_last_line_takes_the_break_before_it() {
    let mut editor = carets("ab\r\ncd", &[5]);
    apply(&mut editor, Command::Cut);
    assert_eq!(text(&editor), "ab");
    let mut editor = carets("ab", &[1]);
    apply(&mut editor, Command::Cut);
    assert_eq!(text(&editor), "");
}

#[test]
fn n_lines_pasted_at_n_carets_put_one_line_at_each() {
    for pasted in ["a\nb\nc", "a\nb\nc\n", "a\r\nb\r\nc"] {
        let mut editor = carets("x\ny\nz", &[1, 3, 5]);
        apply(&mut editor, Command::Paste(pasted));
        assert_eq!(text(&editor), "xa\nyb\nzc", "pasting {pasted:?}");
        assert_eq!(caret_offsets(&editor), [2, 5, 8]);
        assert_eq!(editor.history().undo_depth(), 1);
        apply(&mut editor, Command::Undo);
        assert_eq!(text(&editor), "x\ny\nz");
        assert_eq!(caret_offsets(&editor), [1, 3, 5]);
    }
}

#[test]
fn a_paste_with_another_line_count_goes_whole_at_every_caret() {
    let mut editor = carets("x\ny", &[1, 3]);
    apply(&mut editor, Command::Paste("a\nb\nc"));
    assert_eq!(text(&editor), "xa\nb\nc\nya\nb\nc");
}

#[test]
fn a_line_copied_at_a_caret_pastes_back_whole_at_one_caret() {
    let mut editor = carets("ab\ncd", &[0]);
    let copied = editor.copy_text();
    apply(&mut editor, Command::Paste(&copied));
    assert_eq!(text(&editor), "ab\nab\ncd");
    assert_eq!(caret_offsets(&editor), [3]);
}

#[test]
fn lines_cut_whole_at_n_carets_paste_back_whole_at_n_carets() {
    let mut editor = carets("ab\ncd\nef\ngh", &[1, 7]);
    let pieces = editor.copy_pieces();
    assert_eq!(pieces, ["ab\n", "ef\n"]);
    apply(&mut editor, Command::Cut);
    assert_eq!(text(&editor), "cd\ngh");
    assert_eq!(caret_offsets(&editor), [0, 3]);
    apply(&mut editor, Command::PasteEach(&pieces));
    assert_eq!(text(&editor), "ab\ncd\nef\ngh");
    assert_eq!(caret_offsets(&editor), [3, 9]);
    assert_eq!(editor.history().undo_depth(), 2);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "cd\ngh");
    assert_eq!(caret_offsets(&editor), [0, 3]);
}

#[test]
fn a_piece_per_selection_goes_to_its_own_in_the_buffers_line_ending()
{
    let mut editor = editor(
        "x\r\ny\r\nz",
        &[Selection::caret(0), Selection { anchor: 3, head: 4 }],
    );
    let pieces = ["a\nb".into(), "c".into()];
    apply(&mut editor, Command::PasteEach(&pieces));
    assert_eq!(text(&editor), "a\r\nbx\r\nc\r\nz");
}

#[test]
fn pieces_for_another_number_of_selections_paste_nothing() {
    let mut editor = carets("x\ny", &[0, 2]);
    let pieces = ["a".into(), "b".into(), "c".into()];
    let change = editor
        .apply(Command::PasteEach(&pieces), &Monospace::default());
    assert!(change.is_none());
    assert_eq!(text(&editor), "x\ny");
}
