//! What the code view tests share: a theme whose colours tell the
//! root kinds apart, headless apps with and without layout, and the
//! lookups of what the view shows.

#![allow(dead_code, reason = "each test crate uses its own part")]

use core::time::Duration;

use bevy::app::App;
use bevy::asset::AssetApp;
use bevy::camera::{
    Camera, Camera2d, ComputedCameraValues, RenderTargetInfo,
};
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::math::UVec2;
use bevy::text::{TextColor, TextSpan};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::{UiPlugin, px};
use bevy_fynix::tokens::{SpacingTokens, TextTokens, Tone};
use bevy_fynix::views::{FrameProps, VirtualList, column};
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::{
    CodeDoc, CodeLine, CodeSource, Kind, Overflow, RubrixPlugin,
    SyntaxStyle, SyntaxTokens, code_view, resolve,
};

pub const BODY: Color = Color::WHITE;

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

/// A headless app with no layout.
pub fn app() -> App {
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
    // The first update only starts the clock.
    app.update();
    app
}

/// A headless app that lays out and shapes text, in a 1000 by 1000
/// physical px window at `scale_factor`.
pub fn laid_out_at(scale_factor: f32) -> App {
    let mut app = app();
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

pub fn laid_out() -> App {
    laid_out_at(1.0)
}

#[derive(Resource)]
pub struct Showing(pub CodeSource);

#[derive(Resource)]
pub struct Wrapping(pub Overflow);

/// Mounts a code view of the [`Showing`] source, `width` by `height`
/// logical px, and updates once.
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

/// Each built row's text root, by row.
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

/// The spans of row `row`'s text, which has to be built.
pub fn row_spans(app: &mut App, row: usize) -> Vec<(String, Color)> {
    let root = line(app, row);
    spans_of(app, root)
}
