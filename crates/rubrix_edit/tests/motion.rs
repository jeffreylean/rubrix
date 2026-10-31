use rubrix_edit::{
    Buffer, Command, Editor, History, Layout, Monospace, Motion,
    Selection, Selections,
};

mod common;

use common::{Rng, seed};

fn editor_at(text: &str, at: usize) -> Editor {
    Editor::from_parts(
        Buffer::new(text),
        Selections::new(Selection::caret(at)),
        History::new(),
    )
}

fn apply(editor: &mut Editor, command: Command<'_>) {
    let _ = editor.apply(command, &Monospace::default());
}

fn go(motion: Motion) -> Command<'static> {
    Command::Move {
        motion,
        extend: false,
    }
}

fn shift(motion: Motion) -> Command<'static> {
    Command::Move {
        motion,
        extend: true,
    }
}

fn head(editor: &Editor) -> usize {
    editor.selections().primary().head
}

fn selection(editor: &Editor) -> Selection {
    editor.selections().primary()
}

/// The heads after pressing `command` once per expected stop.
fn stops(
    editor: &mut Editor,
    command: Command<'_>,
    count: usize,
) -> Vec<usize> {
    (0..count)
        .map(|_| {
            apply(editor, command);
            head(editor)
        })
        .collect()
}

const CLUSTERS: &str = "a👨‍👩‍👧‍👦be\u{301}c你好世界";
const CLUSTER_ENDS: [usize; 9] = [1, 26, 27, 30, 31, 34, 37, 40, 43];

#[test]
fn left_and_right_step_over_one_grapheme() {
    let mut editor = editor_at(CLUSTERS, 0);
    let right = stops(&mut editor, go(Motion::Right), 10);
    assert_eq!(right[..9], CLUSTER_ENDS);
    assert_eq!(right[9], 43, "Right at the end stays");
    let left = stops(&mut editor, go(Motion::Left), 10);
    assert_eq!(left, [40, 37, 34, 31, 30, 27, 26, 1, 0, 0]);
}

#[test]
fn backspace_and_delete_remove_one_grapheme() {
    let mut editor = editor_at(CLUSTERS, CLUSTERS.len());
    for &end in CLUSTER_ENDS[..8].iter().rev() {
        apply(&mut editor, Command::Backspace);
        assert_eq!(editor.buffer().to_string(), CLUSTERS[..end]);
        assert_eq!(head(&editor), end);
    }
    let mut editor = editor_at(CLUSTERS, 0);
    for &start in &CLUSTER_ENDS {
        apply(&mut editor, Command::Delete);
        assert_eq!(editor.buffer().to_string(), CLUSTERS[start..]);
        assert_eq!(head(&editor), 0);
    }
}

#[test]
fn a_caret_inside_a_cluster_snaps_to_its_start() {
    let mut editor = editor_at(CLUSTERS, 0);
    for (at, want) in [(5, 1), (28, 27), (29, 27), (32, 31), (44, 43)]
    {
        editor.set_selections(Selections::new(Selection::caret(at)));
        assert_eq!(head(&editor), want, "caret at {at}");
    }
}

#[test]
fn a_crlf_is_one_step() {
    let mut editor = editor_at("ab\r\ncd", 2);
    assert_eq!(stops(&mut editor, go(Motion::Right), 1), [4]);
    assert_eq!(stops(&mut editor, go(Motion::Left), 1), [2]);
    editor.set_selections(Selections::new(Selection::caret(3)));
    assert_eq!(head(&editor), 2, "a caret between \\r and \\n snaps");
    apply(&mut editor, go(Motion::End));
    assert_eq!(head(&editor), 2);
    apply(&mut editor, Command::Delete);
    assert_eq!(editor.buffer().to_string(), "abcd");

    let mut editor = editor_at("ab\r\ncd", 4);
    apply(&mut editor, Command::Backspace);
    assert_eq!(editor.buffer().to_string(), "abcd");
    assert_eq!(head(&editor), 2);
}

const WORDS: &str = "foo_bar.baz(qux)";

#[test]
fn word_motion_stops_at_each_run() {
    let mut editor = editor_at(WORDS, 0);
    let right = stops(&mut editor, go(Motion::WordRight), 6);
    assert_eq!(right, [7, 8, 11, 12, 15, 16]);
    let left = stops(&mut editor, go(Motion::WordLeft), 6);
    assert_eq!(left, [15, 12, 11, 8, 7, 0]);
}

#[test]
fn word_motion_skips_whitespace_and_crosses_lines() {
    let mut editor = editor_at("foo   bar\nbaz", 0);
    let right = stops(&mut editor, go(Motion::WordRight), 4);
    assert_eq!(right, [3, 9, 10, 13]);
    let left = stops(&mut editor, go(Motion::WordLeft), 4);
    assert_eq!(left, [10, 9, 6, 0]);
}

#[test]
fn delete_word_back_removes_one_run_at_a_time() {
    let mut editor = editor_at(WORDS, WORDS.len());
    let mut texts = Vec::new();
    for _ in 0..6 {
        apply(&mut editor, Command::DeleteWordBack);
        texts.push(editor.buffer().to_string());
    }
    assert_eq!(
        texts,
        [
            "foo_bar.baz(qux",
            "foo_bar.baz(",
            "foo_bar.baz",
            "foo_bar.",
            "foo_bar",
            ""
        ]
    );
    assert_eq!(editor.history().undo_depth(), 1);
}

#[test]
fn delete_word_forward_removes_to_the_next_stop() {
    let mut editor = editor_at(WORDS, 0);
    apply(&mut editor, Command::DeleteWordForward);
    assert_eq!(editor.buffer().to_string(), ".baz(qux)");
}

#[test]
fn home_toggles_between_the_indent_and_column_zero() {
    let line = "    let x = 1;";
    let mut editor = editor_at(line, line.len());
    let presses = stops(&mut editor, go(Motion::Home), 3);
    assert_eq!(presses, [4, 0, 4]);
    let mut editor = editor_at(line, 2);
    assert_eq!(stops(&mut editor, go(Motion::Home), 1), [4]);
}

#[test]
fn end_and_document_motions() {
    let text = "ab\r\ncd\nef";
    let mut editor = editor_at(text, 0);
    assert_eq!(stops(&mut editor, go(Motion::End), 1), [2]);
    assert_eq!(
        stops(&mut editor, go(Motion::DocEnd), 1),
        [text.len()]
    );
    assert_eq!(stops(&mut editor, go(Motion::DocStart), 1), [0]);
}

const SHORT_MIDDLE: &str = "abcdefgh\nab\nabcdefgh";

#[test]
fn vertical_motion_keeps_its_goal_across_a_short_line() {
    let mut editor = editor_at(SHORT_MIDDLE, 6);
    let down = stops(&mut editor, go(Motion::Down), 2);
    assert_eq!(down, [11, 18]);
    let up = stops(&mut editor, go(Motion::Up), 2);
    assert_eq!(up, [11, 6]);
    assert_eq!(stops(&mut editor, go(Motion::Up), 1), [0]);
    let mut editor = editor_at(SHORT_MIDDLE, 6);
    apply(&mut editor, go(Motion::Down));
    apply(&mut editor, go(Motion::Left));
    assert_eq!(stops(&mut editor, go(Motion::Down), 1), [13]);
    assert_eq!(
        stops(&mut editor, go(Motion::Down), 1),
        [SHORT_MIDDLE.len()]
    );
}

#[test]
fn page_motion_moves_page_rows() {
    let text: String =
        (0..100).map(|n| format!("{n:03}\n")).collect();
    let mut editor = editor_at(&text, 1);
    let layout = Monospace {
        tab: 4,
        page_rows: 10,
    };
    let _ = editor.apply(go(Motion::PageDown), &layout);
    assert_eq!(head(&editor), 41);
    let _ = editor.apply(go(Motion::PageUp), &layout);
    assert_eq!(head(&editor), 1);
    let _ = editor.apply(go(Motion::PageUp), &layout);
    assert_eq!(head(&editor), 0);
}

#[test]
fn monospace_counts_tabs_to_the_next_stop() {
    let buffer = Buffer::new("\tab\n  \tc\nxxxxxxxxx");
    let layout = Monospace::default();
    assert_eq!(layout.x(&buffer, 1), 4.0);
    assert_eq!(layout.x(&buffer, 3), 6.0);
    assert_eq!(layout.x(&buffer, 7), 4.0);
    assert_eq!(layout.vertical(&buffer, 13, -1, 3.0), Some(6));
    assert_eq!(layout.vertical(&buffer, 13, -2, 2.0), Some(0));
    assert_eq!(layout.vertical(&buffer, 13, -3, 2.0), None);
    assert_eq!(layout.vertical(&buffer, 0, 3, 2.0), None);
}

#[test]
fn shift_extends_every_motion() {
    let mut editor = editor_at("hello world\nsecond", 0);
    apply(&mut editor, shift(Motion::Right));
    apply(&mut editor, shift(Motion::Right));
    assert_eq!(selection(&editor), Selection { anchor: 0, head: 2 });
    apply(&mut editor, shift(Motion::WordRight));
    assert_eq!(selection(&editor), Selection { anchor: 0, head: 5 });
    apply(&mut editor, shift(Motion::Down));
    assert_eq!(
        selection(&editor),
        Selection {
            anchor: 0,
            head: 17
        }
    );
    apply(&mut editor, shift(Motion::Home));
    assert_eq!(
        selection(&editor),
        Selection {
            anchor: 0,
            head: 12
        }
    );
    apply(&mut editor, go(Motion::Right));
    assert_eq!(selection(&editor), Selection::caret(12));
    apply(&mut editor, shift(Motion::Left));
    apply(&mut editor, shift(Motion::WordLeft));
    assert_eq!(
        selection(&editor),
        Selection {
            anchor: 12,
            head: 6
        }
    );
    apply(&mut editor, go(Motion::Right));
    assert_eq!(selection(&editor), Selection::caret(12));
    apply(&mut editor, shift(Motion::WordLeft));
    apply(&mut editor, shift(Motion::WordLeft));
    apply(&mut editor, go(Motion::Left));
    assert_eq!(selection(&editor), Selection::caret(6));
}

#[test]
fn enter_copies_the_indent_up_to_the_caret() {
    for (line, want) in [
        ("    foo", "    foo\n    "),
        ("\tbar", "\tbar\n\t"),
        ("    \tbaz", "    \tbaz\n    \t"),
    ] {
        let mut editor = editor_at(line, line.len());
        apply(&mut editor, Command::Enter);
        assert_eq!(editor.buffer().to_string(), want);
        assert_eq!(head(&editor), want.len());
    }
    let mut editor = editor_at("    foo", 2);
    apply(&mut editor, Command::Enter);
    assert_eq!(editor.buffer().to_string(), "  \n    foo");
    let mut editor = editor_at("\tx\r\ny", 2);
    apply(&mut editor, Command::Enter);
    assert_eq!(editor.buffer().to_string(), "\tx\r\n\t\r\ny");
}

#[test]
fn select_all_and_collapse_to_the_primary() {
    let mut selections = Selections::new(Selection::caret(1));
    selections.add(Selection::caret(3));
    let mut editor = Editor::from_parts(
        Buffer::new("abcd"),
        selections,
        History::new(),
    );
    apply(&mut editor, Command::CollapseToPrimary);
    assert_eq!(editor.selections().len(), 1);
    assert_eq!(head(&editor), 3);
    apply(&mut editor, Command::SelectAll);
    assert_eq!(selection(&editor), Selection { anchor: 0, head: 4 });
}

#[test]
fn collapse_keeps_the_primary_range_and_then_makes_it_a_caret() {
    let mut selections = Selections::new(Selection::caret(0));
    selections.add(Selection { anchor: 3, head: 1 });
    let mut editor = Editor::from_parts(
        Buffer::new("abcd"),
        selections,
        History::new(),
    );
    apply(&mut editor, Command::CollapseToPrimary);
    assert_eq!(selection(&editor), Selection { anchor: 3, head: 1 });
    apply(&mut editor, Command::CollapseToPrimary);
    assert_eq!(selection(&editor), Selection::caret(1));
}

fn on_a_boundary(text: &str, at: usize) -> bool {
    text.is_char_boundary(at)
        && !(text[..at].ends_with('\r')
            && text[at..].starts_with('\n'))
}

#[test]
fn random_motions_keep_carets_on_boundaries() {
    const MOTIONS: [Motion; 12] = [
        Motion::Left,
        Motion::Right,
        Motion::WordLeft,
        Motion::WordRight,
        Motion::Home,
        Motion::End,
        Motion::Up,
        Motion::Down,
        Motion::PageUp,
        Motion::PageDown,
        Motion::DocStart,
        Motion::DocEnd,
    ];
    let seed = seed();
    let mut rng = Rng(seed ^ (23 << 32));
    let text =
        format!("{CLUSTERS}\r\n\tfoo_bar(é)\r\n\r\n  你好 x\r\n");
    let layout = Monospace {
        tab: 4,
        page_rows: 2,
    };
    let mut selections = Selections::new(Selection::caret(0));
    for _ in 0..4 {
        selections.add(Selection::caret(rng.below(70) as usize));
    }
    let mut editor = Editor::from_parts(
        Buffer::new(&text),
        selections,
        History::new(),
    );
    for step in 0..2_000 {
        let command = match rng.below(10) {
            0 => Command::Backspace,
            1 => Command::Delete,
            2 => Command::Type("é🎉"),
            3 => Command::Enter,
            _ => Command::Move {
                motion: MOTIONS[rng.below(12) as usize],
                extend: rng.chance(40),
            },
        };
        let _ = editor.apply(command, &layout);
        let text = editor.buffer().to_string();
        for selection in editor.selections().iter() {
            assert!(
                on_a_boundary(&text, selection.anchor)
                    && on_a_boundary(&text, selection.head),
                "seed {seed} step {step} {command:?}: {selection:?} \
                 in {text:?}"
            );
        }
    }
}
