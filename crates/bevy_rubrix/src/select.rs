use core::fmt::Debug;
use core::ops::Range;

use bevy::camera::Camera;
use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::EntityEvent;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::{QueryFilter, With, Without};
use bevy::ecs::system::{
    Commands, ParamSet, Query, Res, ResMut, SystemParam,
};
use bevy::ecs::world::{EntityWorldMut, World};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::picking::events::PointerPress;
use bevy::picking::pointer::{
    PointerButton, PointerId, PointerLocation, PointerPressState,
};
use bevy::text::{ComputedTextBlock, TextLayout};
use bevy::ui::{
    BackgroundColor, ComputedNode, Display, Node, PositionType,
    ScrollPosition, UiGlobalTransform, px,
};
use bevy::window::Window;
use bevy_fynix::tokens::{TextTokens, Tone};
use bevy_fynix::views::{
    RowSlot, VirtualList, drawn_layout, rich_point,
};
use bevy_fynix::{AnyView, Bevy, Cx, View};
use parley::{Affinity, Cursor, Layout, Selection};
use rubrix_edit::{Command, word_at};

use crate::clipboard;
use crate::code_view::OVERSCAN;
use crate::keymap::{Action, Keymap, Keys, Resolved};
use crate::rows::Shown;

/// Below the code view's overscan, so there a drag step brings to the
/// edge a row already built and laid out. A diff view overscans less,
/// so its step can reach a row not built yet, which the list places
/// by its estimated height until it is built.
const MAX_STEP: f32 = OVERSCAN - 20.0;

const SELECTED_ALPHA: f32 = 0.15;

/// What is selected in a code view, on the view's root entity: byte
/// offsets into [`CodeDoc::text`](crate::CodeDoc::text). A new source
/// clears it. A diff view keeps one too, over the places of its rows.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CodeSelection<At = usize>
where
    At: Send + Sync + 'static,
{
    anchor: Range<At>,
    focus: Range<At>,
    grain: Grain,
    column: Column,
    revision: u64,
    drag: Option<PointerId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Grain {
    #[default]
    Letter,
    Word,
    Line,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Column {
    #[default]
    Only,
    Old,
    New,
}

impl<At: Copy + Ord + Send + Sync + 'static> CodeSelection<At> {
    /// What the first press covered: an empty range for a single
    /// press, the word for a double press, the line for a triple one.
    pub fn anchor(&self) -> Range<At> {
        self.anchor.clone()
    }

    /// What the last press or drag covered, the way
    /// [`CodeSelection::anchor`] does.
    pub fn focus(&self) -> Range<At> {
        self.focus.clone()
    }

    /// The bytes selected, from the anchor to the focus, each whole.
    pub fn range(&self) -> Range<At> {
        self.anchor.start.min(self.focus.start)
            ..self.anchor.end.max(self.focus.end)
    }

    pub(crate) fn clear(&mut self)
    where
        At: Default,
    {
        *self = CodeSelection::default();
    }

    fn live<D: Selectable<At = At>>(
        &self,
        doc: &D,
    ) -> Option<Range<At>> {
        (self.revision == doc.revision()).then(|| self.range())
    }
}

/// What a view's root holds for a selection to read: a row per line
/// of a virtual list, each with a rich text tagged
/// [`Selectable::Line`].
pub(crate) trait Selectable: Component + Sized {
    /// A place in the text, which orders by row first.
    type At: Copy + Ord + Default + Debug + Send + Sync + 'static;

    type Line: Component;

    fn line_of(line: &Self::Line) -> (Entity, usize, Column);

    fn list(&self) -> Option<Entity>;

    /// Changes whenever the rows do, which drops what was selected.
    fn revision(&self) -> u64;

    fn column(&self, across: f32) -> Column;

    fn text(&self, column: Column, row: usize)
    -> Option<RowText<'_>>;

    fn at(&self, row: usize, byte: usize) -> Self::At;

    fn on_row(
        &self,
        column: Column,
        row: usize,
        range: &Range<Self::At>,
    ) -> Range<usize>;

    fn all(&self) -> Range<Self::At>;

    fn copy(&self, column: Column, range: Range<Self::At>) -> String;
}

pub(crate) struct RowText<'a> {
    pub(crate) lead: &'static str,
    pub(crate) line: &'a str,
}

impl RowText<'_> {
    pub(crate) fn len(&self) -> usize {
        self.lead.len() + self.line.len()
    }

    fn source(&self, drawn: usize) -> usize {
        let lead = self.lead.len();
        if drawn < lead {
            drawn
        } else {
            lead + Shown::new(self.line).source(drawn - lead)
        }
    }

    fn drawn(&self, byte: usize) -> usize {
        let lead = self.lead.len();
        if byte < lead {
            byte
        } else {
            lead + Shown::new(self.line).shown(byte - lead)
        }
    }

    fn word(&self, at: usize) -> Range<usize> {
        let lead = self.lead.len();
        if at < lead {
            return word_at(self.lead, at);
        }
        let word = word_at(self.line, at - lead);
        lead + word.start..lead + word.end
    }

    pub(crate) fn push(&self, bytes: Range<usize>, out: &mut String) {
        let lead = self.lead.len();
        out.push_str(
            &self.lead[bytes.start.min(lead)..bytes.end.min(lead)],
        );
        out.push_str(
            &self.line[bytes.start.max(lead) - lead
                ..bytes.end.max(lead) - lead],
        );
    }
}

pub(crate) fn selected_colour(theme: &impl TextTokens) -> Color {
    theme.tone(Tone::Accent).with_alpha(SELECTED_ALPHA)
}

pub(crate) fn attach<D: Selectable>(view: &mut EntityWorldMut) {
    view.insert((CodeSelection::<D::At>::default(), Selects))
        .observe(press::<D>)
        .observe(hold_focus);
}

/// Keeps the focus a press gave a view. Bevy's pointer focus turns
/// the press into an `AcquireFocus` that climbs from the pressed row
/// to the window, where it clears the focus unless a `TabIndex` on
/// the way claims it. A view takes it here and stops it.
pub(crate) fn hold_focus(
    mut acquire: On<AcquireFocus>,
    mut focus: ResMut<InputFocus>,
) {
    acquire.propagate(false);
    let view = acquire.focused_entity;
    if focus.get() != Some(view) {
        focus.set(view, FocusCause::Pressed);
    }
}

pub(crate) fn selectable<T, L>(
    text: impl View<Bevy, T> + 'static,
    slot: RowSlot,
    color: Color,
    tag: impl Fn(usize) -> L + Send + Sync + 'static,
) -> AnyView<Bevy, T>
where
    T: 'static,
    L: Component,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let node = cx.build(text);
        cx.world
            .entity_mut(node)
            .insert(SelectionRects { color, layer: None });
        tag_row(cx, node, slot, tag);
        node
    })
}

pub(crate) fn tag_row<T: 'static, L: Component>(
    cx: &mut Cx<'_, Bevy, T>,
    node: Entity,
    slot: RowSlot,
    tag: impl Fn(usize) -> L + Send + Sync + 'static,
) {
    cx.effect(node, slot.index().into(), move |world, node, &row| {
        world.entity_mut(node).insert(tag(row));
    });
}

/// Sent to a code view's root when it copies text, with the text
/// copied. The text goes to the app's
/// [`EditClipboard`](crate::EditClipboard) too.
#[derive(EntityEvent, Clone, Debug)]
pub struct CodeCopied {
    pub entity: Entity,
    pub text: String,
}

#[derive(Component)]
pub(crate) struct SelectionRects {
    color: Color,
    layer: Option<Entity>,
}

impl SelectionRects {
    pub(crate) fn color(&self) -> Color {
        self.color
    }
}

#[derive(Component)]
pub(crate) struct Selects;

#[derive(Component)]
pub(crate) struct Layer;

#[derive(SystemParam)]
pub(crate) struct Hits<'w, 's, D: Selectable> {
    lists: Query<
        'w,
        's,
        (
            &'static VirtualList,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static ScrollPosition,
        ),
    >,
    lines: Query<
        'w,
        's,
        (
            &'static <D as Selectable>::Line,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static ComputedTextBlock,
            &'static TextLayout,
        ),
    >,
    cameras: Query<'w, 's, &'static Camera>,
}

/// Where a pointer is against a list: the row under it, its point
/// clamped inside the list, and how far past an edge it is.
pub(crate) struct Over {
    pub(crate) row: usize,
    across: f32,
    /// Logical window px, clamped one physical px inside the list.
    pub(crate) inside: Vec2,
    /// Logical px past the top (negative) or bottom edge.
    pub(crate) past: f32,
    view: f32,
    scroll: f32,
}

/// Where `at`, logical window px, is against the list on `node`.
pub(crate) fn over(
    list: &VirtualList,
    node: &ComputedNode,
    transform: &UiGlobalTransform,
    camera: &Camera,
    scroll: &ScrollPosition,
    at: Vec2,
) -> Option<Over> {
    let local = rich_point(node, transform, camera, at)?;
    let logical = node.inverse_scale_factor();
    let size = (node.content_box().size() - node.scrollbar_size)
        .max(Vec2::ONE);
    let held = local.clamp(Vec2::ZERO, size - 1.0);
    let view = size.y * logical;
    let past = if local.y < 0.0 {
        local.y * logical
    } else {
        (local.y - held.y) * logical
    };
    let row = list.row_at(held.y * logical + scroll.y)?;
    Some(Over {
        row,
        across: held.x / size.x,
        inside: at + (held - local) * logical,
        past,
        view,
        scroll: scroll.y,
    })
}

pub(crate) fn on_scrollbars(
    node: &ComputedNode,
    transform: &UiGlobalTransform,
    camera: &Camera,
    at: Vec2,
) -> bool {
    rich_point(node, transform, camera, at).is_some_and(|local| {
        let rows = node.content_box().size() - node.scrollbar_size;
        local.x >= rows.x || local.y >= rows.y
    })
}

/// The drawn byte nearest `at`, logical window px, in a rich text
/// laid out as `layout` on `node`.
pub(crate) fn text_point(
    layout: &Layout<bevy::text::TextBrush>,
    node: &ComputedNode,
    transform: &UiGlobalTransform,
    camera: &Camera,
    at: Vec2,
) -> Option<usize> {
    let point = rich_point(node, transform, camera, at)?;
    Some(Cursor::from_point(layout, point.x, point.y).index())
}

pub(crate) type Pointers<'w, 's> = Query<
    'w,
    's,
    (
        &'static PointerId,
        &'static PointerLocation,
        &'static PointerPressState,
    ),
>;

/// Where the pointer `id` is while its primary button is held.
pub(crate) fn held(
    pointers: &Pointers,
    id: PointerId,
) -> Option<Vec2> {
    pointers
        .iter()
        .find(|&(pointer, _, press)| {
            *pointer == id && press.is_primary_pressed()
        })
        .and_then(|(_, location, _)| location.location())
        .map(|location| location.position)
}

impl<D: Selectable> Hits<'_, '_, D> {
    fn over(&self, list: Entity, at: Vec2) -> Option<Over> {
        let camera = self.cameras.iter().next()?;
        let (state, node, transform, scroll) =
            self.lists.get(list).ok()?;
        over(state, node, transform, camera, scroll, at)
    }

    fn on_scrollbars(&self, list: Entity, at: Vec2) -> bool {
        let Some(camera) = self.cameras.iter().next() else {
            return false;
        };
        self.lists.get(list).ok().is_some_and(
            |(_, node, transform, _)| {
                on_scrollbars(node, transform, camera, at)
            },
        )
    }

    fn byte(
        &self,
        view: Entity,
        doc: &D,
        column: Column,
        row: usize,
        at: Vec2,
    ) -> usize {
        let Some(text) = doc.text(column, row) else {
            return 0;
        };
        let Some(camera) = self.cameras.iter().next() else {
            return 0;
        };
        let shown = self.lines.iter().find(|(line, ..)| {
            D::line_of(line) == (view, row, column)
        });
        let Some((_, node, transform, block, layout)) = shown else {
            return 0;
        };
        let layout = drawn_layout(block, layout, node);
        text_point(&layout, node, transform, camera, at)
            .map_or(0, |drawn| text.source(drawn))
    }
}

fn unit<D: Selectable>(
    doc: &D,
    column: Column,
    row: usize,
    byte: usize,
    grain: Grain,
) -> Range<D::At> {
    let text = doc.text(column, row);
    let bytes = match (grain, text) {
        (Grain::Letter, _) | (_, None) => byte..byte,
        (Grain::Line, Some(text)) => 0..text.len(),
        (Grain::Word, Some(text)) => text.word(byte),
    };
    doc.at(row, bytes.start)..doc.at(row, bytes.end)
}

fn shift_held(keys: Option<&ButtonInput<KeyCode>>) -> bool {
    keys.is_some_and(|keys| {
        keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    })
}

fn press<D: Selectable>(
    mut press: On<PointerPress>,
    mut views: Query<(&D, &mut CodeSelection<D::At>)>,
    hits: Hits<D>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut focus: ResMut<InputFocus>,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let view = press.entity;
    let Ok((doc, mut selection)) = views.get_mut(view) else {
        return;
    };
    press.propagate(false);
    let at = press.pointer.position;
    let Some(list) = doc.list() else {
        return;
    };
    if hits.on_scrollbars(list, at) {
        return;
    }
    let Some(over) = hits.over(list, at) else {
        return;
    };
    if focus.get() != Some(view) {
        focus.set(view, FocusCause::Pressed);
    }
    let extend =
        shift_held(keys.as_deref()) && selection.live(doc).is_some();
    if extend {
        let column = selection.column;
        let byte =
            hits.byte(view, doc, column, over.row, over.inside);
        selection.focus =
            unit(doc, column, over.row, byte, selection.grain);
    } else {
        let grain = match press.count {
            0 | 1 => Grain::Letter,
            2 => Grain::Word,
            _ => Grain::Line,
        };
        let column = doc.column(over.across);
        let byte =
            hits.byte(view, doc, column, over.row, over.inside);
        let anchor = unit(doc, column, over.row, byte, grain);
        *selection = CodeSelection {
            focus: anchor.clone(),
            anchor,
            grain,
            column,
            revision: doc.revision(),
            drag: None,
        };
    }
    selection.drag = Some(press.pointer.id);
}

pub(crate) fn follow_drags<D: Selectable>(
    mut views: Query<(Entity, &D, &mut CodeSelection<D::At>)>,
    pointers: Pointers,
    mut lists: ParamSet<(Hits<D>, Query<&mut VirtualList>)>,
) {
    for (view, doc, mut selection) in &mut views {
        let (Some(dragging), Some(list)) =
            (selection.drag, doc.list())
        else {
            continue;
        };
        let Some(at) = held(&pointers, dragging) else {
            selection.drag = None;
            continue;
        };
        if selection.live(doc).is_none() {
            continue;
        }
        let hits = lists.p0();
        let Some(over) = hits.over(list, at) else {
            continue;
        };
        let column = selection.column;
        let byte =
            hits.byte(view, doc, column, over.row, over.inside);
        let focus =
            unit(doc, column, over.row, byte, selection.grain);
        if selection.focus != focus {
            selection.focus = focus;
        }
        if over.past != 0.0
            && let Ok(mut list) = lists.p1().get_mut(list)
        {
            reveal_past(&mut list, &over);
        }
    }
}

/// Steps `list` toward a pointer held past its top or bottom edge.
pub(crate) fn reveal_past(list: &mut VirtualList, over: &Over) {
    let step = over.past.abs().min(MAX_STEP);
    let target = if over.past > 0.0 {
        let last = list.count().checked_sub(1);
        let end = last
            .and_then(|last| list.row_span(last))
            .map_or(0.0, |span| span.top + span.height);
        if end <= over.scroll + over.view {
            return;
        }
        over.scroll + over.view + step
    } else {
        if over.scroll <= 0.0 {
            return;
        }
        (over.scroll - step).max(0.0)
    };
    let Some(row) = list.row_at(target) else {
        return;
    };
    let Some(span) = list.row_span(row) else {
        return;
    };
    let into = target - span.top;
    let range = if over.past > 0.0 {
        (into - 1.0).max(0.0)..into
    } else {
        into..into + 1.0
    };
    list.scroll_range_into_view(row, range);
}

/// Select-all and copy, as the [`Keymap`] binds them, for the
/// read-only view that holds the input focus. With no focus, or the
/// window's, they go to the only read-only view, or else to the only
/// one with a live selection.
pub(crate) fn keys<D: Selectable>(
    keys: Res<Keys>,
    keymap: Res<Keymap>,
    mut views: Query<(Entity, &D, &mut CodeSelection<D::At>)>,
    every: Query<Entity, With<Selects>>,
    windows: Query<(), With<Window>>,
    mut commands: Commands,
) {
    for press in keys.presses() {
        let Some(Resolved::Action(action)) = keymap.resolve(press)
        else {
            continue;
        };
        let all = action == Action::Command(Command::SelectAll);
        let copy = action == Action::Copy;
        if !all && !copy {
            continue;
        }
        let view = match press.focus {
            Some(focus) if every.contains(focus) => Some(focus),
            Some(focus) if !windows.contains(focus) => None,
            _ => {
                let only = |mut views: Vec<Entity>| {
                    (views.len() == 1).then(|| views.remove(0))
                };
                only(every.iter().collect()).or_else(|| {
                    only(
                        views
                            .iter()
                            .filter(|(_, doc, selection)| {
                                selection.live(*doc).is_some()
                            })
                            .map(|(view, ..)| view)
                            .collect(),
                    )
                })
            }
        };
        let Some(Ok((view, doc, mut selection))) =
            view.map(|view| views.get_mut(view))
        else {
            continue;
        };
        if all {
            let whole = doc.all();
            let column = match selection.live(doc) {
                Some(_) => selection.column,
                None => doc.column(0.0),
            };
            *selection = CodeSelection {
                anchor: whole.start..whole.start,
                focus: whole.end..whole.end,
                column,
                revision: doc.revision(),
                ..CodeSelection::default()
            };
        }
        let Some(range) = selection.live(doc).filter(|_| copy) else {
            continue;
        };
        let text = doc.copy(selection.column, range);
        if text.is_empty() {
            continue;
        }
        commands.queue(move |world: &mut World| {
            clipboard::copy(world, view, text);
        });
    }
}

type Drawn<'a, L> = (
    &'a L,
    &'a ComputedNode,
    &'a ComputedTextBlock,
    &'a TextLayout,
    &'a mut SelectionRects,
    &'a ChildOf,
);

pub(crate) fn draw<D: Selectable>(
    mut lines: Query<Drawn<'_, D::Line>>,
    views: Query<(&D, &CodeSelection<D::At>)>,
    mut layers: Query<(&mut Node, Option<&Children>), With<Layer>>,
    mut nodes: Query<
        (&mut Node, &mut BackgroundColor),
        Without<Layer>,
    >,
    mut commands: Commands,
) {
    for (line, node, block, layout, mut drawn, cell) in &mut lines {
        let (view, row, column) = D::line_of(line);
        let color = drawn.color;
        let boxes = views
            .get(view)
            .ok()
            .and_then(|(doc, selection)| {
                let selected = selection
                    .live(doc)
                    .filter(|_| selection.column == column)?;
                let bytes = doc.on_row(column, row, &selected);
                if bytes.is_empty() {
                    return None;
                }
                let text = doc.text(column, row)?;
                let layout = drawn_layout(block, layout, node);
                let drawn =
                    text.drawn(bytes.start)..text.drawn(bytes.end);
                let logical = node.inverse_scale_factor();
                Some(
                    rects(&layout, drawn, logical, color)
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        paint(
            &boxes,
            &mut drawn,
            cell,
            &mut layers,
            &mut nodes,
            &mut commands,
        );
    }
}

/// The rects that select the drawn bytes `drawn` of `layout`, logical
/// px from the text's origin, each in `color`.
pub(crate) fn rects(
    layout: &Layout<bevy::text::TextBrush>,
    drawn: Range<usize>,
    logical: f32,
    color: Color,
) -> impl Iterator<Item = ([f32; 4], Color)> {
    let at = |byte, affinity| {
        Cursor::from_byte_index(layout, byte, affinity)
    };
    let from = at(drawn.start, Affinity::Downstream);
    let to = at(drawn.end, Affinity::Upstream);
    Selection::new(from, to).geometry(layout).into_iter().map(
        move |(rect, _)| {
            let corners = [rect.x0, rect.y0, rect.x1, rect.y1];
            (corners.map(|v| v as f32 * logical), color)
        },
    )
}

/// Puts `boxes`, logical px from the text's origin, as rects in the
/// selection layer under the text whose cell is `cell`: spawns the
/// layer the first time it has rects, hides it when it has none,
/// reuses rects by index and writes a rect only when it changed.
pub(crate) fn paint<F: QueryFilter>(
    boxes: &[([f32; 4], Color)],
    rects: &mut SelectionRects,
    cell: &ChildOf,
    layers: &mut Query<(&mut Node, Option<&Children>), With<Layer>>,
    nodes: &mut Query<(&mut Node, &mut BackgroundColor), F>,
    commands: &mut Commands,
) {
    let layer = match rects.layer {
        Some(layer) => layer,
        None if boxes.is_empty() => return,
        None => {
            let layer = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        ..Node::default()
                    },
                    Pickable::IGNORE,
                    Layer,
                ))
                .id();
            commands
                .entity(cell.parent())
                .insert_children(0, &[layer]);
            rects.layer = Some(layer);
            layer
        }
    };
    let (mut shown, kids) = match layers.get_mut(layer) {
        Ok((shown, kids)) => (Some(shown), kids),
        Err(_) => (None, None),
    };
    let display = if boxes.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    if let Some(shown) = &mut shown
        && shown.display != display
    {
        shown.display = display;
    }
    let kids = kids.map_or(&[][..], |kids| &kids[..]);
    for (index, &([x0, y0, x1, y1], color)) in
        boxes.iter().enumerate()
    {
        let placed = Node {
            position_type: PositionType::Absolute,
            left: px(x0),
            top: px(y0),
            width: px(x1 - x0),
            height: px(y1 - y0),
            ..Node::default()
        };
        match kids.get(index).and_then(|&kid| nodes.get_mut(kid).ok())
        {
            Some((mut node, mut background)) => {
                if *node != placed {
                    *node = placed;
                }
                if background.0 != color {
                    background.0 = color;
                }
            }
            None => {
                commands.spawn((
                    placed,
                    BackgroundColor(color),
                    Pickable::IGNORE,
                    ChildOf(layer),
                ));
            }
        }
    }
    for &kid in kids.iter().skip(boxes.len()) {
        if let Ok((mut node, _)) = nodes.get_mut(kid)
            && node.display != Display::None
        {
            node.display = Display::None;
        }
    }
}
