//! The editable code view.
//!
//! An [`EditDoc`] is a text being edited, on an entity of its own. A
//! [`code_editor`] shows one. The doc outlives every view of it, so a
//! theme toggle that rebuilds the views keeps the text, the carets
//! and the undo history.

use core::time::Duration;
use std::sync::Arc;

use bevy::camera::visibility::Visibility;
use bevy::clipboard::{ClipboardError, ClipboardRead};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::message::Message;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::schedule::SystemSet;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::ecs::world::World;
use bevy::input::ButtonInput;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusCause, FocusedInput, InputFocus};
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::picking::events::PointerPress;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::ui::{Node, PositionType, ZIndex, px};
use bevy_fynix::tokens::SpacingTokens;
use bevy_fynix::views::{Span, virtual_list};
use bevy_fynix::{
    AnyView, Bevy, Cx, Prop, Signal, View, component, keyed,
};
use rubrix_edit::{Buffer, Change, Command, Editor, Selections};
use rubrix_highlight::{Highlights, Lang, LineEdit, Run, highlight};

use crate::caret::{Built, CaretLayer, Hit, Metrics};
use crate::code_view::{OVERSCAN, Overflow};
use crate::damage::Damage;
use crate::frame::Tx;
use crate::keymap::{Gesture, KeyPress, Keymap, Mods};
use crate::marks::Marks;
use crate::rows::{
    ADVANCE, LINE_HEIGHT, RowFeed, RowStyle, Shown, code_row,
    filling, inserts, marked, shown_spans,
};
use crate::select;
use crate::tokens::{Styles, SyntaxTokens};

/// The most lines a catch-up lexes in one frame.
const CATCH_UP: u32 = 20_000;

/// The steps of an editor's frame, for features and apps to order
/// against.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditSystems {
    /// `PreUpdate`: this frame's key messages, in order, each with
    /// the modifiers held at it and the entity it was sent to.
    Keys,
    /// `PreUpdate`, before fynix places rows in `Update`: every edit,
    /// selection change, splice and lex of the frame. A feature that
    /// feeds edits runs before it, and one that follows edits runs
    /// after it.
    Apply,
    /// `PostUpdate`, after `UiSystems::PostLayout`: carets and
    /// selection rects from this frame's layout.
    Draw,
}

/// A text being edited: its [`Editor`] (buffer, selections, history)
/// and the highlights of its text.
///
/// It sits on an entity the app spawns and keeps, outside any view
/// tree, so views can come and go around it. Show it with
/// [`code_editor`]. Read it through [`EditDoc::editor`]. Change it
/// with an [`EditRequest`], or let a focused view's keys and pointer
/// change it. Every change reaches the highlights and the rows of
/// every view of it in the frame it is made.
#[derive(Component)]
pub struct EditDoc {
    editor: Editor,
    highlights: Highlights,
    damage: Damage,
    changes: u64,
    /// A paste whose clipboard read is still pending, and the view
    /// that asked for it.
    paste: Option<(ClipboardRead, Option<Entity>)>,
}

/// What [`EditDoc::run`] did.
pub(crate) struct Applied {
    pub(crate) change: Change,
    pub(crate) edits: Vec<LineEdit>,
    pub(crate) seq: u64,
}

impl EditDoc {
    /// `text` lexed as `lang`, with one caret at its start.
    ///
    /// # Panics
    ///
    /// Panics if `text` is 4 GiB or longer.
    pub fn new(lang: Lang, text: &str) -> EditDoc {
        EditDoc::from_editor(lang, Editor::new(Buffer::new(text)))
    }

    /// `text` lexed as the language that `path` or the text's first
    /// line names, as
    /// [`CodeSource::detect`](crate::CodeSource::detect) picks it.
    pub fn detect(path: &str, text: &str) -> EditDoc {
        let first = text.lines().next().unwrap_or_default();
        EditDoc::new(Lang::detect(path, first), text)
    }

    /// An editor restored from saved parts, lexed as `lang`.
    pub fn from_editor(lang: Lang, editor: Editor) -> EditDoc {
        let highlights =
            highlight(lang, &editor.buffer().to_string());
        EditDoc {
            editor,
            highlights,
            damage: Damage::default(),
            changes: 0,
            paste: None,
        }
    }

    /// The buffer, the selections and the history, to read. Change
    /// them with an [`EditRequest`].
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn lang(&self) -> Lang {
        self.highlights.lang()
    }

    /// The rows a view of it shows: one per line, the empty line
    /// after a final `\n` included, because a caret can sit there.
    pub fn rows(&self) -> usize {
        self.editor.buffer().line_count() as usize
    }

    /// Line `line`'s text, from 0, without its `\r` or `\n`. `None`
    /// past the last line.
    pub fn line(&self, line: usize) -> Option<&str> {
        (line < self.rows()).then(|| {
            crate::keymap::content(self.editor.buffer(), line as u32)
        })
    }

    /// Line `line`'s runs as last lexed. Current for every line a
    /// view has built.
    pub fn runs(&self, line: usize) -> &[Run] {
        if line >= self.rows() {
            return &[];
        }
        self.highlights.stored_runs(line as u32)
    }

    /// How many changes the doc has taken, undo and redo included.
    pub fn seq(&self) -> u64 {
        self.changes
    }

    /// Runs `edit` on the editor, then, when it changed the text,
    /// lexes the change's lines within `0..reach(edits)` and notes
    /// them as damage. The caller splices the rows it shows.
    pub(crate) fn run(
        &mut self,
        edit: impl FnOnce(&mut Editor) -> Option<Change>,
        reach: impl FnOnce(&[LineEdit], &Editor) -> u32,
    ) -> Option<Applied> {
        let change = edit(&mut self.editor)?;
        let edits: Vec<LineEdit> = change.line_edits().collect();
        let end = reach(&edits, &self.editor);
        let hull = self.highlights.edit_many(
            edits.iter().copied(),
            self.editor.buffer(),
            0..end,
        );
        self.damage.note(&edits, [hull]);
        self.changes += 1;
        Some(Applied {
            change,
            edits,
            seq: self.changes,
        })
    }

    /// Lexes stale lines before `window`, at most [`CATCH_UP`] a
    /// call, and notes the lines whose runs changed. Free while
    /// nothing is stale.
    pub(crate) fn catch_up(&mut self, window: u32) {
        let Some(stale) = self.highlights.stale_from() else {
            return;
        };
        let end = window
            .min(stale.saturating_add(CATCH_UP))
            .min(self.editor.buffer().line_count());
        if stale >= end {
            return;
        }
        let hull = self.highlights.edit_many(
            core::iter::empty(),
            self.editor.buffer(),
            0..end,
        );
        self.damage.note(&[], [hull]);
    }

    pub(crate) fn settle(&mut self) {
        self.damage.settle();
    }

    pub(crate) fn park_paste(
        &mut self,
        read: ClipboardRead,
        view: Option<Entity>,
    ) {
        self.paste = Some((read, view));
    }

    /// The pending paste's text once its read resolves, and the view
    /// that asked for it.
    pub(crate) fn poll_paste(
        &mut self,
    ) -> Option<(Result<String, ClipboardError>, Option<Entity>)>
    {
        let (read, view) = self.paste.as_mut()?;
        let text = read.poll_result()?;
        let view = *view;
        self.paste = None;
        Some((text, view))
    }
}

/// A change to an [`EditDoc`] from outside its views, run through the
/// same transaction as a key, in `PreUpdate` of the frame after it is
/// written, or of the same frame when written before
/// [`EditSystems::Apply`].
#[derive(Message, Clone)]
pub struct EditRequest {
    pub doc: Entity,
    /// The view whose row geometry vertical motions use, and which
    /// keeps the primary caret in sight afterwards. `None` reveals
    /// nothing.
    pub view: Option<Entity>,
    pub request: Request,
}

impl EditRequest {
    pub fn new(doc: Entity, request: Request) -> EditRequest {
        EditRequest {
            doc,
            view: None,
            request,
        }
    }
}

/// What an [`EditRequest`] does, through the doc's transaction.
#[derive(Clone)]
pub struct Request(pub(crate) Arc<dyn Fn(&mut Tx<'_>) + Send + Sync>);

impl Request {
    /// Any editor operation, run inside the transaction: each
    /// [`Tx::apply`] reaches the highlights and the rows.
    pub fn new(
        run: impl Fn(&mut Tx<'_>) + Send + Sync + 'static,
    ) -> Request {
        Request(Arc::new(run))
    }

    /// Replaces the whole text as one undo step.
    pub fn replace(text: impl Into<String>) -> Request {
        let text = text.into();
        Request::new(move |tx| {
            tx.apply(Command::SelectAll);
            tx.apply(Command::Paste(&text));
        })
    }

    /// Types `text` at every caret, as a key would.
    pub fn typed(text: impl Into<String>) -> Request {
        let text = text.into();
        Request::new(move |tx| {
            tx.apply(Command::Type(&text));
        })
    }

    pub fn command(command: Command<'static>) -> Request {
        Request::new(move |tx| {
            tx.apply(command);
        })
    }

    /// Replaces the selections. Not an undo step.
    pub fn select(selections: Selections) -> Request {
        Request::new(move |tx| tx.select(selections.clone()))
    }
}

/// Written once per change an [`EditDoc`] takes, undo and redo
/// included, after its rows are spliced. An app must not feed it
/// back as an edit.
#[derive(Message, Clone, Debug)]
pub struct EditorChanged {
    pub doc: Entity,
    pub change: Change,
    /// [`EditDoc::seq`] after the change.
    pub seq: u64,
}

/// An editable view of `doc`, an entity holding an [`EditDoc`]: a row
/// per line with a gutter, like [`code_view`](fn@crate::code_view),
/// with a caret per selection, a pointer that selects, and keys that
/// edit while it holds `InputFocus`.
///
/// A new doc builds the rows again at its first line. Colours come
/// from the theme when it is built, as with `code_view`.
pub fn code_editor(doc: impl Into<Prop<Entity>>) -> CodeEditor {
    CodeEditor {
        doc: doc.into(),
        overflow: Prop::Value(Overflow::default()),
        size: None,
    }
}

/// See [`code_editor`].
pub struct CodeEditor {
    doc: Prop<Entity>,
    overflow: Prop<Overflow>,
    size: Option<f32>,
}

impl CodeEditor {
    /// What it does with lines wider than itself,
    /// [`Overflow::Scroll`] when unset.
    pub fn overflow(
        mut self,
        overflow: impl Into<Prop<Overflow>>,
    ) -> CodeEditor {
        self.overflow = overflow.into();
        self
    }

    /// The font size, in logical pixels, the theme's body size when
    /// unset.
    pub fn size(mut self, size: f32) -> CodeEditor {
        self.size = Some(size);
        self
    }
}

impl<T> View<Bevy, T> for CodeEditor
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    fn build(self, cx: &mut Cx<'_, Bevy, T>) -> Entity {
        let mut style = RowStyle::new(cx.theme(), self.size);
        style.pad = 4.0 * ADVANCE * style.size;
        let root = cx.build(filling());
        let caret = style.caret;
        let metrics = Metrics::of_size(style.size);
        cx.world
            .entity_mut(root)
            .insert((
                EditView {
                    doc: Entity::PLACEHOLDER,
                    overflow: Overflow::default(),
                    list: None,
                    carets: Entity::PLACEHOLDER,
                    drawn: Drawn {
                        caret: None,
                        shown: false,
                        metrics,
                    },
                },
                EditInput::default(),
                Marks::default(),
            ))
            .observe(press)
            .observe(keep_keys)
            .observe(select::hold_focus);
        cx.effect(root, self.doc, |world, root, &doc| {
            if let Some(mut view) = world.get_mut::<EditView>(root) {
                view.doc = doc;
            }
        });
        cx.effect(root, self.overflow, |world, root, &overflow| {
            if let Some(mut view) = world.get_mut::<EditView>(root) {
                view.overflow = overflow;
            }
        });
        let doc = component::<EditView, Entity>(root, |view| {
            view.map_or(Entity::PLACEHOLDER, |view| view.doc)
        });
        cx.under(root, |cx| {
            cx.build(
                keyed(doc, move |&doc| {
                    list(root, doc, style.clone())
                })
                .within(filling()),
            )
        });
        let carets = cx
            .world
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
                ZIndex(1),
                Pickable::IGNORE,
                Visibility::Hidden,
                CaretLayer { color: caret },
                ChildOf(root),
            ))
            .id();
        if let Some(mut view) = cx.world.get_mut::<EditView>(root) {
            view.carets = carets;
        }
        root
    }
}

fn list<T>(
    view: Entity,
    doc: Entity,
    style: RowStyle,
) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let estimate = (style.size * LINE_HEIGHT).ceil();
        let count = component::<EditDoc, usize>(doc, |doc| {
            doc.map_or(1, EditDoc::rows)
        });
        let sideways = component::<EditView, bool>(view, |view| {
            view.is_none_or(|view| view.overflow == Overflow::Scroll)
        });
        let feed = EditFeed { view, doc };
        let list = cx.build(
            virtual_list(count, estimate, move |slot| {
                code_row(feed, slot, &style)
            })
            .overscan(OVERSCAN)
            .sideways(sideways),
        );
        if let Some(mut edit) = cx.world.get_mut::<EditView>(view) {
            edit.list = Some(list);
        }
        list
    })
}

/// On an editable view's root.
#[derive(Component)]
pub struct EditView {
    doc: Entity,
    overflow: Overflow,
    list: Option<Entity>,
    carets: Entity,
    pub(crate) drawn: Drawn,
}

/// What `caret::draw` measured this frame.
pub(crate) struct Drawn {
    /// The primary caret's bar, logical window px, when its row is
    /// built.
    pub(crate) caret: Option<Rect>,
    /// Whether the caret's centre lies inside the list's viewport.
    pub(crate) shown: bool,
    pub(crate) metrics: Metrics,
}

impl EditView {
    pub fn doc(&self) -> Entity {
        self.doc
    }

    /// The primary caret's bar in logical window pixels, as drawn
    /// this frame, or `None` while its row is not built.
    pub fn caret_rect(&self) -> Option<Rect> {
        self.drawn.caret
    }

    /// Whether the primary caret is inside the viewport.
    pub fn caret_shown(&self) -> bool {
        self.drawn.shown
    }

    pub(crate) fn overflow(&self) -> Overflow {
        self.overflow
    }

    pub(crate) fn list(&self) -> Option<Entity> {
        self.list
    }

    pub(crate) fn carets(&self) -> Entity {
        self.carets
    }
}

/// An editable view's input, beside its [`EditView`]. The press
/// observer queues presses; the frame reads and writes the rest.
#[derive(Component, Default)]
pub(crate) struct EditInput {
    pub(crate) pressed: Vec<Pressed>,
    pub(crate) drag: Option<Drag>,
    /// The rows built at the last layout, moved through this frame's
    /// changes.
    pub(crate) built: Built,
    /// When a key or press last acted: the caret is solid for a
    /// blink period after it.
    pub(crate) keyed_at: Duration,
    /// Whether the view held the input focus at the last blink.
    pub(crate) focused: bool,
    /// Whether presses, keys or requests acted this frame, so the
    /// drawer keeps the caret in sight from the real layout.
    pub(crate) acted: bool,
    /// Where the last press landed, in logical window px, and the
    /// clicks it counted.
    clicked: Option<(Vec2, u8)>,
}

pub(crate) struct Pressed {
    /// Logical window px.
    pub(crate) at: Vec2,
    pub(crate) count: u8,
    pub(crate) pointer: PointerId,
}

#[derive(Clone)]
pub(crate) struct Drag {
    pub(crate) pointer: PointerId,
    pub(crate) gesture: Gesture,
    pub(crate) from: Hit,
    pub(crate) base: Selections,
}

/// While on a view, its keys go to nobody: they would type beside an
/// open IME preedit.
#[derive(Component)]
pub struct Composing;

/// On the rich text of each row of an editable view: the view's root
/// and the line it shows now.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditLine {
    pub view: Entity,
    pub line: u32,
}

/// How an editable view's rows read their doc.
#[derive(Clone, Copy)]
struct EditFeed {
    view: Entity,
    doc: Entity,
}

/// The damage generations a row read at.
#[derive(Clone, Copy, Default)]
struct Stamp {
    doc: u64,
    marks: u64,
}

impl RowFeed for EditFeed {
    type Tag = EditLine;
    type Read = Stamp;

    fn tag(self, row: usize) -> EditLine {
        EditLine {
            view: self.view,
            line: row as u32,
        }
    }

    fn rows(self, world: &World) -> usize {
        world.get::<EditDoc>(self.doc).map_or(1, EditDoc::rows)
    }

    fn wrap(self) -> Signal<bool> {
        component::<EditView, bool>(self.view, |view| {
            view.is_some_and(|view| view.overflow == Overflow::Wrap)
        })
    }

    fn spans(
        self,
        world: &World,
        row: usize,
        styles: &Styles,
    ) -> Vec<Span> {
        let Some(doc) = world.get::<EditDoc>(self.doc) else {
            return Vec::new();
        };
        let Some(line) = doc.line(row) else {
            return Vec::new();
        };
        let marks = world.get::<Marks>(self.view);
        let on = || {
            marks.into_iter().flat_map(|marks| marks.on(row as u32))
        };
        let shown = Shown::with(line, &inserts(on()));
        marked(
            shown_spans(&shown, doc.runs(row), styles),
            &shown,
            on(),
        )
    }

    /// A row reads again when it is new, when a change, a catch-up or
    /// a mark touched its line since it read, or when it skipped a
    /// Mount. The slot moving is checked by the row itself.
    fn reread(
        self,
        world: &World,
        row: usize,
        read: Option<Stamp>,
    ) -> (bool, Stamp) {
        let Some(doc) = world.get::<EditDoc>(self.doc) else {
            return (false, Stamp::default());
        };
        let marks = world.get::<Marks>(self.view).map(Marks::damage);
        let now = Stamp {
            doc: doc.damage.seq(),
            marks: marks.map_or(0, Damage::seq),
        };
        let line = row as u32;
        let fires = read.is_none_or(|read| {
            doc.damage.fires(line, read.doc)
                || marks.is_some_and(|marks| {
                    marks.fires(line, read.marks)
                })
        });
        (fires, now)
    }
}

/// A press on an editable view: takes the focus and queues the press
/// for the frame, which hit-tests it against this frame's rows.
fn press(
    mut press: On<PointerPress>,
    mut views: Query<&mut EditInput, With<EditView>>,
    mut focus: ResMut<InputFocus>,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let view = press.entity;
    let Ok(mut input) = views.get_mut(view) else {
        return;
    };
    press.propagate(false);
    if focus.get() != Some(view) {
        focus.set(view, FocusCause::Pressed);
    }
    let at = press.pointer.position;
    let count = match input.clicked {
        Some((last, count)) if press.count > 1 => {
            if last.distance(at) <= CLICK_SLOP {
                count + 1
            } else {
                1
            }
        }
        _ => press.count,
    }
    .min(3);
    input.clicked = Some((at, count));
    input.pressed.push(Pressed {
        at,
        count,
        pointer: press.pointer.id,
    });
}

/// How far, in logical px, a press may land from the one before and
/// still count toward a double or triple click. Bevy counts every
/// press on an entity inside its interval, wherever it lands.
const CLICK_SLOP: f32 = 4.0;

/// Keeps the keys an editor uses from bubbling to the window, where
/// fynix's shortcuts would run commands bound to the same chords. It
/// only filters: the frame reads key messages itself.
fn keep_keys(
    mut event: On<FocusedInput<KeyboardInput>>,
    keymap: Res<Keymap>,
    held: Option<Res<ButtonInput<KeyCode>>>,
) {
    let press = KeyPress {
        input: event.input.clone(),
        mods: Mods::from_held(held.as_deref()),
        focus: Some(event.focused_entity),
    };
    if keymap.resolve(&press).is_some() {
        event.propagate(false);
    }
}

#[cfg(test)]
mod tests {
    use rubrix_edit::{Monospace, Selection};

    use super::*;

    #[test]
    fn a_typed_letter_rereads_one_of_the_built_rows() {
        let mut world = World::new();
        let text = "let a = 1;\n".repeat(300);
        let doc = world.spawn(EditDoc::new(Lang::Rust, &text)).id();
        let view = world.spawn_empty().id();
        let feed = EditFeed { view, doc };
        let mount = |world: &World, stamps: &mut [Option<Stamp>]| {
            let mut fired = 0;
            for (row, stamp) in stamps.iter_mut().enumerate() {
                let (fires, now) = feed.reread(world, row, *stamp);
                *stamp = Some(now);
                fired += usize::from(fires);
            }
            fired
        };
        let mut stamps = [None; 199];
        world.get_mut::<EditDoc>(doc).expect("a doc").settle();
        assert_eq!(
            mount(&world, &mut stamps),
            199,
            "a new row reads"
        );
        assert_eq!(mount(&world, &mut stamps), 0);
        let mut edit = world.get_mut::<EditDoc>(doc).expect("a doc");
        edit.settle();
        let at = edit.editor().buffer().line_start(5) + 3;
        let typed = edit.run(
            |editor| {
                editor.set_selections(Selections::new(
                    Selection::caret(at),
                ));
                editor
                    .apply(Command::Type("x"), &Monospace::default())
            },
            |_, _| 300,
        );
        assert!(typed.is_some());
        assert_eq!(
            mount(&world, &mut stamps),
            1,
            "only line 5 reads"
        );
        world.get_mut::<EditDoc>(doc).expect("a doc").settle();
        assert_eq!(
            mount(&world, &mut stamps),
            0,
            "it read the change"
        );
    }

    #[test]
    fn a_catch_up_lexes_at_most_its_cap() {
        let mut doc = EditDoc::new(Lang::Rust, &"x\n".repeat(50_000));
        let opened = doc.run(
            |editor| {
                editor
                    .apply(Command::Type("/*"), &Monospace::default())
            },
            |_, _| 10,
        );
        assert!(opened.is_some());
        let stale =
            doc.highlights.stale_from().expect("stale past 10");
        doc.catch_up(u32::MAX);
        let after = doc.highlights.stale_from().expect("still stale");
        assert_eq!(after, stale + CATCH_UP);
    }
}
