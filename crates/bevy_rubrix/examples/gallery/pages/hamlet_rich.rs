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

pub const HEADING_SIZE: f32 = 20.0;

pub const CODE_SIZE: f32 = 13.0;

const STREAMED: &str = "Second *grows*";

const STREAM_FROM: usize = "Sec".len();

const FRAMES_PER_BYTE: u32 = 4;

#[derive(Component)]
pub struct Sample;

#[derive(Component)]
pub struct Streamed;

#[derive(Resource, Default)]
pub struct Second(Option<Started>);

struct Started {
    shown: usize,
    sample: Vec<Entity>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Second>().add_systems(Update, grow);
}

fn grow(mut second: ResMut<Second>, mut frames: Local<u32>) {
    let Some(started) = &second.0 else {
        return;
    };
    if started.shown == STREAMED.len() {
        return;
    }
    *frames += 1;
    if (*frames).is_multiple_of(FRAMES_PER_BYTE)
        && let Some(started) = &mut second.0
    {
        started.shown += 1;
    }
}

pub fn stream(world: &mut World) {
    let sample = spans_in::<Sample>(world);
    world.resource_mut::<Second>().0 = Some(Started {
        shown: STREAM_FROM,
        sample,
    });
}

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
                let shown = second.0.as_ref().map_or(0, |s| s.shown);
                second_spans(&STREAMED[..shown])
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

pub fn streamed<T: SyntaxTokens + SurfaceTokens>(
    world: &mut World,
    theme: &T,
) -> String {
    let spans = spans_in::<Streamed>(world)
        .into_iter()
        .map(|span| describe(world, span, theme))
        .collect::<Vec<_>>();
    let now = spans_in::<Sample>(world);
    let sample = match &world.resource::<Second>().0 {
        None => "not streamed",
        Some(started) if started.sample == now => "kept",
        Some(_) => "changed",
    };
    format!("rich-stream {} sample {sample}", spans.join(" | "))
}

pub fn sample_ids(world: &mut World) -> Vec<String> {
    spans_in::<Sample>(world)
        .into_iter()
        .map(|span| span.to_string())
        .collect()
}

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
