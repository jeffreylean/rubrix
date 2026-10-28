use std::mem;

use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::world::World;
use bevy::ui::{ComputedNode, UiSystems, percent, px};
use bevy_fynix::views::{
    FrameProps, VirtualList, column, label, row,
};
use bevy_fynix::{AnyView, ViewExt, keyed, resource};
use bevy_rubrix::{
    CodeDoc, CodeSource, DiffDoc, DiffSource, Lang, code_view,
    diff_view,
};

use super::code::stress;
use super::diff::{STRESS_PATCH, patch};
use super::{blank_rows, control, find_under};
use crate::{Build, PageView};

#[derive(Resource)]
struct Sources {
    code: CodeSource,
    diff: DiffSource,
}

#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum Shown {
    #[default]
    Code,
    Diff,
}

#[derive(Component)]
struct StressPage;

#[derive(Resource, Default)]
struct Blank {
    frames: u32,
    blank: u32,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Blank>().add_systems(
        PostUpdate,
        measure.after(UiSystems::PostLayout),
    );
}

fn show(world: &mut World, shown: Shown) {
    world.insert_resource(shown);
    world.insert_resource(Blank::default());
}

fn measure(world: &mut World) {
    let Some(list) = shown_list(world) else {
        return;
    };
    let blank = blank_rows(world, list);
    let mut count = world.resource_mut::<Blank>();
    count.frames += 1;
    count.blank += u32::from(blank > 0);
}

fn shown_list(world: &mut World) -> Option<Entity> {
    let page = world
        .query_filtered::<Entity, With<StressPage>>()
        .iter(world)
        .next()?;
    let view = match *world.resource::<Shown>() {
        Shown::Code => find_under::<CodeDoc>(world, page),
        Shown::Diff => find_under::<DiffDoc>(world, page),
    }?;
    find_under::<VirtualList>(world, view)
}

pub fn build() -> PageView {
    AnyView::new(|cx: &mut Build| {
        if !cx.world.contains_resource::<Sources>() {
            cx.world.insert_resource(Sources {
                code: CodeSource::new(Lang::Rust, stress()),
                diff: patch(STRESS_PATCH),
            });
            cx.world.init_resource::<Shown>();
        }
        cx.world.insert_resource(Blank::default());
        let sources = cx.world.resource::<Sources>();
        let (code, diff) =
            (sources.code.clone(), sources.diff.clone());
        let shown = keyed(
            resource::<Shown, _>(|shown| *shown),
            move |shown| match shown {
                Shown::Code => code_view(code.clone()).boxed(),
                Shown::Diff => diff_view(diff.clone()).boxed(),
            },
        )
        .within(column(()).grow(1.0).min_height(px(0.0)));
        cx.build(
            column((
                label("Stress").size(20.0),
                row((
                    control("code", |world| show(world, Shown::Code)),
                    control("diff", |world| show(world, Shown::Diff)),
                ))
                .gap(8.0),
                shown,
            ))
            .height(percent(100.0))
            .min_height(px(0.0))
            .gap(8.0)
            .with(StressPage),
        )
    })
    .boxed()
}

pub fn log(world: &mut World) -> String {
    let Some(list) = shown_list(world) else {
        return "stress none".into();
    };
    let name = match *world.resource::<Shown>() {
        Shown::Code => "code",
        Shown::Diff => "diff",
    };
    let shown =
        last_shown(world, list).unwrap_or_else(|| "none".into());
    format!("stress {name} last {shown}")
}

pub fn blank_line(world: &mut World) -> String {
    let blank = mem::take(&mut *world.resource_mut::<Blank>());
    format!(
        "stress-blank frames {} blank {}",
        blank.frames, blank.blank
    )
}

fn last_shown(world: &World, list: Entity) -> Option<String> {
    let rows = world.get::<VirtualList>(list)?;
    let node = world.get::<ComputedNode>(list)?;
    let view = node.content_box().size() - node.scrollbar_size;
    let bottom = (node.scroll_position.y + view.y)
        * node.inverse_scale_factor();
    let last = rows.row_at(bottom - 0.5)?;
    Some(format!("{} of {}", last + 1, rows.count()))
}
