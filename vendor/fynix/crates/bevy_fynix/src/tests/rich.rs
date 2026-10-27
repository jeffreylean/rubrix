use core::time::Duration;

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
    FontSize, TextBackgroundColor, TextColor, TextFont, TextSpan,
    Underline, UnderlineColor,
};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin};
use fynix::{Motion, ScopedExt};
use parley::{Affinity, Cursor};

use crate::tests::{Plain, app, kids};
use crate::tokens::Tone;
use crate::views::{LineColor, Span, rich, rich_layout, rich_point};
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
}

#[test]
fn shrinking_to_no_spans_leaves_one_blank_span() {
    let mut app = app();
    let root = line(&mut app, words(10));
    let first = kids(&app, root)[0];

    set_line(&mut app, Vec::new());

    assert_eq!(kids(&app, root), vec![first]);
    assert_eq!(
        text(&app, first),
        " ",
        "a space, to keep a line high"
    );

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
                    scale_factor: 1.0,
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
    let layout = rich_layout(app.world(), root).expect("a layout");
    assert_eq!(layout.len(), 1, "one line");
    let mut last = f64::MIN;
    let mut checked = 0;
    for (index, _) in full.char_indices() {
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
    assert_eq!(checked, full.chars().count());
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
    let layout = rich_layout(world, root)?;
    Some(Cursor::from_point(layout, point.x, point.y))
}

#[test]
fn rich_layout_point_of_a_window_position_is_the_byte_under_it() {
    let mut app = laid_out();
    let root = mixed(&mut app);
    let world = app.world();
    let layout = rich_layout(world, root).expect("a layout");
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
    let cursor =
        cursor_at(&mut app, empty, Vec2::ZERO).expect("a cursor");
    assert_eq!(cursor.index(), 0);
}
