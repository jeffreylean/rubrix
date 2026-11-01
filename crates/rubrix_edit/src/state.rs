use crate::buffer::Buffer;
use crate::command::Editor;
use crate::history::History;
use crate::selection::Selections;

/// An [`Editor`] put aside: a snapshot of its buffer, its selections
/// and its history, so an editor made from it later undoes the edits
/// made before, and the revision its owner had counted, so the count
/// carries on.
///
/// [`EditState::save`] shares the buffer's chunks, so the first edit
/// in either copy after it copies each chunk that edit touches. Save
/// once, when the editor goes away, not per edit.
#[derive(Debug)]
pub struct EditState {
    buffer: Buffer,
    selections: Selections,
    history: History,
    revision: u64,
}

impl EditState {
    pub fn save(editor: &Editor, revision: u64) -> EditState {
        EditState {
            buffer: editor.buffer().snapshot(),
            selections: editor.selections().clone(),
            history: editor.history().clone(),
            revision,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// An editor with the saved text, selections and history.
    pub fn restore(self) -> Editor {
        Editor::from_parts(self.buffer, self.selections, self.history)
    }
}
