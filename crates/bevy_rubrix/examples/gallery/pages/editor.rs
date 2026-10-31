use std::mem;
use std::path::PathBuf;

use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::{FromWorld, World};
use bevy::ui::{percent, px};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{FrameProps, column, label, row};
use bevy_fynix::{AnyView, Signal, ViewExt, derived, resource};
use bevy_rubrix::{EditDoc, EditView, Lang, Overflow, code_editor};

use super::code::{
    LARGE_RS, LARGE_TS, flip, read, stress, wrap_name,
};
use super::{Ask, control, wrapping};
use crate::{Build, PageView};

/// The text the editor page shows, on an entity of its own so a theme
/// toggle that builds the page again keeps it. The page makes it the
/// first time it is built.
#[derive(Resource)]
pub struct EditorFile {
    name: String,
    doc: Entity,
    overflow: Overflow,
}

impl FromWorld for EditorFile {
    fn from_world(world: &mut World) -> Self {
        let doc = world.spawn(large_rs()).id();
        Self {
            name: "large.rs.txt".into(),
            doc,
            overflow: Overflow::Scroll,
        }
    }
}

fn large_rs() -> EditDoc {
    EditDoc::new(Lang::Rust, LARGE_RS)
}

pub fn build() -> PageView {
    AnyView::new(|cx: &mut Build| {
        cx.world.init_resource::<EditorFile>();
        cx.build(
            column((
                label("Editor").size(20.0),
                controls(),
                label(size_label()).tone(Tone::Dim),
                code_editor(resource::<EditorFile, _>(|file| {
                    file.doc
                }))
                .overflow(
                    resource::<EditorFile, _>(|file| file.overflow),
                ),
            ))
            .height(percent(100.0))
            .min_height(px(0.0))
            .gap(8.0),
        )
    })
    .boxed()
}

fn size_label() -> Signal<String> {
    let rows = |world: &World| {
        let file = world.resource::<EditorFile>();
        let rows =
            world.get::<EditDoc>(file.doc).map_or(0, EditDoc::rows);
        (file.doc, rows)
    };
    let mut shown = None;
    derived(move |world| {
        let rows = rows(world).1;
        let lines = if rows == 1 { "line" } else { "lines" };
        format!(
            "{}, {rows} {lines}",
            world.resource::<EditorFile>().name
        )
    })
    .when(move |world| {
        let now = Some(rows(world));
        mem::replace(&mut shown, now) != now
    })
}

fn controls() -> PageView {
    let buttons = row((
        control("large.rs", |world| {
            show(world, "large.rs.txt", large_rs());
        }),
        control("large.ts", |world| {
            let doc = EditDoc::new(Lang::TypeScript, LARGE_TS);
            show(world, "large.ts.txt", doc);
        }),
        control("100k lines", |world| {
            let doc = EditDoc::new(Lang::Rust, &stress());
            show(world, "large.rs.txt repeated", doc);
        }),
        control(
            resource::<EditorFile, _>(|file| {
                wrap_name(file.overflow)
            }),
            |world| {
                flip(&mut world.resource_mut::<EditorFile>().overflow)
            },
        ),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

/// Spawns `doc` in place of the doc on show and despawns that one.
/// The view's doc prop changes, so it builds its rows again at line
/// 1.
fn show(world: &mut World, name: &str, doc: EditDoc) {
    let doc = world.spawn(doc).id();
    let mut file = world.resource_mut::<EditorFile>();
    file.name = name.into();
    let old = mem::replace(&mut file.doc, doc);
    world.despawn(old);
}

pub fn open(
    world: &mut World,
    paths: &[PathBuf],
) -> Result<(), String> {
    let (name, text) = read("editor", paths)?;
    let doc = EditDoc::detect(&name, &text);
    world.init_resource::<EditorFile>();
    show(world, &name, doc);
    Ok(())
}

/// The page's answers. Lines and columns count from 1, and a column
/// counts chars from the line's start.
pub fn answer(world: &mut World, ask: Ask) -> Option<String> {
    let doc = world.get_resource::<EditorFile>()?.doc;
    let shown = world
        .query::<&EditView>()
        .iter(world)
        .find(|view| view.doc() == doc)
        .is_some_and(EditView::caret_shown);
    let edit = world.get::<EditDoc>(doc)?;
    let editor = edit.editor();
    let buffer = editor.buffer();
    let place = |at: usize| {
        let line = buffer.line_of(at);
        let column =
            buffer.slice(buffer.line_start(line)..at).chars().count();
        format!("{}:{}", line + 1, column + 1)
    };
    Some(match ask {
        Ask::Caret => {
            let selections = editor.selections();
            let each = selections
                .iter()
                .map(|s| {
                    format!("{}..{}", place(s.anchor), place(s.head))
                })
                .collect::<Vec<_>>();
            format!(
                "{} primary {} {} visible {shown}",
                selections.len(),
                selections.primary_index() + 1,
                each.join(" "),
            )
        }
        Ask::Text(line) => match edit.line(line.get() - 1) {
            Some(text) => format!("{text:?}"),
            None => "none".into(),
        },
        Ask::Undo => format!(
            "{} redo {}",
            editor.history().undo_depth(),
            editor.history().redo_depth()
        ),
        Ask::Ime => return None,
    })
}
