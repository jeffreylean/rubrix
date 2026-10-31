use core::time::Duration;
use std::sync::mpsc;
use std::thread;

use bevy::app::App;
use bevy::asset::AssetApp;
use bevy::camera::{
    Camera, Camera2d, ComputedCameraValues, RenderTargetInfo,
};
use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::Tick;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use bevy::math::{UVec2, Vec2};
use bevy::text::{
    FontSize, InlineBox, LineHeight, StrikethroughColor,
    TextBackgroundColor, TextColor, TextFont, TextSpan, Underline,
    UnderlineColor,
};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::{
    ComputedNode, UiGlobalTransform, UiPlugin, UiScale, px,
};
use fynix::{Motion, ScopedExt};
use parley::{Affinity, Cluster, Cursor};

use crate::tests::{Plain, app, kids};
use crate::tokens::Tone;
use crate::views::{
    FrameProps, LineColor, Rich, Span, column, rich, rich_layout,
    rich_point,
};
use crate::{StateExt, hidden, mount, resource};

#[derive(Resource)]
struct Line(Vec<Span>);

fn words(count: usize) -> Vec<Span> {
    (0..count)
        .map(|index| Span::new(format!("w{index} ")))
        .collect()
}

fn line(app: &mut App, spans: Vec<Span>) -> Entity {
    app.insert_resource(Line(spans));
    let root = mount::<Plain>(
        app.world_mut(),
        rich(resource::<Line, _>(|line| line.0.clone())),
    );
    app.update();
    root
}

fn set_line(app: &mut App, spans: Vec<Span>) {
    app.world_mut().resource_mut::<Line>().0 = spans;
    app.update();
}

fn text(app: &App, span: Entity) -> &str {
    &app.world().get::<TextSpan>(span).expect("a span").0
}

fn color(app: &App, span: Entity) -> Color {
    app.world().get::<TextColor>(span).expect("a span").0
}

fn changed<C: Component>(app: &App, span: Entity) -> Tick {
    app.world()
        .entity(span)
        .get_change_ticks::<C>()
        .expect("the component")
        .changed
}

fn span_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&TextSpan>().iter(world).count()
}

fn ticks(app: &App, span: Entity) -> [Tick; 3] {
    [
        changed::<TextSpan>(app, span),
        changed::<TextFont>(app, span),
        changed::<TextColor>(app, span),
    ]
}

#[test]
fn editing_span_3_of_10_changes_only_its_components() {
    let mut app = app();
    let root = line(&mut app, words(10));
    let spans = kids(&app, root);
    let before = spans
        .iter()
        .map(|&span| ticks(&app, span))
        .collect::<Vec<_>>();

    let mut edited = words(10);
    edited[3] = Span::new("three ").color(Color::srgb(1.0, 0.0, 0.0));
    set_line(&mut app, edited);

    assert_eq!(kids(&app, root), spans, "no span respawned");
    for (index, &span) in spans.iter().enumerate() {
        let after = ticks(&app, span);
        if index == 3 {
            assert_ne!(after[0], before[3][0], "the text changed");
            assert_eq!(after[1], before[3][1], "the font did not");
            assert_ne!(after[2], before[3][2], "the colour changed");
        } else {
            assert_eq!(
                after, before[index],
                "span {index} untouched"
            );
        }
    }
    assert_eq!(text(&app, spans[3]), "three ");
    assert_eq!(color(&app, spans[3]), Color::srgb(1.0, 0.0, 0.0));
}

#[test]
fn growing_from_10_to_12_spans_spawns_2() {
    let mut app = app();
    let root = line(&mut app, words(10));
    let first = kids(&app, root);

    set_line(&mut app, words(12));

    let grown = kids(&app, root);
    assert_eq!(grown.len(), 12);
    assert_eq!(grown[..10], first[..], "the first 10 kept");
    assert_eq!(text(&app, grown[11]), "w11 ");
    assert_eq!(span_count(&mut app), 12, "no span left over");
}

#[test]
fn shrinking_to_no_spans_leaves_one_empty_span() {
    let mut app = app();
    let root = line(&mut app, words(10));
    let first = kids(&app, root)[0];

    set_line(&mut app, Vec::new());

    assert_eq!(kids(&app, root), vec![first]);
    assert_eq!(text(&app, first), "");
    assert_eq!(span_count(&mut app), 1, "the other 9 despawned");

    set_line(&mut app, words(5));
    let grown = kids(&app, root);
    assert_eq!(grown.len(), 5);
    assert_eq!(grown[0], first);
    assert_eq!(text(&app, grown[4]), "w4 ");
}

#[test]
fn a_span_loses_what_its_look_no_longer_has() {
    let mut app = app();
    let plated = Span::new("x")
        .background(Color::BLACK)
        .underline(LineColor::Own(Color::WHITE));
    let root = line(&mut app, vec![plated]);
    let span = kids(&app, root)[0];
    assert!(app.world().get::<TextBackgroundColor>(span).is_some());
    assert!(app.world().get::<UnderlineColor>(span).is_some());

    set_line(
        &mut app,
        vec![Span::new("x").underline(LineColor::Ink)],
    );

    let world = app.world();
    assert!(world.get::<TextBackgroundColor>(span).is_none());
    assert!(world.get::<Underline>(span).is_some());
    assert!(world.get::<UnderlineColor>(span).is_none());
}

#[derive(Resource)]
struct Shade(Tone);

#[test]
fn a_theme_fade_changes_every_span_colour() {
    let mut app = app();
    app.insert_resource(Shade(Tone::Body));
    let own = Color::srgb(0.0, 0.0, 1.0);
    let root = mount::<Plain>(
        app.world_mut(),
        rich(vec![
            Span::new("a "),
            Span::new("b ").color(own),
            Span::new("c"),
        ])
        .tone(resource::<Shade, _>(|shade| shade.0))
        .transition(Motion::Interact),
    );
    app.update();
    let spans = kids(&app, root);
    assert_eq!(color(&app, spans[0]), Color::WHITE);

    app.world_mut().resource_mut::<Shade>().0 = Tone::Accent;
    app.update();
    let orange = Color::srgb(1.0, 0.5, 0.0);
    let midway = color(&app, root);
    assert_ne!(midway, Color::WHITE, "50ms of 100ms");
    assert_ne!(midway, orange, "50ms of 100ms");
    assert_eq!(color(&app, spans[0]), midway);
    assert_eq!(color(&app, spans[2]), midway, "not only the first");
    assert_eq!(
        color(&app, spans[1]),
        own,
        "a span's own colour stays"
    );

    app.update();
    assert_eq!(color(&app, spans[0]), orange);
    assert_eq!(color(&app, spans[2]), orange);
}

#[test]
fn opacity_fades_every_span_and_its_plate() {
    let mut app = app();
    let own = Color::srgb(0.0, 0.0, 1.0);
    let plate = Color::srgb(0.2, 0.2, 0.2);
    let root = mount::<Plain>(
        app.world_mut(),
        rich(vec![
            Span::new("a "),
            Span::new("b ").color(own).background(plate),
        ])
        .opacity(0.25),
    );
    let spans = kids(&app, root);

    assert_eq!(color(&app, spans[0]), Color::WHITE.with_alpha(0.25));
    assert_eq!(color(&app, spans[1]), own.with_alpha(0.25));
    let background = app.world().get::<TextBackgroundColor>(spans[1]);
    assert_eq!(background.unwrap().0, plate.with_alpha(0.25));
}

#[test]
fn a_rich_text_built_hidden_starts_with_every_span_unseen() {
    let mut app = app();
    let root = mount::<Plain>(
        app.world_mut(),
        rich(vec![
            Span::new("a "),
            Span::new("b").color(Color::srgb(0.0, 0.0, 1.0)),
        ])
        .appear::<Plain>(hidden),
    );

    for span in kids(&app, root) {
        assert_eq!(color(&app, span).alpha(), 0.0);
    }
}

#[test]
fn the_root_size_reaches_spans_with_no_size_of_their_own() {
    let mut app = app();
    let root = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("a "), Span::new("b").size(20.0)])
            .size(9.0),
    );
    let spans = kids(&app, root);
    let size =
        |span| app.world().get::<TextFont>(span).unwrap().font_size;

    assert_eq!(size(spans[0]), FontSize::Px(9.0));
    assert_eq!(size(spans[1]), FontSize::Px(20.0));
}

fn laid_out() -> App {
    laid_out_at(1.0)
}

fn laid_out_at(scale_factor: f32) -> App {
    let mut app = app();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(10),
    ));
    app.add_plugins((
        bevy::asset::AssetPlugin::default(),
        bevy::transform::TransformPlugin,
        bevy::input::InputPlugin,
        bevy::picking::PickingPlugin,
        bevy::picking::InteractionPlugin,
        bevy::text::TextPlugin,
    ))
    .init_asset::<bevy::image::Image>()
    .init_asset::<bevy::image::TextureAtlasLayout>()
    .add_plugins(UiPlugin);
    app.world_mut().spawn((
        Camera2d,
        Camera {
            computed: ComputedCameraValues {
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::new(1000, 1000),
                    scale_factor,
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    ));
    app
}

const MIXED: [&str; 4] = ["héllo ", "wörld ", "日本語 ", "😀 x"];

fn mixed(app: &mut App) -> Entity {
    let spans = MIXED
        .iter()
        .map(|&part| Span::new(part))
        .collect::<Vec<_>>();
    let root = mount::<Plain>(app.world_mut(), rich(spans));
    app.update();
    app.update();
    root
}

#[test]
fn rich_layout_maps_every_character_boundary_to_a_point_and_back() {
    let mut app = laid_out();
    let root = mixed(&mut app);

    let full = MIXED.concat();
    let layout = &rich_layout(app.world(), root).expect("a layout");
    assert_eq!(layout.len(), 1, "one line");
    let mut last = f64::MIN;
    let mut checked = 0;
    let ends = full.char_indices().map(|(index, _)| index);
    for index in ends.chain([full.len()]) {
        let caret = Cursor::from_byte_index(
            layout,
            index,
            Affinity::Downstream,
        )
        .geometry(layout, 1.0);
        assert!(caret.x0 > last, "byte {index} is right of the last");
        last = caret.x0;
        let back = Cursor::from_point(
            layout,
            caret.x0 as f32 + 0.5,
            ((caret.y0 + caret.y1) / 2.0) as f32,
        );
        assert_eq!(back.index(), index);
        checked += 1;
    }
    assert_eq!(checked, full.chars().count() + 1, "and the end");
}

fn cursor_at(
    app: &mut App,
    root: Entity,
    at: Vec2,
) -> Option<Cursor> {
    let world = app.world_mut();
    let camera = world
        .query::<&Camera>()
        .single(world)
        .expect("one camera")
        .clone();
    let world = app.world();
    let point = rich_point(
        world.get::<ComputedNode>(root)?,
        world.get::<UiGlobalTransform>(root)?,
        &camera,
        at,
    )?;
    let layout = &rich_layout(world, root)?;
    Some(Cursor::from_point(layout, point.x, point.y))
}

#[test]
fn rich_layout_point_of_a_window_position_is_the_byte_under_it() {
    let mut app = laid_out();
    let root = mixed(&mut app);
    let world = app.world();
    let layout = &rich_layout(world, root).expect("a layout");
    let node = world.get::<ComputedNode>(root).unwrap();
    let corner =
        world.get::<UiGlobalTransform>(root).unwrap().translation
            + node.content_box().min;
    let wörld = "héllo w".len();
    let caret =
        Cursor::from_byte_index(layout, wörld, Affinity::Downstream)
            .geometry(layout, 1.0);
    let at = corner
        + Vec2::new(caret.x0 as f32 + 1.0, caret.y1 as f32 / 2.0);

    let cursor = cursor_at(&mut app, root, at).expect("a cursor");
    assert_eq!(cursor.index(), wörld);
}

fn height(app: &App, node: Entity) -> f32 {
    app.world()
        .get::<ComputedNode>(node)
        .expect("a node")
        .size()
        .y
}

#[test]
fn rich_layout_of_no_text_is_one_line_high() {
    let mut app = laid_out();
    let empty = mount::<Plain>(app.world_mut(), rich(Vec::new()));
    let blanks = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new(""), Span::new("")]),
    );
    let worded =
        mount::<Plain>(app.world_mut(), rich(vec![Span::new("x")]));
    app.update();
    app.update();

    let line = height(&app, worded);
    assert!(line > 0.0);
    assert_eq!(height(&app, empty), line);
    assert_eq!(height(&app, blanks), line);
}

#[derive(Resource)]
struct Size(f32);

#[test]
fn a_rich_text_with_no_text_is_one_line_of_its_size_high() {
    let mut app = laid_out();
    app.insert_resource(Size(10.0));
    let sized = || resource::<Size, _>(|size| size.0);
    let empty = mount::<Plain>(
        app.world_mut(),
        rich(Vec::new()).size(sized()),
    );
    let worded = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("x")]).size(sized()),
    );
    app.update();
    app.update();
    let small = height(&app, worded);
    assert_eq!(height(&app, empty), small);

    app.world_mut().resource_mut::<Size>().0 = 30.0;
    app.update();
    app.update();
    assert!(height(&app, worded) > small);
    assert_eq!(height(&app, empty), height(&app, worded));
}

#[test]
fn a_blank_line_is_as_high_as_its_first_span_size() {
    let mut app = laid_out();
    let blank = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("").size(30.0), Span::new("")]),
    );
    let worded = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("x").size(30.0)]),
    );
    app.update();
    app.update();

    assert_eq!(height(&app, blank), height(&app, worded));
}

#[test]
fn every_point_of_a_rich_text_with_no_text_is_byte_0() {
    let mut app = laid_out();
    let empty = mount::<Plain>(app.world_mut(), rich(Vec::new()));
    let blanks =
        mount::<Plain>(app.world_mut(), rich(vec![Span::new(""); 3]));
    app.update();
    app.update();

    for root in [empty, blanks] {
        let layout =
            &rich_layout(app.world(), root).expect("a layout");
        let points =
            [(0.0, 0.0), (10_000.0, 1.0), (10_000.0, 10_000.0)];
        for (x, y) in points {
            let cursor = Cursor::from_point(layout, x, y);
            assert_eq!(cursor.index(), 0, "caret at ({x}, {y})");
        }
        for (x, y) in points {
            if let Some((cluster, _)) =
                Cluster::from_point(layout, x, y)
            {
                assert_eq!(
                    cluster.text_range(),
                    0..0,
                    "at ({x}, {y})"
                );
            }
        }
        let caret =
            Cursor::from_byte_index(layout, 0, Affinity::Downstream)
                .geometry(layout, 1.0);
        assert!(caret.y1 > caret.y0, "a caret one line high");
    }
}

#[test]
fn a_list_of_empty_spans_keeps_a_span_entity_each() {
    let mut app = app();
    let root = line(&mut app, words(5));
    let first = kids(&app, root);

    set_line(&mut app, vec![Span::new(""); 5]);
    assert_eq!(kids(&app, root), first, "one entity per span");
    assert!(first.iter().all(|&span| text(&app, span).is_empty()));

    set_line(&mut app, words(5));
    assert_eq!(kids(&app, root), first, "the same five");
    assert_eq!(text(&app, first[4]), "w4 ");
}

#[test]
fn a_fade_leaves_the_font_of_a_span_sized_nan_alone() {
    let mut app = app();
    app.insert_resource(Shade(Tone::Body));
    let root = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("a "), Span::new("b").size(f32::NAN)])
            .size(9.0)
            .tone(resource::<Shade, _>(|shade| shade.0))
            .transition(Motion::Interact),
    );
    app.update();
    let span = kids(&app, root)[1];
    let before = changed::<TextFont>(&app, span);

    app.world_mut().resource_mut::<Shade>().0 = Tone::Accent;
    app.update();
    app.update();

    assert_ne!(color(&app, span), Color::WHITE, "the fade ran");
    assert_eq!(changed::<TextFont>(&app, span), before);
    let font = app.world().get::<TextFont>(span).unwrap();
    assert_eq!(font.font_size, FontSize::Px(9.0), "the root's size");
}

#[test]
fn an_empty_rich_text_is_one_line_high_at_each_scale_factor() {
    let sizes =
        [9.0, 12.0, 13.0, 14.0, 15.0, 16.0, 20.0, 24.0, 30.0, 42.0];
    let mut uneven = Vec::new();
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        for size in [None].into_iter().chain(sizes.map(Some)) {
            let mut app = laid_out_at(scale);
            let sized = |rich: Rich| match size {
                Some(size) => rich.size(size),
                None => rich,
            };
            let empty = mount::<Plain>(
                app.world_mut(),
                sized(rich(Vec::new())),
            );
            let blanks = mount::<Plain>(
                app.world_mut(),
                sized(rich(vec![Span::new(""); 3])),
            );
            let worded = mount::<Plain>(
                app.world_mut(),
                sized(rich(vec![Span::new("x")])),
            );
            app.update();
            app.update();
            let line = height(&app, worded);
            assert!(line > 0.0);
            let heights = [height(&app, empty), height(&app, blanks)];
            if heights != [line; 2] {
                uneven.push((scale, size, heights, line));
            }
        }
    }
    assert!(uneven.is_empty(), "empty against worded: {uneven:?}");
}

#[test]
fn an_empty_rich_text_is_as_high_as_its_span_line_height() {
    let mut app = laid_out_at(1.2);
    let empty =
        mount::<Plain>(app.world_mut(), rich(Vec::new()).size(15.0));
    let worded = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("x")]).size(15.0),
    );
    app.update();
    for root in [empty, worded] {
        let span = kids(&app, root)[0];
        app.world_mut()
            .entity_mut(span)
            .insert(LineHeight::RelativeToFont(1.5));
    }
    app.update();
    app.update();

    assert_eq!(height(&app, empty), height(&app, worded));
}

fn a_huge_blank_line_lays_out_at(scale_factor: f32) {
    let size: f32 = 6_990_511.0;
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let mut app = laid_out_at(scale_factor);
        let empty = mount::<Plain>(
            app.world_mut(),
            rich(Vec::new()).size(size),
        );
        app.update();
        app.update();
        let _ = sent.send(height(&app, empty));
    });
    let shown = received
        .recv_timeout(Duration::from_secs(30))
        .expect("two frames within 30 s");
    let line = 1.2 * (size * scale_factor);
    assert!((shown - line).abs() <= 1.0, "{shown} against {line}");
}

#[test]
fn a_blank_line_lays_out_at_a_negative_ui_scale() {
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let mut app = laid_out();
        app.insert_resource(UiScale(-1.25));
        let empty = mount::<Plain>(
            app.world_mut(),
            rich(Vec::new()).size(296.0),
        );
        let span = kids(&app, empty)[0];
        app.world_mut()
            .entity_mut(span)
            .insert(LineHeight::RelativeToFont(1.3));
        app.update();
        app.update();
        let _ = sent.send(());
    });
    received
        .recv_timeout(Duration::from_secs(30))
        .expect("two frames within 30 s");
}

#[test]
fn a_huge_empty_rich_text_lays_out_at_a_fractional_scale() {
    a_huge_blank_line_lays_out_at(1.25);
}

#[test]
fn a_huge_empty_rich_text_lays_out_at_scale_1() {
    a_huge_blank_line_lays_out_at(1.0);
}

#[test]
fn a_nan_root_size_takes_the_theme_size() {
    let mut app = laid_out();
    let nan = mount::<Plain>(
        app.world_mut(),
        rich(vec![Span::new("x")]).size(f32::NAN),
    );
    let empty = mount::<Plain>(
        app.world_mut(),
        rich(Vec::new()).size(f32::NAN),
    );
    let themed =
        mount::<Plain>(app.world_mut(), rich(vec![Span::new("x")]));
    app.update();
    app.update();

    let span = kids(&app, nan)[0];
    let font = app.world().get::<TextFont>(span).unwrap();
    assert_eq!(
        font.font_size,
        FontSize::Px(14.0),
        "the theme's size"
    );
    let held = app.world().get::<InlineBox>(kids(&app, empty)[0]);
    assert!(
        held.unwrap().size.y.is_finite(),
        "a box of finite height"
    );
    assert_eq!(height(&app, empty), height(&app, themed));
    assert_eq!(height(&app, nan), height(&app, themed));
}

#[test]
fn a_fade_treats_a_nan_span_colour_as_unset() {
    let mut app = app();
    app.insert_resource(Shade(Tone::Body));
    let nan = Color::srgb(f32::NAN, 0.0, 0.0);
    let root = mount::<Plain>(
        app.world_mut(),
        rich(vec![
            Span::new("a "),
            Span::new("b")
                .color(nan)
                .background(nan)
                .underline(LineColor::Own(nan))
                .strikethrough(LineColor::Own(nan)),
        ])
        .tone(resource::<Shade, _>(|shade| shade.0))
        .transition(Motion::Interact),
    );
    app.update();
    let [plain, span] = kids(&app, root)[..] else {
        panic!("two spans");
    };

    app.world_mut().resource_mut::<Shade>().0 = Tone::Accent;
    for frame in 0..3 {
        let before = changed::<TextColor>(&app, span);
        let shown = color(&app, plain);
        app.update();
        assert_eq!(
            color(&app, span),
            color(&app, plain),
            "frame {frame}"
        );
        assert_eq!(
            changed::<TextColor>(&app, span) != before,
            color(&app, plain) != shown,
            "frame {frame}: rewritten only when it changed"
        );
    }
    let world = app.world();
    assert!(
        world.get::<TextBackgroundColor>(span).is_none(),
        "no plate"
    );
    assert!(world.get::<UnderlineColor>(span).is_none());
    assert!(world.get::<StrikethroughColor>(span).is_none());
}

#[test]
fn a_word_wider_than_its_text_breaks_at_the_width() {
    const WIDTH: f32 = 200.0;
    let mut app = laid_out();
    let long = format!("ab {}", "x".repeat(100));
    let root = mount::<Plain>(
        app.world_mut(),
        column((rich(vec![Span::new(long)]),)).width(px(WIDTH)),
    );
    app.update();
    app.update();
    let text = kids(&app, root)[0];
    let layout = rich_layout(app.world(), text).expect("a layout");
    let widths = layout
        .lines()
        .map(|line| line.metrics().advance)
        .collect::<Vec<_>>();
    assert!(widths.len() > 2, "the word takes lines: {widths:?}");
    assert!(
        widths.iter().all(|&width| width <= WIDTH),
        "no line runs past {WIDTH} px: {widths:?}"
    );
}
