//! Hamlet's `rich` call site, `rich` at
//! `crates/hamlet_ui/src/markdown/draw.rs:197` in Hamlet 3b8cc93a,
//! rebuilt with fynix's rich text: each markdown block one rich
//! element, with a span for each run of one style. Nothing here uses
//! the gallery's own items, so `tests/hamlet.rs` builds the same
//! page.

use bevy::app::{App, Update};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Local, ResMut};
use bevy::ecs::world::World;
use bevy::text::{
    FontSize, FontSource, FontStyle, FontWeight, Strikethrough,
    TextBackgroundColor, TextColor, TextFont, TextSpan, Underline,
};
use bevy::ui::{percent, px};
use bevy_fynix::tokens::{SpacingTokens, SurfaceTokens, Tone};
use bevy_fynix::views::{
    FrameProps, LineColor, Span, column, label, rich,
};
use bevy_fynix::{AnyView, Bevy, Cx, ViewExt, resource};
use bevy_rubrix::SyntaxTokens;

/// Hamlet's heading size in this sample.
pub const HEADING_SIZE: f32 = 20.0;

/// Inline code, one pixel under the 14 px body, as Hamlet sets it.
pub const CODE_SIZE: f32 = 13.0;

const STREAMED: &str = "Second *grows*";

/// `Sec`, the first text the second block shows.
const STREAM_FROM: usize = 3;

const FRAMES_PER_BYTE: u32 = 4;

/// The column of the sample's blocks, `# Plan` and its paragraph.
#[derive(Component)]
pub struct Sample;

/// The column of the second block, the one that streams.
#[derive(Component)]
pub struct Streamed;

/// How much of the second block has arrived.
#[derive(Resource, Default)]
pub struct Second {
    /// Bytes of [`STREAMED`] shown, 0 before the stream starts.
    shown: usize,
    /// The sample's span entities when the stream started.
    sample: Vec<Entity>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Second>().add_systems(Update, grow);
}

fn grow(mut second: ResMut<Second>, mut frames: Local<u32>) {
    if second.shown == 0 || second.shown == STREAMED.len() {
        return;
    }
    *frames += 1;
    if (*frames).is_multiple_of(FRAMES_PER_BYTE) {
        second.shown += 1;
    }
}

/// Starts the second block again from `Sec`, and notes the sample's
/// spans so that [`streamed`] can tell whether they were kept.
pub fn stream(world: &mut World) {
    let sample = spans_in::<Sample>(world);
    let mut second = world.resource_mut::<Second>();
    second.shown = STREAM_FROM;
    second.sample = sample;
}

/// The page, under `controls`. Its bounds are what the spans need:
/// the plate and the accent from the theme, and the code font.
pub fn view<T>(controls: AnyView<Bevy, T>) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SurfaceTokens + SpacingTokens + Send + Sync,
    T: 'static,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let theme = cx.theme();
        let sample = column((
            rich(vec![sans("Plan").size(HEADING_SIZE).bold()]),
            rich(paragraph(theme)),
        ))
        .gap(8.0)
        .with(Sample);
        let second =
            column((rich(resource::<Second, _>(|second| {
                second_spans(&STREAMED[..second.shown])
            })),))
            .with(Streamed);
        cx.build(
            column((
                label("Hamlet rich").size(20.0),
                controls,
                column((sample, second)).gap(8.0),
            ))
            .height(percent(100.0))
            .min_height(px(0.0))
            .gap(8.0),
        )
    })
    .boxed()
}

fn sans(text: &str) -> Span {
    Span::new(text).font(FontSource::sans_serif())
}

/// ``Use **care** with `rm`, ~~old~~ and [docs](http://x).``
fn paragraph<T: SyntaxTokens + SurfaceTokens>(
    theme: &T,
) -> Vec<Span> {
    vec![
        sans("Use "),
        sans("care").bold(),
        sans(" with "),
        Span::new("rm")
            .font(theme.code_font())
            .size(CODE_SIZE)
            .background(theme.fill()),
        sans(", "),
        sans("old").strikethrough(LineColor::Ink),
        sans(" and "),
        sans("docs")
            .color(theme.tone(Tone::Accent))
            .underline(LineColor::Ink),
        sans("."),
    ]
}

/// The spans of `text`, markdown whose `*` marks open and close
/// italics. An italic run still open shows in italics so far.
fn second_spans(text: &str) -> Vec<Span> {
    text.split('*')
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
        .map(|(at, part)| {
            let span = sans(part);
            if at % 2 == 1 { span.italic() } else { span }
        })
        .collect()
}

/// The span entities of the blocks in the column marked `M`, in
/// order.
pub fn spans_in<M: Component>(world: &mut World) -> Vec<Entity> {
    let Some(column) =
        world.query_filtered::<Entity, With<M>>().iter(world).next()
    else {
        return Vec::new();
    };
    let children = |node| {
        world
            .get::<Children>(node)
            .map(|kids| kids.to_vec())
            .unwrap_or_default()
    };
    children(column)
        .into_iter()
        .flat_map(children)
        .filter(|&span| world.get::<TextSpan>(span).is_some())
        .collect()
}

/// The `rich-styles` log line: every span of the sample, as
/// [`describe`] puts it, `|` between them.
pub fn styles<T: SyntaxTokens + SurfaceTokens>(
    world: &mut World,
    theme: &T,
) -> String {
    let spans = spans_in::<Sample>(world)
        .into_iter()
        .map(|span| describe(world, span, theme))
        .collect::<Vec<_>>();
    format!("rich-styles {}", spans.join(" | "))
}

/// The `rich-stream` log line: the second block's spans, then
/// whether the sample still has the span entities it had when the
/// stream started.
pub fn streamed<T: SyntaxTokens + SurfaceTokens>(
    world: &mut World,
    theme: &T,
) -> String {
    let spans = spans_in::<Streamed>(world)
        .into_iter()
        .map(|span| describe(world, span, theme))
        .collect::<Vec<_>>();
    let now = spans_in::<Sample>(world);
    let started = &world.resource::<Second>().sample;
    let sample = if started.is_empty() {
        "not streamed"
    } else if *started == now {
        "kept"
    } else {
        "changed"
    };
    format!("rich-stream {} sample {sample}", spans.join(" | "))
}

/// The ids of the sample's span entities, for the `entities` topic.
pub fn sample_ids(world: &mut World) -> Vec<String> {
    spans_in::<Sample>(world)
        .into_iter()
        .map(|span| span.to_string())
        .collect()
}

/// A span as `"<text>" <size>`, then each of `bold`, `italic`, its
/// font (`mono`, `sans` or `default`), its ink (`accent`, `dim` or a
/// hex colour when not the body's), `plate` for the theme's fill
/// behind it, `underline` and `struck`.
fn describe<T: SyntaxTokens + SurfaceTokens>(
    world: &World,
    span: Entity,
    theme: &T,
) -> String {
    let entity = world.entity(span);
    let text = entity.get::<TextSpan>().map_or("", |text| &text.0);
    let mut words = vec![format!("{text:?}")];
    if let Some(font) = entity.get::<TextFont>() {
        if let FontSize::Px(size) = font.font_size {
            words.push(size.to_string());
        }
        if font.weight == FontWeight::BOLD {
            words.push("bold".into());
        }
        if font.style == FontStyle::Italic {
            words.push("italic".into());
        }
        words.push(
            if font.font == theme.code_font() {
                "mono"
            } else if font.font == FontSource::sans_serif() {
                "sans"
            } else {
                "default"
            }
            .into(),
        );
    }
    if let Some(&TextColor(ink)) = entity.get::<TextColor>() {
        let named = |tone| theme.tone(tone) == ink;
        if named(Tone::Accent) {
            words.push("accent".into());
        } else if named(Tone::Dim) {
            words.push("dim".into());
        } else if !named(Tone::Body) {
            words.push(hex(ink));
        }
    }
    if let Some(&TextBackgroundColor(plate)) =
        entity.get::<TextBackgroundColor>()
    {
        words.push(if plate == theme.fill() {
            "plate".into()
        } else {
            format!("on {}", hex(plate))
        });
    }
    if entity.contains::<Underline>() {
        words.push("underline".into());
    }
    if entity.contains::<Strikethrough>() {
        words.push("struck".into());
    }
    words.join(" ")
}

fn hex(color: Color) -> String {
    color.to_srgba().to_hex()
}
