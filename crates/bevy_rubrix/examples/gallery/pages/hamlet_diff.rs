use bevy::color::{Alpha, Color, Luminance};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::world::World;
use bevy::text::{TextColor, TextSpan};
use bevy::ui::{BackgroundColor, UiRect, percent, px};
use bevy_fynix::tokens::{SpacingTokens, SurfaceTokens, Tone};
use bevy_fynix::views::{
    FrameProps, VirtualList, column, label, row, scroll,
};
use bevy_fynix::{AnyView, ViewExt};
use bevy_rubrix::{
    DiffDoc, DiffRow, DiffRowAt, DiffSource, SyntaxTokens, diff_view,
};

use super::{FitRows, descendants_in_order, find_under};
use crate::{Build, Mode, PageView};

const LIMIT: u32 = 16;

const FORTY_OLD: &str =
    include_str!("../fixtures/diff/forty-old.txt");
const FORTY_NEW: &str =
    include_str!("../fixtures/diff/forty-new.txt");

#[derive(Component)]
struct Small;

#[derive(Component)]
struct Forty;

pub fn build() -> PageView {
    column((
        label("Hamlet diff").size(20.0),
        scroll((row((
            plate(
                DiffSource::texts(
                    "letters.txt",
                    "a\nb\nc\n",
                    "a\nB\nc\nd\n",
                ),
                Small,
            ),
            plate(
                DiffSource::texts("forty.txt", FORTY_OLD, FORTY_NEW),
                Forty,
            ),
        ))
        .align(bevy::ui::AlignItems::FlexStart)
        .gap(12.0),))
        .grow(1.0),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn plate(source: DiffSource, marker: impl Component) -> PageView {
    AnyView::new(move |cx: &mut Build| {
        let theme = cx.theme();
        cx.build(
            column((column((diff_view(source)
                .compact(true)
                .limit(Some(LIMIT)),))
            .min_height(px(0.0))
            .with((FitRows, marker)),))
            .padding(UiRect::axes(px(10.0), px(8.0)))
            .grow(1.0)
            .width(px(0.0))
            .min_width(px(0.0))
            .fill(theme.fill())
            .border(1.0)
            .border_color(theme.hover())
            .radius(theme.radius()),
        )
    })
    .boxed()
}

pub fn colours_line(world: &mut World) -> String {
    let palette = world.resource::<Mode>().0;
    let Some(view) = view_under::<Small>(world) else {
        return "diff-colours none".into();
    };
    let mut rows = world
        .query::<(Entity, &DiffRowAt)>()
        .iter(world)
        .filter(|(_, at)| at.view == view)
        .map(|(row, at)| (at.index, row))
        .collect::<Vec<_>>();
    rows.sort_unstable();
    let doc = world.get::<DiffDoc>(view).expect("a diff view");
    let lines = rows
        .into_iter()
        .filter(|&(index, _)| {
            matches!(doc.row(index), Some(DiffRow::Body { .. }))
        })
        .map(|(_, row)| {
            let (text, ink) = text_of(world, row);
            let ink =
                ink.map_or("none".into(), |ink| name(&palette, ink));
            let fill = fill_of(world, row)
                .map_or("no fill".into(), |fill| {
                    format!("on {}", shade(fill))
                });
            format!("{text:?} {ink} {fill}")
        })
        .collect::<Vec<_>>();
    format!("diff-colours {}", lines.join(", "))
}

pub fn more_line(world: &mut World) -> String {
    let Some(view) = view_under::<Forty>(world) else {
        return "diff-more none".into();
    };
    let doc = world.get::<DiffDoc>(view).expect("a diff view");
    let (mut lines, mut more) = (0, None);
    for index in 0..doc.len() {
        match doc.row(index) {
            Some(DiffRow::Body { .. }) => lines += 1,
            Some(DiffRow::More { lines }) => more = Some(lines),
            _ => {}
        }
    }
    let rows = doc.len();
    let built = find_under::<VirtualList>(world, view)
        .and_then(|list| world.get::<VirtualList>(list)?.spawned())
        .map_or(0, |spawned| spawned.count());
    let more = more.map_or("none".into(), |more| more.to_string());
    format!(
        "diff-more lines {lines} more {more} rows {rows} built \
         {built}"
    )
}

fn view_under<M: Component>(world: &mut World) -> Option<Entity> {
    let fit = world
        .query_filtered::<Entity, With<M>>()
        .iter(world)
        .next()?;
    find_under::<DiffDoc>(world, fit)
}

fn text_of(world: &World, row: Entity) -> (String, Option<Color>) {
    let spans = descendants_in_order(world, row)
        .into_iter()
        .filter_map(|node| {
            let text = world.get::<TextSpan>(node)?;
            Some((text.0.as_str(), world.get::<TextColor>(node)?.0))
        })
        .collect::<Vec<_>>();
    let text = spans.iter().map(|(text, _)| *text).collect();
    (text, spans.first().map(|&(_, ink)| ink))
}

fn fill_of(world: &World, row: Entity) -> Option<Color> {
    descendants_in_order(world, row)
        .into_iter()
        .find_map(|node| {
            let fill = world.get::<BackgroundColor>(node)?.0;
            (fill.alpha() > 0.0).then_some(fill)
        })
}

fn name<T: SyntaxTokens>(theme: &T, ink: Color) -> String {
    if ink == theme.deletion() {
        "red".into()
    } else if ink == theme.addition() {
        "green".into()
    } else if ink == theme.tone(Tone::Dim) {
        "dim".into()
    } else if ink == theme.tone(Tone::Body) {
        "body".into()
    } else {
        ink.to_srgba().to_hex()
    }
}

fn shade(fill: Color) -> String {
    let srgb = fill.to_srgba();
    let hue = if srgb.red > srgb.green {
        "red"
    } else {
        "green"
    };
    let depth = if fill.luminance() < 0.25 { "deep " } else { "" };
    format!("{depth}{hue}")
}
