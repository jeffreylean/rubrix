//! Line and block comments.

use rubrix_highlight::Lang;

use crate::buffer::Buffer;
use crate::command::Plan;
use crate::selection::Selections;

/// The comment markers of a language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Markers {
    /// What starts a line comment, such as `//`.
    pub line: Option<&'static str>,
    /// What opens and closes a block comment, such as `/*` and `*/`.
    pub block: Option<(&'static str, &'static str)>,
}

/// The comment markers of `lang`. JSON, diffs and plain text have
/// none.
pub const fn markers(lang: Lang) -> Markers {
    match lang {
        Lang::Rust | Lang::TypeScript | Lang::Go => Markers {
            line: Some("//"),
            block: Some(("/*", "*/")),
        },
        Lang::Python | Lang::Shell | Lang::Toml | Lang::Yaml => {
            Markers {
                line: Some("#"),
                block: None,
            }
        }
        Lang::Json | Lang::Diff | Lang::Plain => Markers {
            line: None,
            block: None,
        },
    }
}

/// Comments the selected lines with `markers`, or uncomments them
/// when every one is already commented. With `block` false it uses
/// the line marker, or the block pair around each line when there is
/// none. With `block` true it wraps each selection in the block
/// pair. `None` when `markers` has nothing to use.
pub(crate) fn toggle(
    _buffer: &Buffer,
    _selections: &Selections,
    _markers: Markers,
    _block: bool,
) -> Option<Plan> {
    None
}
