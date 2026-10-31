//! Editing the new side of a diff view.
//!
//! Each file a [`diff_view`](crate::diff_view) can edit gets an
//! [`EditDoc`] of its own when editing turns on, so undo is per file.
//! A press on a new-side line places that doc's caret, and keys
//! reach the doc as [`EditRequest`]s, through the same transaction a
//! code editor's keys take. After that transaction, each change's
//! line edits re-diff only the hunks within reach
//! ([`FileDiff::edit_new`]), patch the file's rows in place
//! ([`Rows::patch`]) and splice the list, in the same `PreUpdate`.

use core::ops::Range;
use core::time::Duration;
use std::time::Instant;

use bevy::app::{App, PostUpdate, PreUpdate};
use bevy::camera::Camera;
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::message::{MessageCursor, MessageWriter, Messages};
use bevy::ecs::observer::On;
use bevy::ecs::query::{With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::ecs::system::{Commands, Local, Query, Res};
use bevy::ecs::world::{DeferredWorld, EntityWorldMut, World};
use bevy::input::ButtonInput;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::math::Vec2;
use bevy::picking::events::PointerPress;
use bevy::picking::pointer::PointerButton;
use bevy::picking::{Pickable, PickingSystems};
use bevy::text::{ComputedTextBlock, TextLayout};
use bevy::ui::{
    BackgroundColor, CalculatedClip, ComputedNode, Node,
    PositionType, ScrollPosition, UiGlobalTransform, UiSystems,
    ZIndex, px,
};
use bevy_fynix::views::{VirtualList, drawn_layout, rich_point};
use parley::{Affinity, Cursor};
use rubrix_diff::{Anchor, FileChange, FileDiff, Layout, Row};
use rubrix_edit::{Command, Selection, Selections};

use crate::damage::BUILT_MAX;
use crate::diff_view::{
    CONTEXT, DiffDoc, DiffLine, DiffRow, Lane, Live, Spot,
    stacked_line,
};
use crate::editor::{
    EditDoc, EditRequest, EditSystems, EditorChanged, Request,
};
use crate::frame::{copy, paste};
use crate::keymap::{Action, KeyPress, Keymap, Keys, Mods, Resolved};
use crate::rows::Shown;
use crate::select::{CodeSelection, Column, Selectable, Selects};

const CARET_WIDTH: f32 = 2.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        (
            keys.after(PickingSystems::Last)
                .after(EditSystems::Keys)
                .before(EditSystems::Apply),
            apply.after(EditSystems::Apply),
        )
            .run_if(any_with_component::<DiffDoc>),
    )
    .add_systems(
        PostUpdate,
        draw.after(UiSystems::PostLayout)
            .run_if(any_with_component::<DiffDoc>),
    );
}

/// A view's editing state, inside its [`DiffDoc`].
#[derive(Default)]
pub(crate) struct Session {
    on: bool,
    /// The file whose new side holds the caret, while a new-side
    /// press put it there.
    pub(crate) active: Option<usize>,
    /// Where the last press landed across the list, 0 to 1, until
    /// the frame reads it after the read-only press has run.
    pressed: Option<f32>,
    /// Counts the edits and catch-ups the rows read again for.
    seq: u64,
    /// `seq` as this frame began, and as the frame before it began.
    began: u64,
    since: u64,
    /// Rows of a file from a row on whose line numbers an edit moved
    /// without moving their slots: `(seq, file, first row)`.
    renumbered: Vec<(u64, usize, u32)>,
    /// Each live doc's damage generation the rows last saw.
    damage: Vec<(Entity, u64)>,
    /// The primary caret the last frame kept in sight.
    head: Option<(usize, usize)>,
    /// The file and the time of the last re-diff.
    rediff: Option<(usize, Duration)>,
    bar: Option<Entity>,
}

/// What a row read last, for [`fires`].
#[derive(Clone, Copy)]
pub(crate) struct Stamp {
    shape: u64,
    seq: u64,
    damage: u64,
}

/// The caret bar of a diff view.
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

/// Whether a row reads again: on its first read, when the rows
/// changed shape, when it skipped a frame of edits, when an edit
/// renumbered it, or when its file's doc damaged its new line.
pub(crate) fn fires(
    world: &World,
    doc: &DiffDoc,
    index: usize,
    stamp: &mut Option<Stamp>,
) -> bool {
    let session = &doc.edit;
    let Some(read) = *stamp else {
        *stamp = Some(Stamp {
            shape: doc.shape,
            seq: session.seq,
            damage: row_doc(world, doc, index)
                .map_or(0, |(_, editor, _)| editor.damage().seq()),
        });
        return true;
    };
    if read.shape == doc.shape && read.seq == session.seq {
        return false;
    }
    let found = row_doc(world, doc, index);
    *stamp = Some(Stamp {
        shape: doc.shape,
        seq: session.seq,
        damage: found
            .as_ref()
            .map_or(0, |(_, editor, _)| editor.damage().seq()),
    });
    if read.shape != doc.shape || read.seq < session.since {
        return true;
    }
    let Some((file, editor, row)) = found else {
        return false;
    };
    let at = (index - doc.body_start(file)) as u32;
    let renumbered =
        session.renumbered.iter().any(|&(seq, of, from)| {
            seq > read.seq && of == file && at >= from
        });
    renumbered
        || row.new_line().is_some_and(|line| {
            editor.damage().fires(line.index, read.damage)
        })
}

/// The edited file a row belongs to, its doc and the row.
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

/// Watches a diff view's presses and keys, and gives it a caret bar.
pub(crate) fn attach(view: &mut EntityWorldMut, colour: Color) {
    let root = view.id();
    view.observe(pressed).observe(keep_keys);
    let bar = view.world_scope(|world| {
        world
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
            .id()
    });
    if let Some(mut doc) = view.get_mut::<DiffDoc>() {
        doc.edit.bar = Some(bar);
    }
}

/// Turns editing on or off. Edits made so far stay shown either way.
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

/// Despawns the docs of a source that was replaced, and gives the
/// new source's files docs while editing is on.
pub(crate) fn restart(
    world: &mut World,
    root: Entity,
    docs: Vec<Entity>,
) {
    for doc in docs {
        world.despawn(doc);
    }
    if let Some(mut doc) = world.get_mut::<DiffDoc>(root) {
        doc.edit.damage.clear();
        doc.edit.head = None;
        doc.edit.rediff = None;
        if doc.edit.on {
            begin(world, root);
        }
    }
}

/// Despawns a removed view's docs.
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

/// Whether file `file`'s new side can be edited: a full diff, as a
/// pair of texts gives, whose new side was not deleted.
fn editable(diff: &FileDiff) -> bool {
    !diff.is_partial()
        && !matches!(
            diff.change(),
            FileChange::Deleted | FileChange::Binary
        )
}

/// Gives each editable file without one a doc of its own, holding the
/// source's new side.
fn begin(world: &mut World, root: Entity) {
    let Some(doc) = world.get::<DiffDoc>(root) else {
        return;
    };
    let starts: Vec<(usize, FileDiff, EditDoc)> =
        (0..doc.files().len())
            .filter(|&file| doc.files()[file].live.is_none())
            .filter_map(|file| {
                let (diff, lang) = doc.source_new(file);
                editable(diff).then(|| {
                    let text = diff.new_lines().text();
                    (file, diff.clone(), EditDoc::new(lang, text))
                })
            })
            .collect();
    for (file, diff, editor) in starts {
        let doc = world.spawn(editor).id();
        if let Some(mut view) = world.get_mut::<DiffDoc>(root) {
            view.bypass_change_detection().file_mut(file).live =
                Some(Live { diff, doc });
        }
    }
}

/// Gives the read-only selection its keys back.
fn leave(world: &mut World, root: Entity) {
    let Some(mut doc) = world.get_mut::<DiffDoc>(root) else {
        return;
    };
    if doc.edit.active.take().is_some() {
        world.entity_mut(root).insert(Selects);
    }
}

/// Notes a primary press for [`keys`] to read once the read-only
/// press has picked its row and byte.
fn pressed(
    press: On<PointerPress>,
    mut docs: Query<&mut DiffDoc>,
    lists: Query<(&ComputedNode, &UiGlobalTransform)>,
    cameras: Query<&Camera>,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let Ok(mut doc) = docs.get_mut(press.entity) else {
        return;
    };
    if !doc.edit.on {
        return;
    }
    let across = doc.list().and_then(|list| {
        let (node, transform) = lists.get(list).ok()?;
        let camera = cameras.iter().next()?;
        let local = rich_point(
            node,
            transform,
            camera,
            press.pointer.position,
        )?;
        let width =
            (node.content_box().size() - node.scrollbar_size).x;
        Some(local.x / width.max(1.0))
    });
    doc.bypass_change_detection().edit.pressed = across;
}

/// Keeps the keys an active diff editor takes from bubbling to the
/// window's shortcuts.
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

/// The new line a press on row `index` in `column` lands on, and its
/// file, when that file is being edited.
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

/// Turns presses on new-side lines into carets, and keys sent to a
/// view whose new side holds the caret into requests, before the
/// editor's frame applies them.
pub(crate) fn keys(
    keys: Res<Keys>,
    keymap: Res<Keymap>,
    mut views: Query<(
        Entity,
        &mut DiffDoc,
        &mut CodeSelection<Spot>,
    )>,
    docs: Query<&EditDoc>,
    mut requests: MessageWriter<EditRequest>,
    mut commands: Commands,
) {
    for (root, mut doc, mut selection) in &mut views {
        let doc = doc.bypass_change_detection();
        if let Some(across) = doc.edit.pressed.take() {
            let range = selection.anchor();
            let column = doc.column(across);
            let placed = new_line(doc, range.start.row, column)
                .filter(|_| range.start.row == range.end.row);
            match placed {
                Some((file, line)) => {
                    let Some(entity) = doc.files()[file]
                        .live
                        .as_ref()
                        .map(|live| live.doc)
                    else {
                        continue;
                    };
                    let Ok(editor) = docs.get(entity) else {
                        continue;
                    };
                    let buffer = editor.editor().buffer();
                    let start = buffer.line_start(line);
                    let len = buffer.line_end(line) - start;
                    let at = |spot: Spot| start + spot.byte.min(len);
                    let caret = Selection {
                        anchor: at(range.start),
                        head: at(range.end),
                    };
                    requests.write(EditRequest::new(
                        entity,
                        Request::select(Selections::new(caret)),
                    ));
                    selection.clear();
                    if doc.edit.active.replace(file).is_none() {
                        commands.entity(root).remove::<Selects>();
                    }
                }
                None => {
                    if doc.edit.active.take().is_some() {
                        commands.entity(root).insert(Selects);
                    }
                }
            }
        }
        let Some(file) = doc.edit.active.filter(|_| doc.edit.on)
        else {
            continue;
        };
        let Some(live) = doc.files()[file].live.as_ref() else {
            continue;
        };
        for press in keys.presses() {
            if press.focus != Some(root) {
                continue;
            }
            let request = match keymap.resolve(press) {
                Some(Resolved::Text(text)) => Request::typed(text),
                Some(Resolved::Action(action)) => request(action),
                None => continue,
            };
            requests.write(EditRequest::new(live.doc, request));
        }
    }
}

/// A bound key's action through the doc's transaction, as the code
/// editor runs it.
fn request(action: Action) -> Request {
    match action {
        Action::Command(command) => Request::command(command),
        Action::Run(run) => Request::new(run),
        Action::Copy => Request::new(copy),
        Action::Cut => Request::new(|tx| {
            copy(tx);
            tx.apply(Command::Cut);
        }),
        Action::Paste => Request::new(paste),
    }
}

/// Moves each edited file's diff, rows and list through the changes
/// its doc took this frame, lexes what its rows may show, and keeps
/// its caret in sight.
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
            session.since = session.began;
            session.began = session.seq;
            let since = session.since;
            session.renumbered.retain(|&(seq, ..)| seq > since);
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

/// One change of file `file`'s doc: each line edit re-diffs, patches
/// the rows and splices the list, in order.
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
    let mut renumber = None;
    {
        let shown = doc.file_mut(file);
        let live = shown.live.as_mut().expect("a live file");
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
            if rediff.inserted != rediff.lines.len() as u32 {
                let from = shown
                    .rows
                    .find(Anchor::New(rediff.lines.start))
                    .unwrap_or(splice.at);
                renumber = Some(
                    renumber.map_or(from, |at: u32| at.min(from)),
                );
            }
            splices.push(splice);
        }
    }
    let took = started.elapsed();
    let session = &mut doc.edit;
    session.seq += 1;
    session.rediff = Some((file, took));
    if let Some(from) = renumber {
        session.renumbered.push((session.seq, file, from));
    }
    let moved = splices
        .iter()
        .any(|splice| splice.removed + splice.inserted > 0);
    let after = anchor.and_then(|line| {
        doc.files()[file].rows.find(Anchor::New(line))
    });
    doc.count();
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

/// How far below the view's top row `index` starts, when it shows.
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

/// Scrolls so that row `index` starts `top` below the view's top
/// again.
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

/// Lexes the stale lines the active file's rows may show, and has the
/// rows of every doc whose damage moved read again.
fn catch_up(world: &mut World, root: Entity) {
    let Some(doc) = world.get::<DiffDoc>(root) else {
        return;
    };
    let docs: Vec<Entity> = doc
        .files()
        .iter()
        .filter_map(|file| Some(file.live.as_ref()?.doc))
        .collect();
    let active = doc
        .edit
        .active
        .and_then(|file| Some(doc.files()[file].live.as_ref()?.doc));
    let mut moved = false;
    let mut seen = Vec::with_capacity(docs.len());
    for entity in docs {
        let Some(mut editor) = world.get_mut::<EditDoc>(entity)
        else {
            continue;
        };
        if active == Some(entity) {
            let buffer = editor.editor().buffer();
            let head = editor.editor().selections().primary().head;
            let window = buffer.line_of(head) + 2 * BUILT_MAX;
            editor.bypass_change_detection().catch_up(window);
        }
        seen.push((entity, editor.damage().seq()));
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
        session.seq += 1;
    }
}

/// Scrolls the active file's caret into sight when it moved.
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
    if doc.edit.head == Some((file, head)) {
        return;
    }
    let line = editor.editor().buffer().line_of(head);
    let row = doc.files()[file]
        .rows
        .find(Anchor::New(line))
        .map(|row| doc.body_start(file) + row as usize);
    let list = doc.list();
    if let Some(mut doc) = world.get_mut::<DiffDoc>(root) {
        doc.bypass_change_detection().edit.head = Some((file, head));
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

/// Draws each view's primary caret on its row's new-side text, while
/// the view holds the input focus and its new side holds the caret.
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

/// The physical corner and size of view `root`'s primary caret, when
/// its row's text is laid out and shows.
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
}
