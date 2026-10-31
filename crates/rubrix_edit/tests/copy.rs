use rubrix_edit::{Buffer, Editor, History, Selection, Selections};

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
fn copy_text_keeps_crlf_and_is_empty_for_carets() {
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
    assert_eq!(carets.copy_text(), "");
}
