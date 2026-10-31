use std::num::NonZeroUsize;
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
use bevy::text::TextLayoutInfo;
use bevy::ui::{
    ComputedNode, FlexWrap, Node, UiGlobalTransform, UiRect,
    UiSystems, px,
};
use bevy_fynix::tokens::{SpacingTokens, SurfaceTokens};
use bevy_fynix::views::{
    BehaviorExt, FrameProps, VirtualList, button, ghost, label,
};
use bevy_fynix::{AnyView, Prop, ScopedExt, ViewExt};
use bevy_rubrix::CodeCopied;

use crate::{Build, Copied, PageView, Palette};

mod about;
mod code;
mod diff;
mod editor;
pub mod hamlet_code;
pub mod hamlet_diff;
pub mod hamlet_rich;
pub mod rich;
pub mod stream;
pub mod stress;
pub mod virtual_list;

const BUTTON_WIDTH: f32 = 140.0;

pub type Open = fn(&mut World, &[PathBuf]) -> Result<(), String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    Caret,
    Text(NonZeroUsize),
    Undo,
    Ime,
}

impl Ask {
    pub fn of(topic: &str) -> Option<Self> {
        Some(match topic {
            "caret" => Self::Caret,
            "undo" => Self::Undo,
            _ => {
                Self::Text(topic.strip_prefix("text ")?.parse().ok()?)
            }
        })
    }
}

pub type Answer = fn(&mut World, Ask) -> Option<String>;

pub struct Page {
    pub name: &'static str,
    pub build: fn() -> PageView,
    pub open: Option<Open>,
    pub answer: Option<Answer>,
}

pub const PAGES: &[Page] = &[
    Page {
        name: "about",
        build: about::build,
        open: None,
        answer: None,
    },
    Page {
        name: "rich",
        build: rich::build,
        open: None,
        answer: None,
    },
    Page {
        name: "virtual_list",
        build: virtual_list::build,
        open: None,
        answer: None,
    },
    Page {
        name: "code",
        build: code::build,
        open: Some(code::open),
        answer: None,
    },
    Page {
        name: "stream",
        build: stream::build,
        open: Some(stream::open),
        answer: None,
    },
    Page {
        name: "diff",
        build: diff::build,
        open: Some(diff::open),
        answer: None,
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
        answer: None,
    },
    Page {
        name: "hamlet_code",
        build: hamlet_code::build,
        open: None,
        answer: None,
    },
    Page {
        name: "hamlet_diff",
        build: hamlet_diff::build,
        open: None,
        answer: None,
    },
    Page {
        name: "stress",
        build: stress::build,
        open: None,
        answer: None,
    },
    Page {
        name: "editor",
        build: editor::build,
        open: Some(editor::open),
        answer: Some(editor::answer),
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
    stress::plugin(app);
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

/// The rows of `list` that show in its viewport, as the frame lays it
/// out, with no glyph drawn on them.
pub fn blank_rows(world: &World, list: Entity) -> usize {
    let (Some(rows), Some(node), Some(at)) = (
        world.get::<VirtualList>(list),
        world.get::<ComputedNode>(list),
        world.get::<UiGlobalTransform>(list),
    ) else {
        return 0;
    };
    let view = node.content_box();
    if rows.count() == 0 || view.height() <= 0.0 {
        return 0;
    }
    let scale = node.inverse_scale_factor();
    let scroll = node.scroll_position.y;
    let top = at.translation.y + view.min.y - scroll;
    let row_at = |y: f32| rows.row_at((y - top) * scale);
    let (Some(first), Some(last)) = (
        row_at(top + scroll + 0.5),
        row_at(top + scroll + view.height() - 0.5),
    ) else {
        return 0;
    };
    let drawn = descendants_in_order(world, list)
        .into_iter()
        .filter(|&node| {
            world
                .get::<TextLayoutInfo>(node)
                .is_some_and(|text| !text.glyphs.is_empty())
        })
        .filter_map(|node| {
            row_at(
                world.get::<UiGlobalTransform>(node)?.translation.y,
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    (first..=last).filter(|row| !drawn.contains(row)).count()
}

/// Hamlet's plate around a code or diff block.
pub fn plated<V: FrameProps>(view: V, theme: &Palette) -> V {
    view.padding(UiRect::axes(px(10.0), px(8.0)))
        .fill(theme.fill())
        .border(1.0)
        .border_color(theme.hover())
        .radius(theme.radius())
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
