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

fn range(anchor: usize, head: usize) -> Selection {
    Selection { anchor, head }
}

fn typed(editor: &mut Editor, text: &str) {
    let _ = editor.apply(Command::Type(text), &Monospace::default());
}

fn listed(editor: &Editor) -> Vec<Selection> {
    editor.selections().iter().collect()
}

#[test]
fn each_opener_wraps_the_selections_and_keeps_them_selected() {
    for (open, close) in [
        ("(", ")"),
        ("[", "]"),
        ("{", "}"),
        ("\"", "\""),
        ("'", "'"),
        ("`", "`"),
    ] {
        let mut editor =
            editor("one two three", &[range(0, 3), range(13, 8)]);
        typed(&mut editor, open);
        assert_eq!(
            editor.buffer().to_string(),
            format!("{open}one{close} two {open}three{close}")
        );
        assert_eq!(
            listed(&editor),
            [range(1, 4), range(16, 11)],
            "{open}"
        );
        assert_eq!(editor.history().undo_depth(), 1);
        let _ = editor.apply(Command::Undo, &Monospace::default());
        assert_eq!(editor.buffer().to_string(), "one two three");
        assert_eq!(listed(&editor), [range(0, 3), range(13, 8)]);
    }
}

#[test]
fn touching_selections_wrap_apart() {
    let mut editor = editor("abcd", &[range(0, 2), range(2, 4)]);
    typed(&mut editor, "(");
    assert_eq!(editor.buffer().to_string(), "(ab)(cd)");
    assert_eq!(listed(&editor), [range(1, 3), range(5, 7)]);
}

#[test]
fn a_caret_types_the_opener() {
    let mut editor = editor("ab", &[Selection::caret(1)]);
    typed(&mut editor, "(");
    assert_eq!(editor.buffer().to_string(), "a(b");
    assert_eq!(listed(&editor), [Selection::caret(2)]);
}

#[test]
fn a_caret_among_ranges_types_over_them_all() {
    let mut editor =
        editor("ab cd", &[range(0, 2), Selection::caret(4)]);
    typed(&mut editor, "[");
    assert_eq!(editor.buffer().to_string(), "[ c[d");
}

#[test]
fn other_text_replaces_the_selection() {
    let mut editor = editor("ab cd", &[range(0, 2)]);
    typed(&mut editor, "<");
    assert_eq!(editor.buffer().to_string(), "< cd");
}
