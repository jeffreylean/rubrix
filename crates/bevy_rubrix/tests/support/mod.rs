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
    Pointer, PointerDrag, PointerDragEnd, PointerPress,
    PointerRelease,
};
use bevy::picking::pointer::{
    Location, PointerAction, PointerButton, PointerId, PointerInput,
};
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
    CodeCopied, CodeDoc, CodeLine, CodeSource, EditDoc, EditLine,
    EditView, Kind, Lang, Overflow, Platform, RubrixPlugin,
    SyntaxStyle, SyntaxTokens, code_editor, code_view, resolve,
};

pub const BODY: Color = Color::WHITE;

pub fn mod_key() -> KeyCode {
    match Platform::host() {
        Platform::Linux => KeyCode::ControlLeft,
        Platform::MacOs => KeyCode::SuperLeft,
    }
}

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
    app.world_mut().spawn(PointerId::Mouse);
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
    chord(app, &[mod_key()], KeyCode::KeyC, "c");
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

fn location(at: Vec2) -> Location {
    Location {
        target: NormalizedRenderTarget::None {
            width: WINDOW,
            height: WINDOW,
        },
        position: at,
    }
}

fn pointer(at: Vec2) -> Pointer {
    Pointer::new(PointerId::Mouse, location(at))
}

fn move_pointer_then(
    app: &mut App,
    at: Vec2,
    action: Option<PointerAction>,
) {
    let actions =
        [Some(PointerAction::Move { delta: Vec2::ZERO }), action];
    for action in actions.into_iter().flatten() {
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location(at),
            action,
        ));
    }
}

pub fn hold_on(app: &mut App, on: Entity, at: Vec2, count: u8) {
    move_pointer_then(
        app,
        at,
        Some(PointerAction::Press(PointerButton::Primary)),
    );
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
    move_pointer_then(
        app,
        at,
        Some(PointerAction::Release(PointerButton::Primary)),
    );
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

/// Bevy sends a drag to the entity first pressed, wherever the
/// pointer is, even when that entity is gone.
pub fn drag_on(app: &mut App, on: Entity, from: Vec2, at: Vec2) {
    move_pointer_then(app, at, None);
    app.world_mut().trigger(PointerDrag {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        distance: at - from,
        delta: at - from,
    });
    app.update();
}

/// A release over nothing, which ends the drag on the entity first
/// pressed and sends no release.
pub fn drag_end_on(app: &mut App, on: Entity, from: Vec2, at: Vec2) {
    move_pointer_then(
        app,
        at,
        Some(PointerAction::Release(PointerButton::Primary)),
    );
    app.world_mut().trigger(PointerDragEnd {
        entity: on,
        pointer: pointer(at),
        button: PointerButton::Primary,
        distance: at - from,
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

#[derive(Resource)]
pub struct Editing(pub Entity);

/// Spawns a doc of `text` and mounts an editable view of it.
pub fn mount_editor(
    app: &mut App,
    lang: Lang,
    text: &str,
    width: f32,
    height: f32,
) -> (Entity, Entity) {
    let doc = app.world_mut().spawn(EditDoc::new(lang, text)).id();
    app.insert_resource(Editing(doc))
        .insert_resource(Wrapping(Overflow::Scroll));
    mount::<Distinct>(
        app.world_mut(),
        column((code_editor(resource::<Editing, _>(|editing| {
            editing.0
        }))
        .overflow(resource::<Wrapping, _>(|wrapping| wrapping.0)),))
        .width(px(width))
        .height(px(height)),
    );
    app.update();
    (editor(app), doc)
}

pub fn editor(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<EditView>>()
        .single(app.world())
        .expect("one editor")
}

pub fn edit_lines(app: &mut App, view: Entity) -> Vec<(u32, Entity)> {
    let mut lines = app
        .world_mut()
        .query::<(Entity, &EditLine)>()
        .iter(app.world())
        .filter(|(_, line)| line.view == view)
        .map(|(entity, line)| (line.line, entity))
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

pub fn edit_line(app: &mut App, view: Entity, line: u32) -> Entity {
    edit_lines(app, view)
        .into_iter()
        .find(|&(shown, _)| shown == line)
        .unwrap_or_else(|| panic!("line {line} is built"))
        .1
}

/// [`point_in`] in logical window px, as a pointer reports it, at any
/// scale.
pub fn point_at(app: &App, root: Entity, byte: usize) -> Vec2 {
    let logical = app
        .world()
        .get::<ComputedNode>(root)
        .expect("a node")
        .inverse_scale_factor();
    point_in(app, root, byte) * logical
}

fn send(
    app: &mut App,
    key_code: KeyCode,
    logical_key: Key,
    text: Option<&str>,
    state: ButtonState,
) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key,
        state,
        text: text.map(Into::into),
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

fn modifier(key: KeyCode) -> Key {
    match key {
        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        _ => Key::Super,
    }
}

/// Every char of `text` pressed and released in one frame, as the
/// gallery's driver types.
pub fn type_text(app: &mut App, text: &str) {
    let unknown = KeyCode::Unidentified(
        bevy::input::keyboard::NativeKeyCode::Unidentified,
    );
    for c in text.chars() {
        let c = c.to_string();
        for state in [ButtonState::Pressed, ButtonState::Released] {
            send(
                app,
                unknown,
                Key::Character(c.as_str().into()),
                Some(&c),
                state,
            );
        }
    }
    app.update();
}

/// `key` pressed with `mods` held in one frame, released in the next.
pub fn key_with(
    app: &mut App,
    mods: &[KeyCode],
    key: KeyCode,
    logical: Key,
) {
    for &held in mods {
        send(app, held, modifier(held), None, ButtonState::Pressed);
    }
    send(app, key, logical.clone(), None, ButtonState::Pressed);
    app.update();
    send(app, key, logical, None, ButtonState::Released);
    for &held in mods {
        send(app, held, modifier(held), None, ButtonState::Released);
    }
    app.update();
}

pub fn chord(
    app: &mut App,
    mods: &[KeyCode],
    key: KeyCode,
    letter: &str,
) {
    key_with(app, mods, key, Key::Character(letter.into()));
}

/// Holds or lets go of `key` as key messages, as the editor reads
/// modifiers.
pub fn hold_key(app: &mut App, key: KeyCode, held: bool) {
    let state = if held {
        ButtonState::Pressed
    } else {
        ButtonState::Released
    };
    send(app, key, modifier(key), None, state);
    app.update();
}
