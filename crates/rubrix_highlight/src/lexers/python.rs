//! Python, lexed as plain text until this language's lexer
//! replaces this file.

use crate::{Runs, State};

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    super::plain::lex(line, state, out)
}
