use rubrix_edit::{
    Assoc, Buffer, Command, Editor, History, Monospace, Motion,
    Selection, Selections,
};

mod common;

use common::{Rng, seed};

fn editor_at(text: &str, carets: &[usize]) -> Editor {
    let mut selections = Selections::new(Selection::caret(carets[0]));
    for &at in &carets[1..] {
        selections.add(Selection::caret(at));
    }
    Editor::from_parts(Buffer::new(text), selections, History::new())
}

fn apply(editor: &mut Editor, command: Command<'_>) {
    let _ = editor.apply(command, &Monospace::default());
}

/// Types `text` one char per keystroke.
fn type_keys(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        apply(editor, Command::Type(&c.to_string()));
    }
}

fn press(editor: &mut Editor, command: Command<'_>, times: usize) {
    for _ in 0..times {
        apply(editor, command);
    }
}

fn shift(motion: Motion) -> Command<'static> {
    Command::Move {
        motion,
        extend: true,
    }
}

fn text(editor: &Editor) -> String {
    editor.buffer().to_string()
}

fn depth(editor: &Editor) -> usize {
    editor.history().undo_depth()
}

fn carets(editor: &Editor) -> Vec<Selection> {
    editor.selections().iter().collect()
}

fn move_to(editor: &mut Editor, at: usize) {
    editor.set_selections(Selections::new(Selection::caret(at)));
}

#[test]
fn a_newline_starts_a_step() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "ab");
    assert_eq!(depth(&editor), 1);
    apply(&mut editor, Command::Enter);
    assert_eq!(depth(&editor), 2);
    type_keys(&mut editor, "cd");
    assert_eq!(depth(&editor), 3);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "ab\n");
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "ab");
}

#[test]
fn typed_text_with_a_line_break_starts_a_step_and_ends_it() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "a");
    apply(&mut editor, Command::Type("b\nc"));
    assert_eq!(depth(&editor), 2);
    type_keys(&mut editor, "d");
    assert_eq!(depth(&editor), 3);
    assert_eq!(text(&editor), "ab\ncd");
}

#[test]
fn a_caret_jump_starts_a_step() {
    let mut editor = editor_at("0123456789", &[0]);
    type_keys(&mut editor, "ab");
    move_to(&mut editor, 8);
    type_keys(&mut editor, "c");
    assert_eq!(depth(&editor), 2);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "ab0123456789");
    assert_eq!(carets(&editor), [Selection::caret(8)]);
}

#[test]
fn a_caret_moved_to_the_left_edge_of_the_run_starts_a_step() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "ab");
    press(
        &mut editor,
        Command::Move {
            motion: Motion::Left,
            extend: false,
        },
        2,
    );
    type_keys(&mut editor, "c");
    assert_eq!(text(&editor), "cab");
    assert_eq!(depth(&editor), 2);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "ab");
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "");
    press(&mut editor, Command::Redo, 2);
    assert_eq!(text(&editor), "cab");
}

#[test]
fn typing_over_a_selection_at_the_runs_end_starts_a_step() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "abc");
    apply(&mut editor, shift(Motion::Left));
    type_keys(&mut editor, "x");
    assert_eq!(text(&editor), "abx");
    assert_eq!(depth(&editor), 2);
    type_keys(&mut editor, "y");
    assert_eq!(
        depth(&editor),
        2,
        "typing goes on after the selection"
    );
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "abc");
    assert_eq!(carets(&editor), [Selection { anchor: 3, head: 2 }]);
}

#[test]
fn a_paste_is_its_own_step() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "a");
    apply(&mut editor, Command::Paste("b"));
    assert_eq!(depth(&editor), 2);
    type_keys(&mut editor, "c");
    assert_eq!(depth(&editor), 3);
}

#[test]
fn a_cut_is_its_own_step() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "abc");
    apply(&mut editor, shift(Motion::Left));
    let cut = editor.apply(Command::Cut, &Monospace::default());
    let removed: Vec<String> = cut
        .unwrap()
        .replacements()
        .map(|rep| rep.old.to_owned())
        .collect();
    assert_eq!(removed, ["c"]);
    assert_eq!(depth(&editor), 2);
    type_keys(&mut editor, "d");
    assert_eq!(depth(&editor), 3);
    assert_eq!(text(&editor), "abd");
}

#[test]
fn a_caret_count_change_starts_a_step() {
    let mut editor = editor_at("\n", &[0]);
    type_keys(&mut editor, "a");
    let mut selections = editor.selections().clone();
    selections.add(Selection::caret(2));
    editor.set_selections(selections);
    type_keys(&mut editor, "b");
    assert_eq!(text(&editor), "ab\nb");
    assert_eq!(depth(&editor), 2);
}

#[test]
fn an_undo_or_a_redo_ends_the_step() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "a");
    apply(&mut editor, Command::Undo);
    apply(&mut editor, Command::Redo);
    type_keys(&mut editor, "b");
    assert_eq!(depth(&editor), 2);

    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "ab");
    move_to(&mut editor, 0);
    type_keys(&mut editor, "x");
    apply(&mut editor, Command::Undo);
    move_to(&mut editor, 2);
    type_keys(&mut editor, "c");
    assert_eq!(depth(&editor), 2);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "ab");
}

#[test]
fn undo_of_backspace_at_touching_carets_restores_both() {
    let mut editor = editor_at("abc", &[1, 2]);
    apply(&mut editor, Command::Backspace);
    assert_eq!(text(&editor), "c");
    assert_eq!(carets(&editor), [Selection::caret(0)]);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "abc");
    assert_eq!(
        carets(&editor),
        [Selection::caret(1), Selection::caret(2)]
    );
}

#[test]
fn typing_backspace_and_delete_runs_each_join() {
    let mut editor = editor_at("", &[0]);
    type_keys(&mut editor, "abcd");
    assert_eq!(depth(&editor), 1);
    press(&mut editor, Command::Backspace, 3);
    assert_eq!(text(&editor), "a");
    assert_eq!(depth(&editor), 2);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "abcd");

    let mut editor = editor_at("abcdef\nghij", &[1, 8]);
    press(&mut editor, Command::Delete, 3);
    assert_eq!(text(&editor), "aef\ng");
    assert_eq!(depth(&editor), 1);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "abcdef\nghij");
    apply(&mut editor, Command::Redo);
    assert_eq!(text(&editor), "aef\ng");
}

#[test]
fn a_backspace_run_at_two_carets_undoes_in_one_step() {
    let mut editor = editor_at("abcd\nefgh", &[4, 9]);
    press(&mut editor, Command::Backspace, 2);
    assert_eq!(text(&editor), "ab\nef");
    assert_eq!(depth(&editor), 1);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "abcd\nefgh");
    assert_eq!(
        carets(&editor),
        [Selection::caret(4), Selection::caret(9)]
    );
}

#[test]
fn undo_restores_the_selections_before_and_redo_after() {
    let mut editor = editor_at("hello", &[2]);
    apply(&mut editor, Command::SelectAll);
    type_keys(&mut editor, "x");
    assert_eq!(carets(&editor), [Selection::caret(1)]);
    apply(&mut editor, Command::Undo);
    assert_eq!(text(&editor), "hello");
    assert_eq!(carets(&editor), [Selection { anchor: 0, head: 5 }]);
    apply(&mut editor, Command::Redo);
    assert_eq!(text(&editor), "x");
    assert_eq!(carets(&editor), [Selection::caret(1)]);
}

#[test]
fn undo_and_redo_return_changes_that_map_offsets() {
    let mut editor = editor_at("world", &[0]);
    apply(&mut editor, Command::Type("hello "));
    let layout = Monospace::default();
    let undo = editor.apply(Command::Undo, &layout).unwrap();
    assert_eq!(undo.map(6, Assoc::After), 0);
    assert_eq!(undo.map(11, Assoc::After), 5);
    let redo = editor.apply(Command::Redo, &layout).unwrap();
    assert_eq!(redo.map(0, Assoc::Before), 0);
    assert_eq!(redo.map(0, Assoc::After), 6);
    assert_eq!(redo.map(5, Assoc::After), 11);
    assert_eq!(editor.apply(Command::Redo, &layout), None);
}

#[test]
fn max_entries_drops_the_oldest() {
    let mut editor = Editor::from_parts(
        Buffer::new(""),
        Selections::new(Selection::caret(0)),
        History::with_max_entries(2),
    );
    type_keys(&mut editor, "a");
    apply(&mut editor, Command::Enter);
    type_keys(&mut editor, "b");
    assert_eq!(depth(&editor), 2);
    press(&mut editor, Command::Undo, 2);
    assert_eq!(text(&editor), "a");
    assert_eq!(
        editor.apply(Command::Undo, &Monospace::default()),
        None
    );
    assert_eq!(History::new().undo_depth(), 0);
}

#[test]
#[should_panic(expected = "no entries")]
fn a_history_needs_an_entry() {
    History::with_max_entries(0);
}

fn random_command(rng: &mut Rng) -> Command<'static> {
    const TYPED: [&str; 6] = ["a", "é", "🎉", " ", "\n", "x\r\ny"];
    const MOTIONS: [Motion; 6] = [
        Motion::Left,
        Motion::Right,
        Motion::WordLeft,
        Motion::Down,
        Motion::Home,
        Motion::End,
    ];
    match rng.below(12) {
        0..=3 => Command::Type(TYPED[rng.below(6) as usize]),
        4 => Command::Enter,
        5 => Command::Backspace,
        6 => Command::Delete,
        7 => Command::DeleteWordBack,
        8 => Command::Paste("p\nq"),
        9 => Command::Cut,
        10 => Command::Undo,
        _ => Command::Move {
            motion: MOTIONS[rng.below(6) as usize],
            extend: rng.chance(30),
        },
    }
}

#[test]
fn undoing_every_step_restores_the_text_and_redo_returns() {
    let seed = seed();
    let mut rng = Rng(seed ^ (19 << 32));
    for case in 0..50 {
        let start = "fn main() {\n    let x = 1;\r\n}\n// é 你好\n";
        let mut editor = Editor::from_parts(
            Buffer::new(start),
            Selections::new(Selection::caret(0)),
            History::with_max_entries(1_000),
        );
        for _ in 0..200 {
            if rng.chance(3) {
                let mut selections = editor.selections().clone();
                let at = rng.below(editor.buffer().len() as u32 + 1);
                selections.add(Selection::caret(at as usize));
                editor.set_selections(selections);
            }
            apply(&mut editor, random_command(&mut rng));
        }
        let end = text(&editor);
        let steps = editor.history().undo_depth();
        press(&mut editor, Command::Undo, steps);
        assert_eq!(text(&editor), start, "seed {seed} case {case}");
        press(&mut editor, Command::Redo, steps);
        assert_eq!(text(&editor), end, "seed {seed} case {case}");
    }
}
