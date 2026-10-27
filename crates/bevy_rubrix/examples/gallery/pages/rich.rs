use bevy::app::{App, Update};
use bevy::camera::Camera;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::EntityEvent;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, ResMut};
use bevy::ecs::world::World;
use bevy::math::Vec2;
use bevy::picking::events::PointerPress;
use bevy::text::FontSource;
use bevy::ui::{
    ComputedNode, ComputedUiTargetCamera, FlexWrap, Node,
    UiGlobalTransform, percent, px,
};
use bevy_fynix::tokens::{SurfaceTokens, TextTokens, Tone};
use bevy_fynix::views::{
    FrameProps, LineColor, Span, column, label, parley, rich,
    rich_layout, rich_point, row, scroll,
};
use bevy_fynix::{AnyView, ViewExt, keyed, resource};

use super::control;
use crate::{Build, PageView, Palette};

const LONG_SPANS: usize = 200;
const ROWS: usize = 65;
const ROW_SPANS: usize = 10;

#[derive(Resource)]
struct LongLine {
    count: usize,
    turn: u32,
    animate: bool,
}

impl Default for LongLine {
    fn default() -> Self {
        Self {
            count: LONG_SPANS,
            turn: 0,
            animate: false,
        }
    }
}

#[derive(Resource)]
struct Toggles {
    plate: bool,
    rows: bool,
}

impl Default for Toggles {
    fn default() -> Self {
        Self {
            plate: true,
            rows: false,
        }
    }
}

#[derive(Resource, Default)]
pub struct Hit {
    pub byte: Option<usize>,
}

#[derive(Component)]
pub struct LongLineRoot;

#[derive(Component)]
pub struct HitLineRoot;

pub fn plugin(app: &mut App) {
    app.init_resource::<LongLine>()
        .init_resource::<Toggles>()
        .init_resource::<Hit>()
        .add_systems(Update, turn);
}

fn turn(mut line: ResMut<LongLine>) {
    if line.animate {
        line.turn += 1;
    }
}

pub fn build() -> PageView {
    column((
        label("Rich text").size(20.0),
        controls(),
        scroll((
            styles(),
            unicode(),
            wrapped(),
            hit_line(),
            long_line(),
            rows(),
        ))
        .gap(12.0)
        .grow(1.0),
    ))
    .height(percent(100.0))
    .min_height(px(0.0))
    .gap(8.0)
    .boxed()
}

fn controls() -> PageView {
    let buttons = row((
        control("recolour", |world| {
            world.resource_mut::<LongLine>().turn += 1;
        }),
        control(
            resource::<LongLine, _>(|line| {
                let on = if line.animate { "on" } else { "off" };
                format!("animate {on}")
            }),
            |world| {
                let mut line = world.resource_mut::<LongLine>();
                line.animate = !line.animate;
            },
        ),
        control("0 spans", |world| {
            world.resource_mut::<LongLine>().count = 0;
        }),
        control("5 spans", |world| {
            world.resource_mut::<LongLine>().count = 5;
        }),
        control("200 spans", |world| {
            world.resource_mut::<LongLine>().count = LONG_SPANS;
        }),
        control(
            // The label stays put, so taking the plate off changes
            // nothing else on screen that would redraw the text.
            "plate",
            |world| {
                let mut toggles = world.resource_mut::<Toggles>();
                toggles.plate = !toggles.plate;
            },
        ),
        control(
            resource::<Toggles, _>(|toggles| {
                format!(
                    "rows {}",
                    if toggles.rows { "on" } else { "off" }
                )
            }),
            |world| {
                let mut toggles = world.resource_mut::<Toggles>();
                toggles.rows = !toggles.rows;
            },
        ),
    ))
    .gap(8.0);
    // fynix's `Frame` has no wrap prop, so this sets the row's
    // `Node`.
    AnyView::new(|cx: &mut Build| {
        let root = cx.build(buttons);
        cx.world
            .get_mut::<Node>(root)
            .expect("a row node")
            .flex_wrap = FlexWrap::Wrap;
        root
    })
    .boxed()
}

fn hue(palette: Palette, degrees: f32) -> Color {
    let lightness = if palette.dark { 0.72 } else { 0.38 };
    Color::hsl(degrees % 360.0, 0.75, lightness)
}

fn sans(text: &str) -> Span {
    Span::new(text).font(FontSource::sans_serif())
}

fn style_spans(palette: Palette, plate: bool) -> Vec<Span> {
    let accent = palette.tone(Tone::Accent);
    let critical = palette.tone(Tone::Critical);
    let plated = Span::new(" plate ");
    vec![
        sans("plain "),
        sans("bold").bold(),
        sans(" "),
        sans("italic").italic(),
        sans(" "),
        Span::new("mono"),
        sans(" "),
        sans("larger").size(22.0),
        sans(" "),
        sans("underlined").underline(LineColor::Own(accent)),
        sans(" "),
        sans("struck").strikethrough(LineColor::Own(critical)),
        sans(" "),
        if plate {
            plated.background(palette.fill())
        } else {
            plated
        },
        sans(" "),
        sans("accent").color(accent),
    ]
}

fn styles() -> PageView {
    AnyView::new(|cx: &mut Build| {
        let palette = *cx.theme();
        cx.build(rich(resource::<Toggles, _>(move |toggles| {
            style_spans(palette, toggles.plate)
        })))
    })
    .boxed()
}

fn unicode() -> PageView {
    rich(vec![
        Span::new("café déjà naïve "),
        Span::new("日本語のテキスト "),
        Span::new("😀"),
        Span::new(" after the emoji"),
    ])
    .boxed()
}

fn wrapped() -> PageView {
    AnyView::new(|cx: &mut Build| {
        let palette = *cx.theme();
        cx.build(rich(vec![
            sans(
                "This paragraph is wider than the page, so it \
                 breaks at word boundaries. Every span keeps its \
                 own style on each line it lands on: ",
            ),
            sans(
                "this bold run starts near the end of one line and \
                 goes on past the break onto the next, ",
            )
            .bold(),
            sans(
                "this coloured run starts a few words later and \
                 runs on past the next break, so its colour has to \
                 carry over onto the following line as well, ",
            )
            .color(hue(palette, 200.0)),
            sans("and this italic run ends the paragraph.").italic(),
        ]))
    })
    .boxed()
}

const HIT_TEXT: [&str; 3] = ["click: ", "héllo wörld ", "日本 x"];

fn hit_line() -> PageView {
    AnyView::new(|cx: &mut Build| {
        let spans = HIT_TEXT
            .iter()
            .map(|&text| Span::new(text))
            .collect::<Vec<_>>();
        let root = cx.build(rich(spans));
        cx.world
            .entity_mut(root)
            .insert(HitLineRoot)
            .observe(remember_hit);
        root
    })
    .boxed()
}

fn remember_hit(press: On<PointerPress>, mut commands: Commands) {
    let root = press.event_target();
    let at = press.pointer.position;
    commands.queue(move |world: &mut World| {
        let offset = character_at(world, root, at);
        world.resource_mut::<Hit>().byte = offset;
    });
}

fn character_at(
    world: &World,
    root: Entity,
    at: Vec2,
) -> Option<usize> {
    let camera = world.get::<ComputedUiTargetCamera>(root)?.get()?;
    let point = rich_point(
        world.get::<ComputedNode>(root)?,
        world.get::<UiGlobalTransform>(root)?,
        world.get::<Camera>(camera)?,
        at,
    )?;
    let layout = rich_layout(world, root)?;
    let (cluster, _) =
        parley::Cluster::from_point_exact(layout, point.x, point.y)?;
    Some(cluster.text_range().start)
}

fn long_spans(line: &LongLine, palette: Palette) -> Vec<Span> {
    (0..line.count)
        .map(|index| {
            let degrees = (index as u32 * 37 + line.turn * 23) as f32;
            Span::new(format!("s{index:03} "))
                .color(hue(palette, degrees))
        })
        .collect()
}

fn long_line() -> PageView {
    AnyView::new(|cx: &mut Build| {
        let palette = *cx.theme();
        let spans = resource::<LongLine, _>(move |line| {
            long_spans(line, palette)
        });
        let root = cx.build(rich(spans));
        cx.world.entity_mut(root).insert(LongLineRoot);
        root
    })
    .boxed()
}

fn row_spans(index: usize, palette: Palette) -> Vec<Span> {
    let mut spans = vec![Span::new(format!("{index:02} "))];
    spans.extend((1..ROW_SPANS).map(|column| {
        let degrees = (column * 41 + index * 7) as f32;
        Span::new(format!("tok{column} "))
            .color(hue(palette, degrees))
    }));
    spans
}

fn rows() -> PageView {
    keyed(resource::<Toggles, _>(|toggles| toggles.rows), |shown| {
        if !*shown {
            return column(()).boxed();
        }
        AnyView::new(|cx: &mut Build| {
            let palette = *cx.theme();
            let rows = (0..ROWS)
                .map(|index| {
                    rich(row_spans(index, palette))
                        .wrap(false)
                        .boxed()
                })
                .collect::<Vec<_>>();
            cx.build(column(rows).gap(0.0))
        })
    })
    .boxed()
}
