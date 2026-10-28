use bevy::app::{App, First, PostUpdate};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ResMut;
use bevy::ecs::world::World;
use bevy::text::{FontSize, FontSource, TextFont};
use bevy::ui::{
    ComputedNode, Node, OverflowAxis, UiGlobalTransform, UiSystems,
    percent, px,
};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{FrameProps, VirtualList, column, label};
use bevy_fynix::{AnyView, ViewExt, keyed, resource};
use bevy_rubrix::{
    CODE_FONT, CodeDoc, CodeLine, CodeSource, CodeStream, Lang,
    Overflow, code_view,
};

use super::hamlet_rich::CODE_SIZE;
use super::{FitRows, blank_rows, control, find_under, plated};
use crate::{Build, PageView};

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

#[derive(Resource, Default)]
struct Fence(Option<Sending>);

#[derive(Resource, Default)]
struct Growth {
    frames: u32,
    shrinks: u32,
    empty: u32,
    blank: u32,
    first: f32,
    last: f32,
    first_rows: usize,
    rows: usize,
}

#[derive(Component)]
struct SamplePlate;

#[derive(Component)]
struct FencePlate;

#[derive(Component)]
struct Caption;

pub fn plugin(app: &mut App) {
    app.init_resource::<Fence>()
        .init_resource::<Growth>()
        .add_systems(First, send)
        .add_systems(
            PostUpdate,
            measure.after(UiSystems::PostLayout),
        );
}

struct Sending {
    stream: CodeStream,
    sent: usize,
}

impl Sending {
    fn send_next(&mut self) {
        let end = (self.sent + BYTES_PER_FRAME).min(FENCE.len());
        self.stream.push(&FENCE[self.sent..end]);
        self.sent = end;
    }
}

fn send(mut fence: ResMut<Fence>) {
    if let Some(sending) = &mut fence.bypass_change_detection().0 {
        sending.send_next();
    }
}

fn start(world: &mut World) {
    let mut sending = Sending {
        stream: CodeSource::streaming(Lang::Rust),
        sent: 0,
    };
    sending.send_next();
    world.resource_mut::<Fence>().0 = Some(sending);
    *world.resource_mut::<Growth>() = Growth::default();
}

fn measure(world: &mut World) {
    if world.resource::<Fence>().0.is_none() {
        return;
    }
    let Some(plate) = world
        .query_filtered::<Entity, With<FencePlate>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let Some(node) = world.get::<ComputedNode>(plate) else {
        return;
    };
    let height = node.size().y * node.inverse_scale_factor();
    let doc = find_under::<CodeDoc>(world, plate)
        .and_then(|view| world.get::<CodeDoc>(view));
    let empty = doc.is_none_or(|doc| doc.text().is_empty());
    let rows = doc.map_or(0, CodeDoc::rows);
    let blank = find_under::<VirtualList>(world, plate)
        .map_or(0, |list| blank_rows(world, list));
    let mut growth = world.resource_mut::<Growth>();
    if growth.frames == 0 {
        growth.first = height;
        growth.first_rows = rows;
    } else if height < growth.last {
        growth.shrinks += 1;
    }
    if empty || height < 1.0 {
        growth.empty += 1;
    }
    if blank > 0 {
        growth.blank += 1;
    }
    growth.last = height;
    growth.rows = rows;
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
                    .0
                    .as_ref()
                    .map(|sending| sending.stream.source())
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

fn plate(
    lang: &'static str,
    source: CodeSource,
    marker: impl Component,
) -> PageView {
    AnyView::new(move |cx: &mut Build| {
        let theme = cx.theme();
        cx.build(plated(
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
            .min_width(px(0.0))
            .shrink(0.0),
            theme,
        ))
    })
    .boxed()
}

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

pub fn growth_line(world: &mut World) -> String {
    let growth = world.resource::<Growth>();
    let grows =
        growth.last > growth.first && growth.rows > growth.first_rows;
    format!(
        "code-grow frames {} from {} to {} rows {} to {} grows {} \
         shrinks {} empty {}",
        growth.frames,
        growth.first,
        growth.last,
        growth.first_rows,
        growth.rows,
        if grows { "yes" } else { "no" },
        growth.shrinks,
        growth.empty
    )
}

pub fn blank_line(world: &mut World) -> String {
    let growth = world.resource::<Growth>();
    format!(
        "code-blank frames {} blank {}",
        growth.frames, growth.blank
    )
}
