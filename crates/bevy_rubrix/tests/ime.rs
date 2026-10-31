mod support;

use bevy::app::{App, Last};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Query, ResMut};
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::{FocusCause, InputFocus, InputFocusPlugin};
use bevy::math::{Rect, Vec2};
use bevy::text::{TextSpan, Underline};
use bevy::ui::{BackgroundColor, ComputedNode, UiGlobalTransform};
use bevy::ui_widgets::TextInputPlugin;
use bevy::window::{Ime, PrimaryWindow, Window};
use bevy_fynix::views::{VirtualList, rich_layout};
use bevy_rubrix::{
    EditDoc, EditRequest, EditView, Lang, Overflow, Preedit, Request,
    Selection, Selections,
};
use support::{
    Wrapping, edit_line, key_with, laid_out, mount_editor, text_of,
    type_text,
};

const TEXT: &str = "fn main() {\n    run();\n}\n";

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn editor(app: &App, doc: Entity) -> &rubrix_edit::Editor {
    app.world().get::<EditDoc>(doc).expect("a doc").editor()
}

fn text(app: &App, doc: Entity) -> String {
    editor(app, doc).buffer().to_string()
}

fn undo_depth(app: &App, doc: Entity) -> usize {
    editor(app, doc).history().undo_depth()
}

fn select(app: &mut App, doc: Entity, view: Entity, at: &[usize]) {
    let mut carets = Selections::new(Selection::caret(at[0]));
    for &caret in &at[1..] {
        carets.add(Selection::caret(caret));
    }
    app.world_mut().write_message(EditRequest {
        doc,
        view: Some(view),
        request: Request::select(carets),
    });
    app.update();
}

fn focus(app: &mut App, view: Entity) {
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
}

fn preedit(text: &str, cursor: Option<(usize, usize)>) -> Ime {
    Ime::Preedit {
        window: Entity::PLACEHOLDER,
        value: text.to_owned(),
        cursor,
    }
}

fn commit(text: &str) -> Ime {
    Ime::Commit {
        window: Entity::PLACEHOLDER,
        value: text.to_owned(),
    }
}

/// Writes `messages` and runs one frame, as winit sends them.
fn send(app: &mut App, messages: impl IntoIterator<Item = Ime>) {
    for message in messages {
        app.world_mut().write_message(message);
    }
    app.update();
}

/// An editor of [`TEXT`], focused, with its caret after line 1's
/// indent.
fn focused() -> (App, Entity, Entity) {
    let mut app = laid_out();
    let (view, doc) =
        mount_editor(&mut app, Lang::Rust, TEXT, 600.0, 300.0);
    settle(&mut app);
    select(&mut app, doc, view, &[TEXT.find("run").expect("run")]);
    focus(&mut app, view);
    settle(&mut app);
    (app, view, doc)
}

/// The texts of the underlined spans of `line`'s row.
fn underlined(app: &mut App, view: Entity, line: u32) -> Vec<String> {
    let row = edit_line(app, view, line);
    let world = app.world();
    world
        .get::<Children>(row)
        .into_iter()
        .flatten()
        .filter(|&&span| world.get::<Underline>(span).is_some())
        .filter_map(|&span| world.get::<TextSpan>(span))
        .map(|span| span.0.clone())
        .collect()
}

#[test]
fn a_preedit_shows_underlined_at_the_caret_and_edits_nothing() {
    let (mut app, view, doc) = focused();
    send(&mut app, [preedit("你好", Some((6, 6)))]);
    settle(&mut app);
    assert_eq!(text(&app, doc), TEXT, "the buffer holds no preedit");
    assert_eq!(undo_depth(&app, doc), 0, "nor does the history");
    let row = edit_line(&mut app, view, 1);
    assert_eq!(text_of(&app, row), "    你好run();");
    assert_eq!(underlined(&mut app, view, 1), ["你好"]);
    let shown = app.world().get::<Preedit>(view).expect("a preedit");
    assert_eq!(shown.cursor(), Some((6, 6)));
    let bars = app
        .world_mut()
        .query_filtered::<&ComputedNode, With<BackgroundColor>>()
        .iter(app.world())
        .map(ComputedNode::size)
        .filter(|size| size.x > 10.0 && (1.0..=2.0).contains(&size.y))
        .count();
    assert_eq!(bars, 1, "a bar at least a pixel tall under it");
}

/// The window-space rects of the preedit's underline bars, top first.
fn bars(app: &mut App) -> Vec<Rect> {
    let mut bars: Vec<Rect> = app
        .world_mut()
        .query_filtered::<(&ComputedNode, &UiGlobalTransform), With<BackgroundColor>>()
        .iter(app.world())
        .filter(|(node, at)| {
            node.size().x > 10.0
                && (1.0..=2.0).contains(&node.size().y)
                && at.translation.x > -1.0e4
        })
        .map(|(node, at)| {
            Rect::from_center_size(at.translation, node.size())
        })
        .collect();
    bars.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
    bars
}

/// The window-space y range of each visual line of `line`'s row.
fn visual_lines(
    app: &mut App,
    view: Entity,
    line: u32,
) -> Vec<(f32, f32)> {
    let row = edit_line(app, view, line);
    let world = app.world();
    let node = world.get::<ComputedNode>(row).expect("a node");
    let at =
        world.get::<UiGlobalTransform>(row).expect("a transform");
    let top = at.translation.y + node.content_box().min.y;
    rich_layout(world, row)
        .expect("a layout")
        .lines()
        .map(|visual| {
            let metrics = visual.metrics();
            (
                top + metrics.block_min_coord,
                top + metrics.block_max_coord,
            )
        })
        .collect()
}

#[test]
fn a_preedit_that_wraps_is_underlined_on_each_visual_line() {
    let (mut app, view, _) = focused();
    app.insert_resource(Wrapping(Overflow::Wrap));
    settle(&mut app);
    send(&mut app, [preedit(&"你好".repeat(20), Some((120, 120)))]);
    settle(&mut app);
    let lines = visual_lines(&mut app, view, 1);
    assert!(lines.len() >= 2, "the preedit wraps: {lines:?}");
    let bars = bars(&mut app);
    assert_eq!(
        bars.len(),
        lines.len(),
        "a bar per visual line: {bars:?}"
    );
    for (bar, (top, bottom)) in bars.iter().zip(&lines) {
        let middle = (top + bottom) / 2.0;
        assert!(
            middle <= bar.min.y && bar.max.y <= *bottom,
            "{bar:?} sits under the text of its own visual line \
             {top}..{bottom}, whose blocks overlap their neighbours'"
        );
    }
}

#[test]
fn a_preedit_of_emoji_only_is_underlined() {
    let (mut app, view, _) = focused();
    for emoji in ["🎉", "😀👍🎉"] {
        let end = emoji.len();
        send(&mut app, [preedit(emoji, Some((end, end)))]);
        settle(&mut app);
        let (top, bottom) = visual_lines(&mut app, view, 1)[0];
        let bars = bars(&mut app);
        assert_eq!(bars.len(), 1, "one bar under {emoji}: {bars:?}");
        assert!(
            top <= bars[0].min.y && bars[0].max.y <= bottom,
            "{:?} sits inside the row {top}..{bottom}",
            bars[0]
        );
    }
}

#[test]
fn a_commit_at_three_carets_is_one_undo_step() {
    let (mut app, view, doc) = focused();
    select(&mut app, doc, view, &[0, 12, 23]);
    let carets = editor(&app, doc).selections().clone();
    send(&mut app, [preedit("ab", None)]);
    send(&mut app, [preedit("", None), commit("ab")]);
    assert_eq!(
        text(&app, doc),
        "abfn main() {\nab    run();\nab}\n",
        "the commit goes in at each caret"
    );
    assert_eq!(undo_depth(&app, doc), 1);
    key_with(
        &mut app,
        &[KeyCode::ControlLeft],
        KeyCode::KeyZ,
        Key::Character("z".into()),
    );
    assert_eq!(text(&app, doc), TEXT, "one undo takes all three");
    assert_eq!(*editor(&app, doc).selections(), carets);
}

#[test]
fn winits_order_commits_once_and_shows_the_next_preedit() {
    let (mut app, view, doc) = focused();
    send(&mut app, [preedit("你", Some((3, 3)))]);
    send(
        &mut app,
        [
            preedit("", None),
            commit("你"),
            preedit("好", Some((3, 3))),
        ],
    );
    settle(&mut app);
    assert_eq!(text(&app, doc), "fn main() {\n    你run();\n}\n");
    assert_eq!(underlined(&mut app, view, 1), ["好"]);
    let row = edit_line(&mut app, view, 1);
    assert_eq!(text_of(&app, row), "    你好run();");
}

#[test]
fn keys_during_a_preedit_insert_nothing() {
    let (mut app, view, doc) = focused();
    send(&mut app, [preedit("ni", Some((2, 2)))]);
    type_text(&mut app, "x");
    key_with(&mut app, &[], KeyCode::Backspace, Key::Backspace);
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    assert_eq!(text(&app, doc), TEXT);
    send(&mut app, [preedit("", None), commit("你")]);
    assert!(app.world().get::<Preedit>(view).is_none());
    type_text(&mut app, "y");
    assert_eq!(text(&app, doc), "fn main() {\n    你yrun();\n}\n");
}

#[derive(Resource, Default)]
struct Seen(Vec<(bool, Vec2)>);

fn see(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut seen: ResMut<Seen>,
) {
    if let Ok(window) = windows.single() {
        seen.0.push((window.ime_enabled, window.ime_position));
    }
}

/// What winit would read in `Last`, after a frame.
fn last_seen(app: &mut App) -> (bool, Vec2) {
    app.update();
    *app.world().resource::<Seen>().0.last().expect("a frame")
}

#[test]
fn the_window_takes_ime_while_the_editor_has_focus() {
    let mut app = laid_out();
    app.add_plugins((InputFocusPlugin, TextInputPlugin))
        .init_resource::<Seen>()
        .add_systems(Last, see);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let (view, doc) =
        mount_editor(&mut app, Lang::Rust, TEXT, 600.0, 300.0);
    settle(&mut app);
    select(&mut app, doc, view, &[TEXT.find("run").expect("run")]);
    assert!(!last_seen(&mut app).0, "off without the focus");
    focus(&mut app, view);
    settle(&mut app);
    let (enabled, position) = last_seen(&mut app);
    assert!(enabled, "on while the editor has the focus");
    let caret = app
        .world()
        .get::<EditView>(view)
        .and_then(EditView::caret_rect)
        .expect("a drawn caret");
    assert_eq!(position, Vec2::new(caret.min.x, caret.max.y));
    app.world_mut().resource_mut::<InputFocus>().clear();
    assert!(!last_seen(&mut app).0, "off a frame after it leaves");
}

fn caret_rect(app: &App, view: Entity) -> Option<Rect> {
    app.world()
        .get::<EditView>(view)
        .and_then(EditView::caret_rect)
}

fn scroll_to(app: &mut App, row: usize) {
    app.world_mut()
        .query::<&mut VirtualList>()
        .single_mut(app.world_mut())
        .expect("one list")
        .scroll_to(row);
    settle(app);
}

#[test]
fn a_preedit_shows_a_hidden_caret_under_the_candidate_box() {
    let mut app = laid_out();
    app.add_plugins((InputFocusPlugin, TextInputPlugin))
        .init_resource::<Seen>()
        .add_systems(Last, see);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let text = "let x = 1;\n".repeat(200);
    let (view, doc) =
        mount_editor(&mut app, Lang::Rust, &text, 600.0, 300.0);
    settle(&mut app);
    select(&mut app, doc, view, &[4]);
    focus(&mut app, view);
    for cursor in [Some((0, 0)), None] {
        scroll_to(&mut app, 150);
        assert_eq!(
            caret_rect(&app, view),
            None,
            "the caret is out of view"
        );
        send(&mut app, [preedit("你好", cursor)]);
        settle(&mut app);
        let caret = caret_rect(&app, view)
            .expect("the preedit brings the caret into view");
        let (_, position) = last_seen(&mut app);
        assert_eq!(position, Vec2::new(caret.min.x, caret.max.y));
        send(&mut app, [preedit("", None)]);
    }
}
