//! Other users' carets on an editable view: drawn, never moved by
//! this user.

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
    /// In the list, so it scrolls with the rows: bars, labels,
    /// tints.
    layer: Entity,
    /// Over the view: edge markers.
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

/// A bar or a tint.
#[derive(Component)]
pub(crate) struct RemoteRect;

/// A label or an edge marker.
#[derive(Component)]
pub(crate) struct RemoteTag;

#[derive(Component)]
pub(crate) struct RemoteEdges;

/// Logical px.
const LABEL_SIZE: f32 = 11.0;
const LABEL_PAD: f32 = 3.0;
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

/// What one frame draws in a view: rects and tags in order, each
/// reusing the pooled node at its index.
#[derive(Default)]
struct Scene {
    /// Content px of the list: left, top, width, height.
    rects: Vec<([f32; 4], Color)>,
    /// In the list's content px, or the edge overlay's px.
    tags: Vec<(Vec2, String, Color, bool)>,
}

/// Draws every editable view's remote carets from this frame's
/// layout, after `UiSystems::PostLayout`. Pieces lay out in the next
/// layout, as the selection rects do: the bars, labels and tints sit
/// in the list's content, so a scroll moves them with the rows in the
/// same layout.
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
        let Some(mut remote) =
            remote.filter(|remote| remote.list == list)
        else {
            if !carets.is_empty() {
                attach(&mut commands, view, list);
            }
            continue;
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
                frame.content(at.translation + node.content_box().min)
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
                    let top = if above < frame.scroll.y {
                        bar[1] + bar[3]
                    } else {
                        above
                    };
                    scene.tags.push((
                        Vec2::new(bar[0], top),
                        caret.label.clone(),
                        caret.color,
                        false,
                    ));
                    placed.push(Placed::Bar(frame.window(bar)));
                }
                None => {
                    let edge = match bar {
                        Some(bar) if bar[1] >= frame.scroll.y => {
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
                        at.x + shape.x - frame.scroll.x
                    });
                    let slot = &mut free[edge as usize];
                    let left = x
                        .max(*slot)
                        .min(frame.edges.x - label_width(&text));
                    *slot = left + label_width(&text) + LABEL_PAD;
                    let top = match edge {
                        Edge::Top => frame.list_top,
                        Edge::Bottom => {
                            frame.list_top + frame.size.y
                                - label_height()
                        }
                    };
                    scene.tags.push((
                        Vec2::new(left.max(0.0), top),
                        text,
                        caret.color,
                        true,
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
        let remote = &mut *remote;
        remote.placed = placed;
        paint(remote, &scene, &mut rects, &mut tags, &mut commands);
    }
}

/// Spawns a view's layer in `list` and its edge overlay, and the
/// [`RemoteView`] that names them.
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

/// A list's place in the window and its edge overlay's, this frame.
struct Frame {
    /// Physical px of the list's content box's top left.
    min: Vec2,
    logical: f32,
    /// Logical px the list is scrolled by.
    scroll: Vec2,
    /// The viewport's logical size, scrollbars left out.
    size: Vec2,
    /// The overlay's logical size, and the list's top in it.
    edges: Vec2,
    list_top: f32,
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
            min,
            logical,
            scroll: node.scroll_position * logical,
            size: (content.size() - node.scrollbar_size) * logical,
            edges,
            list_top,
        }
    }

    /// Physical window px to the list's content px.
    fn content(&self, physical: Vec2) -> Vec2 {
        (physical - self.min) * self.logical + self.scroll
    }

    fn shows(&self, y: f32) -> bool {
        (self.scroll.y..=self.scroll.y + self.size.y).contains(&y)
    }

    /// A rect in content px, in logical window px.
    fn window(&self, [left, top, width, height]: [f32; 4]) -> Rect {
        let min = self.min * self.logical - self.scroll
            + Vec2::new(left, top);
        Rect::from_corners(min, min + Vec2::new(width, height))
    }
}

/// Tints `range`'s bytes on each built row it covers.
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
        let origin =
            frame.content(at.translation + node.content_box().min);
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

fn label_height() -> f32 {
    (LABEL_SIZE * LINE_HEIGHT).ceil()
}

/// A label's width in the code font, whose glyphs all advance alike.
fn label_width(text: &str) -> f32 {
    text.chars().count() as f32 * LABEL_SIZE * crate::rows::ADVANCE
        + 2.0 * LABEL_PAD
}

/// Black or white, whichever reads on `background`.
fn ink(background: Color) -> Color {
    if background.luminance() > 0.5 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}

/// Writes the scene into the view's pooled nodes, spawning the ones
/// it lacks and hiding the rest.
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
    for (i, (at, label, color, edge)) in scene.tags.iter().enumerate()
    {
        let placed = Node {
            position_type: PositionType::Absolute,
            left: px(at.x),
            top: px(at.y),
            padding: UiRect::horizontal(px(LABEL_PAD)),
            ..Node::default()
        };
        let parent = if *edge { remote.edges } else { remote.layer };
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
                            font_size: FontSize::Px(LABEL_SIZE),
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
