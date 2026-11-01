use std::path::PathBuf;

use bevy::app::App;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::ui::{percent, px};
use bevy::ui_widgets::Button;
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{
    BehaviorExt, FrameProps, column, label, row,
};
use bevy_fynix::{ViewExt, resource};
use bevy_rubrix::{
    BracketMatch, DiffDoc, DiffSource, EditDoc, Layout, Overflow,
    diff_view,
};

use super::code::read_file;
use super::editor::one_based_line_col;
use super::{Ask, control, wrapping};
use crate::PageView;

#[path = "../fixtures/diff/large_pairs.rs"]
mod large_pairs;

use large_pairs::{hundred_k_pair, with_review_edits};

const LARGE_PATCH: &str =
    include_str!("../../../../../bench/patch/large.patch.txt");
pub(super) const STRESS_PATCH: &str =
    include_str!("../../../../../bench/patch/stress.patch.txt");
const LARGE_RS: &str =
    include_str!("../../../../../bench/fixtures/rust/large.rs.txt");

macro_rules! fixture_pair {
    ($stem:literal, $ext:literal) => {
        (
            concat!($stem, $ext),
            include_str!(concat!(
                "../fixtures/diff/",
                $stem,
                "-old",
                $ext
            )),
            include_str!(concat!(
                "../fixtures/diff/",
                $stem,
                "-new",
                $ext
            )),
        )
    };
}

const FIXTURE_PAIRS: [(&str, &str, &str); 8] = [
    fixture_pair!("merge", ".rs"),
    fixture_pair!("retype", ".rs"),
    fixture_pair!("forty", ".txt"),
    fixture_pair!("gap", ".txt"),
    fixture_pair!("long", ".rs"),
    fixture_pair!("realign", ".rs"),
    fixture_pair!("words", ".rs"),
    fixture_pair!("wrap", ".rs"),
];

const COMPACT_LIMIT: u32 = 16;

#[derive(Resource)]
struct DiffFile {
    name: String,
    source: DiffSource,
    layout: Layout,
    compact: bool,
    overflow: Overflow,
    edit: bool,
}

impl Default for DiffFile {
    fn default() -> Self {
        Self {
            name: "large.patch.txt".into(),
            source: patch(LARGE_PATCH),
            layout: Layout::Stacked,
            compact: false,
            overflow: Overflow::Scroll,
            edit: false,
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<DiffFile>();
}

/// The `log caret` answer: the selections of the file whose new side
/// holds the caret, and the bracket that matches the one at its
/// primary, as the editor page prints its own, without `visible`.
pub fn answer(world: &mut World, ask: Ask) -> Option<String> {
    if ask != Ask::Caret {
        return None;
    }
    let doc = world
        .query::<&DiffDoc>()
        .iter(world)
        .find_map(DiffDoc::edit_doc)?;
    let editor = world.get::<EditDoc>(doc)?.editor();
    let buffer = editor.buffer();
    let selections = editor.selections();
    let each = selections
        .iter()
        .map(|s| {
            format!(
                "{}..{}",
                one_based_line_col(buffer, s.start()),
                one_based_line_col(buffer, s.end())
            )
        })
        .collect::<Vec<_>>();
    let bracket = world
        .get::<BracketMatch>(doc)
        .and_then(BracketMatch::brackets)
        .map_or_else(
            || "none".into(),
            |pair| one_based_line_col(buffer, pair.far),
        );
    Some(format!(
        "{} primary {} {} bracket {bracket}",
        selections.len(),
        selections.primary_index() + 1,
        each.join(" "),
    ))
}

pub(super) fn patch(text: &str) -> DiffSource {
    DiffSource::patch(text).expect("a bundled patch parses")
}

pub fn build() -> PageView {
    column((
        row((
            label("Diff").size(20.0),
            source_link("pairs", |world| {
                let edited = with_review_edits(LARGE_RS);
                let pairs = [("large.rs", LARGE_RS, edited.as_str())]
                    .into_iter()
                    .chain(FIXTURE_PAIRS);
                show(
                    world,
                    "large.rs and the fixture pairs",
                    DiffSource::pairs(pairs),
                );
            }),
            source_link("100k pair", |world| {
                let (old, new) = hundred_k_pair(LARGE_RS);
                let source =
                    DiffSource::texts("large.rs", &old, &new);
                show(world, "large.rs.txt ten times, edited", source);
            }),
        ))
        .gap(16.0),
        controls(),
        label(resource::<DiffFile, _>(|file| {
            let files = file.source.files().len();
            let noun = if files == 1 { "file" } else { "files" };
            format!("{}, {files} {noun}", file.name)
        }))
        .tone(Tone::Dim),
        diff_view(resource::<DiffFile, _>(|file| {
            file.source.clone()
        }))
        .layout(resource::<DiffFile, _>(|file| file.layout))
        .compact(resource::<DiffFile, _>(|file| file.compact))
        .limit(resource::<DiffFile, _>(|file| {
            file.compact.then_some(COMPACT_LIMIT)
        }))
        .overflow(resource::<DiffFile, _>(|file| file.overflow))
        .edit(resource::<DiffFile, _>(|file| file.edit)),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn controls() -> PageView {
    let buttons = row((
        control("large.patch", |world| {
            show(world, "large.patch.txt", patch(LARGE_PATCH));
        }),
        control("100k patch", |world| {
            show(world, "stress.patch.txt", patch(STRESS_PATCH));
        }),
        control("pair", |world| {
            let source = DiffSource::texts(
                "large.rs",
                LARGE_RS,
                &with_review_edits(LARGE_RS),
            );
            show(world, "large.rs.txt, edited", source);
        }),
        control(
            resource::<DiffFile, _>(|file| match file.layout {
                Layout::Stacked => "stacked".to_owned(),
                Layout::Split => "split".to_owned(),
            }),
            |world| {
                let mut file = world.resource_mut::<DiffFile>();
                file.layout = match file.layout {
                    Layout::Stacked => Layout::Split,
                    Layout::Split => Layout::Stacked,
                };
            },
        ),
        control(
            resource::<DiffFile, _>(|file| match file.overflow {
                Overflow::Scroll => "wrap off".to_owned(),
                Overflow::Wrap => "wrap on".to_owned(),
            }),
            |world| {
                let mut file = world.resource_mut::<DiffFile>();
                file.overflow = match file.overflow {
                    Overflow::Scroll => Overflow::Wrap,
                    Overflow::Wrap => Overflow::Scroll,
                };
            },
        ),
        control(
            resource::<DiffFile, _>(|file| {
                if file.compact {
                    "compact on"
                } else {
                    "compact off"
                }
                .to_owned()
            }),
            |world| {
                let mut file = world.resource_mut::<DiffFile>();
                file.compact = !file.compact;
            },
        ),
        control(
            resource::<DiffFile, _>(|file| {
                if file.edit { "edit on" } else { "edit off" }
                    .to_owned()
            }),
            |world| {
                let mut file = world.resource_mut::<DiffFile>();
                file.edit = !file.edit;
            },
        ),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

fn source_link(
    text: &'static str,
    press: impl Fn(&mut World) + Send + Sync + 'static,
) -> PageView {
    label(text).on_activate(press).tagged(Button).boxed()
}

fn show(world: &mut World, name: &str, source: DiffSource) {
    let mut file = world.resource_mut::<DiffFile>();
    file.name = name.into();
    file.source = source;
}

pub fn open(
    world: &mut World,
    paths: &[PathBuf],
) -> Result<(), String> {
    match paths {
        [path] => {
            let (name, text) = read_file(path)?;
            let source =
                DiffSource::patch(&text).map_err(|error| {
                    format!(
                        "cannot parse {}: {error:?}",
                        path.display()
                    )
                })?;
            show(world, &name, source);
        }
        [old, new] => {
            let (old_name, old) = read_file(old)?;
            let (new_name, new) = read_file(new)?;
            let source = DiffSource::texts(&new_name, &old, &new);
            show(world, &format!("{old_name} → {new_name}"), source);
        }
        _ => {
            return Err(format!(
                "the diff page opens a patch or two files, not {}",
                paths.len()
            ));
        }
    }
    Ok(())
}
