use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use rubrix_edit::word_at;

#[test]
fn a_word_is_an_identifier_a_space_run_or_one_char() {
    let line = "    self.foo_bar(x, 日本);";
    let word = |at| &line[word_at(line, at)];
    assert_eq!(word(9), "foo_bar");
    assert_eq!(word(4), "self");
    assert_eq!(word(8), ".");
    assert_eq!(word(1), "    ");
    assert_eq!(word(line.find('日').unwrap()), "日本");
    assert_eq!(word(line.len()), ";");
    assert_eq!(word_at("", 0), 0..0);
}

#[test]
fn a_combining_accent_stays_with_its_letter() {
    let line = "cafe\u{301} au";
    assert_eq!(word_at(line, 0), 0.."cafe\u{301}".len());
    assert_eq!(word_at(line, "cafe".len()), 0.."cafe\u{301}".len());
}

#[test]
fn an_other_grapheme_stands_alone() {
    assert_eq!(word_at("a::b", 1), 1..2);
    assert_eq!(word_at("a::b", 2), 2..3);
}

/// `bevy_rubrix`'s char-based `select::word_at` as it was before it
/// moved here, the oracle for the fixture test below.
fn char_word_at(line: &str, at: usize) -> Range<usize> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Chars {
        Word,
        Space,
        Other,
    }
    let Some((start, first)) = line[at..]
        .chars()
        .next()
        .map(|c| (at, c))
        .or_else(|| line[..at].char_indices().next_back())
    else {
        return at..at;
    };
    let kind = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Chars::Word
        } else if c.is_whitespace() {
            Chars::Space
        } else {
            Chars::Other
        }
    };
    let class = kind(first);
    if class == Chars::Other {
        return start..start + first.len_utf8();
    }
    let end = line[start..]
        .char_indices()
        .find(|&(_, c)| kind(c) != class)
        .map_or(line.len(), |(off, _)| start + off);
    let begin = line[..start]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| kind(c) == class)
        .last()
        .map_or(start, |(off, _)| off);
    begin..end
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("a fixture directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn word_at_matches_the_char_classifier_on_every_fixture_byte() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files =
        vec![crates.join("../bench/fixtures/rust/large.rs.txt")];
    files_under(
        &crates.join("bevy_rubrix/examples/gallery/fixtures"),
        &mut files,
    );
    let mut checked = 0;
    for path in &files {
        let text = fs::read_to_string(path).expect("a text fixture");
        for (number, line) in text.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let ats = line
                .char_indices()
                .map(|(at, _)| at)
                .chain([line.len()]);
            for at in ats {
                assert_eq!(
                    word_at(line, at),
                    char_word_at(line, at),
                    "{} line {} byte {at}: {line:?}",
                    path.display(),
                    number + 1,
                );
                checked += 1;
            }
        }
    }
    assert!(
        files.len() > 10 && checked > 100_000,
        "{checked} bytes of {} files",
        files.len()
    );
}
