use std::path::PathBuf;

use bevy::app::{App, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, ResMut};
use bevy::ecs::world::World;
use bevy::ui::{FlexWrap, Node, UiSystems, px};
use bevy_fynix::views::{
    BehaviorExt, FrameProps, VirtualList, button, ghost, label,
};
use bevy_fynix::{AnyView, Prop, ScopedExt, ViewExt};
use bevy_rubrix::CodeCopied;

use crate::{Build, Copied, PageView, Palette};

mod about;
mod code;
mod diff;
pub mod hamlet_code;
pub mod hamlet_diff;
pub mod hamlet_rich;
pub mod rich;
pub mod stream;
pub mod stress;
pub mod virtual_list;

const BUTTON_WIDTH: f32 = 140.0;

pub type Open = fn(&mut World, &[PathBuf]) -> Result<(), String>;

pub struct Page {
    pub name: &'static str,
    pub build: fn() -> PageView,
    pub open: Option<Open>,
}

pub const PAGES: &[Page] = &[
    Page {
        name: "about",
        build: about::build,
        open: None,
    },
    Page {
        name: "rich",
        build: rich::build,
        open: None,
    },
    Page {
        name: "virtual_list",
        build: virtual_list::build,
        open: None,
    },
    Page {
        name: "code",
        build: code::build,
        open: Some(code::open),
    },
    Page {
        name: "stream",
        build: stream::build,
        open: Some(stream::open),
    },
    Page {
        name: "diff",
        build: diff::build,
        open: Some(diff::open),
    },
    Page {
        name: "hamlet_rich",
        build: || {
            hamlet_rich::view::<Palette>(control(
                "stream",
                hamlet_rich::stream,
            ))
        },
        open: None,
    },
    Page {
        name: "hamlet_code",
        build: hamlet_code::build,
        open: None,
    },
    Page {
        name: "hamlet_diff",
        build: hamlet_diff::build,
        open: None,
    },
    Page {
        name: "stress",
        build: stress::build,
        open: None,
    },
];

pub fn plugin(app: &mut App) {
    rich::plugin(app);
    virtual_list::plugin(app);
    code::plugin(app);
    stream::plugin(app);
    diff::plugin(app);
    hamlet_rich::plugin(app);
    hamlet_code::plugin(app);
    app.add_systems(PostUpdate, fit_rows.before(UiSystems::Layout))
        .add_observer(
            |copied: On<CodeCopied>, mut last: ResMut<Copied>| {
                last.0 = Some(copied.text.clone());
            },
        );
}

fn control(
    text: impl Into<Prop<String>>,
    press: impl Fn(&mut World) + Send + Sync + 'static,
) -> PageView {
    button(label(text))
        .width(px(BUTTON_WIDTH))
        .rules(ghost)
        .on_activate(press)
        .boxed()
}

fn wrapping(buttons: PageView) -> PageView {
    AnyView::new(|cx: &mut Build| {
        let root = cx.build(buttons);
        cx.world
            .get_mut::<Node>(root)
            .expect("a row node")
            .flex_wrap = FlexWrap::Wrap;
        root
    })
    .boxed()
}

#[derive(Component)]
pub struct FitRows;

fn fit_rows(
    mut boxes: Query<(Entity, &mut Node), With<FitRows>>,
    children: Query<&Children>,
    lists: Query<&VirtualList>,
) {
    for (fit, mut node) in &mut boxes {
        let Some(list) = children
            .iter_descendants(fit)
            .find_map(|node| lists.get(node).ok())
        else {
            continue;
        };
        let rows = list
            .count()
            .checked_sub(1)
            .and_then(|last| list.row_span(last))
            .map_or(0.0, |last| last.top + last.height);
        let height = px(rows.ceil());
        if node.height != height {
            node.height = height;
        }
    }
}

pub fn descendants_in_order(
    world: &World,
    root: Entity,
) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        found.push(node);
        if let Some(children) = world.get::<Children>(node) {
            stack.extend(children.iter().rev());
        }
    }
    found
}

pub fn find_under<C: Component>(
    world: &World,
    root: Entity,
) -> Option<Entity> {
    descendants_in_order(world, root)
        .into_iter()
        .find(|&node| world.get::<C>(node).is_some())
}
