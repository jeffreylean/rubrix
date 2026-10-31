//! Line and block comments.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::Range;

use rubrix_highlight::Lang;

use crate::buffer::Buffer;
use crate::command::{After, Plan};
use crate::motion::{indent, snap};
use crate::selection::{Selection, Selections};

/// How far past a selection's ends a block comment's markers may lie,
/// in bytes, as in Pierre.
const MARGIN: usize = 50;

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
    buffer: &Buffer,
    selections: &Selections,
    markers: Markers,
    block: bool,
) -> Option<Plan> {
    match (block, markers.line, markers.block) {
        (false, Some(token), _) => {
            line_comments(buffer, selections, token)
        }
        (_, _, Some(pair)) => {
            block_comments(buffer, selections, pair, !block)
        }
        _ => None,
    }
}

/// The first and last line of `selection`, leaving out a last line
/// that the selection only reaches at column 0.
fn line_span(buffer: &Buffer, selection: Selection) -> (u32, u32) {
    let first = buffer.line_of(selection.start());
    let last = buffer.line_of(selection.end());
    if last > first && buffer.line_start(last) == selection.end() {
        (first, last - 1)
    } else {
        (first, last)
    }
}

struct Line {
    line: u32,
    /// Where a marker goes: the least indent of the selection's
    /// non-empty lines.
    indent: usize,
    /// Where the line's own marker starts, if it has one.
    marker: Option<usize>,
    /// Only spaces and tabs.
    empty: bool,
    /// The only line its selection covers, so it is commented even
    /// when empty.
    single: bool,
}

/// Pierre's `resolveLineCommentEdits`: comments every covered line
/// at one aligned column per selection when any line that counts
/// lacks the marker, and otherwise removes each marker and one space
/// after it.
fn line_comments(
    buffer: &Buffer,
    selections: &Selections,
    token: &str,
) -> Option<Plan> {
    let mut lines: Vec<Line> = Vec::new();
    for selection in selections.iter() {
        let (first, last) = line_span(buffer, selection);
        let from = lines.len();
        for line in first..=last {
            if lines.last().is_some_and(|seen| seen.line >= line) {
                continue;
            }
            let content = buffer.content(line);
            let indent = indent(content).len();
            lines.push(Line {
                line,
                indent,
                marker: content[indent..]
                    .starts_with(token)
                    .then_some(indent),
                empty: indent == content.len(),
                single: false,
            });
        }
        let added = &mut lines[from..];
        let least = added
            .iter()
            .filter(|line| !line.empty)
            .map(|line| line.indent)
            .min();
        if let Some(least) = least {
            for line in added.iter_mut().filter(|line| !line.empty) {
                line.indent = least;
            }
        }
        if let [only] = added {
            only.single = true;
        }
    }
    let counts = |line: &&Line| !line.empty || line.single;
    let comment = lines
        .iter()
        .filter(counts)
        .any(|line| line.marker.is_none());
    let edits: Vec<(Range<usize>, String)> = if comment {
        let marker = [token, " "].concat();
        lines
            .iter()
            .filter(counts)
            .map(|line| {
                let at = buffer.line_start(line.line) + line.indent;
                (at..at, marker.clone())
            })
            .collect()
    } else {
        lines
            .iter()
            .filter_map(|line| {
                let start = line.marker?;
                let rest = &buffer.content(line.line)[start..];
                let len = token.len()
                    + usize::from(
                        rest[token.len()..].starts_with(' '),
                    );
                let at = buffer.line_start(line.line) + start;
                Some((at..at + len, String::new()))
            })
            .collect()
    };
    (!edits.is_empty()).then_some(Plan {
        edits,
        after: After::Map,
    })
}

/// A block comment found at a range: the bytes to delete with each
/// marker and the text between them.
struct Found {
    open: Range<usize>,
    close: Range<usize>,
    content: Range<usize>,
}

/// Pierre's `findBlockComment`: markers just outside `range`, past
/// whitespace, or else just inside its ends. Each deletion takes one
/// whitespace char beside its marker.
fn find(
    buffer: &Buffer,
    (open, close): (&str, &str),
    range: Range<usize>,
) -> Option<Found> {
    let Range {
        start: from,
        end: to,
    } = range;
    let before =
        buffer.slice(snap(buffer, from.saturating_sub(MARGIN))..from);
    let after = buffer.slice(to..snap(buffer, to + MARGIN));
    let outer_before = before.trim_end();
    let space_after = after.len() - after.trim_start().len();
    if outer_before.ends_with(open)
        && after[space_after..].starts_with(close)
    {
        let space_before = before.len() - outer_before.len();
        let first_space = before[outer_before.len()..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        let last_space = after[..space_after]
            .chars()
            .next_back()
            .map_or(0, char::len_utf8);
        let open_start = from - space_before - open.len();
        let close_end = to + space_after + close.len();
        return Some(Found {
            open: open_start..from - space_before + first_space,
            close: to + space_after - last_space..close_end,
            content: from..to,
        });
    }
    let short = to - from <= 2 * MARGIN;
    let head = buffer.slice(
        from..if short {
            to
        } else {
            snap(buffer, from + MARGIN)
        },
    );
    let tail = buffer.slice(
        if short {
            from
        } else {
            snap(buffer, to - MARGIN)
        }..to,
    );
    let inner_head = head.trim_start();
    let inner_tail = tail.trim_end();
    if !inner_head.starts_with(open) || !inner_tail.ends_with(close) {
        return None;
    }
    let open_start = from + head.len() - inner_head.len();
    let open_end = open_start
        + open.len()
        + inner_head[open.len()..]
            .chars()
            .next()
            .filter(|c| c.is_whitespace())
            .map_or(0, char::len_utf8);
    let close_start =
        to - (tail.len() - inner_tail.len()) - close.len();
    let close_from = close_start
        - inner_tail[..inner_tail.len() - close.len()]
            .chars()
            .next_back()
            .filter(|c| c.is_whitespace())
            .map_or(0, char::len_utf8);
    Some(Found {
        open: open_start..open_end,
        close: close_from..close_start + close.len(),
        content: open_end..close_from,
    })
}

/// Pierre's `resolveBlockCommentEdits`. Each selection's range, or
/// with `linewise` its lines from the first one's indent to the last
/// one's end, merged, is uncommented when every range has a block
/// comment, and otherwise each range without one is wrapped in the
/// markers. Without `linewise` each selection keeps its text selected
/// inside the markers, and a caret lands in the middle of the empty
/// comment.
fn block_comments(
    buffer: &Buffer,
    selections: &Selections,
    (open, close): (&'static str, &'static str),
    linewise: bool,
) -> Option<Plan> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for selection in selections.iter() {
        if !linewise {
            ranges.push(selection.range());
            continue;
        }
        let (first, last) = line_span(buffer, selection);
        let start = buffer.line_start(first)
            + indent(buffer.content(first)).len();
        let range = start..buffer.line_end(last);
        match ranges.last_mut() {
            Some(previous) if range.start <= previous.end => {
                previous.end = previous.end.max(range.end);
            }
            _ => ranges.push(range),
        }
    }
    let found: Vec<Option<Found>> = ranges
        .iter()
        .map(|range| find(buffer, (open, close), range.clone()))
        .collect();
    let uncomment = found.iter().all(Option::is_some);
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for (range, found) in ranges.iter().zip(&found) {
        match found {
            Some(found) if uncomment => {
                edits.push((found.open.clone(), String::new()));
                edits.push((found.close.clone(), String::new()));
            }
            Some(_) => {}
            None if range.is_empty() => {
                edits.push((
                    range.clone(),
                    [open, "  ", close].concat(),
                ));
            }
            None => {
                let (start, end) = (range.start, range.end);
                edits.push((start..start, [open, " "].concat()));
                edits.push((end..end, [" ", close].concat()));
            }
        }
    }
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    if edits.is_empty()
        || edits
            .windows(2)
            .any(|pair| pair[1].0.start < pair[0].0.end)
    {
        return None;
    }
    if linewise {
        return Some(Plan {
            edits,
            after: After::Map,
        });
    }
    let mut targets = ranges.iter().zip(&found);
    let mut after = selections.clone();
    after.update(|selection, _| {
        let (range, found) =
            targets.next().expect("one range per selection");
        let (start, end) = match found {
            Some(found) if uncomment => (
                map(found.content.start, &edits, true),
                map(found.content.end, &edits, false),
            ),
            Some(_) => (
                map(range.start, &edits, true),
                map(range.end, &edits, true),
            ),
            None if range.is_empty() => {
                let at =
                    map(range.start, &edits, false) + open.len() + 1;
                (at, at)
            }
            None => (
                map(range.start, &edits, true),
                map(range.end, &edits, false),
            ),
        };
        if selection.anchor <= selection.head {
            Selection {
                anchor: start,
                head: end,
            }
        } else {
            Selection {
                anchor: end,
                head: start,
            }
        }
    });
    Some(Plan {
        edits,
        after: After::Set(after),
    })
}

/// Where `offset` lands after `edits`, Pierre's `mapOffset`: text
/// inserted at `offset` goes before it when `after`, and an offset
/// inside a replaced range goes to the end of its new text when
/// `after` or its start otherwise. The end of a replaced range goes
/// to the end of its new text.
fn map(
    offset: usize,
    edits: &[(Range<usize>, String)],
    after: bool,
) -> usize {
    let (mut added, mut removed) = (0, 0);
    for (range, text) in edits {
        if offset < range.start {
            break;
        }
        let start = range.start - removed + added;
        if range.is_empty() && offset == range.start {
            if !after {
                break;
            }
            added += text.len();
        } else if offset > range.end {
            added += text.len();
            removed += range.len();
        } else if offset == range.end || after {
            return start + text.len();
        } else {
            return start;
        }
    }
    offset - removed + added
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::{Markers, toggle};
    use crate::buffer::Buffer;
    use crate::selection::{Selection, Selections};

    /// No `Lang` has a block marker and no line marker, so this
    /// builds one.
    #[test]
    fn line_mode_without_a_line_marker_wraps_the_lines_in_a_block() {
        let markers = Markers {
            line: None,
            block: Some(("<!--", "-->")),
        };
        let mut buffer = Buffer::new("  a\n  b\nc");
        for (range, expected) in
            [(3..6, "  <!-- a\n  b -->\nc"), (0..16, "  a\n  b\nc")]
        {
            let selections = Selections::new(Selection {
                anchor: range.start,
                head: range.end,
            });
            let plan = toggle(&buffer, &selections, markers, false)
                .expect("a block comment to toggle");
            let _ = buffer.replace(plan.edits);
            assert_eq!(buffer.to_string(), expected);
        }
    }
}
