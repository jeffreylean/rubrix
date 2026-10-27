use bevy::app::App;
use bevy::ecs::resource::Resource;
use bevy::ui::{percent, px};
use bevy_fynix::views::{
    FrameProps, RowSlot, column, label, row, scroll, virtual_list,
};
use bevy_fynix::{ViewExt, keyed, resource};

use super::control;
use crate::PageView;

/// The height of a one-line row: the body size, 14 px, at Bevy's
/// line height of 1.2, rounded up to whole pixels.
const ESTIMATE: f32 = 17.0;

#[derive(Resource)]
pub struct Rows {
    count: usize,
    plain: bool,
}

impl Default for Rows {
    fn default() -> Self {
        Self {
            count: 10_000,
            plain: false,
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Rows>();
}

pub fn build() -> PageView {
    column((label("Virtual list").size(20.0), controls(), rows()))
        .height(percent(100.0))
        .min_height(px(0.0))
        .gap(8.0)
        .boxed()
}

fn controls() -> PageView {
    row((
        control("10,000 rows", |world| {
            world.resource_mut::<Rows>().count = 10_000;
        }),
        control("100,000 rows", |world| {
            world.resource_mut::<Rows>().count = 100_000;
        }),
        control("append 1,000", |world| {
            world.resource_mut::<Rows>().count += 1_000;
        }),
        control(
            resource::<Rows, _>(|rows| {
                let on = if rows.plain { "on" } else { "off" };
                format!("plain {on}")
            }),
            |world| {
                let mut rows = world.resource_mut::<Rows>();
                rows.plain = !rows.plain;
            },
        ),
    ))
    .gap(8.0)
    .boxed()
}

/// The virtual list, or with the plain stack on, a scroll area of a
/// label per row, built again whenever the count changes.
fn rows() -> PageView {
    let plain =
        resource::<Rows, _>(|rows| rows.plain.then_some(rows.count));
    keyed(plain, |plain| match *plain {
        None => virtual_list(
            resource::<Rows, _>(|rows| rows.count),
            ESTIMATE,
            |slot: RowSlot| {
                label(slot.index().map(text)).wrap(false).boxed()
            },
        )
        .boxed(),
        Some(count) => scroll(
            (0..count)
                .map(|index| label(text(index)).wrap(false).boxed())
                .collect::<Vec<_>>(),
        )
        .gap(0.0)
        .grow(1.0)
        .boxed(),
    })
    .within(column(()).grow(1.0).min_height(px(0.0)))
    .boxed()
}

/// `Row <index>`, three lines long for every hundredth row.
fn text(index: usize) -> String {
    if index.is_multiple_of(100) {
        format!("Row {index}\n  its second line\n  its third line")
    } else {
        format!("Row {index}")
    }
}
