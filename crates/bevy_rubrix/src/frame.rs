//! An editor's frame: one exclusive system makes every change to
//! every [`EditDoc`], each through one transaction.

use std::mem;

use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::query::Has;
use bevy::ecs::system::{
    Local, Query, Res, SystemParam, SystemState,
};
use bevy::ecs::world::{FromWorld, World};
use bevy::input_focus::InputFocus;
use bevy::log::warn;
use bevy::math::Vec2;
use bevy::time::Time;
use bevy_fynix::views::VirtualList;
use rubrix_edit::{
    Buffer, Change, Command, Editor, Layout, Monospace, Selections,
};
use rubrix_highlight::LineEdit;

use crate::caret::{Geometry, Hit, Reveal, RowParts};
use crate::clipboard;
use crate::damage::{BUILT_MAX, reach, splice_rows};
use crate::editor::{
    Applied, Composing, Drag, EditDoc, EditInput, EditLine,
    EditRequest, EditView, EditorChanged,
};
use crate::keymap::{
    Action, Gesture, Keymap, Keys, Resolved, Stroke,
};
use crate::marks::Marks;
use crate::select::{self, Over, Pointers};

const PARTS: &str = "the editor's resources, which RubrixPlugin adds";

/// One doc's transaction, for a key's [`Action::Run`] handler and an
/// [`EditRequest`]: each [`Tx::apply`] edits the
/// text, lexes it, notes the damage, splices every view of the doc
/// and writes an [`EditorChanged`], in key order.
pub struct Tx<'w> {
    world: &'w mut World,
    parts: &'w mut SystemState<Parts<'static, 'static>>,
    doc: Entity,
    view: Option<Entity>,
    layout: Option<Box<dyn Layout>>,
    acted: bool,
}

impl<'w> Tx<'w> {
    fn new(
        world: &'w mut World,
        parts: &'w mut SystemState<Parts<'static, 'static>>,
        doc: Entity,
        view: Option<Entity>,
    ) -> Tx<'w> {
        Tx {
            world,
            parts,
            doc,
            view,
            layout: None,
            acted: false,
        }
    }

    pub fn doc(&self) -> Entity {
        self.doc
    }

    /// The view the key or request came through, whose rows vertical
    /// motions use.
    pub fn view(&self) -> Option<Entity> {
        self.view
    }

    /// # Panics
    ///
    /// Panics if the doc's entity no longer holds an [`EditDoc`].
    pub fn editor(&self) -> &Editor {
        self.world
            .get::<EditDoc>(self.doc)
            .expect("the transaction's doc")
            .editor()
    }

    pub fn world(&mut self) -> &mut World {
        self.world
    }

    /// Runs `command` at every selection. Returns whether the text or
    /// the selections changed.
    pub fn apply(&mut self, command: Command<'_>) -> bool {
        self.transact(&mut |editor, layout| {
            editor.apply(command, &layout)
        })
    }

    /// Replaces the selections. Not an undo step.
    pub fn select(&mut self, selections: Selections) {
        let mut selections = Some(selections);
        self.transact(&mut |editor, _| {
            if let Some(selections) = selections.take() {
                editor.set_selections(selections);
            }
            None
        });
    }

    /// Keeps the view's primary caret in sight after this frame, as
    /// an edit does, though nothing in the doc changed.
    pub(crate) fn keep_in_sight(&mut self) {
        self.acted = true;
    }

    /// Copies `text` from the view, or the doc without one, to the
    /// app's clipboard.
    pub fn copy(&mut self, text: String) {
        self.copy_each(text, Vec::new());
    }

    /// Copies `text` as [`copy`](Self::copy) does, with the piece
    /// each selection gave it, for a paste at as many selections.
    fn copy_each(&mut self, text: String, pieces: Vec<String>) {
        clipboard::copy(
            self.world,
            self.view.unwrap_or(self.doc),
            text,
            pieces,
        );
    }

    /// Makes `layout` the row geometry of this transaction's later
    /// commands, in place of the view's, for a target that is not an
    /// [`EditView`].
    pub(crate) fn lay_out(&mut self, layout: impl Layout + 'static) {
        self.layout = Some(Box::new(layout));
    }

    /// Runs `gesture` from `from` to `to`, and sets its selections
    /// when they moved.
    pub(crate) fn stroke(
        &mut self,
        gesture: Gesture,
        from: Hit,
        to: Hit,
        base: &Selections,
    ) {
        self.transact(&mut |editor, layout| {
            let selections = (gesture.0)(&Stroke {
                buffer: editor.buffer(),
                layout,
                base,
                from,
                to,
            });
            if !selections.iter().eq(editor.selections().iter()) {
                editor.set_selections(selections);
            }
            None
        });
    }

    pub(crate) fn transact(
        &mut self,
        edit: &mut dyn FnMut(
            &mut Editor,
            &dyn Layout,
        ) -> Option<Change>,
    ) -> bool {
        let (doc, view) = (self.doc, self.view);
        let mut parts = self.parts.get_mut(self.world).expect(PARTS);
        let Parts { docs, scene } = &mut parts;
        let scene: &Scene = scene;
        let targets: Vec<Entity> = scene
            .views
            .iter()
            .filter(|(_, edit, ..)| edit.doc() == doc)
            .map(|(target, ..)| target)
            .collect();
        let Ok((_, mut held)) = docs.get_mut(doc) else {
            return false;
        };
        let before = held.editor().selections().clone();
        let rows_before = held.rows();
        let mut run = |layout: &dyn Layout| {
            held.bypass_change_detection().run(
                |editor| edit(editor, layout),
                |edits, editor| {
                    let buffer = editor.buffer();
                    let caret = buffer
                        .line_of(editor.selections().primary().head);
                    let lines = buffer.line_count();
                    targets
                        .iter()
                        .filter_map(|&target| {
                            EditView::reach(
                                scene, target, edits, caret, lines,
                            )
                        })
                        .max()
                        .unwrap_or_else(|| {
                            reach(0, edits, caret, lines)
                        })
                },
            )
        };
        let applied = match self.layout.as_deref() {
            Some(layout) => run(layout),
            None => EditView::with_layout(scene, view, run),
        };
        let acted = applied.is_some()
            || *held.editor().selections() != before;
        let rows = held.rows();
        if rows != rows_before {
            held.set_changed();
        }
        if let Some(applied) = applied {
            for target in targets {
                EditView::splice(self.world, target, &applied, rows);
            }
            self.world.write_message(EditorChanged {
                doc,
                view: self.view,
                change: applied.change,
                seq: applied.seq,
            });
        }
        self.acted |= acted;
        acted
    }
}

#[derive(SystemParam)]
pub(crate) struct Parts<'w, 's> {
    docs: Query<'w, 's, (Entity, &'static mut EditDoc)>,
    scene: Scene<'w, 's>,
}

/// What a target's rows read: the views, their lists and rows, and
/// the frame's input.
#[derive(SystemParam)]
pub(crate) struct Scene<'w, 's> {
    views: Query<
        'w,
        's,
        (
            Entity,
            &'static EditView,
            &'static EditInput,
            Has<Composing>,
        ),
    >,
    lists: Query<'w, 's, &'static VirtualList>,
    marks: Query<'w, 's, &'static Marks>,
    rows: RowParts<'w, 's>,
    pointers: Pointers<'w, 's>,
    keys: Res<'w, Keys>,
    keymap: Res<'w, Keymap>,
}

pub(crate) trait EditTarget {
    /// Runs `run` with the target's row geometry this frame, or with
    /// a monospace layout without a target.
    fn with_layout<R>(
        scene: &Scene,
        target: Option<Entity>,
        run: impl FnOnce(&dyn Layout) -> R,
    ) -> R;

    /// The end of the lex window the target's rows need after
    /// `edits`, `None` while it has no rows.
    fn reach(
        scene: &Scene,
        target: Entity,
        edits: &[LineEdit],
        caret: u32,
        lines: u32,
    ) -> Option<u32>;

    /// Moves the target's rows through one change.
    fn splice(
        world: &mut World,
        target: Entity,
        applied: &Applied,
        rows: usize,
    );

    /// The text a pointer at `at` presses, or drags while `held`.
    fn hit(
        scene: &Scene,
        target: Entity,
        buffer: &Buffer,
        at: Vec2,
        held: bool,
    ) -> Option<(Hit, Over)>;

    /// The list to scroll, and where, to keep the caret at `head` in
    /// sight.
    fn reveal(
        scene: &Scene,
        target: Entity,
        buffer: &Buffer,
        head: usize,
    ) -> Option<(Entity, Reveal)>;
}

fn geometry<'a, 'w, 's>(
    scene: &'a Scene<'w, 's>,
    view: Entity,
) -> Option<Geometry<'a, 'w, 's>> {
    let (_, edit, input, _) = scene.views.get(view).ok()?;
    let area = edit.list()?;
    let list = scene.lists.get(area).ok()?;
    Some(Geometry::new(
        &scene.rows,
        list,
        area,
        &input.built,
        scene.marks.get(view).ok(),
        edit.drawn.metrics,
    ))
}

impl EditTarget for EditView {
    fn with_layout<R>(
        scene: &Scene,
        target: Option<Entity>,
        run: impl FnOnce(&dyn Layout) -> R,
    ) -> R {
        match target.and_then(|view| geometry(scene, view)) {
            Some(geometry) => run(&geometry),
            None => run(&Monospace::default()),
        }
    }

    fn reach(
        scene: &Scene,
        target: Entity,
        edits: &[LineEdit],
        caret: u32,
        lines: u32,
    ) -> Option<u32> {
        let (_, edit, ..) = scene.views.get(target).ok()?;
        let list = scene.lists.get(edit.list()?).ok()?;
        Some(reach(list.first_visible(), edits, caret, lines))
    }

    fn splice(
        world: &mut World,
        target: Entity,
        applied: &Applied,
        rows: usize,
    ) {
        let list =
            world.get::<EditView>(target).and_then(EditView::list);
        if let Some(mut list) =
            list.and_then(|list| world.get_mut::<VirtualList>(list))
        {
            splice_rows(&mut list, &applied.edits, rows);
        }
        if let Some(mut input) = world.get_mut::<EditInput>(target) {
            input
                .bypass_change_detection()
                .built
                .shift(&applied.edits);
        }
    }

    fn hit(
        scene: &Scene,
        target: Entity,
        buffer: &Buffer,
        at: Vec2,
        held: bool,
    ) -> Option<(Hit, Over)> {
        geometry(scene, target)?.hit(buffer, at, held)
    }

    fn reveal(
        scene: &Scene,
        target: Entity,
        buffer: &Buffer,
        head: usize,
    ) -> Option<(Entity, Reveal)> {
        let geometry = geometry(scene, target)?;
        let area = scene.views.get(target).ok()?.1.list()?;
        Some((area, geometry.reveal(buffer, head)))
    }
}

/// What a key press does to the view it was sent to.
pub(crate) enum Act {
    Action(Action),
    Text(String),
}

impl From<Resolved<'_>> for Act {
    fn from(resolved: Resolved<'_>) -> Act {
        match resolved {
            Resolved::Action(action) => Act::Action(action),
            Resolved::Text(text) => Act::Text(text.to_owned()),
        }
    }
}

/// What a frame starts by settling: every doc's and marks' damage,
/// and every view's built rows.
type Settle = (
    Query<'static, 'static, &'static mut EditDoc>,
    Query<'static, 'static, &'static mut Marks>,
    Query<'static, 'static, (Entity, &'static mut EditInput)>,
    Query<'static, 'static, (Entity, &'static EditLine)>,
);

/// The state `apply` keeps across frames.
pub(crate) struct Frame {
    parts: SystemState<Parts<'static, 'static>>,
    settle: SystemState<Settle>,
    requests: MessageCursor<EditRequest>,
}

impl FromWorld for Frame {
    fn from_world(world: &mut World) -> Frame {
        Frame {
            parts: SystemState::new(world),
            settle: SystemState::new(world),
            requests: MessageCursor::default(),
        }
    }
}

/// Every change to every [`EditDoc`] this frame, before fynix places
/// rows. Exclusive, so clipboard providers and key handlers take the
/// world between keys, in key order.
///
/// In order: settle last frame's damage and collect each view's built
/// rows; pending pastes; requests; presses and drags; each key, to
/// the view it was sent to; keep the caret of each view whose
/// presses, keys or requests acted in sight; lex stale lines the
/// views may build.
pub(crate) fn apply(world: &mut World, mut frame: Local<Frame>) {
    let frame = &mut *frame;
    let now = world
        .get_resource::<Time>()
        .map(Time::elapsed)
        .unwrap_or_default();
    {
        let (mut docs, mut marks, mut inputs, tags) =
            frame.settle.get_mut(world).expect(PARTS);
        for mut doc in &mut docs {
            doc.bypass_change_detection().settle();
        }
        for mut marks in &mut marks {
            marks.bypass_change_detection().settle();
        }
        for (view, mut input) in &mut inputs {
            input
                .bypass_change_detection()
                .built
                .collect(view, &tags);
        }
    }
    let mut acted: Vec<Entity> = Vec::new();

    let pastes: Vec<_> = {
        let mut parts = frame.parts.get_mut(world).expect(PARTS);
        parts
            .docs
            .iter_mut()
            .filter_map(|(doc, mut held)| {
                let (text, view) =
                    held.bypass_change_detection().poll_paste()?;
                Some((doc, view, text))
            })
            .collect()
    };
    for (doc, view, text) in pastes {
        match text {
            Ok(text) => {
                let mut tx =
                    Tx::new(world, &mut frame.parts, doc, view);
                paste_text(&mut tx, &text);
                acted.extend(view.filter(|_| tx.acted));
            }
            Err(error) => warn!("clipboard: read failed {error}"),
        }
    }

    let requests: Vec<EditRequest> = world
        .get_resource::<Messages<EditRequest>>()
        .map(|messages| {
            frame.requests.read(messages).cloned().collect()
        })
        .unwrap_or_default();
    for request in requests {
        if world.get::<EditDoc>(request.doc).is_none() {
            continue;
        }
        let mut tx = Tx::new(
            world,
            &mut frame.parts,
            request.doc,
            request.view,
        );
        (request.request.0)(&mut tx);
        acted.extend(request.view.filter(|_| tx.acted));
    }

    acted.extend(strokes(world, frame, now));

    let keys: Vec<(Entity, Entity, Act)> = {
        let parts = frame.parts.get_mut(world).expect(PARTS);
        let scene = &parts.scene;
        scene
            .keys
            .presses()
            .iter()
            .filter_map(|press| {
                let view = press.focus?;
                let (_, edit, _, composing) =
                    scene.views.get(view).ok()?;
                let resolved = scene.keymap.resolve(press)?;
                if composing {
                    return None;
                }
                Some((view, edit.doc(), Act::from(resolved)))
            })
            .collect()
    };
    let mut left: Vec<Entity> = Vec::new();
    for (view, doc, act) in keys {
        if left.contains(&view) || world.get::<EditDoc>(doc).is_none()
        {
            continue;
        }
        let mut tx =
            Tx::new(world, &mut frame.parts, doc, Some(view));
        dispatch(&mut tx, &act);
        acted.push(view);
        if let Some(mut input) = world.get_mut::<EditInput>(view) {
            input.bypass_change_detection().keyed_at = now;
        }
        let focus = world
            .get_resource::<InputFocus>()
            .and_then(InputFocus::get);
        if matches!(act, Act::Action(Action::Run(_)))
            && focus != Some(view)
        {
            left.push(view);
        }
    }

    acted.sort_unstable();
    acted.dedup();
    for view in acted {
        let reveal = {
            let parts = frame.parts.get_mut(world).expect(PARTS);
            parts.scene.views.get(view).ok().and_then(
                |(_, edit, ..)| {
                    let (_, doc) = parts.docs.get(edit.doc()).ok()?;
                    let editor = doc.editor();
                    EditView::reveal(
                        &parts.scene,
                        view,
                        editor.buffer(),
                        editor.selections().primary().head,
                    )
                },
            )
        };
        if let Some((area, reveal)) = reveal
            && let Some(mut list) = world.get_mut::<VirtualList>(area)
        {
            reveal.to(&mut list);
        }
        if let Some(mut input) = world.get_mut::<EditInput>(view) {
            input.bypass_change_detection().acted = true;
        }
    }

    let mut parts = frame.parts.get_mut(world).expect(PARTS);
    let windows: Vec<(Entity, u32)> = parts
        .scene
        .views
        .iter()
        .filter_map(|(_, edit, ..)| {
            let (_, doc) = parts.docs.get(edit.doc()).ok()?;
            let list = parts.scene.lists.get(edit.list()?).ok()?;
            let editor = doc.editor();
            let caret = editor
                .buffer()
                .line_of(editor.selections().primary().head);
            Some((
                edit.doc(),
                catch_up_window(list.first_visible() as u32, caret),
            ))
        })
        .collect();
    for (doc, window) in windows {
        if let Ok((_, mut held)) = parts.docs.get_mut(doc) {
            held.bypass_change_detection().catch_up(window);
        }
    }
}

/// The last line to lex this frame for a view whose rows show from
/// line `first`, with its primary caret on line `caret`: as far past
/// both as the list can build rows.
pub(crate) fn catch_up_window(first: u32, caret: u32) -> u32 {
    first.max(caret).saturating_add(2 * BUILT_MAX)
}

/// Presses, then drags, per view: a press runs the gesture its
/// modifiers and clicks name; a held press runs it again to the
/// pointer, and steps the view past an edge. Returns the views that
/// took a press, whose carets are kept in sight like a key's.
fn strokes(
    world: &mut World,
    frame: &mut Frame,
    now: core::time::Duration,
) -> Vec<Entity> {
    let mut pressed_views = Vec::new();
    let views: Vec<(Entity, Entity)> = {
        let parts = frame.parts.get_mut(world).expect(PARTS);
        parts
            .scene
            .views
            .iter()
            .map(|(view, edit, ..)| (view, edit.doc()))
            .collect()
    };
    for (view, doc) in views {
        let pressed = world
            .get_mut::<EditInput>(view)
            .map(|mut input| {
                mem::take(
                    &mut input.bypass_change_detection().pressed,
                )
            })
            .unwrap_or_default();
        let pressed_now = !pressed.is_empty();
        for press in pressed {
            let found = {
                let parts = frame.parts.get_mut(world).expect(PARTS);
                let gesture = parts.scene.keymap.gesture(
                    parts.scene.keys.for_press(),
                    press.count,
                );
                parts.docs.get(doc).ok().and_then(|(_, held)| {
                    let (hit, _) = EditView::hit(
                        &parts.scene,
                        view,
                        held.editor().buffer(),
                        press.at,
                        false,
                    )
                    .filter(|(_, over)| !over.bar)?;
                    Some((
                        gesture,
                        hit,
                        held.editor().selections().clone(),
                    ))
                })
            };
            let Some((gesture, hit, base)) = found else {
                continue;
            };
            Tx::new(world, &mut frame.parts, doc, Some(view))
                .stroke(gesture, hit, hit, &base);
            pressed_views.push(view);
            if let Some(mut input) = world.get_mut::<EditInput>(view)
            {
                let input = input.bypass_change_detection();
                input.drag = Some(Drag {
                    pointer: press.pointer,
                    gesture,
                    from: hit,
                    base,
                });
                input.keyed_at = now;
            }
        }
        if pressed_now {
            continue;
        }
        let dragged = {
            let parts = frame.parts.get_mut(world).expect(PARTS);
            let scene = &parts.scene;
            scene.views.get(view).ok().and_then(
                |(_, edit, input, _)| {
                    let drag = input.drag.clone()?;
                    let Some(at) =
                        select::held(&scene.pointers, drag.pointer)
                    else {
                        return Some(Err(()));
                    };
                    let (_, held) = parts.docs.get(doc).ok()?;
                    let (hit, over) = EditView::hit(
                        scene,
                        view,
                        held.editor().buffer(),
                        at,
                        true,
                    )?;
                    Some(Ok((drag, hit, over, edit.list())))
                },
            )
        };
        match dragged {
            Some(Err(())) => {
                if let Some(mut input) =
                    world.get_mut::<EditInput>(view)
                {
                    input.bypass_change_detection().drag = None;
                }
            }
            Some(Ok((drag, hit, over, area))) => {
                Tx::new(world, &mut frame.parts, doc, Some(view))
                    .stroke(drag.gesture, drag.from, hit, &drag.base);
                if (over.past != 0.0 || over.past_x != 0.0)
                    && let Some(mut list) = area.and_then(|area| {
                        world.get_mut::<VirtualList>(area)
                    })
                {
                    select::reveal_past(&mut list, &over);
                }
            }
            None => {}
        }
    }
    pressed_views
}

/// One key, through the transaction of the view it was sent to.
pub(crate) fn dispatch(tx: &mut Tx, act: &Act) {
    match act {
        Act::Text(text) => {
            tx.apply(Command::Type(text));
        }
        Act::Action(Action::Command(command)) => {
            tx.apply(*command);
        }
        Act::Action(Action::Copy) => copy(tx),
        Act::Action(Action::Cut) => {
            copy(tx);
            tx.apply(Command::Cut);
        }
        Act::Action(Action::Paste) => paste(tx),
        Act::Action(Action::Run(run)) => run(tx),
    }
}

fn copy(tx: &mut Tx) {
    let text = tx.editor().copy_text();
    if !text.is_empty() {
        let pieces = tx.editor().copy_pieces();
        tx.copy_each(text, pieces);
    }
}

/// Pastes `text` read from the clipboard: when the last copy wrote it
/// and gave one piece to each selection there is now, each piece goes
/// back to its own selection, as in Pierre. Otherwise the text goes
/// in as [`Command::Paste`] puts it.
fn paste_text(tx: &mut Tx, text: &str) {
    let selections = tx.editor().selections().len();
    match clipboard::pieces(tx.world, text, selections) {
        Some(pieces) => tx.apply(Command::PasteEach(&pieces)),
        None => tx.apply(Command::Paste(text)),
    };
}

/// Pastes at once when the clipboard read is ready, and otherwise
/// parks the read on the doc for later frames.
fn paste(tx: &mut Tx) {
    let mut read = clipboard::read(tx.world);
    match read.poll_result() {
        Some(Ok(text)) => {
            paste_text(tx, &text);
        }
        Some(Err(error)) => warn!("clipboard: read failed {error}"),
        None => {
            let view = tx.view;
            if let Some(mut doc) = tx.world.get_mut::<EditDoc>(tx.doc)
            {
                doc.bypass_change_detection().park_paste(read, view);
            }
        }
    }
}
