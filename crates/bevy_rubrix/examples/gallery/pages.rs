use std::path::PathBuf;

use bevy::app::App;
use bevy::ecs::world::World;

use crate::PageView;

mod about;
pub mod rich;

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
];

pub fn plugin(app: &mut App) {
    rich::plugin(app);
}
