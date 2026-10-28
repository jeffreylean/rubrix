use std::fs;
use std::path::PathBuf;

use bevy::app::App;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::ui::{percent, px};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{FrameProps, column, label, row};
use bevy_fynix::{ViewExt, resource};
use bevy_rubrix::{
    CodeSource, Kind, Lang, Overflow, SyntaxStyle, SyntaxTokens,
    code_view, default_style,
};

use super::{control, wrapping};
use crate::{PageView, Palette};

pub(super) const LARGE_RS: &str =
    include_str!("../../../../../bench/fixtures/rust/large.rs.txt");
const LARGE_TS: &str =
    include_str!("../../../../../bench/fixtures/ts/large.ts.txt");

const STRESS_LINES: usize = 100_000;

#[derive(Resource)]
struct CodeFile {
    name: String,
    source: CodeSource,
    overflow: Overflow,
}

impl Default for CodeFile {
    fn default() -> Self {
        Self {
            name: "large.rs.txt".into(),
            source: large_rs(),
            overflow: Overflow::Scroll,
        }
    }
}

fn large_rs() -> CodeSource {
    CodeSource::new(Lang::Rust, LARGE_RS)
}

pub fn plugin(app: &mut App) {
    app.init_resource::<CodeFile>();
}

impl SyntaxTokens for Palette {
    fn style(&self, kind: Kind) -> SyntaxStyle {
        let color =
            |dark, light| SyntaxStyle::color(self.pick(dark, light));
        match kind {
            Kind::Keyword => color(0xBB9AF7, 0x7847BD),
            Kind::String => color(0x9ECE6A, 0x48752A),
            Kind::Comment => color(0x737AA2, 0x8A8DA0),
            Kind::Number => color(0xFF9E64, 0xB15C00),
            Kind::Function => color(0x7AA2F7, 0x2F5FC4),
            Kind::Type => color(0x2AC3DE, 0x006F8F),
            Kind::Constant => color(0xE0AF68, 0x8F5E15),
            Kind::Attribute => color(0x73DACA, 0x2E6E65),
            _ => default_style(self, kind),
        }
    }
}

pub fn build() -> PageView {
    column((
        label("Code").size(20.0),
        controls(),
        label(resource::<CodeFile, _>(|file| {
            let rows = file.source.rows();
            let lines = if rows == 1 { "line" } else { "lines" };
            format!("{}, {rows} {lines}", file.name)
        }))
        .tone(Tone::Dim),
        code_view(resource::<CodeFile, _>(|file| {
            file.source.clone()
        }))
        .overflow(resource::<CodeFile, _>(|file| file.overflow)),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn controls() -> PageView {
    let buttons = row((
        control("large.rs", |world| {
            show(world, "large.rs.txt", large_rs());
        }),
        control("large.ts", |world| {
            let source = CodeSource::new(Lang::TypeScript, LARGE_TS);
            show(world, "large.ts.txt", source);
        }),
        control("100k lines", |world| {
            let source = CodeSource::new(Lang::Rust, stress());
            show(world, "large.rs.txt repeated", source);
        }),
        control(
            resource::<CodeFile, _>(|file| match file.overflow {
                Overflow::Scroll => "wrap off".to_owned(),
                Overflow::Wrap => "wrap on".to_owned(),
            }),
            |world| {
                let mut file = world.resource_mut::<CodeFile>();
                file.overflow = match file.overflow {
                    Overflow::Scroll => Overflow::Wrap,
                    Overflow::Wrap => Overflow::Scroll,
                };
            },
        ),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

fn show(world: &mut World, name: &str, source: CodeSource) {
    let mut file = world.resource_mut::<CodeFile>();
    file.name = name.into();
    file.source = source;
}

pub(super) fn stress() -> String {
    let mut text = String::with_capacity(LARGE_RS.len() * 10);
    for line in LARGE_RS.lines().cycle().take(STRESS_LINES) {
        text.push_str(line);
        text.push('\n');
    }
    text
}

pub fn open(
    world: &mut World,
    paths: &[PathBuf],
) -> Result<(), String> {
    let (name, text) = read("code", paths)?;
    let source = CodeSource::detect(&name, text);
    show(world, &name, source);
    Ok(())
}

pub(super) fn read(
    page: &str,
    paths: &[PathBuf],
) -> Result<(String, String), String> {
    let [path] = paths else {
        return Err(format!(
            "the {page} page opens one file, not {}",
            paths.len()
        ));
    };
    let bytes = fs::read(path).map_err(|error| {
        format!("cannot read {}: {error}", path.display())
    })?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if text.len() >= u32::MAX as usize {
        return Err(format!("{} is 4 GiB or more", path.display()));
    }
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok((name, text))
}
