//! Headless text editing for rubrix.
//!
//! An [`Editor`] holds a [`Buffer`], its [`Selections`] and its
//! [`History`]. [`Editor::apply`] runs one [`Command`] at every
//! selection and returns the [`Change`] it made to the text. A view
//! feeds [`Change::line_edits`] to
//! `rubrix_highlight::Highlights::edit_many` and to its row list, and
//! moves its own positions through [`Change::map`].
//!
//! # Positions
//!
//! A position is a byte offset into the text, on a char boundary. The
//! text splits into lines at every `\n`, so a text with n `\n` bytes
//! has n + 1 lines. A `\r` before a `\n` stays in its line, as
//! [`rubrix_highlight::Lines`] requires, and every offset counts it.
//! A caret never sits between that `\r` and its `\n`.

#![no_std]

extern crate alloc;

mod buffer;
mod change;
mod command;
mod history;
mod motion;
mod selection;

pub use buffer::{Buffer, Eol};
pub use change::{Assoc, Change, Replacement};
pub use command::{Command, Editor};
pub use history::History;
pub use motion::{Layout, Monospace, Motion, graphemes, word_at};
pub use selection::{Selection, Selections};

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "find")]
pub mod find;
