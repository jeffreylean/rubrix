use std::path::PathBuf;

use bevy::app::App;
use bevy::ecs::observer::On;
use bevy::ecs::system::ResMut;
use bevy::ecs::world::World;
use bevy::ui::{FlexWrap, Node, px};
use bevy_fynix::views::{
    BehaviorExt, FrameProps, button, ghost, label,
};
use bevy_fynix::{AnyView, Prop, ScopedExt, ViewExt};
use bevy_rubrix::CodeCopied;

use crate::{Build, Copied, PageView};

mod about;
mod code;
mod diff;
pub mod rich;
pub mod stream;
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
];

pub fn plugin(app: &mut App) {
    rich::plugin(app);
    virtual_list::plugin(app);
    code::plugin(app);
    stream::plugin(app);
    diff::plugin(app);
    app.add_observer(
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
