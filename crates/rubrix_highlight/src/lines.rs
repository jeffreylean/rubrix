use alloc::vec::Vec;

/// The text of a document, one line at a time.
///
/// [`Highlights::edit`](crate::Highlights::edit) and
/// [`Highlights::runs`](crate::Highlights::runs) read text only
/// through this trait, so any text buffer can plug in.
pub trait Lines {
    /// The text of line `n`, counting from 0. The lines are the
    /// pieces of the text split at every `\n`, so a line holds no
    /// `\n`. A `\r` that ends a line stays in it, and the highlights
    /// read it as part of the terminator, as
    /// [`highlight`](crate::highlight) does.
    ///
    /// # Panics
    ///
    /// May panic if `n` is not a line of the text.
    fn line(&self, n: u32) -> &str;
}

impl<S: AsRef<str>> Lines for [S] {
    fn line(&self, n: u32) -> &str {
        self[n as usize].as_ref()
    }
}

impl<S: AsRef<str>> Lines for Vec<S> {
    fn line(&self, n: u32) -> &str {
        self[n as usize].as_ref()
    }
}
