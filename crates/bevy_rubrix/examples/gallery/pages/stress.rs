//! The 100,000-line stress file in a code view and the 100,000-line
//! stress patch in a diff view, one at a time. Side by side, a scroll
//! of either took 4.9 to 5.1 ms a frame at p95, over the 3.0 ms that
//! R12 and R15 give each view.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::ui::{ComputedNode, percent, px};
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
use super::{control, find_under};
use crate::{Build, PageView};

/// Both sources, made on the first visit and kept, since each takes
/// a while to make.
#[derive(Resource)]
struct Sources {
    code: CodeSource,
    diff: DiffSource,
}

/// The view on show.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum Shown {
    #[default]
    Code,
    Diff,
}

#[derive(Component)]
struct StressPage;

pub fn build() -> PageView {
    AnyView::new(|cx: &mut Build| {
        if !cx.world.contains_resource::<Sources>() {
            cx.world.insert_resource(Sources {
                code: CodeSource::new(Lang::Rust, stress()),
                diff: patch(STRESS_PATCH),
            });
            cx.world.init_resource::<Shown>();
        }
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
                    control("code", |world| {
                        world.insert_resource(Shown::Code);
                    }),
                    control("diff", |world| {
                        world.insert_resource(Shown::Diff);
                    }),
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

/// The `stress` log line: the view on show, and the last of its rows
/// that shows, counted from 1.
pub fn log(world: &mut World) -> String {
    let Some(page) = world
        .query_filtered::<Entity, With<StressPage>>()
        .iter(world)
        .next()
    else {
        return "stress none".into();
    };
    match *world.resource::<Shown>() {
        Shown::Code => format!(
            "stress code last {}",
            last_shown::<CodeDoc>(world, page)
        ),
        Shown::Diff => format!(
            "stress diff last {}",
            last_shown::<DiffDoc>(world, page)
        ),
    }
}

fn last_shown<D: Component>(world: &World, page: Entity) -> String {
    let shown = find_under::<D>(world, page)
        .and_then(|view| find_under::<VirtualList>(world, view))
        .and_then(|list| {
            let rows = world.get::<VirtualList>(list)?;
            let node = world.get::<ComputedNode>(list)?;
            let view =
                node.content_box().size() - node.scrollbar_size;
            let bottom = (node.scroll_position.y + view.y)
                * node.inverse_scale_factor();
            let last = rows.row_at(bottom - 0.5)?;
            Some(format!("{} of {}", last + 1, rows.count()))
        });
    shown.unwrap_or_else(|| "none".into())
}
