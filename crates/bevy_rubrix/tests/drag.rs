mod support;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::math::{Rect, Vec2};
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform};
use bevy_fynix::views::VirtualList;
use bevy_rubrix::{EditDoc, EditView, Lang};
use support::{
    drag_on, edit_line, hold_on, laid_out, list, mount_editor,
    point_at, release_on,
};

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn head(app: &App, doc: Entity) -> usize {
    app.world()
        .get::<EditDoc>(doc)
        .expect("a doc")
        .editor()
        .selections()
        .primary()
        .head
}

/// The list's box in logical window px, its scrollbars included.
fn list_box(app: &mut App) -> Rect {
    let area = list(app);
    let world = app.world();
    let node = world.get::<ComputedNode>(area).expect("a node");
    let at =
        world.get::<UiGlobalTransform>(area).expect("a transform");
    let logical = node.inverse_scale_factor();
    let content = node.content_box();
    Rect::from_corners(
        (at.translation + content.min) * logical,
        (at.translation + content.max) * logical,
    )
}

fn scroll_x(app: &mut App) -> f32 {
    let area = list(app);
    app.world().get::<ScrollPosition>(area).expect("a scroll").x
}

fn caret_shown(app: &App, view: Entity) -> bool {
    app.world()
        .get::<EditView>(view)
        .expect("a view")
        .caret_shown()
}

/// Holds the pointer at `to` for `frames` frames of a drag from
/// `from`.
fn hold_at(
    app: &mut App,
    on: Entity,
    from: Vec2,
    to: Vec2,
    frames: u32,
) {
    for _ in 0..frames {
        drag_on(app, on, from, to);
    }
}

#[test]
fn a_drag_held_below_or_right_of_the_rows_scrolls_and_extends() {
    let mut app = laid_out();
    let text: String =
        (0..200).map(|n| format!("line {n}\n")).collect();
    let (view, doc) =
        mount_editor(&mut app, Lang::Plain, &text, 600.0, 300.0);
    settle(&mut app);
    let row = edit_line(&mut app, view, 2);
    let from = point_at(&app, row, 2);
    hold_on(&mut app, row, from, 1);
    let shown = list_box(&mut app);
    hold_at(
        &mut app,
        row,
        from,
        Vec2::new(from.x, shown.max.y + 20.0),
        30,
    );
    let area = list(&mut app);
    let first = app
        .world()
        .get::<VirtualList>(area)
        .expect("a list")
        .first_visible();
    assert!(first > 0, "the view scrolls down");
    let line = text[..head(&app, doc)].matches('\n').count();
    assert!(line > first, "the head follows to line {line}");
    release_on(&mut app, row, from);

    settle(&mut app);
    let top = edit_line(&mut app, view, first as u32 + 1);
    let from = point_at(&app, top, 0);
    hold_on(&mut app, top, from, 1);
    let below = edit_line(&mut app, view, first as u32 + 3);
    let right =
        Vec2::new(shown.max.x + 20.0, point_at(&app, below, 0).y);
    hold_at(&mut app, top, from, right, 2);
    let end = text
        .match_indices('\n')
        .nth(first + 3)
        .map(|(at, _)| at)
        .expect("line ends");
    assert_eq!(
        head(&app, doc),
        end,
        "past the right edge, the head is at the line's end"
    );
}

#[test]
fn a_drag_held_past_a_side_scrolls_the_view_sideways() {
    let mut app = laid_out();
    let long = "x".repeat(300);
    let text = format!("{long}\nshort\n");
    let (view, doc) =
        mount_editor(&mut app, Lang::Plain, &text, 600.0, 300.0);
    settle(&mut app);
    let row = edit_line(&mut app, view, 0);
    let shown = list_box(&mut app);
    let from = point_at(&app, row, 10);
    hold_on(&mut app, row, from, 1);
    hold_at(
        &mut app,
        row,
        from,
        Vec2::new(shown.max.x + 40.0, from.y),
        60,
    );
    assert!(
        scroll_x(&mut app) > 0.0,
        "past the right, the view scrolls right"
    );
    assert_eq!(head(&app, doc), long.len(), "to the line's end");
    assert!(caret_shown(&app, view), "with the caret in view");
    release_on(&mut app, row, from);

    settle(&mut app);
    let row = edit_line(&mut app, view, 0);
    let from = Vec2::new((shown.min.x + shown.max.x) / 2.0, from.y);
    hold_on(&mut app, row, from, 1);
    hold_at(
        &mut app,
        row,
        from,
        Vec2::new(shown.min.x - 40.0, from.y),
        60,
    );
    assert_eq!(
        scroll_x(&mut app),
        0.0,
        "past the left, the view scrolls back"
    );
    assert_eq!(head(&app, doc), 0, "to the line's start");
    assert!(caret_shown(&app, view), "with the caret in view");
}
