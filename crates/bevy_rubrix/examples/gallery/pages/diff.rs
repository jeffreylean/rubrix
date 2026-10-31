use std::path::PathBuf;

use bevy::app::App;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::ui::{percent, px};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{
    BehaviorExt, FrameProps, button, column, label, row, tint,
};
use bevy_fynix::{ScopedExt, ViewExt, resource};
use bevy_rubrix::{DiffSource, Layout, Overflow, diff_view};

use super::code::read_file;
use super::{control, wrapping};
use crate::PageView;

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
                let (old, new) = hundred_k_pair();
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
    button(label(text)).rules(tint).on_activate(press).boxed()
}

fn show(world: &mut World, name: &str, source: DiffSource) {
    let mut file = world.resource_mut::<DiffFile>();
    file.name = name.into();
    file.source = source;
}

fn with_review_edits(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (at, line) in text.lines().enumerate() {
        match at {
            119 => out.push_str(&line.replace("theme", "palette")),
            2000..2006 => continue,
            5000 => {
                out.push_str("// Added by the pair's edit.\n");
                out.push_str("const PAIRED: bool = true;\n");
                out.push_str(line);
            }
            8000 => out.push_str(&format!("{line} // changed")),
            _ => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

fn hundred_k_pair() -> (String, String) {
    let old = LARGE_RS.repeat(10);
    let mut new = String::with_capacity(old.len() + old.len() / 400);
    for (at, line) in old.split_inclusive('\n').enumerate() {
        if at % 100 == 0 {
            new.push_str(line.strip_suffix('\n').unwrap_or(line));
            new.push_str(" // edit\n");
        } else {
            new.push_str(line);
        }
    }
    (old, new)
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
