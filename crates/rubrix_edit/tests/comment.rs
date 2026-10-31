use rubrix_edit::{
    Buffer, Command, Editor, History, Markers, Monospace, Selection,
    Selections, markers,
};
use rubrix_highlight::Lang;

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

fn toggle(editor: &mut Editor, lang: Lang, block: bool) {
    let _ = editor.apply(
        Command::ToggleComment { lang, block },
        &Monospace::default(),
    );
}

fn text(editor: &Editor) -> String {
    editor.buffer().to_string()
}

fn listed(editor: &Editor) -> Vec<Selection> {
    editor.selections().iter().collect()
}

#[test]
fn every_lang_has_the_markers_of_the_plan() {
    let slashes = Markers {
        line: Some("//"),
        block: Some(("/*", "*/")),
    };
    let hash = Markers {
        line: Some("#"),
        block: None,
    };
    let none = Markers {
        line: None,
        block: None,
    };
    for lang in Lang::ALL {
        let expected = match lang.name() {
            "rust" | "ts" | "go" => slashes,
            "python" | "shell" | "toml" | "yaml" => hash,
            "json" | "diff" | "plain" => none,
            name => panic!("no expected markers for {name}"),
        };
        assert_eq!(markers(lang), expected, "{}", lang.name());
    }
}

const RUST: &str =
    "fn main() {\n    let a = 1;\n\n        let b = 2;\n}\n";

#[test]
fn rust_lines_comment_at_their_least_indent_and_uncomment() {
    let mut editor = editor(RUST, &[range(12, 46)]);
    toggle(&mut editor, Lang::Rust, false);
    assert_eq!(
        text(&editor),
        "fn main() {\n    // let a = 1;\n\n    //     let b = \
         2;\n}\n"
    );
    toggle(&mut editor, Lang::Rust, false);
    assert_eq!(text(&editor), RUST);
}

#[test]
fn one_uncommented_line_comments_them_all() {
    let start = "// a\nb\n// c\n";
    let mut editor = editor(start, &[range(0, 12)]);
    toggle(&mut editor, Lang::Rust, false);
    assert_eq!(text(&editor), "// // a\n// b\n// // c\n");
}

#[test]
fn uncomment_takes_one_space_after_the_marker() {
    let mut editor = editor("//a\n//  b\n", &[range(0, 9)]);
    toggle(&mut editor, Lang::Rust, false);
    assert_eq!(text(&editor), "a\n b\n");
}

#[test]
fn a_caret_on_an_empty_line_comments_it() {
    let mut editor = editor("a\n\nb", &[Selection::caret(2)]);
    toggle(&mut editor, Lang::Rust, false);
    assert_eq!(text(&editor), "a\n// \nb");
}

#[test]
fn several_carets_comment_their_lines_in_one_undo_step() {
    let mut editor = editor(
        "a\nb\nc\n",
        &[Selection::caret(0), Selection::caret(4)],
    );
    toggle(&mut editor, Lang::Rust, false);
    assert_eq!(text(&editor), "// a\nb\n// c\n");
    assert_eq!(editor.history().undo_depth(), 1);
    let _ = editor.apply(Command::Undo, &Monospace::default());
    assert_eq!(text(&editor), "a\nb\nc\n");
    assert_eq!(
        listed(&editor),
        [Selection::caret(0), Selection::caret(4)]
    );
}

#[test]
fn python_uses_a_hash() {
    let mut editor = editor("x = 1\n", &[Selection::caret(0)]);
    toggle(&mut editor, Lang::Python, false);
    assert_eq!(text(&editor), "# x = 1\n");
    toggle(&mut editor, Lang::Python, false);
    assert_eq!(text(&editor), "x = 1\n");
}

#[test]
fn a_block_comment_keeps_the_inner_text_selected() {
    let mut editor = editor("let a = b + c;", &[range(8, 13)]);
    toggle(&mut editor, Lang::Rust, true);
    assert_eq!(text(&editor), "let a = /* b + c */;");
    assert_eq!(listed(&editor), [range(11, 16)]);
    toggle(&mut editor, Lang::Rust, true);
    assert_eq!(text(&editor), "let a = b + c;");
    assert_eq!(listed(&editor), [range(8, 13)]);
}

#[test]
fn a_block_comment_selected_whole_is_uncommented_to_its_content() {
    let mut editor = editor("x /* y */ z", &[range(9, 2)]);
    toggle(&mut editor, Lang::Rust, true);
    assert_eq!(text(&editor), "x y z");
    assert_eq!(listed(&editor), [range(3, 2)]);
}

#[test]
fn a_block_comment_at_a_caret_puts_the_caret_inside() {
    let mut editor = editor("ab", &[Selection::caret(1)]);
    toggle(&mut editor, Lang::Rust, true);
    assert_eq!(text(&editor), "a/*  */b");
    assert_eq!(listed(&editor), [Selection::caret(4)]);
}

#[test]
fn json_has_no_comments() {
    let json = "{\n  \"a\": 1\n}\n";
    for block in [false, true] {
        let mut editor = editor(json, &[range(0, 12)]);
        toggle(&mut editor, Lang::Json, block);
        assert_eq!(text(&editor), json);
        assert_eq!(editor.history().undo_depth(), 0);
    }
}
