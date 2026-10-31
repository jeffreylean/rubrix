mod support;

use core::time::Duration;

use bevy::app::App;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::resource::Resource;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::text::{TextBackgroundColor, TextColor, TextSpan};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::px;
use bevy::ui::widget::Text;
use bevy_fynix::views::{FrameProps, column};
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::{
    DiffDoc, DiffEditStatus, DiffRow, DiffSource, Layout, Overflow,
    Row, RubrixPlugin, diff_view,
};
use support::diff::{
    Plain, doc, line_text, settle, sides, slots, texts,
};
use support::{
    chord, keep_copies, key_with, lay_out, point_in, press_on,
    type_text,
};

const OLD: &str =
    "fn a() {}\nlet keep = 1;\nlet x = 1;\nlet tail = 2;\n";
const NEW: &str =
    "fn a() {}\nlet keep = 1;\nlet x = 2;\nlet tail = 2;\n";

#[derive(Resource)]
struct Shape(Layout);

fn editing(source: DiffSource, layout: Layout) -> App {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        FynixPlugin::<Plain>::default(),
        RubrixPlugin,
    ))
    .insert_resource(Theme(Plain))
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(50),
    ))
    .insert_resource(Shape(layout));
    lay_out(&mut app, 1.0);
    keep_copies(&mut app);
    mount::<Plain>(
        app.world_mut(),
        column((diff_view(source)
            .layout(resource::<Shape, _>(|shape| shape.0))
            .overflow(Overflow::Scroll)
            .edit(true),))
        .width(px(900.0))
        .height(px(600.0)),
    );
    settle(&mut app);
    app
}

/// The first row of file `file` that `pick` accepts.
fn row_where(
    app: &mut App,
    file: usize,
    pick: impl Fn(Row) -> bool,
) -> usize {
    let doc = doc(app);
    (0..doc.len())
        .find(|&index| {
            matches!(
                doc.row(index),
                Some(DiffRow::Body { file: of, row }) if of == file && pick(row)
            )
        })
        .expect("a matching row")
}

fn new_text(app: &mut App, file: usize) -> String {
    doc(app).diff(file).new_lines().text().to_owned()
}

fn old_text(app: &mut App, file: usize) -> String {
    doc(app).diff(file).old_lines().text().to_owned()
}

fn status(app: &mut App) -> Option<DiffEditStatus> {
    let world = app.world_mut();
    let doc =
        world.query::<&DiffDoc>().single(world).expect("one view");
    doc.edit_status(world)
}

/// Presses at byte `byte` of the stacked line text of row `index`.
fn click_stacked(app: &mut App, index: usize, byte: usize) {
    let text = line_text(app, index);
    let at = point_in(app, text, byte);
    press_on(app, text, at, 1);
    settle(app);
}

#[test]
fn a_keystroke_after_a_press_on_an_old_side_row_changes_nothing() {
    let mut app =
        editing(DiffSource::texts("a.rs", OLD, NEW), Layout::Stacked);
    let deletion = row_where(&mut app, 0, |row| {
        matches!(row, Row::Deletion { .. })
    });
    click_stacked(&mut app, deletion, 4);
    type_text(&mut app, "zz");
    settle(&mut app);
    assert_eq!(new_text(&mut app, 0), NEW);
    assert_eq!(old_text(&mut app, 0), OLD);
    assert!(status(&mut app).is_none());
}

#[test]
fn a_press_on_an_added_row_types_into_the_new_side() {
    let mut app =
        editing(DiffSource::texts("a.rs", OLD, NEW), Layout::Stacked);
    let addition = row_where(&mut app, 0, |row| {
        matches!(row, Row::Addition { .. })
    });
    click_stacked(&mut app, addition, "let x = 2".len());
    type_text(&mut app, "0");
    settle(&mut app);
    assert_eq!(new_text(&mut app, 0), NEW.replace("x = 2", "x = 20"));
    assert_eq!(old_text(&mut app, 0), OLD);
    let status = status(&mut app).expect("a caret");
    assert_eq!((status.file, status.hunks, status.undo), (0, 1, 1));
    assert_eq!(status.hunk.as_deref(), Some("@@ -1,4 +1,4 @@"));
}

#[test]
fn undo_in_one_file_leaves_another_files_edit_alone() {
    let source =
        DiffSource::pairs([("a.rs", OLD, NEW), ("b.rs", OLD, NEW)]);
    let mut app = editing(source, Layout::Stacked);
    for file in [0, 1] {
        let addition = row_where(&mut app, file, |row| {
            matches!(row, Row::Addition { .. })
        });
        click_stacked(&mut app, addition, 0);
        type_text(&mut app, if file == 0 { "a" } else { "b" });
        settle(&mut app);
    }
    assert_eq!(new_text(&mut app, 0), NEW.replace("let x", "alet x"));
    assert_eq!(new_text(&mut app, 1), NEW.replace("let x", "blet x"));
    chord(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyZ, "z");
    settle(&mut app);
    assert_eq!(new_text(&mut app, 1), NEW);
    assert_eq!(new_text(&mut app, 0), NEW.replace("let x", "alet x"));
    assert_slots_show_their_rows(&mut app);
}

#[test]
fn split_rows_edit_the_new_column_and_leave_the_old_one() {
    let mut app =
        editing(DiffSource::texts("a.rs", OLD, NEW), Layout::Split);
    let paired = row_where(&mut app, 0, |row| {
        matches!(row, Row::Paired { .. })
    });
    let cells = sides(&mut app, paired);
    let at = point_in(&app, cells.new, "let x = ".len());
    press_on(&mut app, cells.new, at, 1);
    settle(&mut app);
    type_text(&mut app, "4");
    settle(&mut app);
    assert_eq!(new_text(&mut app, 0), NEW.replace("x = 2", "x = 42"));

    let cells = sides(&mut app, paired);
    let at = point_in(&app, cells.old, "let x = ".len());
    press_on(&mut app, cells.old, at, 1);
    settle(&mut app);
    type_text(&mut app, "9");
    settle(&mut app);
    assert_eq!(new_text(&mut app, 0), NEW.replace("x = 2", "x = 42"));
    assert_eq!(old_text(&mut app, 0), OLD);
}

#[test]
fn an_enter_inside_a_hunk_moves_the_new_numbers_below_and_keeps_the_old()
 {
    let mut app =
        editing(DiffSource::texts("a.rs", OLD, NEW), Layout::Stacked);
    let addition = row_where(&mut app, 0, |row| {
        matches!(row, Row::Addition { .. })
    });
    click_stacked(&mut app, addition, 0);
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    settle(&mut app);
    let tail = row_where(&mut app, 0, |row| {
        row.old_line().is_some_and(|line| line.index == 3)
    });
    let Some(DiffRow::Body { row, .. }) = doc(&mut app).row(tail)
    else {
        panic!("a body row");
    };
    assert_eq!(row.old_line().map(|line| line.number.get()), Some(4));
    assert_eq!(row.new_line().map(|line| line.number.get()), Some(5));
    assert_slots_show_their_rows(&mut app);
}

/// Each built slot's text runs, with their colours and backgrounds,
/// by row index.
fn drawn(app: &mut App) -> Vec<(usize, Vec<String>)> {
    slots(app)
        .into_iter()
        .map(|(index, slot)| {
            let world = app.world();
            let mut found = Vec::new();
            let mut stack = vec![slot];
            while let Some(node) = stack.pop() {
                let text = world
                    .get::<Text>(node)
                    .map(|text| text.0.clone())
                    .or_else(|| {
                        world
                            .get::<TextSpan>(node)
                            .map(|span| span.0.clone())
                    });
                if let Some(text) = text {
                    found.push(format!(
                        "{text:?} {:?} {:?}",
                        world
                            .get::<TextColor>(node)
                            .map(|colour| colour.0),
                        world
                            .get::<TextBackgroundColor>(node)
                            .map(|colour| colour.0),
                    ));
                }
                if let Some(children) = world.get::<Children>(node) {
                    stack.extend(children.iter().rev());
                }
            }
            (index, found)
        })
        .collect()
}

/// Asserts that every built slot shows its row: its line numbers and
/// its line's text.
fn assert_slots_show_their_rows(app: &mut App) {
    for (index, slot) in slots(app) {
        let Some(DiffRow::Body { file, row }) = doc(app).row(index)
        else {
            continue;
        };
        if matches!(row, Row::Gap { .. }) {
            continue;
        }
        let shown = texts(app, slot);
        let numbers: Vec<String> = [row.old_line(), row.new_line()]
            .into_iter()
            .flatten()
            .map(|line| line.number.to_string())
            .collect();
        let gutter: Vec<String> =
            shown[0].split_whitespace().map(str::to_owned).collect();
        assert_eq!(
            gutter, numbers,
            "row {index} {row:?} shows {shown:?}"
        );
        let diff = doc(app).diff(file);
        let line = match row {
            Row::Deletion { old } => diff.old_lines().line(old.index),
            _ => diff
                .new_lines()
                .line(row.new_line().expect("a new line").index),
        };
        assert_eq!(
            shown.last().map(String::as_str),
            Some(line),
            "row {index} {row:?}"
        );
    }
}

fn numbered_with(count: usize, changed: &[(usize, &str)]) -> String {
    (1..=count)
        .map(|line| {
            let text = changed
                .iter()
                .find(|&&(at, _)| at == line)
                .map_or_else(
                    || format!("let line_{line} = {line};"),
                    |(_, text)| (*text).to_owned(),
                );
            format!("{text}\n")
        })
        .collect()
}

#[test]
fn rows_shown_after_a_revert_match_their_rows() {
    let old = numbered_with(40, &[]);
    let new = numbered_with(
        40,
        &[(5, "let line_5 = 50;"), (30, "let line_30 = 300;")],
    );
    let mut app = editing(
        DiffSource::texts("a.rs", &old, &new),
        Layout::Stacked,
    );
    let addition = row_where(
        &mut app,
        0,
        |row| matches!(row, Row::Addition { new } if new.index == 4),
    );
    click_stacked(&mut app, addition, "let line_5 = 50".len());
    key_with(&mut app, &[], KeyCode::Backspace, Key::Backspace);
    settle(&mut app);
    assert_eq!(
        new_text(&mut app, 0),
        numbered_with(40, &[(30, "let line_30 = 300;")])
    );
    assert_eq!(doc(&mut app).diff(0).hunks().len(), 1);
    assert_slots_show_their_rows(&mut app);
}

#[test]
fn an_edit_redraws_the_word_marks_of_both_rows_of_a_pair() {
    let old = "let total = sum(a);\n";
    let new = "let count = sum(b);\n";
    let edited = "let count = sum(a);\n";
    let mut app =
        editing(DiffSource::texts("a.rs", old, new), Layout::Stacked);
    let addition = row_where(&mut app, 0, |row| {
        matches!(row, Row::Addition { .. })
    });
    click_stacked(&mut app, addition, "let count = sum(b".len());
    key_with(&mut app, &[], KeyCode::Backspace, Key::Backspace);
    type_text(&mut app, "a");
    settle(&mut app);
    assert_eq!(new_text(&mut app, 0), edited);
    let mut fresh = editing(
        DiffSource::texts("a.rs", old, edited),
        Layout::Stacked,
    );
    assert_eq!(drawn(&mut app), drawn(&mut fresh));
}
