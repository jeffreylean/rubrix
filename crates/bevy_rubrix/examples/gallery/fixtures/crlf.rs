//! The theme the examples share.

use core::time::Duration;

use bevy::color::Color;
use bevy_fynix::tokens::{
    Curve, Motion, MotionTokens, SpacingTokens, SurfaceTokens,
    TextTokens, Tone,
};

/// The examples' theme. Nothing in the views names it: they ask for
/// whatever token traits they need, and this implements them.
pub struct Monokai;

fn hex(rgb: u32) -> Color {
    let [_, r, g, b] = rgb.to_be_bytes();
    Color::srgb_u8(r, g, b)
}

impl TextTokens for Monokai {
    fn tone(&self, tone: Tone) -> Color {
        match tone {
            Tone::Body => hex(0xFCFCFA),
            Tone::Dim => hex(0x939293),
            Tone::Faint => hex(0x727072),
            Tone::Accent => hex(0xFFD866),
            Tone::OnAccent => hex(0x2D2A2E),
            Tone::Critical => hex(0xFF6188),
        }
    }

    fn body_size(&self) -> f32 {
        14.0
    }

    fn small_size(&self) -> f32 {
        11.0
    }
}

impl SurfaceTokens for Monokai {
    fn fill(&self) -> Color {
        hex(0x403E41)
    }

    fn hover(&self) -> Color {
        hex(0x5B595C)
    }

    fn panel(&self) -> Color {
        hex(0x221F22)
    }

    fn accent(&self) -> Color {
        hex(0xFFD866)
    }
}

impl SpacingTokens for Monokai {
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

impl MotionTokens for Monokai {
    fn motion(&self, motion: Motion) -> Curve {
        let millis = match motion {
            Motion::Interact => 180,
            Motion::Expand => 280,
        };
        Curve {
            duration: Duration::from_millis(millis),
            // Ease out: quick to answer, gentle to land.
            ease: |t| 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}
//! Docking on its own. Three panels start as tabs in one area. Try:
//! - dragging a tab along its bar to reorder it,
//! - dragging a tab onto another area's bar to move it there,
//! - dragging a tab onto an area's edge to split the area,
//! - dragging the line between two areas to resize them,
//! - pressing Escape mid-drag to cancel,
//! - closing a tab, and adding it back from the "+" of a bar.
//!
//! `cargo run -p bevy_fynix --example dock`

mod common;

use bevy::DefaultPlugins;
use bevy::app::{App, Startup};
use bevy::camera::Camera2d;
use bevy::ecs::world::World;
use bevy::ui::percent;
use bevy_fynix::dock::{
    DockAreaStyle, DockLeaf, DockPlugin, DockRegistry, DockTree,
    DockWindowKind, dock,
};
use bevy_fynix::views::{FrameProps, column, label};
use bevy_fynix::{AnyView, Bevy, FynixPlugin, Theme, ViewExt, mount};
use common::Monokai;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FynixPlugin::<Monokai>::default(),
            DockPlugin::<Monokai>::default(),
        ))
        .insert_resource(Theme(Monokai))
        .add_systems(Startup, setup)
        .run();
}

fn setup(world: &mut World) {
    world.spawn(Camera2d);

    let mut registry = world.resource_mut::<DockRegistry<Monokai>>();
    for (id, name) in [
        ("panel_a", "Panel A"),
        ("panel_b", "Panel B"),
        ("panel_c", "Panel C"),
    ] {
        registry.register(
            id,
            DockWindowKind::new(name, move || panel(name)),
        );
    }

    world.resource_mut::<DockTree>().set_root_leaf(
        DockLeaf::new("root", DockAreaStyle::TabBar).with_windows(
            vec![
                "panel_a".into(),
                "panel_b".into(),
                "panel_c".into(),
            ],
        ),
    );

    mount::<Monokai>(
        world,
        column((dock::<Monokai>(),))
            .width(percent(100.0))
            .height(percent(100.0)),
    );
}

/// A panel's whole content: its name.
fn panel(name: &'static str) -> AnyView<Bevy, Monokai> {
    label(name).size(20.0).boxed()
}
//! The crate's views in a window, one section per idea: a theme
//! implemented through token traits, an app-wide set rule, bound
//! labels, hover rules with transitions, one state rule reaching a
//! button's parts, button styles (bundles and a custom one), a scoped
//! rule, a folding section, field rows, text and number fields, a
//! screen switch and a keyed list that rebuild structure and animate
//! views in and out, menus (a dropdown, a right-click menu and a
//! tooltip), and a reduced-motion switch.
//!
//! `cargo run -p bevy_fynix --example gallery`

use bevy::DefaultPlugins;
use bevy::app::{App, Startup};
use bevy::asset::Handle;
use bevy::camera::Camera2d;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::time::Time;
use bevy::ui::{AlignItems, FlexDirection, UiRect, percent, px};
use bevy_fynix::dock::{
    DockAreaStyle, DockLeaf, DockPlugin, DockRegistry, DockTree,
    DockWindowKind, Edge, dock,
};
use bevy_fynix::tokens::{
    Motion, SpacingTokens, SurfaceTokens, Tone,
};
use bevy_fynix::views::{
    AnimatedField, BehaviorExt, ContextMenuExt, Frame, FrameProps,
    HasAction, Label, TooltipExt, button, checkbox, column, danger,
    dropdown, field_row, foldable, frame, ghost, icon_button, label,
    menu_bar, menu_item, number_field, primary, row, scroll,
    segmented, text_field, tint,
};
use bevy_fynix::{
    AnyView, Bevy, Cx, FynixPlugin, Hovered, Pressed, ReducedMotion,
    ScopedExt, StateExt, Style, Theme, View, ViewExt, each, hidden,
    keyed, mount, resource, style,
};

mod common;

use common::Monokai;

/// What a view is built with, in this app.
type Build<'a> = Cx<'a, Bevy, Monokai>;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FynixPlugin::<Monokai>::default(),
            DockPlugin::<Monokai>::default(),
        ))
        .insert_resource(Theme(Monokai))
        .insert_resource(Clicks(0))
        .insert_resource(Mode(0))
        .insert_resource(Agreed(false))
        .insert_resource(Easing(0))
        .insert_resource(Screen::Overview)
        .insert_resource(Title("fynix".into()))
        .insert_resource(Amount(1.0))
        .insert_resource(Copies(1))
        .insert_resource(Rows {
            ids: vec![1, 2, 3],
            next: 4,
        })
        .add_systems(Startup, setup)
        .run();
}

/// How many times the counter's button was pressed.
#[derive(Resource)]
struct Clicks(u32);

/// The segment picked in the segmented control.
#[derive(Resource)]
struct Mode(usize);

/// Whether the checkbox is ticked.
#[derive(Resource)]
struct Agreed(bool);

fn setup(world: &mut World) {
    world.spawn(Camera2d);
    seed_dock(world);
    mount::<Monokai>(world, gallery());
}

/// Three window kinds, two of them tabbed in one area and the third
/// in an area beside it.
fn seed_dock(world: &mut World) {
    let mut registry = world.resource_mut::<DockRegistry<Monokai>>();
    for (id, name) in
        [("notes", "Notes"), ("log", "Log"), ("stats", "Stats")]
    {
        let window = DockWindowKind::new(name, move || {
            label(format!("The {name} window")).boxed()
        });
        registry.register(id, window);
    }
    let mut tree = world.resource_mut::<DockTree>();
    let main = tree.set_root_leaf(
        DockLeaf::new("main", DockAreaStyle::TabBar)
            .with_windows(vec!["notes".into(), "log".into()]),
    );
    tree.split(main, Edge::Right, "stats".into());
}

/// Split panes with tabs: drag the line between them (it shows when
/// hovered), click a tab, close one, or add a window with the plus.
/// Insert a `DockIcons` to draw the close and plus as images.
fn docking() -> impl View<Bevy, Monokai> {
    column((dock::<Monokai>(),))
        .width(px(520.0))
        .height(px(200.0))
}

fn gallery() -> AnyView<Bevy, Monokai> {
    AnyView::new(|cx: &mut Build| {
        // The app's preamble: every label is 13px unless its call
        // site or an inner scope says otherwise.
        cx.set::<Label>(|label, _| label.size(13.0));
        let panel = cx.theme().panel();
        cx.build(
            scroll((
                label("Fynix on Bevy").size(20.0),
                section("Bound values", bound_values()),
                section("Hover, with a transition", hover_list()),
                section(
                    "One state rule across a button's parts",
                    parts(),
                ),
                section(
                    "Button variants as rule bundles",
                    variants(),
                ),
                section("Styles", styles()),
                section("Segmented control and checkbox", controls()),
                section("A scoped rule", scoped()),
