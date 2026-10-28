#![allow(dead_code, reason = "each test crate uses its own part")]

pub mod diff;

use core::time::Duration;

use bevy::app::App;
use bevy::asset::AssetApp;
use bevy::camera::{
    Camera, Camera2d, ComputedCameraValues, NormalizedRenderTarget,
    RenderTargetInfo,
};
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input::{ButtonInput, ButtonState};
use bevy::math::{UVec2, Vec2};
use bevy::picking::backend::HitData;
use bevy::picking::events::{
    Pointer, PointerDrag, PointerPress, PointerRelease,
};
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::text::{TextColor, TextSpan};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::{ComputedNode, UiGlobalTransform, UiPlugin, px};
use bevy_fynix::tokens::{SpacingTokens, TextTokens, Tone};
use bevy_fynix::views::parley::{Affinity, Cursor};
use bevy_fynix::views::{
    FrameProps, VirtualList, column, rich_layout,
};
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::{
    CodeCopied, CodeDoc, CodeLine, CodeSource, Kind, Overflow,
    RubrixPlugin, SyntaxStyle, SyntaxTokens, code_view, resolve,
};

pub const BODY: Color = Color::WHITE;

const WINDOW: u32 = 1000;

pub struct Distinct;

fn root_colour(kind: Kind) -> Color {
    Color::srgb_u8(kind as u8 * 4, 64, 128)
}

impl TextTokens for Distinct {
    fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Body => BODY,
            _ => Color::BLACK,
        }
    }

    fn body_size(&self) -> f32 {
        14.0
    }

    fn small_size(&self) -> f32 {
        11.0
    }
}

impl SpacingTokens for Distinct {
    fn gap(&self) -> f32 {
        8.0
    }

    fn row(&self) -> f32 {
        20.0
    }

    fn radius(&self) -> f32 {
        3.0
    }
}

impl SyntaxTokens for Distinct {
    fn style(&self, kind: Kind) -> SyntaxStyle {
        match kind.parent() {
            None => SyntaxStyle::color(root_colour(kind)),
            Some(_) => SyntaxStyle::default(),
        }
    }
}

pub fn colour(kind: Kind) -> Color {
    resolve(&Distinct, kind).0
}

fn plain() -> App {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        FynixPlugin::<Distinct>::default(),
        RubrixPlugin,
    ))
    .insert_resource(Theme(Distinct))
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(50),
    ));
    app
}

pub fn app() -> App {
    let mut app = plain();
    // The first update only starts the clock.
    app.update();
    app
}

pub fn laid_out_at(scale_factor: f32) -> App {
    let mut app = plain();
    lay_out(&mut app, scale_factor);
    app
}

pub fn lay_out(app: &mut App, scale_factor: f32) {
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
                    physical_size: UVec2::splat(WINDOW),
                    scale_factor,
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    ));
    app.update();
}

pub fn laid_out() -> App {
    laid_out_at(1.0)
}

#[derive(Resource)]
pub struct Showing(pub CodeSource);

#[derive(Resource)]
pub struct Wrapping(pub Overflow);

pub fn mount_view(
    app: &mut App,
    source: CodeSource,
    width: f32,
    height: f32,
) -> Entity {
    app.insert_resource(Showing(source))
        .insert_resource(Wrapping(Overflow::Scroll));
    mount::<Distinct>(
        app.world_mut(),
        column((code_view(resource::<Showing, _>(|showing| {
            showing.0.clone()
        }))
        .overflow(resource::<Wrapping, _>(|wrapping| wrapping.0)),))
        .width(px(width))
        .height(px(height)),
    );
    app.update();
    view(app)
}

pub fn view(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<CodeDoc>>()
        .single(app.world())
        .expect("one code view")
}

pub fn doc(app: &mut App) -> &CodeDoc {
    let view = view(app);
    app.world().get::<CodeDoc>(view).expect("a code view")
}

pub fn list(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
}

pub fn list_state(app: &mut App) -> &VirtualList {
    let list = list(app);
    app.world().get::<VirtualList>(list).expect("a list")
}

pub fn spans_of(app: &App, root: Entity) -> Vec<(String, Color)> {
    let world = app.world();
    world
        .get::<Children>(root)
        .map(|kids| {
            kids.iter()
                .copied()
                .filter_map(|span| {
                    let text = world.get::<TextSpan>(span)?;
                    let colour = world.get::<TextColor>(span)?;
                    Some((text.0.clone(), colour.0))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn text_of(app: &App, root: Entity) -> String {
    spans_of(app, root)
        .into_iter()
        .map(|(text, _)| text)
        .collect()
}

pub fn lines(app: &mut App) -> Vec<(usize, Entity)> {
    let mut lines = app
        .world_mut()
        .query::<(Entity, &CodeLine)>()
        .iter(app.world())
        .map(|(entity, line)| (line.row, entity))
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

pub fn line(app: &mut App, row: usize) -> Entity {
    lines(app)
        .into_iter()
        .find(|&(shown, _)| shown == row)
        .unwrap_or_else(|| panic!("row {row} is built"))
        .1
}

pub fn row_spans(app: &mut App, row: usize) -> Vec<(String, Color)> {
    let root = line(app, row);
    spans_of(app, root)
}

#[derive(Resource, Default)]
pub struct Copied(pub Vec<String>);

pub fn keep_copies(app: &mut App) {
    app.init_resource::<Copied>().add_observer(
        |copied: On<CodeCopied>, mut all: ResMut<Copied>| {
            all.0.push(copied.text.clone());
        },
    );
}

pub fn copy(app: &mut App) -> String {
    let before = app.world().resource::<Copied>().0.len();
    ctrl(app, KeyCode::KeyC, "c");
    let copied = &app.world().resource::<Copied>().0;
    copied.get(before).cloned().unwrap_or_default()
}

pub fn corner(app: &App, root: Entity) -> Vec2 {
    let world = app.world();
    let node = world.get::<ComputedNode>(root).expect("a node");
    world
        .get::<UiGlobalTransform>(root)
        .expect("a transform")
        .translation
        + node.content_box().min
}

pub fn point_in(app: &App, root: Entity, byte: usize) -> Vec2 {
    let corner = corner(app, root);
    let layout = &rich_layout(app.world(), root).expect("a layout");
    let caret =
        Cursor::from_byte_index(layout, byte, Affinity::Downstream)
            .geometry(layout, 1.0);
    corner
        + Vec2::new(
            caret.x0 as f32 + 1.0,
            ((caret.y0 + caret.y1) / 2.0) as f32,
        )
}

pub fn pointer(at: Vec2) -> Pointer {
    let location = Location {
        target: NormalizedRenderTarget::None {
            width: WINDOW,
            height: WINDOW,
        },
        position: at,
    };
    Pointer::new(PointerId::Mouse, location)
}

pub fn hold_on(app: &mut App, on: Entity, at: Vec2, count: u8) {
    app.world_mut().trigger(PointerPress {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
        count,
    });
    app.update();
}

pub fn release_on(app: &mut App, on: Entity, at: Vec2) {
    app.world_mut().trigger(PointerRelease {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
    });
    app.update();
}

pub fn press_on(app: &mut App, on: Entity, at: Vec2, count: u8) {
    hold_on(app, on, at, count);
    release_on(app, on, at);
}

pub fn drag_on(app: &mut App, on: Entity, from: Vec2, at: Vec2) {
    app.world_mut().trigger(PointerDrag {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        distance: at - from,
        delta: at - from,
    });
    app.update();
}

pub fn shift(app: &mut App, held: bool) {
    let mut keys =
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    if held {
        keys.press(KeyCode::ShiftLeft);
    } else {
        keys.release(KeyCode::ShiftLeft);
    }
}

pub fn ctrl(app: &mut App, key: KeyCode, letter: &str) {
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
