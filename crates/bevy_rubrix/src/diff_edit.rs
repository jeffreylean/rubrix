use core::ops::Range;
use core::time::Duration;
use std::time::Instant;

use bevy::app::{App, PostUpdate, PreUpdate};
use bevy::camera::visibility::Visibility;
use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::message::{MessageCursor, MessageWriter, Messages};
use bevy::ecs::observer::On;
use bevy::ecs::query::{With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Local, Query, Res};
use bevy::ecs::world::{DeferredWorld, EntityWorldMut, World};
use bevy::input::ButtonInput;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::math::Vec2;
use bevy::picking::events::PointerPress;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::picking::{Pickable, PickingSystems};
use bevy::text::{ComputedTextBlock, TextLayout};
use bevy::ui::{
    BackgroundColor, CalculatedClip, ComputedNode, Node,
    PositionType, ScrollPosition, UiGlobalTransform, UiSystems,
    ZIndex, px,
};
use bevy_fynix::views::{VirtualList, drawn_layout};
use parley::{Affinity, Cursor};
use rubrix_diff::{
    Anchor, FileChange, FileDiff, Layout, Rediff, Row, RowSplice,
    Rows,
};
use rubrix_edit::{
    Buffer, Layout as _, Monospace, SCAN_LINES, Selection, Selections,
};

use crate::commands::{BRACKET_ALPHA, keep, refreshed};
use crate::damage::BUILT_MAX;
use crate::diff_view::{
    CONTEXT, DiffDoc, DiffLine, DiffRow, Lane, Live, Spot,
    number_width, stacked_line,
};
use crate::editor::{
    EditDoc, EditRequest, EditSystems, EditorChanged, Request,
};
use crate::frame::{Act, catch_up_window, dispatch};
use crate::keymap::{Gesture, KeyPress, Keymap, Keys, Mods};
use crate::rows::Shown;
use crate::select::{
    self, CodeSelection, Column, Hits, Layer, Over, Pointers,
    SELECTED_ALPHA, Selectable, SelectionRects, Selects,
};
use crate::{BracketMatch, Hit};

const CARET_WIDTH: f32 = 2.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        (
            keys.after(PickingSystems::Last)
                .after(EditSystems::Keys)
                .before(EditSystems::Apply)
                .run_if(editing),
            apply.after(EditSystems::Apply).run_if(shows_edits),
        ),
    )
    .add_systems(
        PostUpdate,
        (
            draw.after(UiSystems::PostLayout).run_if(editing),
            tint.after(UiSystems::PostLayout).run_if(shows_edits),
        ),
    );
}

fn editing(docs: Query<&DiffDoc>) -> bool {
    docs.iter().any(|doc| doc.edit.on)
}

fn shows_edits(docs: Query<&DiffDoc>) -> bool {
    docs.iter()
        .any(|doc| doc.edit.on || doc.edit.has_live_docs())
}

#[derive(Default)]
pub(crate) struct Session {
    on: bool,
    pub(crate) active: Option<usize>,
    pressed: Option<Press>,
    drag: Option<Drag>,
    reads: u64,
    reads_this_frame: u64,
    reads_last_frame: u64,
    redrawn: Vec<Redraw>,
    damage: Vec<(Entity, u64)>,
    in_sight: Option<(usize, usize)>,
    rediff: Option<(usize, Duration)>,
    colour: Color,
    /// Logical px of one column of the rows' monospace font.
    advance: f32,
    bar: Option<Entity>,
}

/// A primary press on a diff view, for [`keys`] to place.
#[derive(Clone, Copy)]
struct Press {
    at: Vec2,
    count: u8,
    pointer: PointerId,
}

/// A press on a file's new side, run again to the pointer while its
/// button stays held, as the editor runs its gestures.
struct Drag {
    pointer: PointerId,
    file: usize,
    column: Column,
    gesture: Gesture,
    from: Hit,
    to: Hit,
    base: Selections,
}

/// On each live file's doc, so the editor's bracket match leaves it
/// to the diff, which lexes and matches it with its rows' window.
#[derive(Component)]
pub(crate) struct LiveFile;

/// A file's new side as vertical motion sees it: the lines a row
/// shows. A caret steps over the lines a gap hides, as Pierre's moves
/// step over folds.
#[derive(Clone)]
pub(crate) struct DiffLayout {
    hidden: Vec<Range<u32>>,
    /// Lines from here on have no row.
    lines: u32,
}

impl DiffLayout {
    fn of(doc: &DiffDoc, file: usize) -> DiffLayout {
        DiffLayout {
            hidden: doc.files()[file].rows.hidden_new().collect(),
            lines: doc.diff(file).new_lines().len().max(1),
        }
    }

    fn shows(&self, line: u32) -> bool {
        line < self.lines && self.hiding(line).is_none()
    }

    fn hiding(&self, line: u32) -> Option<&Range<u32>> {
        let at = self.hidden.partition_point(|run| run.end <= line);
        self.hidden.get(at).filter(|run| run.contains(&line))
    }

    /// The next line that shows below `line`, or above it.
    fn step(&self, line: u32, down: bool) -> Option<u32> {
        let mut next = if down {
            line.checked_add(1)?
        } else {
            line.checked_sub(1)?
        };
        while let Some(run) = self.hiding(next) {
            next = if down {
                run.end
            } else {
                run.start.checked_sub(1)?
            };
        }
        (next < self.lines).then_some(next)
    }
}

impl rubrix_edit::Layout for DiffLayout {
    fn x(&self, buffer: &Buffer, offset: usize) -> f32 {
        Monospace::default().x(buffer, offset)
    }

    /// With no line showing that way, the end of the caret's line,
    /// or its start, so the caret never lands where no row shows.
    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        rows: i32,
        x: f32,
    ) -> Option<usize> {
        let line = buffer.line_of(from);
        let mut at = line;
        for _ in 0..rows.unsigned_abs() {
            match self.step(at, rows > 0) {
                Some(next) => at = next,
                None => break,
            }
        }
        if at == line && rows != 0 {
            return Some(if rows > 0 {
                buffer.line_end(line)
            } else {
                buffer.line_start(line)
            });
        }
        Monospace::default().vertical(
            buffer,
            buffer.line_start(at),
            0,
            x,
        )
    }

    fn page_rows(&self) -> u32 {
        Monospace::default().page_rows
    }
}

impl Session {
    /// `damage` holds a row per live file's doc after each catch-up.
    fn has_live_docs(&self) -> bool {
        !self.damage.is_empty()
    }
}

struct Redraw {
    reads: u64,
    file: usize,
    rows: Range<u32>,
}

#[derive(Clone, Copy)]
pub(crate) struct Stamp {
    reshapes: u64,
    reads: u64,
    damage: u64,
}

#[derive(Component)]
pub(crate) struct DiffCaret;

/// The diff-edit state of the file that holds the caret, for an app's
/// status line or a test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffEditStatus {
    pub file: usize,
    /// The file's hunks now.
    pub hunks: usize,
    /// Undo steps the file's editor holds.
    pub undo: usize,
    /// How long the last re-diff of this file took, the new side's
    /// text splice included.
    pub rediff: Option<Duration>,
    /// The new-side word ranges of the caret's row, as
    /// [`Row::words`] gives them.
    pub marks: Vec<Range<u32>>,
    /// The `@@` header of the hunk that holds the caret's line.
    pub hunk: Option<String>,
    /// The caret's line, by index into the new side.
    pub line: u32,
}

pub(crate) fn fires(
    world: &World,
    doc: &DiffDoc,
    index: impl FnOnce() -> usize,
    stamp: &mut Option<Stamp>,
) -> bool {
    let session = &doc.edit;
    let Some(read) = *stamp else {
        let index = index();
        *stamp = Some(Stamp {
            reshapes: doc.reshapes,
            reads: session.reads,
            damage: row_doc(world, doc, index)
                .map_or(0, |(_, editor, _)| editor.damage().seq()),
        });
        return true;
    };
    if read.reshapes == doc.reshapes && read.reads == session.reads {
        return false;
    }
    let index = index();
    let found = row_doc(world, doc, index);
    *stamp = Some(Stamp {
        reshapes: doc.reshapes,
        reads: session.reads,
        damage: found
            .as_ref()
            .map_or(0, |(_, editor, _)| editor.damage().seq()),
    });
    if read.reshapes != doc.reshapes
        || read.reads < session.reads_last_frame
    {
        return true;
    }
    let Some((file, editor, row)) = found else {
        let Some(DiffRow::Header { file }) = doc.row(index) else {
            return false;
        };
        return session.redrawn.iter().any(|redraw| {
            redraw.reads > read.reads && redraw.file == file
        });
    };
    let at = (index - doc.body_start(file)) as u32;
    let redrawn = session.redrawn.iter().any(|redraw| {
        redraw.reads > read.reads
            && redraw.file == file
            && redraw.rows.contains(&at)
    });
    redrawn
        || row.new_line().is_some_and(|line| {
            editor.damage().fires(line.index, read.damage)
        })
}

fn row_doc<'w>(
    world: &'w World,
    doc: &DiffDoc,
    index: usize,
) -> Option<(usize, &'w EditDoc, Row)> {
    let DiffRow::Body { file, row } = doc.row(index)? else {
        return None;
    };
    let live = doc.files()[file].live.as_ref()?;
    Some((file, world.get::<EditDoc>(live.doc)?, row))
}

pub(crate) fn attach(
    view: &mut EntityWorldMut,
    colour: Color,
    advance: f32,
) {
    view.observe(pressed).observe(keep_keys);
    if let Some(mut doc) = view.get_mut::<DiffDoc>() {
        doc.edit.colour = colour;
        doc.edit.advance = advance;
    }
}

pub(crate) fn set(world: &mut World, root: Entity, on: bool) {
    let Some(mut doc) = world.get_mut::<DiffDoc>(root) else {
        return;
    };
    doc.edit.on = on;
    if on {
        begin(world, root);
    } else {
        leave(world, root);
    }
}

pub(crate) fn restart(
    world: &mut World,
    root: Entity,
    docs: Vec<Entity>,
) {
    for doc in docs {
        world.despawn(doc);
    }
    leave(world, root);
    if let Some(mut doc) = world.get_mut::<DiffDoc>(root) {
        doc.edit.damage.clear();
        doc.edit.in_sight = None;
        doc.edit.rediff = None;
        if doc.edit.on {
            begin(world, root);
        }
    }
}

pub(crate) fn drop_docs(
    mut world: DeferredWorld,
    context: HookContext,
) {
    let docs: Vec<Entity> = world
        .get::<DiffDoc>(context.entity)
        .map(|doc| {
            doc.files()
                .iter()
                .filter_map(|file| Some(file.live.as_ref()?.doc))
                .collect()
        })
        .unwrap_or_default();
    let mut commands = world.commands();
    for doc in docs {
        commands.entity(doc).despawn();
    }
}

fn editable(diff: &FileDiff) -> bool {
    !diff.is_partial()
        && !matches!(
            diff.change(),
            FileChange::Deleted | FileChange::Binary
        )
}

fn begin(world: &mut World, root: Entity) {
    let Some(doc) = world.get::<DiffDoc>(root) else {
        return;
    };
    let starts: Vec<(usize, FileDiff, EditDoc)> =
        (0..doc.files().len())
            .filter(|&file| doc.files()[file].live.is_none())
            .filter_map(|file| {
                let (diff, lang) = doc.source_diff(file);
                editable(diff).then(|| {
                    let text = diff.new_lines().text();
                    (file, diff.clone(), EditDoc::new(lang, text))
                })
            })
            .collect();
    let bar = doc.edit.bar.is_none().then_some(doc.edit.colour);
    for (file, diff, editor) in starts {
        let doc = world.spawn((editor, LiveFile)).id();
        if let Some(mut view) = world.get_mut::<DiffDoc>(root) {
            view.bypass_change_detection().file_mut(file).live =
                Some(Live { diff, doc });
        }
    }
    if let Some(colour) = bar {
        let bar = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(CARET_WIDTH),
                    ..Node::default()
                },
                BackgroundColor(colour),
                Pickable::IGNORE,
                ZIndex(1),
                Visibility::Hidden,
                DiffCaret,
                ChildOf(root),
            ))
            .id();
        if let Some(mut view) = world.get_mut::<DiffDoc>(root) {
            view.bypass_change_detection().edit.bar = Some(bar);
        }
    }
}

fn leave(world: &mut World, root: Entity) {
    let Some(mut doc) = world.get_mut::<DiffDoc>(root) else {
        return;
    };
    let bar = doc.edit.bar;
    doc.edit.drag = None;
    if doc.edit.active.take().is_some() {
        world.entity_mut(root).insert(Selects);
    }
    if let Some(mut visibility) =
        bar.and_then(|bar| world.get_mut::<Visibility>(bar))
    {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

fn pressed(press: On<PointerPress>, mut docs: Query<&mut DiffDoc>) {
    if press.button != PointerButton::Primary {
        return;
    }
    let Ok(mut doc) = docs.get_mut(press.entity) else {
        return;
    };
    if !doc.edit.on {
        return;
    }
    doc.bypass_change_detection().edit.pressed = Some(Press {
        at: press.pointer.position,
        count: press.count,
        pointer: press.pointer.id,
    });
}

fn keep_keys(
    mut event: On<FocusedInput<KeyboardInput>>,
    docs: Query<&DiffDoc>,
    keymap: Res<Keymap>,
    held: Option<Res<ButtonInput<KeyCode>>>,
) {
    let active = docs
        .get(event.focused_entity)
        .is_ok_and(|doc| doc.edit.active.is_some());
    let press = KeyPress {
        input: event.input.clone(),
        mods: Mods::from_held(held.as_deref()),
        focus: Some(event.focused_entity),
    };
    if active && keymap.resolve(&press).is_some() {
        event.propagate(false);
    }
}

fn new_line(
    doc: &DiffDoc,
    index: usize,
    column: Column,
) -> Option<(usize, u32)> {
    if doc.is_compact() {
        return None;
    }
    let DiffRow::Body { file, row } = doc.row(index)? else {
        return None;
    };
    doc.files()[file].live.as_ref()?;
    let line = match column {
        Column::New => row.new_line()?,
        Column::Only => match stacked_line(row)? {
            (Lane::New, line, _) => line,
            (Lane::Old, ..) => return None,
        },
        Column::Old => return None,
    };
    Some((file, line.index))
}

pub(crate) fn keys(
    keys: Res<Keys>,
    keymap: Res<Keymap>,
    mut views: Query<(
        Entity,
        &mut DiffDoc,
        &mut CodeSelection<Spot>,
    )>,
    docs: Query<&EditDoc>,
    (hits, pointers): (Hits<DiffDoc>, Pointers),
    mut requests: MessageWriter<EditRequest>,
    mut commands: Commands,
) {
    for (root, mut doc, mut selection) in &mut views {
        let doc = doc.bypass_change_detection();
        if let Some(press) = doc.edit.pressed.take() {
            let over = doc
                .list()
                .and_then(|list| hits.over(list, press.at))
                .filter(|over| !over.bar);
            let placed = over.and_then(|over| {
                let column = doc.column(over.across);
                let (file, _) = new_line(doc, over.row, column)?;
                let live = doc.files()[file].live.as_ref()?;
                let editor = docs.get(live.doc).ok()?.editor();
                let from = hit(
                    doc,
                    &hits,
                    root,
                    file,
                    column,
                    editor.buffer(),
                    &over,
                );
                let base = match doc.edit.active {
                    Some(active) if active == file => {
                        editor.selections().clone()
                    }
                    _ => {
                        Selections::new(Selection::caret(from.offset))
                    }
                };
                Some((
                    live.doc,
                    Drag {
                        pointer: press.pointer,
                        file,
                        column,
                        gesture: keymap
                            .gesture(keys.for_press(), press.count),
                        from,
                        to: from,
                        base,
                    },
                ))
            });
            match placed {
                Some((entity, drag)) => {
                    requests.write(stroke(entity, doc, &drag));
                    selection.clear();
                    if doc.edit.active.replace(drag.file).is_none() {
                        commands.entity(root).remove::<Selects>();
                    }
                    doc.edit.drag = Some(drag);
                }
                None => {
                    doc.edit.drag = None;
                    if doc.edit.active.take().is_some() {
                        commands.entity(root).insert(Selects);
                    }
                }
            }
        } else if let Some(mut drag) = doc.edit.drag.take()
            && let Some(at) = select::held(&pointers, drag.pointer)
        {
            follow(
                root,
                doc,
                &mut drag,
                at,
                &hits,
                &docs,
                &mut requests,
                &mut commands,
            );
            doc.edit.drag = Some(drag);
        }
        let Some(file) = doc.edit.active.filter(|_| doc.edit.on)
        else {
            continue;
        };
        let Some(live) = doc.files()[file].live.as_ref() else {
            continue;
        };
        let mut layout = None;
        for press in keys.presses() {
            if press.focus != Some(root) {
                continue;
            }
            let Some(act) = keymap.resolve(press).map(Act::from)
            else {
                continue;
            };
            let layout = layout
                .get_or_insert_with(|| DiffLayout::of(doc, file))
                .clone();
            requests.write(EditRequest::new(
                live.doc,
                Request::new(move |tx| {
                    tx.lay_out(layout.clone());
                    dispatch(tx, &act);
                }),
            ));
        }
    }
}

/// Runs `drag` to the pointer at `at`, and steps the list past an
/// edge the pointer is held beyond.
#[expect(clippy::too_many_arguments, reason = "one system's params")]
fn follow(
    root: Entity,
    doc: &DiffDoc,
    drag: &mut Drag,
    at: Vec2,
    hits: &Hits<DiffDoc>,
    docs: &Query<&EditDoc>,
    requests: &mut MessageWriter<EditRequest>,
    commands: &mut Commands,
) {
    let Some(list) = doc.list() else {
        return;
    };
    let Some(over) = hits.over(list, at) else {
        return;
    };
    let Some(live) = doc.files()[drag.file].live.as_ref() else {
        return;
    };
    let Ok(editor) = docs.get(live.doc) else {
        return;
    };
    let buffer = editor.editor().buffer();
    let to =
        hit(doc, hits, root, drag.file, drag.column, buffer, &over);
    if to != drag.to {
        drag.to = to;
        requests.write(stroke(live.doc, doc, drag));
    }
    if over.past != 0.0 {
        commands.queue(move |world: &mut World| {
            if let Some(mut list) = world.get_mut::<VirtualList>(list)
            {
                select::reveal_past(&mut list, &over);
            }
        });
    }
}

/// The request that runs `drag`'s gesture to its pointer with the
/// file's rows as the layout. A selection a gesture puts wholly on a
/// line no row shows, as a column drag across a gap does, is dropped.
fn stroke(entity: Entity, doc: &DiffDoc, drag: &Drag) -> EditRequest {
    let layout = DiffLayout::of(doc, drag.file);
    let (gesture, from, to) = (drag.gesture, drag.from, drag.to);
    let base = drag.base.clone();
    EditRequest::new(
        entity,
        Request::new(move |tx| {
            tx.lay_out(layout.clone());
            tx.stroke(gesture, from, to, &base);
            let editor = tx.editor();
            let buffer = editor.buffer();
            let mut kept = editor.selections().clone();
            let unseen: Vec<usize> = kept
                .iter()
                .enumerate()
                .filter(|(_, selection)| {
                    let line = buffer.line_of(selection.head);
                    line == buffer.line_of(selection.anchor)
                        && !layout.shows(line)
                })
                .map(|(index, _)| index)
                .collect();
            if unseen.is_empty() || unseen.len() == kept.len() {
                return;
            }
            for index in unseen.into_iter().rev() {
                kept.remove(index);
            }
            tx.select(kept);
        }),
    )
}

/// What the pointer over `over` presses on file `file`'s new side in
/// `column`. A row that shows no new line presses the start of the
/// new line where it sits, and a row outside the file the file's
/// start or end.
fn hit(
    doc: &DiffDoc,
    hits: &Hits<DiffDoc>,
    root: Entity,
    file: usize,
    column: Column,
    buffer: &Buffer,
    over: &Over,
) -> Hit {
    let rows = &doc.files()[file].rows;
    let last = buffer.line_count() - 1;
    let (offset, pointed) = match over
        .row
        .checked_sub(doc.body_start(file))
    {
        None => (0, None),
        Some(at) if at as u32 >= rows.len() => (buffer.len(), None),
        Some(at) => {
            let line = rows
                .new_at(at as u32)
                .map_or(last, |line| line.min(last));
            let start = buffer.line_start(line);
            let len = buffer.line_end(line) - start;
            let pointed =
                new_line(doc, over.row, column).and_then(|_| {
                    hits.point(
                        root,
                        doc,
                        column,
                        over.row,
                        over.inside,
                    )
                });
            let byte = pointed.map_or(0, |(byte, _)| byte.min(len));
            (start + byte, pointed.map(|(_, x)| x))
        }
    };
    let x = Monospace::default().x(buffer, offset);
    let pointer =
        pointed.map_or(x, |x| x / doc.edit.advance.max(1.0));
    Hit {
        line: buffer.line_of(offset),
        offset,
        x,
        pointer,
    }
}

pub(crate) fn apply(
    world: &mut World,
    mut cursor: Local<MessageCursor<EditorChanged>>,
) {
    let changes: Vec<EditorChanged> = world
        .get_resource::<Messages<EditorChanged>>()
        .map(|messages| cursor.read(messages).cloned().collect())
        .unwrap_or_default();
    let roots: Vec<Entity> = world
        .query_filtered::<Entity, With<DiffDoc>>()
        .iter(world)
        .collect();
    for root in roots {
        let Some(mut doc) = world.get_mut::<DiffDoc>(root) else {
            continue;
        };
        {
            let session = &mut doc.bypass_change_detection().edit;
            session.reads_last_frame = session.reads_this_frame;
            session.reads_this_frame = session.reads;
            let since = session.reads_last_frame;
            session.redrawn.retain(|redraw| redraw.reads > since);
        }
        for change in &changes {
            let file = world.get::<DiffDoc>(root).and_then(|doc| {
                doc.files().iter().position(|file| {
                    file.live
                        .as_ref()
                        .is_some_and(|live| live.doc == change.doc)
                })
            });
            if let Some(file) = file {
                rediff(world, root, file, change);
            }
        }
        catch_up(world, root);
        reveal(world, root);
    }
}

fn rediff(
    world: &mut World,
    root: Entity,
    file: usize,
    change: &EditorChanged,
) {
    let Some(doc) = world.get::<DiffDoc>(root) else {
        return;
    };
    let Some(live) = doc.files()[file].live.as_ref() else {
        return;
    };
    let Some(editor) = world.get::<EditDoc>(live.doc) else {
        return;
    };
    let buffer = editor.editor().buffer();
    let edits: Vec<(Range<u32>, String)> = change
        .change
        .line_edits()
        .map(|edit| {
            let start = buffer.line_start(edit.first);
            let next = edit.first + edit.inserted;
            let end = if next < buffer.line_count() {
                buffer.line_start(next)
            } else {
                buffer.len()
            };
            let text = buffer.slice(start..end).into_owned();
            (edit.first..edit.first + edit.removed, text)
        })
        .collect();
    let list = doc.list();
    let body = doc.body_start(file);
    let collapsed = doc.is_collapsed(file);
    let anchor = edits.first().map(|(lines, _)| lines.start);
    let kept = anchor.and_then(|line| {
        let row = doc.files()[file].rows.find(Anchor::New(line))?;
        screen_top(world, list?, body + row as usize)
    });

    let mut doc = world.get_mut::<DiffDoc>(root).expect("the view");
    let doc = doc.as_mut();
    let started = Instant::now();
    let mut splices = Vec::with_capacity(edits.len());
    let mut redrawn: Option<Range<u32>> = None;
    {
        let shown = doc.file_mut(file);
        let live = shown.live.as_mut().expect("a live file");
        let width = number_width(&live.diff);
        for (lines, text) in &edits {
            let len = live.diff.new_lines().len();
            let lines = lines.start.min(len)..lines.end.min(len);
            let rediff = live.diff.edit_new(
                lines.clone(),
                text,
                CONTEXT,
                &mut shown.expansion,
            );
            let splice = shown.rows.patch(
                &live.diff,
                &shown.expansion,
                &rediff,
            );
            let rows = changed_rows(&shown.rows, &rediff, splice);
            redrawn = Some(redrawn.map_or(rows.clone(), |seen| {
                seen.start.min(rows.start)..u32::MAX
            }));
            splices.push(splice);
        }
        if number_width(&live.diff) != width {
            redrawn = Some(0..u32::MAX);
        }
    }
    let took = started.elapsed();
    let session = &mut doc.edit;
    session.reads += 1;
    session.rediff = Some((file, took));
    if let Some(rows) = redrawn.filter(|rows| !rows.is_empty()) {
        session.redrawn.push(Redraw {
            reads: session.reads,
            file,
            rows,
        });
    }
    let moved = splices
        .iter()
        .any(|splice| splice.removed + splice.inserted > 0);
    let after = anchor.and_then(|line| {
        doc.files()[file].rows.find(Anchor::New(line))
    });
    doc.place_files();
    let Some(list) = list.filter(|_| !collapsed && moved) else {
        return;
    };
    if let Some(mut rows) = world.get_mut::<VirtualList>(list) {
        for splice in &splices {
            rows.splice(
                body + splice.at as usize,
                splice.removed as usize,
                splice.inserted as usize,
            );
        }
    }
    if let (Some(top), Some(row)) = (kept, after) {
        keep_top(world, list, body + row as usize, top);
    }
}

fn changed_rows(
    rows: &Rows,
    rediff: &Rediff,
    splice: RowSplice,
) -> Range<u32> {
    let hunks =
        rediff.hunks.start..rediff.hunks.start + rediff.new_hunks;
    let mut changed = rebuilt(splice);
    for hunk in [hunks.start, hunks.end.saturating_sub(1)] {
        if hunks.contains(&hunk) {
            let of = rows.hunk_rows(hunk);
            changed =
                changed.start.min(of.start)..changed.end.max(of.end);
        }
    }
    if rediff.inserted != rediff.lines.len() as u32 {
        changed.end = u32::MAX;
    }
    changed
}

/// The rows whose slots can show a removed row after the list takes
/// `splice`: the inserted rows, and every index a freed slot of a
/// removed row can land on again without reading as moved.
fn rebuilt(splice: RowSplice) -> Range<u32> {
    splice.at..splice.at + splice.inserted.max(splice.removed)
}

fn screen_top(
    world: &World,
    list: Entity,
    index: usize,
) -> Option<f32> {
    let rows = world.get::<VirtualList>(list)?;
    let scroll = world.get::<ScrollPosition>(list)?.y;
    let node = world.get::<ComputedNode>(list)?;
    let height =
        node.content_box().size().y * node.inverse_scale_factor();
    let span = rows.row_span(index)?;
    let top = span.top - scroll;
    (top + span.height > 0.0 && top < height).then_some(top)
}

fn keep_top(world: &mut World, list: Entity, index: usize, top: f32) {
    let Some(span) = world
        .get::<VirtualList>(list)
        .and_then(|rows| rows.row_span(index))
    else {
        return;
    };
    if let Some(mut scroll) = world.get_mut::<ScrollPosition>(list) {
        let y = (span.top - top).max(0.0);
        if scroll.y != y {
            scroll.y = y;
        }
    }
}

fn catch_up(world: &mut World, root: Entity) {
    let Some(doc) = world.get::<DiffDoc>(root) else {
        return;
    };
    let first_list_row = doc
        .list()
        .and_then(|list| world.get::<VirtualList>(list))
        .map_or(0, VirtualList::first_visible);
    let active = doc.edit.active.filter(|_| doc.edit.on);
    let docs: Vec<(Entity, u32, bool)> = (0..doc.files().len())
        .filter_map(|file| {
            let live = doc.files()[file].live.as_ref()?;
            Some((
                live.doc,
                last_shown_line(doc, file, first_list_row),
                active == Some(file),
            ))
        })
        .collect();
    let mut moved = false;
    let mut seen = Vec::with_capacity(docs.len());
    for (entity, last, active) in docs {
        let Some(mut editor) = world.get_mut::<EditDoc>(entity)
        else {
            continue;
        };
        let buffer = editor.editor().buffer();
        let caret = buffer
            .line_of(editor.editor().selections().primary().head);
        let mut window = catch_up_window(last, caret);
        if active {
            window = window.max(caret.saturating_add(SCAN_LINES));
        }
        editor.bypass_change_detection().catch_up(window);
        seen.push((entity, editor.damage().seq()));
        if active {
            match_brackets(world, entity);
        }
    }
    let Some(mut doc) = world.get_mut::<DiffDoc>(root) else {
        return;
    };
    let session = &mut doc.bypass_change_detection().edit;
    for &(entity, seq) in &seen {
        let was =
            session.damage.iter().find(|(doc, _)| *doc == entity);
        moved |= was.is_some_and(|&(_, was)| was != seq);
    }
    session.damage = seen;
    if moved {
        session.reads += 1;
    }
}

/// Finds the bracket pair at the caret of the doc on `entity`, which
/// the caller lexed through the window the match scans.
fn match_brackets(world: &mut World, entity: Entity) {
    let Some(editor) = world.get::<EditDoc>(entity) else {
        return;
    };
    let Some(fresh) =
        refreshed(editor, world.get::<BracketMatch>(entity))
    else {
        return;
    };
    match world.get_mut::<BracketMatch>(entity) {
        Some(held) => keep(held, fresh),
        None => {
            world.entity_mut(entity).insert(fresh);
        }
    }
}

fn last_shown_line(
    doc: &DiffDoc,
    file: usize,
    list_row: usize,
) -> u32 {
    if doc.is_collapsed(file) {
        return 0;
    }
    let at = list_row.saturating_sub(doc.body_start(file)) as u32;
    doc.files()[file]
        .rows
        .range(at..at.saturating_add(2 * BUILT_MAX))
        .filter_map(|row| row.new_line())
        .last()
        .map_or(0, |line| line.index)
}

fn reveal(world: &mut World, root: Entity) {
    let Some(doc) = world.get::<DiffDoc>(root) else {
        return;
    };
    let Some(file) = doc.edit.active else {
        return;
    };
    let Some(live) = doc.files()[file].live.as_ref() else {
        return;
    };
    let Some(editor) = world.get::<EditDoc>(live.doc) else {
        return;
    };
    let head = editor.editor().selections().primary().head;
    if doc.edit.in_sight == Some((file, head)) {
        return;
    }
    let line = editor.editor().buffer().line_of(head);
    let row = doc.files()[file]
        .rows
        .find(Anchor::New(line))
        .map(|row| doc.body_start(file) + row as usize);
    let list = doc.list();
    if let Some(mut doc) = world.get_mut::<DiffDoc>(root) {
        doc.bypass_change_detection().edit.in_sight =
            Some((file, head));
    }
    let (Some(row), Some(list)) = (row, list) else {
        return;
    };
    if screen_top(world, list, row).is_some() {
        return;
    }
    if let Some(mut rows) = world.get_mut::<VirtualList>(list) {
        rows.scroll_into_view(row);
    }
}

type Texts<'a> = (
    &'a DiffLine,
    &'a ComputedNode,
    &'a UiGlobalTransform,
    &'a ComputedTextBlock,
    &'a TextLayout,
    Option<&'a CalculatedClip>,
);

type Bar<'a> =
    (&'a mut Node, &'a mut UiGlobalTransform, &'a mut Visibility);

pub(crate) fn draw(
    views: Query<(
        Entity,
        &DiffDoc,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    docs: Query<&EditDoc>,
    texts: Query<Texts, Without<DiffCaret>>,
    mut bars: Query<Bar, (With<DiffCaret>, Without<DiffDoc>)>,
    focus: Option<Res<InputFocus>>,
) {
    let focused = focus.and_then(|focus| focus.get());
    for (root, doc, root_node, root_at) in &views {
        let Some(bar) = doc.edit.bar else {
            continue;
        };
        let Ok((mut node, mut transform, mut visibility)) =
            bars.get_mut(bar)
        else {
            continue;
        };
        let placed = (focused == Some(root))
            .then(|| caret(doc, &docs, &texts, root))
            .flatten();
        let Some((corner, size)) = placed else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let logical = root_node.inverse_scale_factor();
        let origin = root_at.translation + root_node.border_box().min;
        let to = Node {
            position_type: PositionType::Absolute,
            left: px((corner.x - origin.x) * logical),
            top: px((corner.y - origin.y) * logical),
            width: px(size.x * logical),
            height: px(size.y * logical),
            ..Node::default()
        };
        node.set_if_neq(to);
        *transform =
            UiGlobalTransform::from_translation(corner + size / 2.0);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

fn caret(
    doc: &DiffDoc,
    docs: &Query<&EditDoc>,
    texts: &Query<Texts, Without<DiffCaret>>,
    root: Entity,
) -> Option<(Vec2, Vec2)> {
    let file = doc.edit.active.filter(|_| doc.edit.on)?;
    let shown = &doc.files()[file];
    let editor = docs.get(shown.live.as_ref()?.doc).ok()?.editor();
    let buffer = editor.buffer();
    let head = editor.selections().primary().head;
    let line = buffer.line_of(head);
    let index = doc.body_start(file)
        + shown.rows.find(Anchor::New(line))? as usize;
    let column = match doc.layout() {
        Layout::Stacked => Column::Only,
        Layout::Split => Column::New,
    };
    let (_, node, at, block, layout, clip) =
        texts.iter().find(|(tag, ..)| {
            (tag.view, tag.index, tag.column) == (root, index, column)
        })?;
    let text = doc.diff(file).new_lines();
    let content = if line < text.len() {
        text.line(line)
    } else {
        ""
    };
    let byte = (head - buffer.line_start(line)).min(content.len());
    let drawn = Shown::new(content).shown(byte);
    let laid = drawn_layout(block, layout, node);
    let scale = 1.0 / node.inverse_scale_factor();
    let bbox =
        Cursor::from_byte_index(&laid, drawn, Affinity::Downstream)
            .geometry(&laid, CARET_WIDTH * scale);
    let corner = (at.translation
        + node.content_box().min
        + Vec2::new(bbox.x0 as f32, bbox.y0 as f32))
    .round();
    let size =
        Vec2::new(CARET_WIDTH * scale, (bbox.y1 - bbox.y0) as f32)
            .round();
    let middle = corner + Vec2::new(0.5, size.y / 2.0);
    if clip.is_some_and(|clip| !clip.contains_point(middle)) {
        return None;
    }
    Some((corner, size))
}

/// The rects a live file's new-side rows draw under their text:
/// the editor's selections, its carets but the primary, which the bar
/// draws, and the bracket pair at the primary caret.
#[derive(Component)]
pub(crate) struct EditTints(SelectionRects);

type Tinted<'a> = (
    Entity,
    &'a DiffLine,
    &'a ComputedNode,
    &'a ComputedTextBlock,
    &'a TextLayout,
    &'a ChildOf,
    Option<&'a mut EditTints>,
);

type Rects<'a> = (&'a mut Node, &'a mut BackgroundColor);

pub(crate) fn tint(
    views: Query<&DiffDoc>,
    docs: Query<(&EditDoc, Option<&BracketMatch>)>,
    mut lines: Query<Tinted<'_>>,
    mut layers: Query<(&mut Node, Option<&Children>), With<Layer>>,
    mut nodes: Query<Rects<'_>, (Without<Layer>, Without<DiffCaret>)>,
    focus: Option<Res<InputFocus>>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|focus| focus.get());
    for (text, line, node, block, layout, cell, tints) in &mut lines {
        let boxes = views
            .get(line.view)
            .ok()
            .map(|doc| {
                let carets = focused == Some(line.view);
                tints_on(
                    doc, &docs, line, carets, block, layout, node,
                )
            })
            .unwrap_or_default();
        match tints {
            Some(mut tints) => select::paint(
                &boxes,
                &mut tints.0,
                cell,
                &mut layers,
                &mut nodes,
                &mut commands,
            ),
            None if boxes.is_empty() => {}
            None => {
                commands.entity(text).insert(EditTints(
                    SelectionRects::new(Color::NONE),
                ));
            }
        }
    }
}

fn tints_on(
    doc: &DiffDoc,
    docs: &Query<(&EditDoc, Option<&BracketMatch>)>,
    tag: &DiffLine,
    carets: bool,
    block: &ComputedTextBlock,
    layout: &TextLayout,
    node: &ComputedNode,
) -> Vec<([f32; 4], Color)> {
    let Some((file, line)) = new_line(doc, tag.index, tag.column)
    else {
        return Vec::new();
    };
    let Some(live) = doc.files()[file].live.as_ref() else {
        return Vec::new();
    };
    let Ok((editor, brackets)) = docs.get(live.doc) else {
        return Vec::new();
    };
    if doc.edit.active != Some(file) || !doc.edit.on {
        return Vec::new();
    }
    let editor = editor.editor();
    let buffer = editor.buffer();
    let (start, end) =
        (buffer.line_start(line), buffer.line_end(line));
    let colour = doc.edit.colour;
    let selections = editor.selections();
    let mut spans = Vec::new();
    let mut bars = Vec::new();
    for (index, selection) in selections.iter().enumerate() {
        let range = selection.range();
        if range.start < end && range.end > start {
            spans.push((
                range.start.max(start)..range.end.min(end),
                colour.with_alpha(SELECTED_ALPHA),
            ));
        }
        if carets
            && index != selections.primary_index()
            && (start..=end).contains(&selection.head)
        {
            bars.push(selection.head);
        }
    }
    let pair = brackets.and_then(BracketMatch::brackets);
    for at in pair.into_iter().flat_map(|pair| [pair.near, pair.far])
    {
        if (start..end).contains(&at) {
            spans
                .push((at..at + 1, colour.with_alpha(BRACKET_ALPHA)));
        }
    }
    if spans.is_empty() && bars.is_empty() {
        return Vec::new();
    }
    let text = doc.diff(file).new_lines();
    let content = if line < text.len() {
        text.line(line)
    } else {
        ""
    };
    let shown = Shown::new(content);
    let drawn =
        |at: usize| shown.shown((at - start).min(content.len()));
    let laid = drawn_layout(block, layout, node);
    let logical = node.inverse_scale_factor();
    let mut boxes = Vec::new();
    for (bytes, tint) in spans {
        let bytes = drawn(bytes.start)..drawn(bytes.end);
        boxes.extend(select::rects(&laid, bytes, logical, tint));
    }
    for head in bars {
        let bbox = Cursor::from_byte_index(
            &laid,
            drawn(head),
            Affinity::Downstream,
        )
        .geometry(&laid, CARET_WIDTH / logical);
        let [x0, y0, y1] =
            [bbox.x0, bbox.y0, bbox.y1].map(|v| v as f32 * logical);
        boxes.push(([x0, y0, x0 + CARET_WIDTH, y1], colour));
    }
    boxes
}

impl DiffDoc {
    /// The diff-edit state of the file whose new side holds the
    /// caret, `None` while no new-side press has placed one.
    pub fn edit_status(
        &self,
        world: &World,
    ) -> Option<DiffEditStatus> {
        let file = self.edit.active?;
        let shown = &self.files()[file];
        let editor =
            world.get::<EditDoc>(shown.live.as_ref()?.doc)?.editor();
        let diff = self.diff(file);
        let line = editor
            .buffer()
            .line_of(editor.selections().primary().head);
        let row = shown
            .rows
            .find(Anchor::New(line))
            .and_then(|row| shown.rows.get(row));
        let marks = row
            .filter(|row| {
                row.new_line().is_some_and(|new| new.index == line)
            })
            .and_then(|row| row.words(diff))
            .map_or_else(Vec::new, |(_, new)| new);
        let hunk = diff.hunks().iter().find(|hunk| {
            let start = hunk
                .blocks()
                .first()
                .map_or(0, |block| block.new_range().start);
            (start..start + hunk.new_len().max(1)).contains(&line)
        });
        Some(DiffEditStatus {
            file,
            hunks: diff.hunks().len(),
            undo: editor.history().undo_depth(),
            rediff: self
                .edit
                .rediff
                .filter(|&(of, _)| of == file)
                .map(|(_, took)| took),
            marks,
            hunk: hunk.map(|hunk| hunk.header().to_string()),
            line,
        })
    }

    /// Whether a press on a new-side line edits it.
    pub fn is_editing(&self) -> bool {
        self.edit.on
    }

    /// The doc of the file whose new side holds the caret, `None`
    /// while no new-side press has placed one.
    pub fn edit_doc(&self) -> Option<Entity> {
        let file = self.edit.active?;
        Some(self.files()[file].live.as_ref()?.doc)
    }
}
