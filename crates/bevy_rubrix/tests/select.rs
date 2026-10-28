mod support;

use core::ops::Range;

use bevy::app::App;
use bevy::camera::NormalizedRenderTarget;
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input::{ButtonInput, ButtonState};
use bevy::math::Vec2;
use bevy::picking::backend::HitData;
use bevy::picking::events::{
    Pointer, PointerDrag, PointerDragEnd, PointerPress,
    PointerRelease,
};
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy_fynix::views::parley::{Affinity, Cursor};
use bevy_fynix::views::{VirtualList, rich_layout};
use bevy_rubrix::{
    CodeCopied, CodeSelection, CodeSource, Lang, Overflow,
};
use support::{
    Wrapping, doc, laid_out, line, list, list_state, mount_view, view,
};

#[derive(Resource, Default)]
struct Copied(Vec<String>);

fn selecting(source: CodeSource) -> App {
    let mut app = laid_out();
    app.init_resource::<Copied>().add_observer(
        |copied: On<CodeCopied>, mut all: ResMut<Copied>| {
            all.0.push(copied.text.clone());
        },
    );
    mount_view(&mut app, source, 600.0, 400.0);
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
    let world = app.world();
    let layout = rich_layout(world, root).expect("a layout");
    let node = world.get::<ComputedNode>(root).expect("a node");
    let corner = world
        .get::<UiGlobalTransform>(root)
        .expect("a transform")
        .translation
        + node.content_box().min;
    let caret =
        Cursor::from_byte_index(layout, byte, Affinity::Downstream)
            .geometry(layout, 1.0);
    corner
        + Vec2::new(
            caret.x0 as f32 + 1.0,
            ((caret.y0 + caret.y1) / 2.0) as f32,
        )
}

fn pointer(at: Vec2) -> Pointer {
    let location = Location {
        target: NormalizedRenderTarget::None {
            width: 1000,
            height: 1000,
        },
        position: at,
    };
    Pointer::new(PointerId::Mouse, location)
}

fn press_at(app: &mut App, row: usize, byte: usize, count: u8) {
    hold_at(app, row, byte, count);
    let on = line(app, row);
    let at = point_of(app, row, byte);
    app.world_mut().trigger(PointerRelease {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
    });
    app.update();
}

fn hold_at(app: &mut App, row: usize, byte: usize, count: u8) {
    let at = point_of(app, row, byte);
    let on = line(app, row);
    app.world_mut().trigger(PointerPress {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
        count,
    });
    app.update();
}

fn shift(app: &mut App, held: bool) {
    let mut keys =
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    if held {
        keys.press(KeyCode::ShiftLeft);
    } else {
        keys.release(KeyCode::ShiftLeft);
    }
}

fn ctrl(app: &mut App, key: KeyCode, letter: &str) {
    let send = |app: &mut App, key_code, logical_key, state| {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key,
            state,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    };
    let letter = Key::Character(letter.into());
    send(
        app,
        KeyCode::ControlLeft,
        Key::Control,
        ButtonState::Pressed,
    );
    send(app, key, letter.clone(), ButtonState::Pressed);
    app.update();
    send(app, key, letter, ButtonState::Released);
    send(
        app,
        KeyCode::ControlLeft,
        Key::Control,
        ButtonState::Released,
    );
    app.update();
}

fn selected(app: &mut App) -> Range<usize> {
    let view = view(app);
    app.world()
        .get::<CodeSelection>(view)
        .expect("a selection")
        .range()
}

fn copy(app: &mut App) -> String {
    ctrl(app, KeyCode::KeyC, "c");
    app.world()
        .resource::<Copied>()
        .0
        .last()
        .cloned()
        .unwrap_or_default()
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

#[test]
fn a_wrapped_row_maps_a_point_on_its_second_visual_line() {
    let long = "word ".repeat(60);
    let text = format!("{long}\nshort\n");
    let mut app =
        selecting(CodeSource::new(Lang::Plain, text.as_str()));
    app.world_mut().resource_mut::<Wrapping>().0 = Overflow::Wrap;
    settle(&mut app);
    let root = line(&mut app, 0);
    let layout = rich_layout(app.world(), root).expect("a layout");
    assert!(layout.len() >= 2, "the line wraps");
    let second = layout.get(1).expect("a second line").text_range();
    let byte = second.start + 2;
    press_at(&mut app, 0, byte, 1);
    assert_eq!(selected(&mut app), byte..byte);
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
    app.world_mut().trigger(PointerDrag {
        entity: on,
        pointer: pointer(below),
        button: PointerButton::Primary,
        distance: Vec2::new(0.0, 200.0),
        delta: Vec2::new(0.0, 200.0),
    });
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
    let below = point_of(&mut app, 3, 2) + Vec2::new(0.0, 600.0);
    app.world_mut().trigger(PointerDrag {
        entity: on,
        pointer: pointer(below),
        button: PointerButton::Primary,
        distance: Vec2::new(0.0, 600.0),
        delta: Vec2::new(0.0, 600.0),
    });
    settle(&mut app);
    assert!(list_state(&mut app).first_visible() > 0, "it scrolls");
    app.world_mut().trigger(PointerDragEnd {
        entity: on,
        pointer: pointer(below),
        button: PointerButton::Primary,
        distance: Vec2::new(0.0, 600.0),
    });
    app.update();
    let first = list_state(&mut app).first_visible();
    let range = selected(&mut app);
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(list_state(&mut app).first_visible(), first);
    assert_eq!(selected(&mut app), range);
}
