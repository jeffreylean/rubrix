use std::ops::Range;

use rubrix_diff::word_changes;

fn marked<'a>(line: &'a str, ranges: &[Range<u32>]) -> Vec<&'a str> {
    ranges
        .iter()
        .map(|range| &line[range.start as usize..range.end as usize])
        .collect()
}

fn changes<'a>(
    old: &'a str,
    new: &'a str,
) -> (Vec<&'a str>, Vec<&'a str>) {
    let (old_ranges, new_ranges) =
        word_changes(old, new).expect("short lines");
    (marked(old, &old_ranges), marked(new, &new_ranges))
}

#[test]
fn one_changed_word() {
    assert_eq!(
        changes(
            "let count = items.len();",
            "let total = items.len();"
        ),
        (vec!["count"], vec!["total"])
    );
}

#[test]
fn a_changed_word_next_to_punctuation() {
    assert_eq!(
        changes("call(alpha, beta);", "call(gamma, beta);"),
        (vec!["alpha"], vec!["gamma"])
    );
    assert_eq!(
        changes("let total = a + b;", "let total = a - b;"),
        (vec!["+"], vec!["-"])
    );
}

#[test]
fn a_renamed_receiver_leaves_the_dot_and_method_unmarked() {
    assert_eq!(
        changes(
            "*tally.entry(word).or_insert(0) += 1;",
            "*word_counts.entry(word).or_insert(0) += 1;"
        ),
        (vec!["tally"], vec!["word_counts"])
    );
}

#[test]
fn a_version_bump_marks_only_the_changed_digit() {
    assert_eq!(
        changes("version = \"0.4.0\"", "version = \"0.5.0\""),
        (vec!["4"], vec!["5"])
    );
}

#[test]
fn an_apostrophe_splits_a_word() {
    assert_eq!(
        changes("I don't know", "I won't know"),
        (vec!["don"], vec!["won"])
    );
}

#[test]
fn parentheses_split_a_call_from_its_argument() {
    assert_eq!(
        changes("foo(bar)", "foo(baz)"),
        (vec!["bar"], vec!["baz"])
    );
}

#[test]
fn two_changes_one_space_apart_merge() {
    assert_eq!(
        changes("foo(a b)", "foo(c d)"),
        (vec!["a b"], vec!["c d"])
    );
}

#[test]
fn two_changes_further_apart_stay_separate() {
    assert_eq!(
        changes("one keep two", "uno keep dos"),
        (vec!["one", "two"], vec!["uno", "dos"])
    );
}

#[test]
fn an_insertion_marks_only_the_new_side() {
    assert_eq!(
        changes("let x = 1;", "let mut x = 1;"),
        (vec![], vec!["mut "])
    );
}

#[test]
fn equal_lines_have_no_ranges() {
    assert_eq!(word_changes("same", "same"), Some((vec![], vec![])));
}

#[test]
fn a_line_over_1000_bytes_has_no_ranges() {
    let at_limit = "a".repeat(1000);
    let over = "a".repeat(1001);
    assert!(word_changes(&at_limit, "b").is_some());
    assert_eq!(word_changes(&over, "b"), None);
    assert_eq!(word_changes("b", &over), None);
}

#[test]
fn multi_byte_ranges_end_on_char_boundaries() {
    let (old, new) = ("let s = \"naïve\";", "let s = \"naive\";");
    let (old_ranges, new_ranges) = word_changes(old, new).unwrap();
    for (line, ranges) in [(old, &old_ranges), (new, &new_ranges)] {
        assert!(!ranges.is_empty());
        for range in ranges {
            assert!(line.is_char_boundary(range.start as usize));
            assert!(line.is_char_boundary(range.end as usize));
        }
    }
    assert_eq!(marked(old, &old_ranges), ["naïve"]);
    assert_eq!(marked(new, &new_ranges), ["naive"]);

    let (old, new) = ("名前 = 値", "名称 = 値");
    let (old_ranges, new_ranges) = word_changes(old, new).unwrap();
    for (line, ranges) in [(old, &old_ranges), (new, &new_ranges)] {
        for range in ranges {
            assert!(line.is_char_boundary(range.start as usize));
            assert!(line.is_char_boundary(range.end as usize));
        }
    }
    assert!(marked(old, &old_ranges).concat().contains('前'));
    assert!(marked(new, &new_ranges).concat().contains('称'));
}
