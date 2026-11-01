use rubrix_edit::Affinity::{self, Downstream, Upstream};
use rubrix_edit::{
    Buffer, Command, EditState, Editor, History, Layout, Monospace,
    Motion, Selection, Selections, Wrapped,
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
fn line_start_goes_to_column_zero_from_anywhere_on_the_line() {
    let text = "first\n    let x = 1;";
    for at in [text.len(), 10, 6] {
        let mut editor = editor_at(text, at);
        assert_eq!(
            stops(&mut editor, go(Motion::LineStart), 2),
            [6, 6]
        );
    }
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
        page_rows: 10,
        ..Monospace::default()
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
    let x = |at| layout.x(&buffer, at, Downstream);
    assert_eq!(x(1), 4.0);
    assert_eq!(x(3), 6.0);
    assert_eq!(x(7), 4.0);
    let up =
        |rows, x| layout.vertical(&buffer, 13, Downstream, rows, x);
    assert_eq!(up(-1, 3.0), Some((6, Downstream)));
    assert_eq!(up(-2, 2.0), Some((0, Downstream)));
    assert_eq!(up(-3, 2.0), None);
    assert_eq!(layout.vertical(&buffer, 0, Downstream, 3, 2.0), None);
    assert_eq!(
        layout.vertical(&buffer, 0, Downstream, 1, f32::INFINITY),
        Some((8, Downstream)),
        "an x past the row's end lands on its end"
    );
}

const INDENTED: &str = "  abcdefghij  klmnop";

struct WrapAt(&'static [usize]);

impl WrapAt {
    fn wrapped(&self, buffer: &Buffer, line: u32) -> SoftRows {
        SoftRows(
            self.0,
            buffer.line_end(line) - buffer.line_start(line),
        )
    }
}

struct SoftRows(&'static [usize], usize);

impl SoftRows {
    fn index(&self, at: usize, affinity: Affinity) -> usize {
        let after = match affinity {
            Downstream => {
                self.0.partition_point(|&start| start <= at)
            }
            Upstream => self.0.partition_point(|&start| start < at),
        };
        after.saturating_sub(1)
    }

    fn range(&self, index: usize) -> std::ops::Range<usize> {
        self.0[index]
            ..self.0.get(index + 1).map_or(self.1, |&next| next)
    }

    fn count(&self) -> i32 {
        self.0.len() as i32
    }
}

impl Layout for WrapAt {
    fn x(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> f32 {
        let line = buffer.line_of(offset);
        let rel = offset - buffer.line_start(line);
        let rows = self.wrapped(buffer, line);
        (rel - rows.range(rows.index(rel, affinity)).start) as f32
    }

    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        affinity: Affinity,
        rows: i32,
        x: f32,
    ) -> Option<(usize, Affinity)> {
        let mut line = buffer.line_of(from);
        let rel = from - buffer.line_start(line);
        let mut index =
            self.wrapped(buffer, line).index(rel, affinity) as i32
                + rows;
        while index < 0 {
            line = line.checked_sub(1)?;
            index += self.wrapped(buffer, line).count();
        }
        while index >= self.wrapped(buffer, line).count() {
            index -= self.wrapped(buffer, line).count();
            line += 1;
            if line >= buffer.line_count() {
                return None;
            }
        }
        let start = buffer.line_start(line);
        let row = self.wrapped(buffer, line).range(index as usize);
        let at = (row.start + x as usize).min(row.end);
        let line_end = buffer.line_end(line);
        let row = start + row.start..start + row.end;
        Some((
            start + at,
            Affinity::on_row(start + at, &row, line_end),
        ))
    }

    fn row(
        &self,
        buffer: &Buffer,
        offset: usize,
        affinity: Affinity,
    ) -> std::ops::Range<usize> {
        let line = buffer.line_of(offset);
        let start = buffer.line_start(line);
        let rows = self.wrapped(buffer, line);
        let row = rows.range(rows.index(offset - start, affinity));
        start + row.start..start + row.end
    }

    fn page_rows(&self) -> u32 {
        1
    }
}

const INDENTED_ROWS: WrapAt = WrapAt(&[0, 12]);

fn heads(editor: &Editor) -> (usize, Affinity) {
    let selections = editor.selections();
    (selections.primary().head, selections.affinity(0))
}

#[test]
fn home_and_end_resolve_on_the_soft_row() {
    let mut editor = editor_at(INDENTED, 16);
    let _ = editor.apply(go(Motion::LineStart), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Downstream), "start");
    let mut editor = editor_at(INDENTED, 16);
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (20, Downstream), "end");
    let mut editor = editor_at(INDENTED, 16);
    let _ = editor.apply(go(Motion::Home), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (14, Downstream), "textStart");
    let _ = editor.apply(go(Motion::Home), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Downstream), "textStart again");
}

#[test]
fn end_on_a_wrapped_row_stays_upstream_of_the_break() {
    let mut editor = editor_at(INDENTED, 5);
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Upstream));
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Upstream), "a second End stays");
    let _ = editor.apply(go(Motion::Home), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (2, Downstream),
        "Home goes to row 0's indent"
    );
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    let _ = editor.apply(go(Motion::LineStart), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (0, Downstream),
        "LineStart goes to row 0's start"
    );
}

#[test]
fn home_on_a_continuation_row_stays_on_it() {
    let mut editor = editor_at(INDENTED, 12);
    let _ = editor.apply(go(Motion::Home), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (14, Downstream));
    let _ = editor.apply(go(Motion::Home), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Downstream));
    let _ = editor.apply(go(Motion::LineStart), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Downstream));
}

#[test]
fn vertical_motion_from_a_row_end_keeps_its_row() {
    let text = format!("{INDENTED}\n{INDENTED}");
    let mut editor = editor_at(&text, 5);
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Upstream));
    let _ = editor.apply(go(Motion::Down), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (20, Downstream),
        "down lands at the end of row 1"
    );
    let _ = editor.apply(go(Motion::Up), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (12, Upstream),
        "up comes back to the end of row 0"
    );
    let _ = editor.apply(go(Motion::Up), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (0, Downstream),
        "up on the first row goes to the text's start"
    );
    let mut editor = editor_at(&text, 2);
    let _ = editor.apply(go(Motion::Down), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (14, Downstream),
        "down from row 0 lands on row 1 of the same line"
    );
    let _ = editor.apply(go(Motion::Down), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (23, Downstream),
        "then on the next line"
    );
}

#[test]
fn typing_and_selection_changes_reset_the_affinity() {
    let mut editor = editor_at(INDENTED, 5);
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Upstream));
    let _ = editor.apply(Command::Type("x"), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (13, Downstream));
    let mut editor = editor_at(INDENTED, 5);
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    editor.set_selections(Selections::new(Selection::caret(12)));
    assert_eq!(heads(&editor), (12, Downstream));
    let mut editor = editor_at(INDENTED, 5);
    let _ = editor.apply(shift(Motion::End), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (12, Upstream));
    let _ = editor.apply(shift(Motion::Right), &INDENTED_ROWS);
    assert_eq!(
        heads(&editor),
        (13, Downstream),
        "an extended Right steps on from the break"
    );
    assert_eq!(selection(&editor).anchor, 5);
}

#[test]
fn set_selections_keeps_a_head_affinity_the_snap_leaves_in_place() {
    let mut editor = editor_at(INDENTED, 5);
    editor.set_selections(Selections::with_affinity(
        Selection::caret(12),
        Upstream,
    ));
    assert_eq!(heads(&editor), (12, Upstream));
    let _ = editor.apply(go(Motion::Home), &INDENTED_ROWS);
    assert_eq!(heads(&editor), (2, Downstream), "Home acts on row 0");
    let mut editor = editor_at("e\u{301}xyz", 0);
    editor.set_selections(Selections::with_affinity(
        Selection::caret(1),
        Upstream,
    ));
    assert_eq!(
        heads(&editor),
        (0, Downstream),
        "a head snapped out of a cluster"
    );
}

#[test]
fn a_kept_head_keeps_its_affinity_through_a_save_and_a_merge() {
    let mut editor = editor_at(INDENTED, 5);
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    let restored = EditState::save(&editor, 0).restore();
    assert_eq!(heads(&restored), (12, Upstream));
    let mut selections = Selections::new(Selection::caret(2));
    selections.add(Selection::caret(5));
    let mut editor = Editor::from_parts(
        Buffer::new(INDENTED),
        selections,
        History::new(),
    );
    let _ = editor.apply(go(Motion::End), &INDENTED_ROWS);
    assert_eq!(editor.selections().len(), 1);
    assert_eq!(heads(&editor), (12, Upstream));
}

#[test]
fn a_wrapping_monospace_breaks_before_the_word_that_overflows() {
    let starts = |text, columns| {
        let rows = Wrapped::wrap(text, columns, 4);
        (0..rows.count())
            .map(|i| rows.range(i).start)
            .collect::<Vec<_>>()
    };
    assert_eq!(starts("let x = 1;", None), [0]);
    assert_eq!(starts("aaa bbb ccc", Some(5)), [0, 4, 8]);
    assert_eq!(
        starts("abcdefgh", Some(3)),
        [0, 3, 6],
        "a long word breaks at the column"
    );
    assert_eq!(
        starts("ab    cd", Some(4)),
        [0, 6],
        "spaces hang past the edge"
    );
    assert_eq!(
        starts("\tab cd", Some(6)),
        [0, 4],
        "a tab takes its stop's columns"
    );
    assert_eq!(
        starts("é你好", Some(2)),
        [0, 5],
        "rows start on grapheme boundaries"
    );
}

#[test]
fn end_and_home_on_a_mid_word_break() {
    let layout = Monospace {
        columns: Some(4),
        ..Monospace::default()
    };
    let text = "abcdefgh\nij";
    let mut editor = editor_at(text, 1);
    let _ = editor.apply(go(Motion::End), &layout);
    assert_eq!(heads(&editor), (4, Upstream));
    let _ = editor.apply(go(Motion::Right), &layout);
    assert_eq!(heads(&editor), (5, Downstream));
    let _ = editor.apply(go(Motion::Home), &layout);
    assert_eq!(
        heads(&editor),
        (4, Downstream),
        "Home on row 1 goes to its start"
    );
    let _ = editor.apply(go(Motion::End), &layout);
    assert_eq!(
        heads(&editor),
        (8, Downstream),
        "End on the last row is the line's end"
    );
    let _ = editor.apply(go(Motion::Down), &layout);
    assert_eq!(heads(&editor), (11, Downstream));
    let _ = editor.apply(go(Motion::Up), &layout);
    assert_eq!(
        heads(&editor),
        (8, Downstream),
        "up keeps the goal x of the End before the short line"
    );
    assert_eq!(layout.x(editor.buffer(), 4, Upstream), 4.0);
    assert_eq!(layout.x(editor.buffer(), 4, Downstream), 0.0);
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

fn on_a_boundary(text: &str, at: usize) -> bool {
    text.is_char_boundary(at)
        && !(text[..at].ends_with('\r')
            && text[at..].starts_with('\n'))
}

#[test]
fn random_motions_keep_carets_on_boundaries() {
    const MOTIONS: [Motion; 13] = [
        Motion::Left,
        Motion::Right,
        Motion::WordLeft,
        Motion::WordRight,
        Motion::Home,
        Motion::LineStart,
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
        page_rows: 2,
        columns: Some(6),
        ..Monospace::default()
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
                motion: MOTIONS[rng.below(13) as usize],
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
