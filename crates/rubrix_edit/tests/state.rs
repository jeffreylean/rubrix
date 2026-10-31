use rubrix_edit::{
    Buffer, Command, EditState, Editor, History, Monospace,
    Selection, Selections,
};

fn apply(editor: &mut Editor, command: Command<'_>) {
    let _ = editor.apply(command, &Monospace::default());
}

fn edited() -> Editor {
    let mut selections = Selections::new(Selection::caret(3));
    selections.add(Selection {
        anchor: 8,
        head: 10,
    });
    let mut editor = Editor::from_parts(
        Buffer::new("one\ntwo\nthree\n"),
        selections,
        History::new(),
    );
    apply(&mut editor, Command::Type("!"));
    apply(&mut editor, Command::Enter);
    apply(&mut editor, Command::Type("x"));
    editor
}

#[test]
fn a_round_trip_keeps_the_text_the_selections_and_the_undo_depth() {
    let editor = edited();
    let restored = EditState::save(&editor).restore();
    assert_eq!(
        restored.buffer().to_string(),
        editor.buffer().to_string()
    );
    assert!(
        restored.selections().iter().eq(editor.selections().iter())
    );
    assert_eq!(
        restored.selections().primary(),
        editor.selections().primary()
    );
    assert_eq!(restored.history().undo_depth(), 3);
    assert_eq!(restored.history().redo_depth(), 0);
}

#[test]
fn undo_after_a_round_trip_restores_the_text_before_the_edits() {
    let mut restored = EditState::save(&edited()).restore();
    for _ in 0..3 {
        apply(&mut restored, Command::Undo);
    }
    assert_eq!(restored.buffer().to_string(), "one\ntwo\nthree\n");
    assert_eq!(restored.history().redo_depth(), 3);
}

#[test]
fn an_edit_after_a_save_leaves_the_saved_text() {
    let mut editor = edited();
    let state = EditState::save(&editor);
    let saved = editor.buffer().to_string();
    apply(&mut editor, Command::SelectAll);
    apply(&mut editor, Command::Type("gone"));
    assert_eq!(state.buffer().to_string(), saved);
    assert_eq!(state.restore().buffer().to_string(), saved);
}
