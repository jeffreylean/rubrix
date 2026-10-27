//! `RUBRIX_DRIVE=<script>` runs a drive script, see [`drive`].
//! `RUBRIX_NO_VSYNC=1` presents without vsync, for perf runs, and
//! `0` presents with it. A scripted run defaults to without, and any
//! other value exits with code 2.

mod drive;
mod pages;

use core::time::Duration;
use std::ffi::OsStr;

use bevy::DefaultPlugins;
use bevy::app::{App, AppExit, Plugin, PluginGroup, Startup, Update};
use bevy::asset::{AssetId, Assets};
use bevy::camera::Camera2d;
use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::{not, resource_exists};
use bevy::ecs::system::{Commands, ResMut};
use bevy::ecs::world::World;
use bevy::math::{UVec2, Vec2};
use bevy::text::Font;
use bevy::time::{Real, Time};
use bevy::ui::{JustifyContent, ScrollPosition, UiRect, percent, px};
use bevy::window::{
    PresentMode, PrimaryWindow, Window, WindowPlugin,
    WindowResolution,
};
use bevy_fynix::scroll::{ScrollGoal, ScrollbarStyle};
use bevy_fynix::tokens::{
    Curve, Motion, MotionTokens, SpacingTokens, SurfaceTokens,
    TextTokens, Tone,
};
use bevy_fynix::views::{
    BehaviorExt, FrameProps, button, column, ghost, label, row,
};
use bevy_fynix::{
    AnyView, Bevy, Cx, FynixPlugin, ScopedExt, Theme, ViewExt, keyed,
    mount, resource,
};
use bevy_rubrix::RubrixPlugin;
use pages::PAGES;

const DARK: Palette = Palette { dark: true };
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const SIDEBAR_WIDTH: f32 = 200.0;
pub const PAGE_CENTER: Vec2 = Vec2::new(
    (SIDEBAR_WIDTH + WIDTH as f32) / 2.0,
    HEIGHT as f32 / 2.0,
);
const WINDOW_WAIT: Duration = Duration::from_secs(5);

const FONT: &[u8] =
    include_bytes!("../../assets/fonts/JetBrainsMonoNL-Regular.ttf");

pub type Build<'a> = Cx<'a, Bevy, Palette>;

pub type PageView = AnyView<Bevy, Palette>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
}

impl Palette {
    fn pick(self, dark: u32, light: u32) -> Color {
        let [_, r, g, b] =
            (if self.dark { dark } else { light }).to_be_bytes();
        Color::srgb_u8(r, g, b)
    }

    pub fn name(self) -> &'static str {
        if self.dark { "dark" } else { "light" }
    }

    /// Fynix's bars are white at low alpha, which a light panel
    /// swallows.
    fn scrollbar(self) -> ScrollbarStyle {
        if self.dark {
            return ScrollbarStyle::default();
        }
        let ink = Color::srgb_u8(0x1C, 0x1C, 0x20);
        ScrollbarStyle {
            rest: ink.with_alpha(0.75),
            hot: ink.with_alpha(0.9),
            ..Default::default()
        }
    }
}

impl TextTokens for Palette {
    fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Body => self.pick(0xE6E6EA, 0x1C1C20),
            Tone::Dim => self.pick(0x9A9AA4, 0x60606A),
            Tone::Faint => self.pick(0x6A6A74, 0x9A9AA4),
            Tone::Accent => self.pick(0x7AA2F7, 0x2F5FC4),
            Tone::OnAccent => self.pick(0x15151A, 0xFFFFFF),
            Tone::Critical => self.pick(0xF7768E, 0xC4314B),
        }
    }

    fn body_size(&self) -> f32 {
        14.0
    }

    fn small_size(&self) -> f32 {
        11.0
    }
}

impl SurfaceTokens for Palette {
    fn fill(&self) -> Color {
        self.pick(0x2C2C32, 0xE2E2E8)
    }

    fn hover(&self) -> Color {
        self.pick(0x3A3A42, 0xD2D2DA)
    }

    fn panel(&self) -> Color {
        self.pick(0x1E1E22, 0xF5F5F7)
    }

    fn accent(&self) -> Color {
        self.pick(0x7AA2F7, 0x2F5FC4)
    }
}

impl SpacingTokens for Palette {
    fn gap(&self) -> f32 {
        8.0
    }

    fn row(&self) -> f32 {
        24.0
    }

    fn radius(&self) -> f32 {
        4.0
    }
}

impl MotionTokens for Palette {
    fn motion(&self, _: Motion) -> Curve {
        Curve {
            duration: Duration::from_millis(120),
            ease: |t| t,
        }
    }
}

/// The index in [`PAGES`] of the page on show.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selected(pub usize);

/// The palette on show. `Theme<Palette>` holds the same value, but
/// fynix takes the theme out of the world while it builds views, so
/// a view that reads the palette reads it here.
#[derive(Resource, Clone, Copy)]
pub struct Mode(pub Palette);

#[derive(Component)]
struct PageArea;

#[derive(Resource)]
struct SavedScroll {
    area: Entity,
    offsets: Vec<Vec2>,
}

fn page_areas(world: &mut World) -> Vec<Entity> {
    world
        .query_filtered::<Entity, With<PageArea>>()
        .iter(world)
        .collect()
}

fn scrollers(world: &World, area: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut stack = vec![area];
    while let Some(node) = stack.pop() {
        if world.get::<ScrollGoal>(node).is_some() {
            found.push(node);
        }
        if let Some(children) = world.get::<Children>(node) {
            stack.extend(children.iter().rev());
        }
    }
    found
}

fn save_scroll(world: &mut World) {
    let Some(&area) = page_areas(world).first() else {
        return;
    };
    let offsets = scrollers(world, area)
        .into_iter()
        .filter_map(|node| world.get::<ScrollPosition>(node))
        .map(|position| **position)
        .collect();
    world.insert_resource(SavedScroll { area, offsets });
}

fn restore_scroll(world: &mut World) {
    let old = world.resource::<SavedScroll>().area;
    let Some(area) =
        page_areas(world).into_iter().find(|area| *area != old)
    else {
        return;
    };
    let saved = world
        .remove_resource::<SavedScroll>()
        .expect("the system runs while a save is held");
    for (node, offset) in
        scrollers(world, area).into_iter().zip(saved.offsets)
    {
        if let Some(mut position) =
            world.get_mut::<ScrollPosition>(node)
        {
            **position = offset;
        }
    }
}

fn toggle_theme(world: &mut World) {
    save_scroll(world);
    let palette = Palette {
        dark: !world.resource::<Mode>().0.dark,
    };
    world.insert_resource(Mode(palette));
    world.insert_resource(palette.scrollbar());
    world.resource_mut::<Theme<Palette>>().0 = palette;
}

/// The text this gallery last copied, for the `clipboard` drive step.
/// The shared system clipboard would show another gallery's copy.
#[derive(Resource, Default)]
pub struct Copied(pub Option<String>);

#[derive(Resource)]
pub struct Ready;

fn main() -> AppExit {
    let drive = match drive::from_env() {
        Ok(drive) => drive,
        Err(error) => {
            println!("drive: {error}");
            return AppExit::from_code(2);
        }
    };
    let present_mode = match present_mode(
        drive.is_some(),
        std::env::var_os("RUBRIX_NO_VSYNC").as_deref(),
    ) {
        Ok(mode) => mode,
        Err(error) => {
            println!("{error}");
            return AppExit::from_code(2);
        }
    };
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "rubrix gallery".into(),
                name: Some("rubrix-gallery".into()),
                resolution: WindowResolution::new(WIDTH, HEIGHT),
                present_mode,
                ..Default::default()
            }),
            ..Default::default()
        }),
        FynixPlugin::<Palette>::default(),
        RubrixPlugin,
        ShellPlugin,
    ))
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        mount_when_sized.run_if(not(resource_exists::<Ready>)),
    );
    if let Some(drive) = drive {
        app.add_plugins(drive);
    }
    app.run()
}

/// A scripted run presents without vsync unless `RUBRIX_NO_VSYNC=0`
/// says otherwise. Hyprland sends an unfocused window about one frame
/// per second under vsync, and a screenshot taken then is black.
fn present_mode(
    scripted: bool,
    no_vsync: Option<&OsStr>,
) -> Result<PresentMode, String> {
    match no_vsync {
        None if scripted => Ok(PresentMode::AutoNoVsync),
        None => Ok(PresentMode::AutoVsync),
        Some(value) if value == OsStr::new("1") => {
            Ok(PresentMode::AutoNoVsync)
        }
        Some(value) if value == OsStr::new("0") => {
            Ok(PresentMode::AutoVsync)
        }
        Some(value) => Err(format!(
            "RUBRIX_NO_VSYNC must be unset, 0 or 1, not {value:?}"
        )),
    }
}

pub struct ShellPlugin;

impl Plugin for ShellPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Theme(DARK))
            .insert_resource(Mode(DARK))
            .insert_resource(DARK.scrollbar())
            .insert_resource(Selected(0))
            .init_resource::<Copied>()
            .add_plugins(pages::plugin)
            .add_systems(
                Update,
                restore_scroll.run_if(resource_exists::<SavedScroll>),
            );
    }
}

fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    fonts
        .insert(AssetId::default(), Font::from_bytes(FONT.to_vec()))
        .expect("the default font handle accepts a font");
    commands.spawn(Camera2d);
}

fn mount_when_sized(world: &mut World) {
    let sized = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .iter(world)
        .any(|window| {
            window.physical_size() == UVec2::new(WIDTH, HEIGHT)
        });
    if !sized
        && world.resource::<Time<Real>>().elapsed() < WINDOW_WAIT
    {
        return;
    }
    mount::<Palette>(world, shell());
    world.insert_resource(Ready);
}

pub fn shell() -> impl bevy_fynix::View<Bevy, Palette> {
    keyed(resource::<Mode, _>(|mode| mode.0), |_| {
        AnyView::new(|cx: &mut Build| {
            let panel = cx.theme().panel();
            cx.build(
                row((sidebar(), page()))
                    .width(percent(100.0))
                    .height(percent(100.0))
                    .min_height(px(0.0))
                    .gap(0.0)
                    .fill(panel),
            )
        })
    })
    .within(
        column(())
            .width(percent(100.0))
            .height(percent(100.0))
            .min_height(px(0.0)),
    )
}

fn sidebar() -> AnyView<Bevy, Palette> {
    AnyView::new(|cx: &mut Build| {
        let fill = cx.theme().fill();
        let entries = PAGES
            .iter()
            .enumerate()
            .map(|(index, page)| {
                button(label(page.name).tone(
                    resource::<Selected, _>(move |selected| {
                        if selected.0 == index {
                            Tone::Accent
                        } else {
                            Tone::Body
                        }
                    }),
                ))
                .width(percent(100.0))
                .justify(JustifyContent::Start)
                .rules(ghost)
                .on_activate(move |world| {
                    world.insert_resource(Selected(index));
                })
                .boxed()
            })
            .collect::<Vec<_>>();
        cx.build(
            column((
                label("rubrix").size(18.0),
                column(entries).gap(2.0),
                column(()).grow(1.0),
                button(label(resource::<Mode, _>(|mode| {
                    format!("theme: {}", mode.0.name())
                })))
                .width(percent(100.0))
                .justify(JustifyContent::Start)
                .rules(ghost)
                .on_activate(toggle_theme),
            ))
            .width(px(SIDEBAR_WIDTH))
            .shrink(0.0)
            .height(percent(100.0))
            .padding(UiRect::all(px(12.0)))
            .gap(12.0)
            .fill(fill),
        )
    })
}

fn page() -> AnyView<Bevy, Palette> {
    keyed(resource::<Selected, _>(|selected| *selected), |selected| {
        (PAGES[selected.0].build)()
    })
    .within(
        column(())
            .grow(1.0)
            .height(percent(100.0))
            .min_width(px(0.0))
            .padding(UiRect::all(px(20.0)))
            .with(PageArea),
    )
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(value: &str) -> Option<&OsStr> {
        Some(OsStr::new(value))
    }

    #[test]
    fn a_scripted_run_skips_vsync_unless_told_not_to() {
        assert_eq!(
            present_mode(true, None),
            Ok(PresentMode::AutoNoVsync)
        );
        assert_eq!(
            present_mode(true, mode("0")),
            Ok(PresentMode::AutoVsync)
        );
        assert_eq!(
            present_mode(true, mode("1")),
            Ok(PresentMode::AutoNoVsync)
        );
    }

    #[test]
    fn an_interactive_run_keeps_vsync_unless_asked() {
        assert_eq!(
            present_mode(false, None),
            Ok(PresentMode::AutoVsync)
        );
        assert_eq!(
            present_mode(false, mode("1")),
            Ok(PresentMode::AutoNoVsync)
        );
    }

    #[test]
    fn a_value_other_than_0_or_1_is_refused() {
        for scripted in [true, false] {
            for value in ["true", "", "2", " 1"] {
                let error = present_mode(scripted, mode(value))
                    .expect_err(value);
                assert!(
                    error.contains(
                        "RUBRIX_NO_VSYNC must be unset, 0 or 1"
                    ),
                    "{error}"
                );
            }
        }
    }
}
