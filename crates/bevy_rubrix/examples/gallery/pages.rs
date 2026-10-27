use std::path::PathBuf;

use bevy::app::App;
use bevy::ecs::world::World;
use bevy::ui::px;
use bevy_fynix::views::{
    BehaviorExt, FrameProps, button, ghost, label,
};
use bevy_fynix::{Prop, ScopedExt, ViewExt};

use crate::PageView;

mod about;
pub mod rich;
pub mod virtual_list;

const BUTTON_WIDTH: f32 = 140.0;

pub struct Page {
    pub name: &'static str,
    pub build: fn() -> PageView,
    pub open: Option<fn(&mut World, &[PathBuf])>,
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
];

pub fn plugin(app: &mut App) {
    rich::plugin(app);
    virtual_list::plugin(app);
}

/// A button of a page's controls, labelled `text`.
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
