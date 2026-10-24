use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::model::{
    Edit, FileChange, FileDiff, Invariant, Meta, PatchBuilder,
    header_start,
};

/// Why [`parse_patch`] gave up, and where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatchError {
    /// The 1-based line of the patch that broke. A hunk whose lines
    /// disagree with its header counts is named by its `@@` line.
    pub line: usize,
    pub kind: PatchErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatchErrorKind {
    /// An `@@` line that is not `@@ -a[,b] +c[,d] @@`, or whose start
    /// is 0 for a side with lines.
    HunkHeader,
    /// The hunk's body has fewer or more lines than its header
    /// counts.
    HunkCount,
    /// The patch breaks a rule of the diff model: a `\ No newline`
    /// line with nothing before it, a hunk before the end of the one
    /// above it, or a file kind that disagrees with its hunks.
    Model(Invariant),
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: ", self.line)?;
        match self.kind {
            PatchErrorKind::HunkHeader => {
                f.write_str("malformed hunk header")
            }
            PatchErrorKind::HunkCount => f.write_str(
                "the hunk's lines disagree with the counts in its \
                 header",
            ),
            PatchErrorKind::Model(invariant) => invariant.fmt(f),
        }
    }
}

impl core::error::Error for PatchError {}

/// Parses a patch into one partial [`FileDiff`] per file, in patch
/// order.
///
/// Reads `git diff`, `git log -p` and `git format-patch` output, and
/// plain `diff -u` output. Text before the first `diff --git` or
/// `---` header is skipped, as are mail headers, commit messages and
/// signatures between files. A file's header may hold any of
/// `old mode`, `new mode`, `new file mode`, `deleted file mode`,
/// `rename from` and `to`, `copy from` and `to`, and `---` and `+++`
/// names. A `Binary files ... differ` or `GIT binary patch` file
/// becomes [`FileChange::Binary`] with no hunks.
///
/// A name has its `a/` or `b/` prefix removed in a git patch, and
/// its C-style quoting undone. A body line may be an empty line for
/// an empty context line. `\r` stays part of a line.
///
/// Every file is partial: it holds the lines inside its hunks and
/// nothing else, so its gaps cannot expand.
///
/// # Errors
///
/// Fails with the 1-based line of the first hunk whose lines
/// disagree with its header counts, whose header is malformed, or
/// whose lines break the diff model.
pub fn parse_patch(text: &str) -> Result<Vec<FileDiff>, PatchError> {
    let mut input = Input {
        rest: text,
        line: 1,
    };
    let mut files = Vec::new();
    while let Some(head) = next_file(&mut input) {
        files.push(parse_file(&mut input, head)?);
    }
    Ok(files)
}

/// The lines not read yet.
struct Input<'a> {
    rest: &'a str,
    /// The 1-based number of the next line.
    line: usize,
}

impl<'a> Input<'a> {
    fn ahead(&self) -> impl Iterator<Item = &'a str> {
        let lines = self.rest.split_inclusive('\n');
        lines.map(|line| line.strip_suffix('\n').unwrap_or(line))
    }

    fn peek(&self) -> Option<&'a str> {
        self.ahead().next()
    }

    fn next(&mut self) -> Option<&'a str> {
        let line = self.rest.split_inclusive('\n').next()?;
        self.rest = &self.rest[line.len()..];
        self.line += 1;
        Some(line.strip_suffix('\n').unwrap_or(line))
    }
}

fn trim_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
}

/// What a file's header lines say so far.
struct Head {
    line: usize,
    git: bool,
    kind: Kind,
    binary: bool,
    old_name: Option<String>,
    new_name: Option<String>,
    old_mode: Option<u32>,
    new_mode: Option<u32>,
}

/// Skips to the next file's header. For a git file, takes the
/// `diff --git` line and leaves the rest.
fn next_file(input: &mut Input) -> Option<Head> {
    loop {
        let git = input.peek()?.starts_with("diff --git ");
        if git || starts_plain_file(input) {
            let mut head = Head {
                line: input.line,
                git,
                kind: Kind::Modified,
                binary: false,
                old_name: None,
                new_name: None,
                old_mode: None,
                new_mode: None,
            };
            if git {
                let line = input.next()?;
                let rest = line.strip_prefix("diff --git ")?;
                head.old_name = same_name(trim_cr(rest));
                head.new_name = head.old_name.clone();
            }
            return Some(head);
        }
        input.next();
    }
}

fn starts_plain_file(input: &Input) -> bool {
    let mut lines = input.ahead();
    let first = lines.next().is_some_and(|l| l.starts_with("--- "));
    first && lines.next().is_some_and(|l| l.starts_with("+++ "))
}

fn parse_file(
    input: &mut Input,
    mut head: Head,
) -> Result<FileDiff, PatchError> {
    read_header(input, &mut head);
    let mut builder = PatchBuilder::new();
    let mut hunks = 0;
    while let Some(line) = input.peek() {
        if line.starts_with("@@ ") {
            parse_hunk(input, &mut builder)?;
            hunks += 1;
        } else if line.starts_with("diff --git ")
            || starts_plain_file(input)
        {
            break;
        } else {
            input.next();
        }
    }
    let line = head.line;
    builder
        .finish(head.into_meta(hunks > 0))
        .map_err(|invariant| PatchError {
            line,
            kind: PatchErrorKind::Model(invariant),
        })
}

fn read_header(input: &mut Input, head: &mut Head) {
    while let Some(line) = input.peek() {
        if line.starts_with("diff --git ") || line.starts_with("@@ ")
        {
            break;
        }
        input.next();
        let line = trim_cr(line);
        if let Some(name) = line.strip_prefix("--- ") {
            head.old_name = path_name(name, head.git, "a/");
            if head.old_name.is_none() {
                head.kind = Kind::Added;
            }
        } else if let Some(name) = line.strip_prefix("+++ ") {
            head.new_name = path_name(name, head.git, "b/");
            if head.new_name.is_none() {
                head.kind = Kind::Deleted;
            }
        } else if let Some(mode) = line.strip_prefix("old mode ") {
            head.old_mode = parse_mode(mode);
        } else if let Some(mode) = line.strip_prefix("new mode ") {
            head.new_mode = parse_mode(mode);
        } else if let Some(mode) = line.strip_prefix("new file mode ")
        {
            head.kind = Kind::Added;
            head.new_mode = parse_mode(mode);
        } else if let Some(mode) =
            line.strip_prefix("deleted file mode ")
        {
            head.kind = Kind::Deleted;
            head.old_mode = parse_mode(mode);
        } else if let Some(name) = line.strip_prefix("rename from ") {
            head.kind = Kind::Renamed;
            head.old_name = Some(decode(name));
        } else if let Some(name) = line.strip_prefix("rename to ") {
            head.new_name = Some(decode(name));
        } else if let Some(name) = line.strip_prefix("copy from ") {
            head.kind = Kind::Copied;
            head.old_name = Some(decode(name));
        } else if let Some(name) = line.strip_prefix("copy to ") {
            head.new_name = Some(decode(name));
        } else if line == "GIT binary patch"
            || (line.starts_with("Binary files ")
                && line.ends_with(" differ"))
        {
            head.binary = true;
        }
    }
}

impl Head {
    fn into_meta(self, has_hunks: bool) -> Meta {
        let change = match (self.binary, self.kind, has_hunks) {
            (true, ..) => FileChange::Binary,
            (_, Kind::Added, _) => FileChange::Added,
            (_, Kind::Deleted, _) => FileChange::Deleted,
            (_, Kind::Renamed, false) => FileChange::Renamed,
            (_, Kind::Renamed, true) => FileChange::RenamedModified,
            (_, Kind::Copied, _) => FileChange::Copied,
            (_, Kind::Modified, _) => FileChange::Modified,
        };
        Meta {
            change,
            old_name: self
                .old_name
                .filter(|_| self.kind != Kind::Added),
            new_name: self
                .new_name
                .filter(|_| self.kind != Kind::Deleted),
            old_mode: self.old_mode,
            new_mode: self.new_mode,
        }
    }
}

fn parse_mode(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim(), 8).ok()
}

/// A `--- ` or `+++ ` name, `None` for `/dev/null`. Git adds a tab
/// after a name with a space, and plain `diff -u` a timestamp.
fn path_name(value: &str, git: bool, prefix: &str) -> Option<String> {
    let unquoted = value.starts_with('"');
    let value = match value.split_once('\t') {
        Some((name, _)) if !unquoted => name,
        _ => value,
    };
    if value == "/dev/null" {
        return None;
    }
    let mut name = decode(value);
    if git && name.starts_with(prefix) {
        name.drain(..prefix.len());
    }
    Some(name)
}

/// The name in a `diff --git a/<name> b/<name>` line, when both
/// halves are the same name. Any other line gives its names through
/// the `---` and `rename` lines that follow.
fn same_name(rest: &str) -> Option<String> {
    let middle = rest.len() / 2;
    if rest.len().is_multiple_of(2) || rest.as_bytes()[middle] != b' '
    {
        return None;
    }
    let strip = |half: &str, prefix: &str| {
        let name = decode(half);
        name.strip_prefix(prefix).map(String::from)
    };
    let old = strip(&rest[..middle], "a/")?;
    let new = strip(&rest[middle + 1..], "b/")?;
    (old == new).then_some(old)
}

/// A name as git writes it: bare, or in double quotes with C
/// escapes and octal bytes.
fn decode(value: &str) -> String {
    let Some(quoted) = value.strip_prefix('"') else {
        return value.into();
    };
    let mut bytes = Vec::with_capacity(quoted.len());
    let mut rest = quoted.bytes();
    while let Some(byte) = rest.next() {
        let byte = match byte {
            b'"' => break,
            b'\\' => match rest.next() {
                Some(b'a') => 7,
                Some(b'b') => 8,
                Some(b'f') => 12,
                Some(b'n') => b'\n',
                Some(b'r') => b'\r',
                Some(b't') => b'\t',
                Some(b'v') => 11,
                Some(first @ b'0'..=b'7') => {
                    let mut value = u32::from(first - b'0');
                    for _ in 0..2 {
                        let digits = rest.clone().next();
                        match digits {
                            Some(d @ b'0'..=b'7') => {
                                value =
                                    value * 8 + u32::from(d - b'0');
                                rest.next();
                            }
                            _ => break,
                        }
                    }
                    value as u8
                }
                Some(other) => other,
                None => break,
            },
            other => other,
        };
        bytes.push(byte);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn parse_hunk(
    input: &mut Input,
    builder: &mut PatchBuilder,
) -> Result<(), PatchError> {
    let start = input.line;
    let header = input.next().map(trim_cr).unwrap_or_default();
    let at = |kind| PatchError { line: start, kind };
    let ((old_start, old_count), (new_start, new_count), heading) =
        hunk_header(header).ok_or(at(PatchErrorKind::HunkHeader))?;
    let model = |line, invariant| PatchError {
        line,
        kind: PatchErrorKind::Model(invariant),
    };
    builder
        .hunk(old_start, new_start, heading)
        .map_err(|invariant| model(start, invariant))?;

    loop {
        let (old_len, new_len) = builder.hunk_lens();
        if old_len >= old_count && new_len >= new_count {
            break;
        }
        let here = input.line;
        let line =
            input.next().ok_or(at(PatchErrorKind::HunkCount))?;
        let (edit, text) = match line.as_bytes().first() {
            Some(b' ') => (Edit::Context, &line[1..]),
            None => (Edit::Context, ""),
            Some(b'-') => (Edit::Delete, &line[1..]),
            Some(b'+') => (Edit::Insert, &line[1..]),
            Some(b'\\') => {
                builder
                    .no_newline()
                    .map_err(|invariant| model(here, invariant))?;
                continue;
            }
            Some(_) => return Err(at(PatchErrorKind::HunkCount)),
        };
        let fits = match edit {
            Edit::Context => {
                old_len < old_count && new_len < new_count
            }
            Edit::Delete => old_len < old_count,
            Edit::Insert => new_len < new_count,
        };
        if !fits {
            return Err(at(PatchErrorKind::HunkCount));
        }
        builder
            .line(edit, text)
            .map_err(|invariant| model(here, invariant))?;
    }

    while input.peek().is_some_and(|line| line.starts_with('\\')) {
        let here = input.line;
        input.next();
        builder
            .no_newline()
            .map_err(|invariant| model(here, invariant))?;
    }
    if input.peek().is_some_and(is_body_line) {
        return Err(at(PatchErrorKind::HunkCount));
    }
    Ok(())
}

/// A line that can only continue a hunk, not start whatever follows
/// it: a `--- ` file header and a mail signature's `-- ` start with
/// `-` as well.
fn is_body_line(line: &str) -> bool {
    match line.as_bytes().first() {
        Some(b' ' | b'+') => true,
        Some(b'-') => {
            !line.starts_with("--- ") && line.trim_end() != "--"
        }
        _ => false,
    }
}

type Range = (u32, u32);

/// `@@ -a[,b] +c[,d] @@[ heading]` as the 0-based `(start, count)` of
/// each side and the heading.
fn hunk_header(line: &str) -> Option<(Range, Range, &str)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, heading) = rest.split_once(" @@")?;
    let heading = heading.strip_prefix(' ').unwrap_or(heading);
    Some((side(old)?, side(new)?, heading))
}

fn side(text: &str) -> Option<Range> {
    let (start, count) = match text.split_once(',') {
        Some((start, count)) => (start, count.parse().ok()?),
        None => (text, 1),
    };
    let start = header_start(start.parse().ok()?, count)?;
    Some((start, count))
}
