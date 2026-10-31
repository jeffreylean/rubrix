use bevy::app::App;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::{Mut, World};
use bevy::ui::{Display, Val, percent, px};
use bevy_fynix::views::{
    FrameProps, RowSlot, VirtualList, column, frame, label, row,
    scroll, virtual_list,
};
use bevy_fynix::{Signal, ViewExt, keyed, resource};

use super::{control, wrapping};
use crate::PageView;

/// The height of a one-line row: the body size, 14 px, at Bevy's
/// line height of 1.2, rounded up to whole pixels.
const ESTIMATE: f32 = 17.0;

const SPLICED: usize = 20;
const MIDDLE: usize = 5_000;
const ABOVE: usize = 1_000;
const JUMP: usize = 9_000;
const WIDE: f32 = 3_000.0;

#[derive(Clone, Copy, PartialEq)]
enum Label {
    Row(usize),
    New(usize),
    Grown(usize),
    Wide(usize),
}

impl Label {
    fn number(self, index: usize) -> usize {
        match self {
            Self::Row(n) | Self::Grown(n) | Self::Wide(n) => n,
            Self::New(_) => index,
        }
    }
}

#[derive(Resource)]
struct Rows {
    labels: Vec<Label>,
    plain: bool,
    follow: bool,
    splice: bool,
    sideways: bool,
}

impl Default for Rows {
    fn default() -> Self {
        Self {
            labels: numbered(10_000),
            plain: false,
            follow: false,
            splice: false,
            sideways: false,
        }
    }
}

fn numbered(count: usize) -> Vec<Label> {
    (0..count).map(Label::Row).collect()
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Rows>();
}

pub fn build() -> PageView {
    column((
        label("Virtual list").size(20.0),
        controls(),
        splice_controls(),
        rows(),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn toggle(
    name: &'static str,
    on: fn(&Rows) -> bool,
    flip: fn(&mut Rows),
) -> PageView {
    control(
        resource::<Rows, _>(move |rows| {
            format!("{name} {}", if on(rows) { "on" } else { "off" })
        }),
        move |world| flip(&mut world.resource_mut::<Rows>()),
    )
}

fn controls() -> PageView {
    let buttons = row((
        control("10,000 rows", |world| {
            world.resource_mut::<Rows>().labels = numbered(10_000);
        }),
        control("100,000 rows", |world| {
            world.resource_mut::<Rows>().labels = numbered(100_000);
        }),
        control("append 1,000", |world| {
            let mut rows = world.resource_mut::<Rows>();
            let len = rows.labels.len();
            rows.labels.extend((len..len + 1_000).map(Label::Row));
        }),
        toggle("plain", |rows| rows.plain, |rows| rows.plain ^= true),
        toggle(
            "follow",
            |rows| rows.follow,
            |rows| rows.follow ^= true,
        ),
        toggle(
            "splice",
            |rows| rows.splice,
            |rows| rows.splice ^= true,
        ),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

fn splice_controls() -> PageView {
    let buttons = row((
        control("insert 20", |world| {
            splice(world, MIDDLE, 0, new_rows());
        }),
        control("remove 20", |world| {
            splice(world, MIDDLE, SPLICED, Vec::new());
        }),
        control("insert above", |world| {
            splice(world, ABOVE, 0, new_rows());
        }),
        control("grow above", |world| {
            let at = ABOVE + 1;
            let was = world.resource::<Rows>().labels[at];
            splice(world, at, 1, vec![Label::Grown(was.number(at))]);
        }),
        control("jump and insert", |world| {
            if let Some(mut list) = first_list(world) {
                list.scroll_to(JUMP);
            }
            splice(world, ABOVE, 0, new_rows());
        }),
        control("reveal wide", |world| {
            let labels = &mut world.resource_mut::<Rows>().labels;
            labels[MIDDLE] =
                Label::Wide(labels[MIDDLE].number(MIDDLE));
            if let Some(mut list) = first_list(world) {
                list.scroll_x_into_view(WIDE..WIDE);
            }
        }),
        toggle(
            "sideways",
            |rows| rows.sideways,
            |rows| rows.sideways ^= true,
        ),
    ))
    .gap(8.0)
    .display(resource::<Rows, _>(|rows| {
        if rows.splice {
            Display::Flex
        } else {
            Display::None
        }
    }));
    wrapping(buttons.boxed())
}

fn new_rows() -> Vec<Label> {
    (1..=SPLICED).map(Label::New).collect()
}

fn first_list(world: &mut World) -> Option<Mut<'_, VirtualList>> {
    world.query::<&mut VirtualList>().iter_mut(world).next()
}

fn splice(
    world: &mut World,
    at: usize,
    removed: usize,
    inserted: Vec<Label>,
) {
    let added = inserted.len();
    world
        .resource_mut::<Rows>()
        .labels
        .splice(at..at + removed, inserted);
    if let Some(mut list) = first_list(world) {
        list.splice(at, removed, added);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Plain(usize),
    List { sideways: bool },
}

fn rows() -> PageView {
    let shape = resource::<Rows, _>(|rows| {
        if rows.plain {
            Shape::Plain(rows.labels.len())
        } else {
            Shape::List {
                sideways: rows.splice && rows.sideways,
            }
        }
    });
    keyed(shape, |shape| match *shape {
        Shape::List { sideways } => virtual_list(
            resource::<Rows, _>(|rows| rows.labels.len()),
            ESTIMATE,
            move |slot: RowSlot| {
                let text = label(row_label(slot, text)).wrap(false);
                if !sideways {
                    return text.boxed();
                }
                let end = label(row_label(slot, end)).wrap(false);
                let wide = row((text, frame().grow(1.0), end))
                    .width(row_label(slot, width));
                slot.sideways(wide.boxed())
            },
        )
        .follow_bottom(resource::<Rows, _>(|rows| rows.follow))
        .sideways(sideways)
        .boxed(),
        Shape::Plain(count) => scroll(
            (0..count)
                .map(|index| {
                    label(text(Label::Row(index))).wrap(false).boxed()
                })
                .collect::<Vec<_>>(),
        )
        .gap(0.0)
        .grow(1.0)
        .boxed(),
    })
    .within(column(()).grow(1.0).min_height(px(0.0)))
    .boxed()
}

fn row_label<T: 'static>(
    slot: RowSlot,
    read: fn(Label) -> T,
) -> Signal<T> {
    let at = slot.index();
    let mut moved = slot.index();
    let mut edited = resource::<Rows, _>(|_| ());
    Signal::new(
        move |world: &World| {
            let labels = &world.resource::<Rows>().labels;
            let index = at.get(world);
            read(
                labels
                    .get(index)
                    .copied()
                    .unwrap_or(Label::Row(index)),
            )
        },
        move |world: &World| {
            moved.changed(world) | edited.changed(world)
        },
    )
}

fn text(label: Label) -> String {
    match label {
        Label::Row(n) if n.is_multiple_of(100) => {
            format!("Row {n}\n  its second line\n  its third line")
        }
        Label::Row(n) | Label::Wide(n) => format!("Row {n}"),
        Label::New(k) => format!("New {k}"),
        Label::Grown(n) => {
            format!("Row {n}\n  grown to\n  three lines")
        }
    }
}

fn end(label: Label) -> String {
    match label {
        Label::Wide(n) => format!("the end of Row {n}"),
        _ => String::new(),
    }
}

fn width(label: Label) -> Val {
    match label {
        Label::Wide(_) => px(WIDE),
        _ => Val::Auto,
    }
}
