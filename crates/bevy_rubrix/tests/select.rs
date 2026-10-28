mod support;

use core::ops::Range;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::input::keyboard::KeyCode;
use bevy::math::{Rect, Vec2};
use bevy::text::TextLayoutInfo;
use bevy::ui::{ComputedNode, Display, Node, UiGlobalTransform};
use bevy_fynix::views::VirtualList;
use bevy_rubrix::{CodeSelection, CodeSource, Lang, Overflow};
use support::{
    Wrapping, copy, corner, ctrl, doc, drag_end_on, drag_on, hold_on,
    keep_copies, laid_out, line, list, list_state, mount_view,
    point_in, press_on, shift, spans_of, view,
};

fn selecting(source: CodeSource) -> App {
    selecting_in(source, 600.0)
}

fn selecting_in(source: CodeSource, width: f32) -> App {
    let mut app = laid_out();
    keep_copies(&mut app);
    mount_view(&mut app, source, width, 400.0);
    settle(&mut app);
    app
}

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn point_of(app: &mut App, row: usize, byte: usize) -> Vec2 {
    let root = line(app, row);
    point_in(app, root, byte)
}

fn press_at(app: &mut App, row: usize, byte: usize, count: u8) {
    let at = point_of(app, row, byte);
    press_point(app, row, at, count);
}

fn press_point(app: &mut App, row: usize, at: Vec2, count: u8) {
    let on = line(app, row);
    press_on(app, on, at, count);
}

fn hold_at(app: &mut App, row: usize, byte: usize, count: u8) {
    let at = point_of(app, row, byte);
    let on = line(app, row);
    hold_on(app, on, at, count);
}

fn selected(app: &mut App) -> Range<usize> {
    let view = view(app);
    app.world()
        .get::<CodeSelection>(view)
        .expect("a selection")
        .range()
}

const SNIPPET: &str =
    "fn main() {\n    self.foo_bar = 1;\n\n    x\n}\n";

#[test]
fn a_double_press_selects_one_identifier() {
    let mut app = selecting(CodeSource::new(Lang::Rust, SNIPPET));
    let foo = "    self.".len() + 2;
    press_at(&mut app, 1, foo, 2);
    let start = SNIPPET.find("foo_bar").expect("the identifier");
    assert_eq!(selected(&mut app), start..start + "foo_bar".len());
    press_at(&mut app, 1, "    se".len(), 2);
    let start = SNIPPET.find("self").expect("self");
    assert_eq!(selected(&mut app), start..start + 4);
}

#[test]
fn a_triple_press_selects_the_line_and_copies_it() {
    let mut app = selecting(CodeSource::new(Lang::Rust, SNIPPET));
    press_at(&mut app, 1, 3, 3);
    assert_eq!(selected(&mut app), doc(&mut app).line_range(1));
    assert_eq!(copy(&mut app), "    self.foo_bar = 1;");
}

#[test]
fn a_copy_across_a_blank_line_keeps_it() {
    let mut app = selecting(CodeSource::new(Lang::Rust, SNIPPET));
    press_at(&mut app, 1, 4, 1);
    shift(&mut app, true);
    press_at(&mut app, 3, 5, 1);
    shift(&mut app, false);
    assert_eq!(copy(&mut app), "self.foo_bar = 1;\n\n    x");
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

#[test]
fn a_selection_outlives_its_anchor_row() {
    let text = numbered(10_000);
    let mut app =
        selecting(CodeSource::new(Lang::Plain, text.as_str()));
    press_at(&mut app, 3, 2, 1);
    shift(&mut app, true);
    press_at(&mut app, 5, 4, 1);
    shift(&mut app, false);
    let before = selected(&mut app);
    let anchor = line(&mut app, 3);

    let list = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(list)
        .expect("a list")
        .scroll_to(5_000);
    settle(&mut app);
    assert!(
        !list_state(&mut app).spawned().expect("rows").contains(&3),
        "the anchor row is gone"
    );
    assert!(
        support::lines(&mut app)
            .iter()
            .all(|&(row, root)| root != anchor || row != 3),
        "its entity shows another row"
    );
    assert_eq!(selected(&mut app), before);

    app.world_mut()
        .get_mut::<VirtualList>(list)
        .expect("a list")
        .scroll_to(0);
    settle(&mut app);
    assert_eq!(selected(&mut app), before);
    let start = text.find("line 4").expect("line 4") + 2;
    let end = text.find("line 6").expect("line 6") + 4;
    assert_eq!(copy(&mut app), text[start..end]);
}

#[test]
fn ctrl_a_selects_the_whole_source_built_or_not() {
    let text = numbered(10_000);
    let mut app =
        selecting(CodeSource::new(Lang::Plain, text.as_str()));
    ctrl(&mut app, KeyCode::KeyA, "a");
    assert_eq!(selected(&mut app), 0..text.len());
    assert_eq!(copy(&mut app), text);
}

const LONG_LINE: &str =
    include_str!("../examples/gallery/fixtures/long-line.rs");

const GALLERY_VIEW: f32 = 1280.0 - 200.0 - 2.0 * 20.0;

const WRAPPED_ROW: usize = 2;

fn long_line_wrapped() -> App {
    let mut app = selecting_in(
        CodeSource::new(Lang::Rust, LONG_LINE),
        GALLERY_VIEW,
    );
    app.world_mut().resource_mut::<Wrapping>().0 = Overflow::Wrap;
    settle(&mut app);
    app
}

fn drawn_lines(app: &App, root: Entity) -> Vec<Range<f32>> {
    let corner = corner(app, root);
    let info =
        app.world().get::<TextLayoutInfo>(root).expect("drawn");
    let mut lines = info
        .run_geometry
        .iter()
        .map(|run| {
            corner.y + run.bounds.min.y..corner.y + run.bounds.max.y
        })
        .collect::<Vec<_>>();
    lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    lines.dedup();
    lines
}

#[test]
fn a_press_on_a_wrapped_line_selects_the_byte_drawn_under_it() {
    let mut app = long_line_wrapped();
    let root = line(&mut app, WRAPPED_ROW);
    assert!(drawn_lines(&app, root).len() >= 3, "line 3 wraps");
    assert!(spans_of(&app, root).len() > 5, "highlighted spans");
    let text =
        doc(&mut app).line(WRAPPED_ROW).expect("line 3").to_owned();
    let letter =
        |at: usize| text.as_bytes()[at].is_ascii_alphabetic();
    let line_start = doc(&mut app).line_range(WRAPPED_ROW).start;
    let corner = corner(&app, root);
    let info =
        app.world().get::<TextLayoutInfo>(root).expect("drawn");
    let glyphs = &info.glyphs;
    assert_eq!(glyphs.len(), text.len(), "a glyph per byte");
    let between = (1..glyphs.len())
        .filter(|&at| letter(at - 1) && letter(at))
        .filter(|&at| {
            glyphs[at - 1].line_index == glyphs[at].line_index
        })
        .step_by(5)
        .map(|at| {
            let (left, right) = (&glyphs[at - 1], &glyphs[at]);
            let x = (left.position.x + right.position.x) / 2.0 + 1.0;
            (
                at,
                right.line_index,
                corner + Vec2::new(x, right.position.y),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        between.iter().filter(|&&(_, line, _)| line > 0).count()
            >= 10,
        "points on the later visual lines"
    );
    for (at, line, point) in between {
        press_point(&mut app, WRAPPED_ROW, point, 1);
        let byte = line_start + at;
        assert_eq!(
            selected(&mut app),
            byte..byte,
            "drawn between bytes {} and {at} on visual line {line}",
            at - 1
        );
    }
}

fn rects_of(app: &App, root: Entity) -> Vec<Rect> {
    let world = app.world();
    let cell = world.get::<ChildOf>(root).expect("a cell").parent();
    let layer = world.get::<Children>(cell).expect("children")[0];
    world
        .get::<Children>(layer)
        .map(|rects| {
            rects
                .iter()
                .copied()
                .filter(|&rect| {
                    world.get::<Node>(rect).expect("a node").display
                        != Display::None
                })
                .map(|rect| {
                    let node = world
                        .get::<ComputedNode>(rect)
                        .expect("a node");
                    let centre = world
                        .get::<UiGlobalTransform>(rect)
                        .expect("a transform")
                        .translation;
                    Rect::from_center_size(centre, node.size())
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_selected_wrapped_line_keeps_every_row_and_rects_its_drawn_lines()
{
    let mut app = long_line_wrapped();
    let rows = doc(&mut app).rows();
    let spans = |app: &mut App| {
        let list = list_state(app);
        (0..rows)
            .map(|row| list.row_span(row).expect("a row"))
            .map(|span| (span.top, span.height))
            .collect::<Vec<_>>()
    };
    let before = spans(&mut app);
    press_at(&mut app, WRAPPED_ROW, 3, 3);
    assert_eq!(
        selected(&mut app),
        doc(&mut app).line_range(WRAPPED_ROW)
    );
    settle(&mut app);
    assert_eq!(spans(&mut app), before, "row tops and heights");

    let root = line(&mut app, WRAPPED_ROW);
    let world = app.world();
    let text = world.get::<ComputedNode>(root).expect("a node");
    let centre = world
        .get::<UiGlobalTransform>(root)
        .expect("a transform")
        .translation;
    let (top, bottom) = (
        centre.y - text.size().y / 2.0,
        centre.y + text.size().y / 2.0,
    );
    let rects = rects_of(&app, root);
    for rect in &rects {
        assert!(
            rect.min.y >= top - 1.0 && rect.max.y <= bottom + 1.0,
            "{rect:?} within {top}..{bottom}"
        );
    }
    let mut lines = rects
        .iter()
        .map(|rect| rect.min.y..rect.max.y)
        .collect::<Vec<_>>();
    lines.sort_by(|a, b| a.start.total_cmp(&b.start));
    let drawn = drawn_lines(&app, root);
    assert_eq!(lines.len(), drawn.len(), "a rect per drawn line");
    for (rect, line) in lines.iter().zip(&drawn) {
        assert!(
            (rect.start - line.start).abs() < 0.5
                && (rect.end - line.end).abs() < 0.5,
            "rect {rect:?} on drawn line {line:?}"
        );
    }
}

#[test]
fn a_drag_below_a_following_stream_keeps_it_following() {
    let stream = CodeSource::streaming(Lang::Plain);
    let mut app = selecting(stream.source());
    for n in 0..60 {
        stream.push(&format!("line {n}\n"));
        app.update();
    }
    assert!(list_state(&mut app).following());
    let last = doc(&mut app).rows() - 1;
    hold_at(&mut app, last, 2, 1);
    let below = point_of(&mut app, last, 2) + Vec2::new(0.0, 200.0);
    let on = line(&mut app, last);
    let from = point_of(&mut app, last, 2);
    drag_on(&mut app, on, from, below);
    for n in 60..120 {
        stream.push(&format!("line {n}\n"));
        app.update();
        assert!(list_state(&mut app).following(), "following at {n}");
    }
    let rows = doc(&mut app).rows();
    let state = list_state(&mut app);
    assert_eq!(*state.spawned().expect("rows").end(), rows - 1);
    let range = selected(&mut app);
    assert!(
        range.end > doc(&mut app).line_range(last).end,
        "{range:?}"
    );
}

#[test]
fn a_drag_ended_outside_the_view_stops_scrolling() {
    let text = numbered(10_000);
    let mut app =
        selecting(CodeSource::new(Lang::Plain, text.as_str()));
    hold_at(&mut app, 3, 2, 1);
    let on = line(&mut app, 3);
    let from = point_of(&mut app, 3, 2);
    let below = from + Vec2::new(0.0, 600.0);
    drag_on(&mut app, on, from, below);
    settle(&mut app);
    assert!(list_state(&mut app).first_visible() > 0, "it scrolls");
    drag_end_on(&mut app, on, from, below);
    let first = list_state(&mut app).first_visible();
    let range = selected(&mut app);
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(list_state(&mut app).first_visible(), first);
    assert_eq!(selected(&mut app), range);
}
