//! The unified diff lexer, for `.diff` and `.patch`.
//!
//! A hunk line is one run, so the lexer never looks inside the code
//! a diff carries. The state holds the lines the current hunk still
//! owes, which decides whether a line that starts with `---` is a
//! removed line (`-` and then `-- note`) or a file header.
//!
//! | Line                      | Runs                                      |
//! | ------------------------- | ----------------------------------------- |
//! | `+added`                  | `diff.plus`                               |
//! | `-removed`                | `diff.minus`                              |
//! | ` context`                | `plain`                                   |
//! | `\ No newline at ...`     | `comment`                                 |
//! | `@@ -1,2 +1,3 @@ fn f()` | `punctuation.special`, `number`, `title`  |
//! | `--- a/x`, `+++ b/x`      | `operator`, `namespace`                   |
//! | `diff --git a b`          | `keyword`, `namespace`                    |
//! | `index 1a..2b 100644`     | `keyword`, `constant`                     |
//! | `new mode 100644`         | `keyword`, `number`                       |
//! | `rename from x`           | `keyword`, `namespace`                    |
//!
//! Anything else, such as the `commit` and `Author:` lines that
//! `git log -p` prints, is `plain`. Outside a hunk, a `+` or `-`
//! line that is not a file header still reads as added or removed,
//! so a snippet with no headers colours.

use super::space_end;
use crate::{Kind, Runs, State};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Hunk {
    old: u32,
    new: u32,
}

impl Hunk {
    fn load(state: &State) -> Hunk {
        let word = state.word();
        Hunk {
            old: (word >> 32) as u32,
            new: word as u32,
        }
    }

    fn store(self) -> State {
        State::from_word(
            u64::from(self.old) << 32 | u64::from(self.new),
        )
    }

    const fn is_open(self) -> bool {
        self.old > 0 || self.new > 0
    }
}

const HEADERS: [(&[u8], Kind); 14] = [
    (b"diff ", Kind::Namespace),
    (b"index ", Kind::Constant),
    (b"new file mode ", Kind::Number),
    (b"deleted file mode ", Kind::Number),
    (b"old mode ", Kind::Number),
    (b"new mode ", Kind::Number),
    (b"similarity index ", Kind::Number),
    (b"dissimilarity index ", Kind::Number),
    (b"rename from ", Kind::Namespace),
    (b"rename to ", Kind::Namespace),
    (b"copy from ", Kind::Namespace),
    (b"copy to ", Kind::Namespace),
    (b"Binary files ", Kind::Namespace),
    (b"GIT binary patch", Kind::Namespace),
];

pub(crate) fn lex(
    line: &str,
    state: &State,
    out: &mut Runs<'_>,
) -> State {
    let s = line.as_bytes();
    let mut hunk = Hunk::load(state);
    let end = s.len();
    match s.first() {
        None => {
            hunk.old = hunk.old.saturating_sub(1);
            hunk.new = hunk.new.saturating_sub(1);
        }
        Some(b' ') => {
            out.push(end, Kind::Plain);
            hunk.old = hunk.old.saturating_sub(1);
            hunk.new = hunk.new.saturating_sub(1);
        }
        Some(b'\\') => out.push(end, Kind::Comment),
        Some(b'+') if hunk.is_open() => {
            out.push(end, Kind::DiffPlus);
            hunk.new = hunk.new.saturating_sub(1);
        }
        Some(b'-') if hunk.is_open() => {
            out.push(end, Kind::DiffMinus);
            hunk.old = hunk.old.saturating_sub(1);
        }
        Some(&marker @ (b'+' | b'-')) => {
            hunk = Hunk::default();
            if is_file_header(s, marker) {
                file_header(s, out);
            } else if marker == b'+' {
                out.push(end, Kind::DiffPlus);
            } else {
                out.push(end, Kind::DiffMinus);
            }
        }
        Some(b'@') => hunk = hunk_header(s, out),
        Some(_) => {
            hunk = Hunk::default();
            header(s, out);
        }
    }
    hunk.store()
}

fn is_file_header(s: &[u8], marker: u8) -> bool {
    s.starts_with(&[marker; 3])
        && s.get(3).is_none_or(|&b| b == b' ' || b == b'\t')
}

fn file_header(s: &[u8], out: &mut Runs<'_>) {
    out.push(3, Kind::Operator);
    out.space(space_end(s, 3));
    out.push(s.len(), Kind::Namespace);
}

fn header(s: &[u8], out: &mut Runs<'_>) {
    let found =
        HEADERS.iter().find(|(prefix, _)| s.starts_with(prefix));
    let Some(&(prefix, value)) = found else {
        out.push(s.len(), Kind::Plain);
        return;
    };
    out.push(prefix.trim_ascii_end().len(), Kind::Keyword);
    out.space(prefix.len());
    out.push(s.len(), value);
}

fn hunk_header(s: &[u8], out: &mut Runs<'_>) -> Hunk {
    let markers = s.iter().take_while(|&&b| b == b'@').count();
    if markers < 2 {
        out.push(s.len(), Kind::Plain);
        return Hunk::default();
    }
    out.push(markers, Kind::PunctuationSpecial);
    let mut ranges = [None; 3];
    let mut found = 0;
    let mut i = markers;
    while i < s.len() {
        let start = space_end(s, i);
        out.space(start);
        if start == s.len() {
            break;
        }
        let end = word_end(s, start);
        let word = &s[start..end];
        if word.len() == markers && word.iter().all(|&b| b == b'@') {
            out.push(end, Kind::PunctuationSpecial);
            out.space(space_end(s, end));
            out.push(s.len(), Kind::Title);
            break;
        }
        match range(word) {
            Some(range) => {
                out.push(end, Kind::Number);
                if found < ranges.len() {
                    ranges[found] = Some(range);
                    found += 1;
                }
            }
            None => out.push(end, Kind::Plain),
        }
        i = end;
    }
    match (markers, ranges) {
        (2, [Some((b'-', old)), Some((b'+', new)), None]) => {
            Hunk { old, new }
        }
        _ => Hunk::default(),
    }
}

fn word_end(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && !s[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// A range such as `-12,7` or `+3` as its sign and line count.
fn range(word: &[u8]) -> Option<(u8, u32)> {
    let (&sign, rest) = word.split_first()?;
    if sign != b'-' && sign != b'+' {
        return None;
    }
    let digits =
        rest.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let count = match &rest[digits..] {
        [] => 1,
        [b',', tail @ ..] if tail.iter().all(u8::is_ascii_digit) => {
            number(tail)
        }
        _ => return None,
    };
    Some((sign, count))
}

fn number(digits: &[u8]) -> u32 {
    digits.iter().fold(0, |n: u32, &d| {
        n.saturating_mul(10).saturating_add(u32::from(d - b'0'))
    })
}

#[cfg(test)]
mod tests {
    use super::{Hunk, range};
    use crate::State;

    #[test]
    fn hunks_round_trip() {
        for hunk in [
            Hunk::default(),
            Hunk { old: 1, new: 0 },
            Hunk {
                old: u32::MAX,
                new: u32::MAX,
            },
        ] {
            assert_eq!(Hunk::load(&hunk.store()), hunk);
        }
        assert_eq!(Hunk::default().store(), State::default());
    }

    #[test]
    fn any_word_loads() {
        for word in [u64::MAX, 1, 1 << 40, 0xFFFF_FFFF] {
            let hunk = Hunk::load(&State::from_word(word));
            assert_eq!(Hunk::load(&hunk.store()), hunk);
        }
    }

    #[test]
    fn ranges() {
        assert_eq!(range(b"-12,7"), Some((b'-', 7)));
        assert_eq!(range(b"+3"), Some((b'+', 1)));
        assert_eq!(range(b"+0,0"), Some((b'+', 0)));
        assert_eq!(
            range(b"+99999999999,99999999999"),
            Some((b'+', u32::MAX))
        );
        assert_eq!(range(b"-"), None);
        assert_eq!(range(b"-a"), None);
        assert_eq!(range(b"fn"), None);
    }
}
