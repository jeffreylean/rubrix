use core::ops::Range;

use bevy::camera::Camera;
use bevy::clipboard::Clipboard;
use bevy::color::Color;
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
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::picking::events::{
    PointerDrag, PointerDragEnd, PointerPress, PointerRelease,
};
use bevy::picking::pointer::PointerButton;
use bevy::text::ComputedTextBlock;
use bevy::ui::{
    BackgroundColor, ComputedNode, Display, Node, PositionType,
    ScrollPosition, UiGlobalTransform, px,
};
use bevy_fynix::views::{VirtualList, rich_point};
use parley::{Affinity, Cursor, Selection};

use crate::code_view::{CodeDoc, CodeLine, OVERSCAN, Shown};

/// Below a whole text's overscan, so the row a drag step brings to
/// the edge is already built and laid out.
const MAX_STEP: f32 = OVERSCAN - 20.0;

/// What is selected in a code view, on the view's root entity: byte
/// offsets into [`CodeDoc::text`]. A new source clears it.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CodeSelection {
    anchor: Range<usize>,
    focus: Range<usize>,
    grain: Grain,
    drag: Option<Vec2>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Grain {
    #[default]
    Letter,
    Word,
    Line,
}

impl CodeSelection {
    /// What the first press covered: an empty range for a single
    /// press, the word for a double press, the line for a triple one.
    pub fn anchor(&self) -> Range<usize> {
        self.anchor.clone()
    }

    /// What the last press or drag covered, the way
    /// [`CodeSelection::anchor`] does.
    pub fn focus(&self) -> Range<usize> {
        self.focus.clone()
    }

    /// The bytes selected, from the anchor to the focus, each whole.
    pub fn range(&self) -> Range<usize> {
        self.anchor.start.min(self.focus.start)
            ..self.anchor.end.max(self.focus.end)
    }

    pub(crate) fn clear(&mut self) {
        *self = CodeSelection::default();
    }
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
    pub(crate) color: Color,
    pub(crate) layer: Option<Entity>,
}

#[derive(Component)]
pub(crate) struct Layer;

#[derive(SystemParam)]
pub(crate) struct Hits<'w, 's> {
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
            &'static CodeLine,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static ComputedTextBlock,
        ),
    >,
    cameras: Query<'w, 's, &'static Camera>,
}

struct Over {
    row: usize,
    inside: Vec2,
    past: f32,
    view: f32,
    scroll: f32,
}

impl Hits<'_, '_> {
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
        doc: &CodeDoc,
        row: usize,
        at: Vec2,
    ) -> usize {
        let start = doc.line_range(row).start;
        let Some(camera) = self.cameras.iter().next() else {
            return start;
        };
        let shown = self
            .lines
            .iter()
            .find(|(line, ..)| **line == CodeLine { view, row });
        let Some((_, node, transform, block)) = shown else {
            return start;
        };
        let Some(point) = rich_point(node, transform, camera, at)
        else {
            return start;
        };
        let cursor =
            Cursor::from_point(block.buffer(), point.x, point.y);
        let line = doc.line(row).unwrap_or_default();
        start + Shown::new(line).source(cursor.index())
    }
}

fn unit(
    doc: &CodeDoc,
    row: usize,
    byte: usize,
    grain: Grain,
) -> Range<usize> {
    let line = doc.line_range(row);
    match grain {
        Grain::Letter => byte..byte,
        Grain::Line => line,
        Grain::Word => {
            let text = doc.line(row).unwrap_or_default();
            let word = word_at(text, byte - line.start);
            line.start + word.start..line.start + word.end
        }
    }
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

pub(crate) fn press(
    mut press: On<PointerPress>,
    mut views: Query<(&CodeDoc, &mut CodeSelection)>,
    hits: Hits,
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
    let byte = hits.byte(view, doc, over.row, over.inside);
    let extend = shift_held(keys.as_deref())
        && *selection != CodeSelection::default();
    if extend {
        selection.focus = unit(doc, over.row, byte, selection.grain);
    } else {
        let grain = match press.count {
            0 | 1 => Grain::Letter,
            2 => Grain::Word,
            _ => Grain::Line,
        };
        let anchor = unit(doc, over.row, byte, grain);
        *selection = CodeSelection {
            focus: anchor.clone(),
            anchor,
            grain,
            drag: None,
        };
    }
    selection.drag = Some(at);
}

pub(crate) fn drag(
    drag: On<PointerDrag>,
    mut selections: Query<&mut CodeSelection>,
) {
    if let Ok(mut selection) = selections.get_mut(drag.entity)
        && selection.drag.is_some()
    {
        selection.drag = Some(drag.pointer.position);
    }
}

pub(crate) fn release(
    release: On<PointerRelease>,
    mut selections: Query<&mut CodeSelection>,
) {
    if let Ok(mut selection) = selections.get_mut(release.entity) {
        selection.drag = None;
    }
}

/// A release away from the view reaches it only as the end of the
/// drag, which goes to the pressed row wherever the pointer is.
pub(crate) fn drag_end(
    end: On<PointerDragEnd>,
    mut selections: Query<&mut CodeSelection>,
) {
    if let Ok(mut selection) = selections.get_mut(end.entity) {
        selection.drag = None;
    }
}

pub(crate) fn follow_drags(
    mut views: Query<(Entity, &CodeDoc, &mut CodeSelection)>,
    mut lists: ParamSet<(Hits, Query<&mut VirtualList>)>,
) {
    for (view, doc, mut selection) in &mut views {
        let (Some(at), Some(list)) = (selection.drag, doc.list())
        else {
            continue;
        };
        let hits = lists.p0();
        let Some(over) = hits.over(list, at) else {
            continue;
        };
        let byte = hits.byte(view, doc, over.row, over.inside);
        let focus = unit(doc, over.row, byte, selection.grain);
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

pub(crate) fn keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut views: Query<(Entity, &CodeDoc, &mut CodeSelection)>,
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
        .filter(|&view| views.contains(view))
        .or_else(|| {
            let mut every = views.iter().map(|(view, ..)| view);
            every.next().filter(|_| every.next().is_none())
        });
    let Some(Ok((view, doc, mut selection))) =
        view.map(|view| views.get_mut(view))
    else {
        return;
    };
    if all {
        let len = doc.text().len();
        *selection = CodeSelection {
            anchor: 0..0,
            focus: len..len,
            ..CodeSelection::default()
        };
    }
    if copy && !selection.range().is_empty() {
        let text = doc.text()[selection.range()].to_owned();
        if let Some(mut clipboard) = clipboard {
            let _ = clipboard.set_text(text.as_str());
        }
        commands.trigger(CodeCopied { entity: view, text });
    }
}

pub(crate) fn draw(
    mut lines: Query<(
        &CodeLine,
        &ComputedNode,
        &ComputedTextBlock,
        &mut SelectionRects,
        &ChildOf,
    )>,
    views: Query<(&CodeDoc, &CodeSelection)>,
    mut layers: Query<(&mut Node, Option<&Children>), With<Layer>>,
    mut rects: Query<&mut Node, Without<Layer>>,
    mut commands: Commands,
) {
    for (line, node, block, mut drawn, cell) in &mut lines {
        let boxes = views
            .get(line.view)
            .ok()
            .and_then(|(doc, selection)| {
                let selected = selection.range();
                let bytes = doc.line_range(line.row);
                let from = selected.start.max(bytes.start);
                let to = selected.end.min(bytes.end);
                if from >= to {
                    return None;
                }
                let shown = Shown::new(doc.line(line.row)?);
                let layout = block.buffer();
                let at = |byte: usize, affinity| {
                    Cursor::from_byte_index(
                        layout,
                        shown.shown(byte - bytes.start),
                        affinity,
                    )
                };
                let logical = node.inverse_scale_factor();
                Some(
                    Selection::new(
                        at(from, Affinity::Downstream),
                        at(to, Affinity::Upstream),
                    )
                    .geometry(layout)
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
