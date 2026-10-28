//! The 100,000-line stress file in a code view beside the
//! 100,000-line stress patch in a diff view.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::ui::{ComputedNode, ScrollPosition, percent, px};
use bevy_fynix::views::{
    FrameProps, VirtualList, column, label, row,
};
use bevy_fynix::{AnyView, ViewExt};
use bevy_rubrix::{
    CodeDoc, CodeSource, DiffDoc, DiffSource, Lang, code_view,
    diff_view,
};

use super::code::stress;
use super::diff::{STRESS_PATCH, patch};
use super::find_under;
use crate::{Build, PageView};

/// Both sources, made on the first visit and kept, since each takes
/// a while to make.
#[derive(Resource)]
struct Sources {
    code: CodeSource,
    diff: DiffSource,
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
        }
        let sources = cx.world.resource::<Sources>();
        let (code, diff) =
            (sources.code.clone(), sources.diff.clone());
        cx.build(
            column((
                label("Stress").size(20.0),
                row((code_view(code), diff_view(diff)))
                    .grow(1.0)
                    .min_height(px(0.0))
                    .gap(12.0),
            ))
            .height(percent(100.0))
            .min_height(px(0.0))
            .gap(8.0)
            .with(StressPage),
        )
    })
    .boxed()
}

/// The `stress` log line: the last row each view shows, counted from
/// 1, of its rows.
pub fn log(world: &mut World) -> String {
    let Some(page) = world
        .query_filtered::<Entity, With<StressPage>>()
        .iter(world)
        .next()
    else {
        return "stress none".into();
    };
    let code = last_shown::<CodeDoc>(world, page);
    let diff = last_shown::<DiffDoc>(world, page);
    format!("stress code last {code} diff last {diff}")
}

fn last_shown<D: Component>(world: &World, page: Entity) -> String {
    let shown = find_under::<D>(world, page)
        .and_then(|view| find_under::<VirtualList>(world, view))
        .and_then(|list| {
            let rows = world.get::<VirtualList>(list)?;
            let node = world.get::<ComputedNode>(list)?;
            let top = world.get::<ScrollPosition>(list)?.y;
            let height = node.size().y * node.inverse_scale_factor();
            let last = rows.row_at(top + height - 1.0)?;
            Some(format!("{} of {}", last + 1, rows.count()))
        });
    shown.unwrap_or_else(|| "none".into())
}
