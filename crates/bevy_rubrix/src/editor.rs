//! The editable code view.
//!
//! An [`EditDoc`] is a text being edited, on an entity of its own. A
//! [`code_editor`] shows one. The doc outlives every view of it, so a
//! theme toggle that rebuilds the views keeps the text, the carets
//! and the undo history.

use std::sync::Arc;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::message::Message;
use bevy::ecs::schedule::SystemSet;
use bevy::math::Rect;
use bevy_fynix::tokens::SpacingTokens;
use bevy_fynix::{Bevy, Cx, Prop, View};
use rubrix_edit::{Change, Command, Editor, Layout, Selections};
use rubrix_highlight::{Lang, Run};

use crate::code_view::Overflow;
use crate::tokens::SyntaxTokens;

/// The steps of an editor's frame, for features and apps to order
/// against.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditSystems {
    /// `PreUpdate`: this frame's key messages, in order, each with the
    /// modifiers held at it.
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
/// change it.
#[derive(Component)]
pub struct EditDoc {
    editor: Editor,
}

impl EditDoc {
    /// `text` lexed as `lang`, with one caret at its start.
    ///
    /// # Panics
    ///
    /// Panics if `text` is 4 GiB or longer.
    pub fn new(lang: Lang, text: &str) -> EditDoc {
        let _ = (lang, text);
        todo!()
    }

    /// `text` lexed as the language that `path` or the text's first
    /// line names, as [`CodeSource::detect`](crate::CodeSource::detect)
    /// picks it.
    pub fn detect(path: &str, text: &str) -> EditDoc {
        let first = text.lines().next().unwrap_or_default();
        EditDoc::new(Lang::detect(path, first), text)
    }

    /// An editor restored from saved parts, lexed as `lang`.
    pub fn from_editor(lang: Lang, editor: Editor) -> EditDoc {
        let _ = (lang, editor);
        todo!()
    }

    /// The buffer, the selections and the history, to read. Change
    /// them with an [`EditRequest`].
    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn lang(&self) -> Lang {
        todo!()
    }

    /// The rows a view of it shows: one per line, the empty line after
    /// a final `\n` included, because a caret can sit there.
    pub fn rows(&self) -> usize {
        self.editor.buffer().line_count() as usize
    }

    /// Line `line`'s text, from 0, without its `\r` or `\n`. `None`
    /// past the last line.
    pub fn line(&self, line: usize) -> Option<&str> {
        let _ = line;
        todo!()
    }

    /// Line `line`'s runs as last lexed. Current for every line a view
    /// has built.
    pub fn runs(&self, line: usize) -> &[Run] {
        let _ = line;
        todo!()
    }

    /// How many changes the doc has taken, undo and redo included.
    pub fn seq(&self) -> u64 {
        todo!()
    }
}

/// A change to an [`EditDoc`] from outside its views, run through the
/// same transaction as a key.
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

/// What an [`EditRequest`] does to the editor. It returns the change
/// it made, or `None` for none.
#[derive(Clone)]
pub struct Request(
    #[expect(
        clippy::type_complexity,
        reason = "one shared editor operation"
    )]
    pub(crate) Arc<
        dyn Fn(&mut Editor, &dyn Layout) -> Option<Change> + Send + Sync,
    >,
);

impl Request {
    /// Any editor operation, run inside the transaction.
    pub fn new(
        run: impl Fn(&mut Editor, &dyn Layout) -> Option<Change>
        + Send
        + Sync
        + 'static,
    ) -> Request {
        Request(Arc::new(run))
    }

    /// Replaces the whole text as one undo step.
    pub fn replace(text: impl Into<String>) -> Request {
        let _ = text.into();
        todo!()
    }

    /// Types `text` at every caret, as a key would.
    pub fn typed(text: impl Into<String>) -> Request {
        let _ = text.into();
        todo!()
    }

    pub fn command(command: Command<'static>) -> Request {
        let _ = command;
        todo!()
    }

    /// Replaces the selections. Not an undo step.
    pub fn select(selections: Selections) -> Request {
        let _ = selections;
        todo!()
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
/// per line with a gutter, like [`code_view`](crate::code_view), with
/// a caret per selection, a pointer that selects, and keys that edit
/// while it holds `InputFocus`.
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
        let _ = (cx, self.doc, self.overflow, self.size);
        todo!()
    }
}

/// On an editable view's root.
#[derive(Component)]
pub struct EditView {
    doc: Entity,
}

impl EditView {
    pub fn doc(&self) -> Entity {
        self.doc
    }

    /// The primary caret's bar in logical window pixels, as drawn
    /// this frame, or `None` while its row is not built.
    pub fn caret_rect(&self) -> Option<Rect> {
        todo!()
    }

    /// Whether the primary caret is inside the viewport.
    pub fn caret_shown(&self) -> bool {
        todo!()
    }
}

/// On the rich text of each row of an editable view: the view's root
/// and the line it shows now.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditLine {
    pub view: Entity,
    pub line: u32,
}
