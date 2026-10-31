use std::ops::Range;

use rubrix_edit::{
    Brackets, Buffer, SCAN_CHARS, SCAN_LINES, matching_bracket,
};
use rubrix_highlight::{Kind, Lang};

/// The match at `at` in `text`, skipping the string and comment runs
/// of `text` highlighted as Rust.
fn matched(text: &str, at: usize) -> Option<Brackets> {
    let highlights = rubrix_highlight::highlight(Lang::Rust, text);
    matching_bracket(&Buffer::new(text), at, |line| {
        rubrix_highlight::spans(highlights.stored_runs(line))
            .filter(|(_, kind)| {
                matches!(kind.root(), Kind::String | Kind::Comment)
            })
            .map(|(range, _)| range)
            .collect()
    })
}

fn none(_: u32) -> Vec<Range<usize>> {
    Vec::new()
}

fn pair(near: usize, far: usize) -> Option<Brackets> {
    Some(Brackets { near, far })
}

#[test]
fn an_opener_matches_forward_across_lines() {
    let text = "fn f() {\n    g();\n}\n";
    assert_eq!(matched(text, 8), pair(7, 18));
    assert_eq!(matched(text, 7), pair(7, 18));
}

#[test]
fn a_closer_matches_backward_past_nested_pairs() {
    let text = "a[b[c]d[e]]";
    assert_eq!(matched(text, 11), pair(10, 1));
    assert_eq!(matched(text, 6), pair(5, 3));
}

#[test]
fn the_bracket_before_the_caret_wins() {
    assert_eq!(matched("(a)[b]", 3), pair(2, 0));
}

#[test]
fn no_bracket_beside_the_caret_matches_nothing() {
    assert_eq!(matched("(a b)", 2), None);
    assert_eq!(matched("(a b", 1), None);
}

#[test]
fn a_bracket_inside_a_string_is_skipped() {
    let text = "f(\")\", x)";
    assert_eq!(matched(text, 2), pair(1, 8));
    assert_eq!(matched(text, 4), None);
}

#[test]
fn a_caret_after_a_paren_inside_a_string_matches_nothing() {
    assert_eq!(matched("let s = \"(a)\";", 10), None);
}

#[test]
fn the_scan_stops_at_its_line_bound() {
    let lines = SCAN_LINES as usize;
    let within = format!("{{{}}}", "\n".repeat(lines - 1));
    assert_eq!(
        matching_bracket(&Buffer::new(&within), 1, none),
        pair(0, lines)
    );
    let past = format!("{{{}}}", "\n".repeat(lines));
    assert_eq!(matching_bracket(&Buffer::new(&past), 1, none), None);
}

#[test]
fn the_scan_stops_at_its_char_bound() {
    let within = format!("({})", " ".repeat(SCAN_CHARS - 2));
    assert_eq!(
        matching_bracket(&Buffer::new(&within), 1, none),
        pair(0, SCAN_CHARS - 1)
    );
    let past = format!("({})", " ".repeat(SCAN_CHARS - 1));
    assert_eq!(matching_bracket(&Buffer::new(&past), 1, none), None);
}
