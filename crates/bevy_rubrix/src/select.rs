use core::fmt::Debug;
use core::ops::Range;

use bevy::camera::Camera;
use bevy::clipboard::Clipboard;
use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::EntityEvent;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::observer::On;
use bevy::ecs::query::{With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{
    Commands, ParamSet, Query, Res, ResMut, SystemParam,
};
use bevy::ecs::world::EntityWorldMut;
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
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
use bevy_fynix::tokens::{TextTokens, Tone};
use bevy_fynix::views::{
    RowSlot, VirtualList, drawn_layout, rich_point,
};
use bevy_fynix::{AnyView, Bevy, Cx, View};
use parley::{Affinity, Cursor, Selection};

use crate::code_view::{OVERSCAN, Shown};

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
        .observe(press::<D>);
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

/// Sent to a code view's root when `Ctrl+C` copies its selection,
/// with the text copied. The text goes to Bevy's [`Clipboard`] too,
/// when the app has one.
#[derive(EntityEvent, Clone, Debug)]
pub struct CodeCopied {
    pub entity: Entity,
    pub text: String,
}

#[derive(Resource, Default)]
pub(crate) struct KeyTarget(Option<Entity>);

#[derive(Component)]
pub(crate) struct SelectionRects {
    color: Color,
    layer: Option<Entity>,
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

struct Over {
    row: usize,
    across: f32,
    inside: Vec2,
    past: f32,
    view: f32,
    scroll: f32,
}

impl<D: Selectable> Hits<'_, '_, D> {
    fn over(&self, list: Entity, at: Vec2) -> Option<Over> {
        let camera = self.cameras.iter().next()?;
        let (state, node, transform, scroll) =
            self.lists.get(list).ok()?;
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
        let row = state.row_at(held.y * logical + scroll.y)?;
        Some(Over {
            row,
            across: held.x / size.x,
            inside: at + (held - local) * logical,
            past,
            view,
            scroll: scroll.y,
        })
    }

    fn on_scrollbars(&self, list: Entity, at: Vec2) -> bool {
        let Some(camera) = self.cameras.iter().next() else {
            return false;
        };
        self.lists.get(list).ok().is_some_and(
            |(_, node, transform, _)| {
                rich_point(node, transform, camera, at).is_some_and(
                    |local| {
                        let rows = node.content_box().size()
                            - node.scrollbar_size;
                        local.x >= rows.x || local.y >= rows.y
                    },
                )
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
        let Some(point) = rich_point(node, transform, camera, at)
        else {
            return 0;
        };
        let layout = drawn_layout(block, layout, node);
        let cursor = Cursor::from_point(&layout, point.x, point.y);
        text.source(cursor.index())
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Chars {
    Word,
    Space,
    Other,
}

pub(crate) fn word_at(line: &str, at: usize) -> Range<usize> {
    let Some((start, first)) = line[at..]
        .chars()
        .next()
        .map(|c| (at, c))
        .or_else(|| line[..at].char_indices().next_back())
    else {
        return at..at;
    };
    let kind = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Chars::Word
        } else if c.is_whitespace() {
            Chars::Space
        } else {
            Chars::Other
        }
    };
    let class = kind(first);
    if class == Chars::Other {
        return start..start + first.len_utf8();
    }
    let end = line[start..]
        .char_indices()
        .find(|&(_, c)| kind(c) != class)
        .map_or(line.len(), |(off, _)| start + off);
    let begin = line[..start]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| kind(c) == class)
        .last()
        .map_or(start, |(off, _)| off);
    begin..end
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
    mut pressed: ResMut<KeyTarget>,
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
    pressed.0 = Some(view);
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

/// Follows the pressed pointer itself, not the drag events Bevy sends
/// to the pressed row, because a view may rebuild that row mid-drag.
pub(crate) fn follow_drags<D: Selectable>(
    mut views: Query<(Entity, &D, &mut CodeSelection<D::At>)>,
    pointers: Query<(
        &PointerId,
        &PointerLocation,
        &PointerPressState,
    )>,
    mut lists: ParamSet<(Hits<D>, Query<&mut VirtualList>)>,
) {
    for (view, doc, mut selection) in &mut views {
        let (Some(dragging), Some(list)) =
            (selection.drag, doc.list())
        else {
            continue;
        };
        let held = pointers
            .iter()
            .find(|&(id, _, press)| {
                *id == dragging && press.is_primary_pressed()
            })
            .and_then(|(_, location, _)| location.location());
        let Some(at) = held.map(|location| location.position) else {
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

fn reveal_past(list: &mut VirtualList, over: &Over) {
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

pub(crate) fn keys<D: Selectable>(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut views: Query<(Entity, &D, &mut CodeSelection<D::At>)>,
    every: Query<Entity, With<Selects>>,
    pressed: Res<KeyTarget>,
    clipboard: Option<ResMut<Clipboard>>,
    mut commands: Commands,
) {
    let Some(keys) = keys else {
        return;
    };
    let ctrl = keys
        .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let all = ctrl && keys.just_pressed(KeyCode::KeyA);
    let copy = ctrl && keys.just_pressed(KeyCode::KeyC);
    if !all && !copy {
        return;
    }
    let view = pressed
        .0
        .filter(|&view| every.contains(view))
        .or_else(|| {
            let mut every = every.iter();
            every.next().filter(|_| every.next().is_none())
        });
    let Some(Ok((view, doc, mut selection))) =
        view.map(|view| views.get_mut(view))
    else {
        return;
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
        return;
    };
    let text = doc.copy(selection.column, range);
    if text.is_empty() {
        return;
    }
    if let Some(mut clipboard) = clipboard {
        let _ = clipboard.set_text(text.as_str());
    }
    commands.trigger(CodeCopied { entity: view, text });
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
    mut rects: Query<&mut Node, Without<Layer>>,
    mut commands: Commands,
) {
    for (line, node, block, layout, mut drawn, cell) in &mut lines {
        let (view, row, column) = D::line_of(line);
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
                let at = |byte: usize, affinity| {
                    Cursor::from_byte_index(
                        &layout,
                        text.drawn(byte),
                        affinity,
                    )
                };
                let logical = node.inverse_scale_factor();
                Some(
                    Selection::new(
                        at(bytes.start, Affinity::Downstream),
                        at(bytes.end, Affinity::Upstream),
                    )
                    .geometry(&layout)
                    .into_iter()
                    .map(|(rect, _)| {
                        [rect.x0, rect.y0, rect.x1, rect.y1]
                            .map(|v| v as f32 * logical)
                    })
                    .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        let layer = match drawn.layer {
            Some(layer) => layer,
            None if boxes.is_empty() => continue,
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
                drawn.layer = Some(layer);
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
        for (index, [x0, y0, x1, y1]) in
            boxes.iter().copied().enumerate()
        {
            let placed = Node {
                position_type: PositionType::Absolute,
                left: px(x0),
                top: px(y0),
                width: px(x1 - x0),
                height: px(y1 - y0),
                ..Node::default()
            };
            match kids
                .get(index)
                .and_then(|&kid| rects.get_mut(kid).ok())
            {
                Some(mut node) => {
                    if *node != placed {
                        *node = placed;
                    }
                }
                None => {
                    commands.spawn((
                        placed,
                        BackgroundColor(drawn.color),
                        Pickable::IGNORE,
                        ChildOf(layer),
                    ));
                }
            }
        }
        for &kid in kids.iter().skip(boxes.len()) {
            if let Ok(mut node) = rects.get_mut(kid)
                && node.display != Display::None
            {
                node.display = Display::None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::word_at;

    #[test]
    fn a_word_is_an_identifier_a_space_run_or_one_char() {
        let line = "    self.foo_bar(x, 日本);";
        let word = |at| &line[word_at(line, at)];
        assert_eq!(word(9), "foo_bar");
        assert_eq!(word(4), "self");
        assert_eq!(word(8), ".");
        assert_eq!(word(1), "    ");
        assert_eq!(word(line.find('日').unwrap()), "日本");
        assert_eq!(word(line.len()), ";");
        assert_eq!(word_at("", 0), 0..0);
    }
}
