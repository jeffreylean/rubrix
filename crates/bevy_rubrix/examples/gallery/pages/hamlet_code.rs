//! Hamlet's `code_view` call site, `code_view` at
//! `crates/hamlet_ui/src/markdown/draw.rs:289` in Hamlet 3b8cc93a,
//! rebuilt with rubrix's code view: a fenced block on a bordered
//! plate under its language, in 13 px mono that wraps, since nothing
//! in a chat scrolls sideways.

use bevy::app::{App, First, PostUpdate};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::text::{FontSize, FontSource, TextFont};
use bevy::ui::{
    ComputedNode, Node, OverflowAxis, UiGlobalTransform, UiRect,
    UiSystems, percent, px,
};
use bevy_fynix::tokens::{SpacingTokens, SurfaceTokens, Tone};
use bevy_fynix::views::{FrameProps, VirtualList, column, label};
use bevy_fynix::{AnyView, ViewExt, keyed, resource};
use bevy_rubrix::{
    CODE_FONT, CodeDoc, CodeLine, CodeSource, CodeStream, Lang,
    Overflow, code_view,
};

use super::{FitRows, control, find_under};
use crate::{Build, PageView};

/// Hamlet's size for code, under the 14 px body.
const CODE_SIZE: f32 = 13.0;

const CAPTION_SIZE: f32 = 11.0;

const SAMPLE: &str = r#"fn main() {
    let words = ["care", "rm", "old", "docs"];
    println!("{} words, and this line runs on past the edge of the plate so that it has to wrap, because a chat never scrolls sideways: {words:?}", words.len());
}"#;

const FENCE: &str = r#"use std::collections::HashMap;

/// Counts each word of `text`.
fn count(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let counts = count("the plate grows as the lines arrive");
    for (word, n) in &counts {
        println!("{word}: {n}");
    }
}
"#;

const BYTES_PER_FRAME: usize = 3;

/// The fenced block that streams in, still open, after the button.
#[derive(Resource, Default)]
struct Fence {
    stream: Option<(CodeStream, usize)>,
}

/// How the streamed plate's height went, frame by frame, since the
/// stream started.
#[derive(Resource, Default)]
struct Growth {
    frames: u32,
    shrinks: u32,
    empty: u32,
    first: f32,
    last: f32,
}

/// The plate of the sample block.
#[derive(Component)]
struct SamplePlate;

/// The plate of the streamed block.
#[derive(Component)]
struct FencePlate;

#[derive(Component)]
struct Caption;

pub fn plugin(app: &mut App) {
    app.init_resource::<Fence>()
        .init_resource::<Growth>()
        .add_systems(First, send)
        .add_systems(PostUpdate, measure.after(UiSystems::Layout));
}

fn send(mut fence: ResMut<Fence>) {
    let Some((stream, sent)) = &mut fence.stream else {
        return;
    };
    if *sent == FENCE.len() {
        return;
    }
    let end = (*sent + BYTES_PER_FRAME).min(FENCE.len());
    stream.push(&FENCE[*sent..end]);
    *sent = end;
}

/// Starts the fenced block again. The first bytes go with the press,
/// so the plate never shows without a line.
fn start(world: &mut World) {
    let stream = CodeSource::streaming(Lang::Rust);
    stream.push(&FENCE[..BYTES_PER_FRAME]);
    world.resource_mut::<Fence>().stream =
        Some((stream, BYTES_PER_FRAME));
    *world.resource_mut::<Growth>() = Growth::default();
}

fn measure(
    fence: Res<Fence>,
    mut growth: ResMut<Growth>,
    plates: Query<(Entity, &ComputedNode), With<FencePlate>>,
    children: Query<&Children>,
    docs: Query<&CodeDoc>,
) {
    if fence.stream.is_none() {
        return;
    }
    let Ok((plate, node)) = plates.single() else {
        return;
    };
    let height = node.size().y * node.inverse_scale_factor();
    let empty = children
        .iter_descendants(plate)
        .find_map(|node| docs.get(node).ok())
        .is_none_or(|doc| doc.text().is_empty());
    let growth = &mut *growth;
    if growth.frames == 0 {
        growth.first = height;
    } else if height < growth.last {
        growth.shrinks += 1;
    }
    if empty || height < 1.0 {
        growth.empty += 1;
    }
    growth.last = height;
    growth.frames += 1;
}

pub fn build() -> PageView {
    column((
        label("Hamlet code").size(20.0),
        control("stream", start),
        plate(
            "rust",
            CodeSource::new(Lang::Rust, SAMPLE),
            SamplePlate,
        ),
        keyed(
            resource::<Fence, _>(|fence| {
                fence
                    .stream
                    .as_ref()
                    .map(|(stream, _)| stream.source())
            }),
            |source| match source {
                Some(source) => {
                    plate("rust", source.clone(), FencePlate)
                }
                None => column(()).boxed(),
            },
        )
        .within(column(()).min_width(px(0.0))),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

/// `source` on a plate under its `lang`, as Hamlet draws a fenced
/// block: 10 by 8 px in, a 1 px border, the caption 4 px above the
/// code.
fn plate(
    lang: &'static str,
    source: CodeSource,
    marker: impl Component,
) -> PageView {
    AnyView::new(move |cx: &mut Build| {
        let theme = cx.theme();
        cx.build(
            column((
                column((label(lang)
                    .size(CAPTION_SIZE)
                    .tone(Tone::Dim),))
                .with(Caption),
                column((code_view(source)
                    .size(CODE_SIZE)
                    .overflow(Overflow::Wrap),))
                .min_height(px(0.0))
                .with((FitRows, marker)),
            ))
            .gap(4.0)
            .padding(UiRect::axes(px(10.0), px(8.0)))
            .min_width(px(0.0))
            .shrink(0.0)
            .fill(theme.fill())
            .border(1.0)
            .border_color(theme.hover())
            .radius(theme.radius()),
        )
    })
    .boxed()
}

/// The `code-plate` log line, read off the sample plate: where the
/// caption sits, the code's size and font, whether the view scrolls
/// sideways, its rows, and the most lines one row wraps onto.
pub fn plate_line(world: &mut World) -> String {
    let Some(code) = world
        .query_filtered::<Entity, With<SamplePlate>>()
        .iter(world)
        .next()
    else {
        return "code-plate none".into();
    };
    let caption = world
        .query_filtered::<Entity, With<Caption>>()
        .iter(world)
        .next();
    let edges = |node: Entity| {
        let size = world.get::<ComputedNode>(node)?.size();
        let y = world.get::<UiGlobalTransform>(node)?.translation.y;
        Some((y - size.y / 2.0, y + size.y / 2.0))
    };
    let caption = match caption.and_then(edges).zip(edges(code)) {
        Some(((_, bottom), (top, _))) if bottom <= top => "above",
        Some(_) => "below",
        None => "unplaced",
    };
    let font = find_under::<CodeLine>(world, code)
        .and_then(|line| world.get::<TextFont>(line));
    let text = font.map_or_else(
        || "none".to_owned(),
        |font| {
            let size = match font.font_size {
                FontSize::Px(size) => size.to_string(),
                other => format!("{other:?}"),
            };
            let mono = font.font == FontSource::Handle(CODE_FONT);
            format!(
                "{size} {}",
                if mono { "mono" } else { "not mono" }
            )
        },
    );
    let Some(list) = find_under::<VirtualList>(world, code) else {
        return format!(
            "code-plate caption {caption} text {text} no list"
        );
    };
    let sideways =
        match world.get::<Node>(list).map(|node| node.overflow.x) {
            Some(OverflowAxis::Scroll) => "on",
            _ => "off",
        };
    let list = world.get::<VirtualList>(list).expect("a list");
    let tallest = (0..list.count())
        .filter_map(|row| list.row_span(row))
        .map(|span| span.height)
        .fold(0.0, f32::max);
    let line = (CODE_SIZE * 1.2).ceil();
    format!(
        "code-plate caption {caption} text {text} sideways \
         {sideways} rows {} tallest {} lines",
        list.count(),
        (tallest / line).round()
    )
}

/// The `code-grow` log line: how many frames the streamed plate was
/// measured, its height then and now, how often it shrank, and how
/// often it showed no line.
pub fn growth_line(world: &mut World) -> String {
    let growth = world.resource::<Growth>();
    format!(
        "code-grow frames {} from {} to {} shrinks {} empty {}",
        growth.frames,
        growth.first,
        growth.last,
        growth.shrinks,
        growth.empty
    )
}
