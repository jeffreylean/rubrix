use std::path::PathBuf;

use bevy::ecs::world::World;

use crate::PageView;

mod about;

pub struct Page {
    pub name: &'static str,
    pub build: fn() -> PageView,
    pub open: Option<fn(&mut World, &[PathBuf])>,
}

pub const PAGES: &[Page] = &[Page {
    name: "about",
    build: about::build,
    open: None,
}];
