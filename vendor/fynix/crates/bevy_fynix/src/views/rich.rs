use std::sync::Arc;

use bevy::camera::Camera;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::{Component, Mutable};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::world::{EntityWorldMut, World};
use bevy::math::Vec2;
use bevy::text::{
    ComputedTextBlock, FontSize, FontSource, FontStyle, FontWeight,
    InlineBox, InlineBoxKind, LineHeight, Strikethrough,
    StrikethroughColor, TextBackgroundColor, TextBrush, TextColor,
    TextFont, TextLayout, TextSpan, Underline, UnderlineColor,
};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use fynix::{Patch, element};
use parley::Layout;

use super::label::PatchWrap;
use crate::patch::{Paint, PatchInk, PatchOpacity, PatchScale};
use crate::prop::Prop;
use crate::state::own_when;
use crate::tokens::{TextTokens, Tone};
use crate::transition::{blend_color, blend_f32};
use crate::visual::faded;
use crate::{Bevy, Styled};

/// Text in more than one style: a [`Text`] root with a [`TextSpan`]
/// under it for each of its spans.
///
/// The pointer lands on the span under it, and its events bubble up
/// to the root. Bevy fires a click only when the press and the
/// release land on one entity, so a click from one span to the next
/// fires none.
#[element(backend = Bevy, theme = TextTokens, prepare = prepare)]
pub struct Rich {
    /// The font of a span whose look has none. Bevy's default font
    /// when unset.
    #[elem(patch = PatchBodyFont)]
    pub font: Prop<FontSource>,
    /// The size of a span whose look has none. The theme's body
    /// size when unset.
    #[elem(
        default = theme.body_size(),
        patch = PatchBodySize,
        blend = blend_f32
    )]
    pub size: Prop<f32>,
    /// The colour of a span whose look has none, by role. Body when
    /// unset.
    #[elem(
        shown = Color,
        with = |tone, theme| theme.tone(tone),
        patch = PatchBodyInk,
        blend = blend_color
    )]
    pub tone: Prop<Tone>,
    /// Whether it breaks onto more lines. It does when unset.
    #[elem(default = true, patch = PatchWrap)]
    pub wrap: Prop<bool>,
    /// How opaque every span is, 1.0 when unset.
    #[elem(default = 1.0, patch = PatchBodyOpacity, blend = blend_f32)]
    pub opacity: Prop<f32>,
    /// The factor it is scaled by around its centre after layout,
    /// 1.0 when unset.
    #[elem(default = 1.0, patch = PatchScale, blend = blend_f32)]
    pub scale: Prop<f32>,
    #[elem(patch = PatchSpans)]
    pub spans: Prop<Spans>,
}

pub fn rich(spans: impl Into<Prop<Vec<Span>>>) -> Rich {
    Rich {
        spans: spans.into().map(Spans::from),
        ..Rich::unset()
    }
}

own_when!(Rich);

/// A run of text in a [`Rich`], and how it differs from the text
/// around it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub look: Look,
}

/// How a [`Span`] is drawn. Each `None` takes what the [`Rich`] says,
/// or Bevy's default where it says nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    /// The rich text's tone when `None`.
    pub color: Option<Color>,
    /// The rich text's font when `None`.
    pub font: Option<FontSource>,
    /// The rich text's size when `None` or NaN. Bevy lays out no
    /// text for a size of zero or less.
    pub size: Option<f32>,
    pub weight: Option<FontWeight>,
    pub style: Option<FontStyle>,
    /// A fill behind the span, the height of its line.
    pub background: Option<Color>,
    pub underline: Option<LineColor>,
    pub strikethrough: Option<LineColor>,
}

/// The colour of a line drawn under or through a span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineColor {
    /// The span's text colour.
    Ink,
    Own(Color),
}

impl Span {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            look: Look::default(),
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.look.color = Some(color);
        self
    }

    pub fn font(mut self, font: impl Into<FontSource>) -> Self {
        self.look.font = Some(font.into());
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.look.size = Some(size);
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.look.weight = Some(weight);
        self
    }

    pub fn bold(self) -> Self {
        self.weight(FontWeight::BOLD)
    }

    pub fn style(mut self, style: FontStyle) -> Self {
        self.look.style = Some(style);
        self
    }

    pub fn italic(self) -> Self {
        self.style(FontStyle::Italic)
    }

    pub fn background(mut self, color: Color) -> Self {
        self.look.background = Some(color);
        self
    }

    pub fn underline(mut self, color: LineColor) -> Self {
        self.look.underline = Some(color);
        self
    }

    pub fn strikethrough(mut self, color: LineColor) -> Self {
        self.look.strikethrough = Some(color);
        self
    }
}

/// The spans of a [`Rich`], shared because fynix copies a prop on
/// every update while its element changes.
#[derive(Clone, Debug, Default)]
pub struct Spans(Arc<[Span]>);

impl From<Vec<Span>> for Spans {
    fn from(spans: Vec<Span>) -> Self {
        Self(spans.into())
    }
}

impl PartialEq for Spans {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

/// The parley layout of the rich text `root` as Bevy last laid it
/// out, in physical pixels from the top left of its content box. Its
/// byte offsets index the text of the spans joined in order. A rich
/// text with no text lays out one line and no bytes, so every point
/// in it is byte 0.
pub fn rich_layout(
    world: &World,
    root: Entity,
) -> Option<&Layout<TextBrush>> {
    world
        .get::<ComputedTextBlock>(root)
        .map(ComputedTextBlock::buffer)
}

/// Where `at`, a point in logical pixels of the window such as a
/// pointer's, falls in the [`rich_layout`] of a text on `node` at
/// `transform`, seen through `camera`. It maps `at` the way Bevy's
/// UI picking maps a pointer. Hand the point to
/// [`parley::Cursor::from_point`] for the caret nearest it, or to
/// [`parley::Cluster::from_point`] for the character under it.
pub fn rich_point(
    node: &ComputedNode,
    transform: &UiGlobalTransform,
    camera: &Camera,
    at: Vec2,
) -> Option<Vec2> {
    let mut physical =
        at * camera.target_scaling_factor().unwrap_or(1.0);
    if let Some(viewport) = camera.physical_viewport_rect() {
        physical -= viewport.min.as_vec2();
    }
    Some(
        transform.try_inverse()?.transform_point2(physical)
            - node.content_box().min,
    )
}

#[derive(Component, Clone, Default)]
struct Body {
    font: FontSource,
    size: f32,
}

#[derive(Component)]
struct Drawn(Look);

fn prepare(world: &mut World, node: Entity) {
    world.entity_mut(node).insert((
        Text::default(),
        TextFont::default(),
        TextColor::default(),
        TextLayout::default(),
        Paint::default(),
        Body::default(),
    ));
}

fn spans_of(world: &World, root: Entity) -> Vec<Entity> {
    world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .copied()
        .filter(|&child| world.get::<Drawn>(child).is_some())
        .collect()
}

fn put<C: Component<Mutability = Mutable> + PartialEq>(
    entity: &mut EntityWorldMut,
    value: C,
) {
    match entity.get_mut::<C>() {
        Some(mut current) => {
            current.set_if_neq(value);
        }
        None => {
            entity.insert(value);
        }
    }
}

fn put_or_take<C: Component<Mutability = Mutable> + PartialEq>(
    entity: &mut EntityWorldMut,
    value: Option<C>,
) {
    match value {
        Some(value) => put(entity, value),
        None => {
            entity.remove::<C>();
        }
    }
}

fn mark<C: Component + Default>(
    entity: &mut EntityWorldMut,
    on: bool,
) {
    if on != entity.contains::<C>() {
        if on {
            entity.insert(C::default());
        } else {
            entity.remove::<C>();
        }
    }
}

fn font_of(look: &Look, body: &Body) -> TextFont {
    TextFont {
        font: look.font.clone().unwrap_or_else(|| body.font.clone()),
        font_size: FontSize::Px(
            look.size
                .filter(|size| !size.is_nan())
                .unwrap_or(body.size),
        ),
        weight: look.weight.unwrap_or(FontWeight::NORMAL),
        style: look.style.unwrap_or_default(),
        ..TextFont::default()
    }
}

fn draw(
    entity: &mut EntityWorldMut,
    look: &Look,
    body: &Body,
    paint: Paint,
) {
    let fade = |color| faded(color, paint.opacity);
    put(entity, font_of(look, body));
    put(entity, TextColor(fade(look.color.unwrap_or(paint.ink))));
    match look.background {
        Some(color) => put(entity, TextBackgroundColor(fade(color))),
        // bevy_ui_render has no
        // `RemovedComponents<TextBackgroundColor>`, so it redraws a
        // span that loses its plate only when something else on the
        // span changes.
        None => {
            if entity.take::<TextBackgroundColor>().is_some()
                && let Some(mut ink) = entity.get_mut::<TextColor>()
            {
                ink.set_changed();
            }
        }
    }
    let own = |line: Option<LineColor>| match line {
        Some(LineColor::Own(color)) => Some(fade(color)),
        Some(LineColor::Ink) | None => None,
    };
    mark::<Underline>(entity, look.underline.is_some());
    put_or_take(entity, own(look.underline).map(UnderlineColor));
    mark::<Strikethrough>(entity, look.strikethrough.is_some());
    put_or_take(
        entity,
        own(look.strikethrough).map(StrikethroughColor),
    );
}

fn inherited(world: &World, root: Entity) -> Option<(Body, Paint)> {
    Some((
        world.get::<Body>(root)?.clone(),
        *world.get::<Paint>(root)?,
    ))
}

/// Bevy lays out no text for an empty span, so a rich text with no
/// text would measure no height.
fn hold_line(world: &mut World, root: Entity) {
    let Some(size) = world.get::<Body>(root).map(|body| body.size)
    else {
        return;
    };
    let line = match world
        .get::<LineHeight>(root)
        .copied()
        .unwrap_or_default()
    {
        LineHeight::Px(px) => px,
        LineHeight::RelativeToFont(scale) => scale * size,
    };
    let spans = spans_of(world, root);
    let blank = spans.iter().all(|&span| {
        world
            .get::<TextSpan>(span)
            .is_none_or(|text| text.0.is_empty())
    });
    let Some(&first) = spans.first() else {
        return;
    };
    put_or_take(
        &mut world.entity_mut(first),
        blank.then_some(InlineBox {
            kind: InlineBoxKind::InFlow,
            size: Vec2::new(0.0, line),
        }),
    );
}

/// A rich text with no spans keeps one empty span: a text that loses
/// its last child loses `Children` with it, and Bevy lays a text out
/// again when `Children` changes but not when it is removed, so its
/// old glyphs would stay.
pub struct PatchSpans;

impl Patch<Bevy, Spans> for PatchSpans {
    fn patch(world: &mut World, node: Entity, spans: &Spans) {
        let Some((body, paint)) = inherited(world, node) else {
            return;
        };
        let none = [Span::default()];
        let spans = if spans.0.is_empty() {
            &none[..]
        } else {
            &spans.0[..]
        };
        let shown = spans_of(world, node);
        for (index, span) in spans.iter().enumerate() {
            let Some(&kid) = shown.get(index) else {
                let mut kid = world.spawn((
                    TextSpan(span.text.clone()),
                    Drawn(span.look.clone()),
                    ChildOf(node),
                ));
                draw(&mut kid, &span.look, &body, paint);
                continue;
            };
            let mut kid = world.entity_mut(kid);
            if let Some(mut text) = kid.get_mut::<TextSpan>()
                && text.0 != span.text
            {
                text.0.clone_from(&span.text);
            }
            let Some(mut drawn) = kid.get_mut::<Drawn>() else {
                continue;
            };
            if drawn.0 != span.look {
                drawn.0.clone_from(&span.look);
                draw(&mut kid, &span.look, &body, paint);
            }
        }
        for &kid in shown.get(spans.len()..).unwrap_or_default() {
            world.entity_mut(kid).despawn();
        }
        hold_line(world, node);
    }
}

fn redraw(world: &mut World, root: Entity) {
    let Some((body, paint)) = inherited(world, root) else {
        return;
    };
    for kid in spans_of(world, root) {
        let mut kid = world.entity_mut(kid);
        if let Some(look) =
            kid.get::<Drawn>().map(|drawn| drawn.0.clone())
        {
            draw(&mut kid, &look, &body, paint);
        }
    }
}

fn rebody(
    world: &mut World,
    root: Entity,
    edit: impl FnOnce(&mut Body),
) {
    let Some(mut body) = world.get_mut::<Body>(root) else {
        return;
    };
    edit(&mut body);
    let body = body.clone();
    if let Some(mut font) = world.get_mut::<TextFont>(root) {
        font.font = body.font;
        font.font_size = FontSize::Px(body.size);
    }
    redraw(world, root);
    hold_line(world, root);
}

pub struct PatchBodyInk;

impl Patch<Bevy, Color> for PatchBodyInk {
    fn patch(world: &mut World, node: Entity, ink: &Color) {
        PatchInk::patch(world, node, ink);
        redraw(world, node);
    }
}

pub struct PatchBodyOpacity;

impl Patch<Bevy, f32> for PatchBodyOpacity {
    fn patch(world: &mut World, node: Entity, opacity: &f32) {
        PatchOpacity::patch(world, node, opacity);
        redraw(world, node);
    }
}

pub struct PatchBodyFont;

impl Patch<Bevy, FontSource> for PatchBodyFont {
    fn patch(world: &mut World, node: Entity, font: &FontSource) {
        rebody(world, node, |body| body.font.clone_from(font));
    }
}

pub struct PatchBodySize;

impl Patch<Bevy, f32> for PatchBodySize {
    fn patch(world: &mut World, node: Entity, size: &f32) {
        rebody(world, node, |body| body.size = *size);
    }
}
