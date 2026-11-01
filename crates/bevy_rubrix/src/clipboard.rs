//! Copy, cut and paste for editable and read-only views.

use bevy::clipboard::{Clipboard, ClipboardError, ClipboardRead};
use bevy::ecs::entity::Entity;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::{Mut, World};
use bevy::log::warn;

use crate::select::CodeCopied;

/// Where copy and cut write and paste reads. Its methods take the
/// world, so a provider can reach the app's resources.
pub trait ClipboardProvider: Send + Sync + 'static {
    fn write(
        &mut self,
        world: &mut World,
        text: &str,
    ) -> Result<(), ClipboardError>;

    /// Starts a read. A `ClipboardRead::Pending` read is polled once
    /// per frame until it resolves, and the paste lands then.
    fn read(&mut self, world: &mut World) -> ClipboardRead;
}

/// The app's clipboard provider. Replace it with
/// `app.insert_resource(EditClipboard::new(provider))`.
#[derive(Resource)]
pub struct EditClipboard {
    provider: Box<dyn ClipboardProvider>,
    /// The text the last copy wrote and the piece each of its
    /// selections gave, as Pierre keeps them under a clipboard type
    /// of its own beside the plain text.
    pieces: Option<(String, Vec<String>)>,
}

impl EditClipboard {
    pub fn new(provider: impl ClipboardProvider) -> EditClipboard {
        EditClipboard {
            provider: Box::new(provider),
            pieces: None,
        }
    }
}

impl Default for EditClipboard {
    fn default() -> EditClipboard {
        EditClipboard::new(SystemClipboard)
    }
}

/// Bevy's [`Clipboard`] resource, the one the app's clipboard plugin
/// made. It never makes a second one, since two can deadlock. Without
/// the resource, a write fails with
/// [`ClipboardError::ClipboardNotSupported`] and a read resolves to
/// that error.
pub struct SystemClipboard;

impl ClipboardProvider for SystemClipboard {
    fn write(
        &mut self,
        world: &mut World,
        text: &str,
    ) -> Result<(), ClipboardError> {
        match world.get_resource_mut::<Clipboard>() {
            Some(mut clipboard) => clipboard.set_text(text),
            None => Err(ClipboardError::ClipboardNotSupported),
        }
    }

    fn read(&mut self, world: &mut World) -> ClipboardRead {
        match world.get_resource_mut::<Clipboard>() {
            Some(mut clipboard) => clipboard.fetch_text(),
            None => ClipboardRead::Ready(Err(
                ClipboardError::ClipboardNotSupported,
            )),
        }
    }
}

/// Copies `text` from `view` through the provider, logging a refused
/// write, and triggers [`CodeCopied`] on the view either way. Keeps
/// `pieces`, the text each selection gave, for [`pieces`] when there
/// are two or more.
pub(crate) fn copy(
    world: &mut World,
    view: Entity,
    text: String,
    pieces: Vec<String>,
) {
    let failed = world.resource_scope(
        |world, mut clipboard: Mut<EditClipboard>| {
            clipboard.pieces =
                (pieces.len() > 1).then(|| (text.clone(), pieces));
            clipboard.provider.write(world, &text).err()
        },
    );
    if let Some(error) = failed {
        warn!("clipboard: set_text failed {error}");
    }
    world.trigger(CodeCopied { entity: view, text });
}

pub(crate) fn read(world: &mut World) -> ClipboardRead {
    world.resource_scope(
        |world, mut clipboard: Mut<EditClipboard>| {
            clipboard.provider.read(world)
        },
    )
}

/// The pieces of the last copy, when it wrote `text` and gave one
/// piece to each of `selections`. A clipboard written since by
/// another copy, or by another app, reads as other text.
pub(crate) fn pieces(
    world: &World,
    text: &str,
    selections: usize,
) -> Option<Vec<String>> {
    let (copied, pieces) =
        world.resource::<EditClipboard>().pieces.as_ref()?;
    (copied == text && pieces.len() == selections)
        .then(|| pieces.clone())
}
