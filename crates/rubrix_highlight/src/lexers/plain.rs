//! Text with no syntax: one plain run per non-empty line.

use crate::{Kind, Runs, State};

pub(crate) fn lex(
    line: &str,
    _: &State,
    out: &mut Runs<'_>,
) -> State {
    out.push(line.len(), Kind::Plain);
    State::default()
}
