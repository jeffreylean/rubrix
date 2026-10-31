use core::ops::Range;

use bevy::color::{Alpha, Color, Luminance};
use bevy::ecs::change_detection::Mut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::{With, Without};
use bevy::ecs::system::{Commands, Query};
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::text::{
    ComputedTextBlock, FontSize, TextColor, TextFont, TextLayout,
};
use bevy::ui::widget::Text;
use bevy::ui::{
    BackgroundColor, ComputedNode, Display, Node, PositionType,
    UiGlobalTransform, UiRect, ZIndex, px,
};
use bevy_fynix::views::{VirtualList, drawn_layout};
use rubrix_edit::{Assoc, Buffer, Change};

use crate::caret::{CARET_WIDTH, Geometry, RowParts};
use crate::editor::{EditDoc, EditLine, EditView};
use crate::hooks::Follows;
use crate::keymap::content;
use crate::marks::Marks;
use crate::rows::{LINE_HEIGHT, Shown, inserts};
use crate::select;

/// Other users' carets in an [`EditDoc`]'s text, on the doc's entity,
/// in byte offsets. The app fills it; every view of the doc draws
/// each as a bar at its head with its label above it, and a ranged
/// one also tints its range. A caret whose head is outside a view's
/// viewport shows as a marker at the view's top or bottom edge
/// instead. Nothing the user does moves one, but each follows the
/// doc's changes, and text typed exactly at a caret pushes it right.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RemoteCarets(pub Vec<RemoteCaret>);

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteCaret {
    pub anchor: usize,
    pub head: usize,
    pub color: Color,
    pub label: String,
}

impl Follows for RemoteCarets {
    fn follow(&mut self, change: &Change) {
        for caret in &mut self.0 {
            caret.anchor = change.map(caret.anchor, Assoc::After);
            caret.head = change.map(caret.head, Assoc::After);
        }
    }
}

/// How a view shows one remote caret.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placed {
    /// A bar, in logical window pixels as of this frame's layout.
    Bar(Rect),
    /// A marker at the view's top or bottom edge.
    Edge(Edge),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
}

/// On an editable view's root once its doc has [`RemoteCarets`]: how
/// it showed each of them this frame.
#[derive(Component)]
pub struct RemoteView {
    list: Entity,
    layer: Entity,
    edges: Entity,
    rects: Vec<Entity>,
    tags: Vec<Entity>,
    placed: Vec<Placed>,
}

impl RemoteView {
    /// One per caret, in [`RemoteCarets`] order.
    pub fn placed(&self) -> &[Placed] {
        &self.placed
    }
}

#[derive(Component)]
pub(crate) struct RemoteRect;

#[derive(Component)]
pub(crate) struct RemoteTag;

#[derive(Component)]
pub(crate) struct RemoteEdges;

const LABEL_SIZE_PX: f32 = 11.0;
const LABEL_PAD_PX: f32 = 3.0;
const TINT: f32 = 0.32;

type TextParts = (
    &'static ComputedNode,
    &'static UiGlobalTransform,
    &'static ComputedTextBlock,
    &'static TextLayout,
);

type Rects<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut BackgroundColor),
    (With<RemoteRect>, Without<Text>),
>;

type Tags<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Node,
        &'static mut BackgroundColor,
        &'static mut Text,
        &'static mut TextColor,
        &'static ChildOf,
    ),
    With<RemoteTag>,
>;

#[derive(Default)]
struct Scene {
    rects: Vec<([f32; 4], Color)>,
    tags: Vec<(Vec2, String, Color, TagIn)>,
}

#[derive(Clone, Copy, PartialEq)]
enum TagIn {
    List,
    Edges,
}

#[expect(
    clippy::too_many_arguments,
    reason = "one drawer over disjoint queries"
)]
pub(crate) fn draw(
    parts: RowParts,
    lines: Query<(Entity, &EditLine)>,
    texts: Query<TextParts, With<EditLine>>,
    mut views: Query<(
        Entity,
        &EditView,
        &Marks,
        Option<&mut RemoteView>,
    )>,
    docs: Query<(&EditDoc, Option<&RemoteCarets>)>,
    lists: Query<(&VirtualList, &ComputedNode, &UiGlobalTransform)>,
    overlays: Query<
        (&ComputedNode, &UiGlobalTransform),
        With<RemoteEdges>,
    >,
    mut rects: Rects,
    mut tags: Tags,
    mut commands: Commands,
) {
    for (view, edit, marks, remote) in &mut views {
        let (Ok((doc, carets)), Some(list)) =
            (docs.get(edit.doc()), edit.list())
        else {
            continue;
        };
        let carets = carets.map_or(&[][..], |carets| &carets.0[..]);
        let mut remote = match remote {
            Some(remote) if remote.list == list => remote,
            stale => {
                if let Some(stale) = stale {
                    commands.entity(stale.layer).try_despawn();
                    commands.entity(stale.edges).try_despawn();
                    if carets.is_empty() {
                        commands.entity(view).remove::<RemoteView>();
                    }
                }
                if !carets.is_empty() {
                    attach(&mut commands, view, list);
                }
                continue;
            }
        };
        let Ok((state, area, area_at)) = lists.get(list) else {
            continue;
        };
        let mut built = crate::caret::Built::default();
        built.collect(view, &lines);
        let geometry = Geometry::new(
            &parts,
            state,
            list,
            &built,
            Some(marks),
            edit.drawn.metrics,
        );
        let mut rows: Vec<(u32, Entity)> = lines
            .iter()
            .filter(|(_, line)| line.view == view)
            .map(|(text, line)| (line.line, text))
            .collect();
        rows.sort_unstable_by_key(|&(line, _)| line);
        let frame =
            Frame::of(area, area_at, overlays.get(remote.edges).ok());
        let buffer = doc.editor().buffer();
        let row = |line: u32| {
            let i = rows.partition_point(|&(at, _)| at < line);
            let &(at, text) = rows.get(i)?;
            (at == line).then_some(text)
        };
        let origin = |text: Entity| {
            texts.get(text).ok().map(|(node, at, ..)| {
                frame.content_from_physical(
                    at.translation + node.content_box().min,
                )
            })
        };
        let any_origin =
            rows.iter().find_map(|&(_, text)| origin(text));
        let mut scene = Scene::default();
        let mut placed = Vec::with_capacity(carets.len());
        let mut free = [0.0f32; 2];
        for caret in carets {
            let head = caret.head.min(buffer.len());
            let line = buffer.line_of(head);
            let shape = geometry.caret(buffer, head);
            let bar = row(line).and_then(origin).map(|at| {
                [
                    at.x + shape.x,
                    at.y + shape.top,
                    CARET_WIDTH,
                    shape.height,
                ]
            });
            match bar.filter(|bar| frame.shows(bar[1] + bar[3] / 2.0))
            {
                Some(bar) => {
                    scene.rects.push((bar, caret.color));
                    let height = label_height();
                    let above = bar[1] - height;
                    let top = if above < frame.scroll_logical.y {
                        bar[1] + bar[3]
                    } else {
                        above
                    };
                    scene.tags.push((
                        Vec2::new(bar[0], top),
                        caret.label.clone(),
                        caret.color,
                        TagIn::List,
                    ));
                    placed.push(Placed::Bar(
                        frame.window_from_content(bar),
                    ));
                }
                None => {
                    let edge = match bar {
                        Some(bar)
                            if bar[1] >= frame.scroll_logical.y =>
                        {
                            Edge::Bottom
                        }
                        Some(_) => Edge::Top,
                        None if line
                            < state.first_visible() as u32 =>
                        {
                            Edge::Top
                        }
                        None => Edge::Bottom,
                    };
                    let text = match edge {
                        Edge::Top => format!("↑ {}", caret.label),
                        Edge::Bottom => format!("↓ {}", caret.label),
                    };
                    let x = any_origin.map_or(0.0, |at| {
                        at.x + shape.x - frame.scroll_logical.x
                    });
                    let slot = &mut free[edge as usize];
                    let left = x.max(*slot).min(
                        frame.overlay_logical.x
                            - monospace_label_width(&text),
                    );
                    *slot = left
                        + monospace_label_width(&text)
                        + LABEL_PAD_PX;
                    let top = match edge {
                        Edge::Top => frame.list_top_logical,
                        Edge::Bottom => {
                            frame.list_top_logical
                                + frame.viewport_logical.y
                                - label_height()
                        }
                    };
                    scene.tags.push((
                        Vec2::new(left.max(0.0), top),
                        text,
                        caret.color,
                        TagIn::Edges,
                    ));
                    placed.push(Placed::Edge(edge));
                }
            }
            if caret.anchor != caret.head {
                let range = caret
                    .anchor
                    .min(caret.head)
                    .min(buffer.len())
                    ..caret.anchor.max(caret.head).min(buffer.len());
                tint(
                    &mut scene,
                    &texts,
                    buffer,
                    marks,
                    &rows,
                    &frame,
                    range,
                    caret
                        .color
                        .with_alpha(caret.color.alpha() * TINT),
                );
            }
        }
        drop_covered_by_later(&mut scene.rects, |over, under| {
            over.0 == under.0 && over.1.alpha() >= 1.0
        });
        drop_covered_by_later(&mut scene.tags, |over, under| {
            over.0 == under.0
                && over.3 == under.3
                && over.1.chars().count() >= under.1.chars().count()
                && over.2.alpha() >= 1.0
        });
        let remote = &mut *remote;
        remote.placed = placed;
        paint(remote, &scene, &mut rects, &mut tags, &mut commands);
    }
}

fn attach(commands: &mut Commands, view: Entity, list: Entity) {
    let layer = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                ..Node::default()
            },
            ZIndex(1),
            Pickable::IGNORE,
            ChildOf(list),
        ))
        .id();
    let edges = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                right: px(0.0),
                top: px(0.0),
                bottom: px(0.0),
                overflow: bevy::ui::Overflow::clip(),
                ..Node::default()
            },
            ZIndex(2),
            Pickable::IGNORE,
            RemoteEdges,
            ChildOf(view),
        ))
        .id();
    commands.entity(view).insert(RemoteView {
        list,
        layer,
        edges,
        rects: Vec::new(),
        tags: Vec::new(),
        placed: Vec::new(),
    });
}

struct Frame {
    min_physical: Vec2,
    logical: f32,
    scroll_logical: Vec2,
    viewport_logical: Vec2,
    overlay_logical: Vec2,
    list_top_logical: f32,
}

impl Frame {
    fn of(
        node: &ComputedNode,
        at: &UiGlobalTransform,
        overlay: Option<(&ComputedNode, &UiGlobalTransform)>,
    ) -> Frame {
        let logical = node.inverse_scale_factor();
        let content = node.content_box();
        let min = at.translation + content.min;
        let (edges, list_top) = overlay.map_or(
            (Vec2::ZERO, 0.0),
            |(overlay, overlay_at)| {
                let origin =
                    overlay_at.translation + overlay.border_box().min;
                (
                    overlay.size() * logical,
                    (min.y - origin.y) * logical,
                )
            },
        );
        Frame {
            min_physical: min,
            logical,
            scroll_logical: node.scroll_position * logical,
            viewport_logical: (content.size() - node.scrollbar_size)
                * logical,
            overlay_logical: edges,
            list_top_logical: list_top,
        }
    }

    fn content_from_physical(&self, physical: Vec2) -> Vec2 {
        (physical - self.min_physical) * self.logical
            + self.scroll_logical
    }

    fn shows(&self, y: f32) -> bool {
        (self.scroll_logical.y
            ..=self.scroll_logical.y + self.viewport_logical.y)
            .contains(&y)
    }

    fn window_from_content(
        &self,
        [left, top, width, height]: [f32; 4],
    ) -> Rect {
        let min = self.min_physical * self.logical
            - self.scroll_logical
            + Vec2::new(left, top);
        Rect::from_corners(min, min + Vec2::new(width, height))
    }
}

#[expect(clippy::too_many_arguments, reason = "one row walk")]
fn tint(
    scene: &mut Scene,
    texts: &Query<TextParts, With<EditLine>>,
    buffer: &Buffer,
    marks: &Marks,
    rows: &[(u32, Entity)],
    frame: &Frame,
    range: Range<usize>,
    color: Color,
) {
    let (first, last) =
        (buffer.line_of(range.start), buffer.line_of(range.end));
    let from = rows.partition_point(|&(line, _)| line < first);
    for &(line, text) in
        rows[from..].iter().take_while(|&&(line, _)| line <= last)
    {
        let Ok((node, at, block, layout)) = texts.get(text) else {
            continue;
        };
        let start = buffer.line_start(line);
        let line_text = content(buffer, line);
        let bytes = range.start.max(start) - start
            ..(range.end - start).min(line_text.len());
        if bytes.start >= bytes.end {
            continue;
        }
        let shown = Shown::with(line_text, &inserts(marks.on(line)));
        let drawn = shown.shown(bytes.start)..shown.shown(bytes.end);
        let origin = frame.content_from_physical(
            at.translation + node.content_box().min,
        );
        let parley = drawn_layout(block, layout, node);
        let logical = node.inverse_scale_factor();
        scene.rects.extend(
            select::rects(&parley, drawn, logical, color).map(
                |([x0, y0, x1, y1], color)| {
                    (
                        [
                            origin.x + x0,
                            origin.y + y0,
                            x1 - x0,
                            y1 - y0,
                        ],
                        color,
                    )
                },
            ),
        );
    }
}

fn drop_covered_by_later<T>(
    items: &mut Vec<T>,
    later_covers: impl Fn(&T, &T) -> bool,
) {
    let mut kept: Vec<T> = Vec::with_capacity(items.len());
    for item in items.drain(..).rev() {
        if !kept.iter().any(|later| later_covers(later, &item)) {
            kept.push(item);
        }
    }
    kept.reverse();
    *items = kept;
}

fn label_height() -> f32 {
    (LABEL_SIZE_PX * LINE_HEIGHT).ceil()
}

fn monospace_label_width(text: &str) -> f32 {
    text.chars().count() as f32 * LABEL_SIZE_PX * crate::rows::ADVANCE
        + 2.0 * LABEL_PAD_PX
}

fn ink(background: Color) -> Color {
    if background.luminance() > 0.5 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

fn paint(
    remote: &mut RemoteView,
    scene: &Scene,
    rects: &mut Rects,
    tags: &mut Tags,
    commands: &mut Commands,
) {
    for (i, &([left, top, width, height], color)) in
        scene.rects.iter().enumerate()
    {
        let placed = Node {
            position_type: PositionType::Absolute,
            left: px(left),
            top: px(top),
            width: px(width),
            height: px(height),
            ..Node::default()
        };
        match remote
            .rects
            .get(i)
            .and_then(|&kid| rects.get_mut(kid).ok())
        {
            Some((mut node, mut background)) => {
                if *node != placed {
                    *node = placed;
                }
                if background.0 != color {
                    background.0 = color;
                }
            }
            None => remote.rects.push(
                commands
                    .spawn((
                        placed,
                        BackgroundColor(color),
                        Pickable::IGNORE,
                        RemoteRect,
                        ChildOf(remote.layer),
                    ))
                    .id(),
            ),
        }
    }
    for &kid in remote.rects.iter().skip(scene.rects.len()) {
        if let Ok((node, _)) = rects.get_mut(kid) {
            hide(node);
        }
    }
    for (i, (at, label, color, tag_in)) in
        scene.tags.iter().enumerate()
    {
        let placed = Node {
            position_type: PositionType::Absolute,
            left: px(at.x),
            top: px(at.y),
            padding: UiRect::horizontal(px(LABEL_PAD_PX)),
            ..Node::default()
        };
        let parent = match tag_in {
            TagIn::List => remote.layer,
            TagIn::Edges => remote.edges,
        };
        match remote
            .tags
            .get(i)
            .and_then(|&kid| tags.get_mut(kid).ok())
        {
            Some((
                mut node,
                mut background,
                mut text,
                mut ink_color,
                at,
            )) => {
                if *node != placed {
                    *node = placed;
                }
                if background.0 != *color {
                    background.0 = *color;
                    ink_color.0 = ink(*color);
                }
                if text.0 != *label {
                    text.0.clone_from(label);
                }
                if at.parent() != parent {
                    commands
                        .entity(remote.tags[i])
                        .insert(ChildOf(parent));
                }
            }
            None => remote.tags.push(
                commands
                    .spawn((
                        placed,
                        Text::new(label.clone()),
                        TextFont {
                            font: crate::CODE_FONT.into(),
                            font_size: FontSize::Px(LABEL_SIZE_PX),
                            ..TextFont::default()
                        },
                        TextColor(ink(*color)),
                        TextLayout::no_wrap(),
                        BackgroundColor(*color),
                        Pickable::IGNORE,
                        RemoteTag,
                        ChildOf(parent),
                    ))
                    .id(),
            ),
        }
    }
    for &kid in remote.tags.iter().skip(scene.tags.len()) {
        if let Ok((node, ..)) = tags.get_mut(kid) {
            hide(node);
        }
    }
}

fn hide(mut node: Mut<Node>) {
    if node.display != Display::None {
        node.display = Display::None;
    }
}
