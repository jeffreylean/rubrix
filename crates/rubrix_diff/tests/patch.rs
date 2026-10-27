use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rubrix_diff::{
    Block, Expansion, FileChange, FileDiff, Hunk, Invariant, Layout,
    PatchError, PatchErrorKind, Row, parse_patch, rows,
};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn patch_text(name: &str) -> String {
    let path = manifest().join("tests/patches").join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn files(name: &str) -> Vec<FileDiff> {
    let files = parse_patch(&patch_text(&format!("{name}.patch")))
        .unwrap_or_else(|err| panic!("{name}: {err}"));
    for file in &files {
        assert!(file.is_partial(), "{name} is partial");
        assert_eq!(file.check(), Ok(()), "{name}");
    }
    files
}

fn only(name: &str) -> FileDiff {
    let [file] = <[FileDiff; 1]>::try_from(files(name))
        .unwrap_or_else(|files| {
            panic!("{name} has {} files, not 1", files.len())
        });
    file
}

fn headers(file: &FileDiff) -> Vec<String> {
    let header = |hunk: &Hunk| hunk.header().to_string();
    file.hunks().iter().map(header).collect()
}

fn lines(lines: &rubrix_diff::Lines) -> Vec<&str> {
    (0..lines.len()).map(|i| lines.line(i)).collect()
}

#[test]
fn modified_file_gets_its_hunks_headings_and_counts() {
    let file = only("modified");
    assert_eq!(file.change(), FileChange::Modified);
    assert_eq!(file.old_name(), Some("lib.rs"));
    assert_eq!(file.new_name(), Some("lib.rs"));
    assert_eq!((file.added(), file.removed()), (2, 2));
    assert_eq!(
        headers(&file),
        ["@@ -1,7 +1,7 @@", "@@ -26,7 +26,7 @@ line 21"]
    );
    assert_eq!(file.hunks()[1].heading(), "line 21");
    let hunk = &file.hunks()[0];
    assert_eq!((hunk.old_len(), hunk.new_len()), (7, 7));
    assert_eq!(hunk.blocks().len(), 3);
    assert!(matches!(
        hunk.blocks()[1],
        Block::Change {
            old_len: 1,
            new_len: 1,
            ..
        }
    ));
    let table = rows(&file, Layout::Stacked, &Expansion::new());
    let numbers: Vec<_> = table
        .range(..)
        .filter_map(|row| row.new_line().map(|l| l.number.get()))
        .collect();
    assert_eq!(&numbers[..4], [1, 2, 3, 4]);
    assert_eq!(numbers.last(), Some(&32));
    assert!(table.range(..).any(|r| matches!(r, Row::Gap { .. })));
}

#[test]
fn hunk_counts_default_to_one() {
    let file = only("one-line");
    assert_eq!(headers(&file), ["@@ -1 +1 @@"]);
    assert_eq!(lines(file.old_lines()), ["one"]);
    assert_eq!(lines(file.new_lines()), ["two"]);
}

#[test]
fn renamed_and_edited_file_keeps_both_names() {
    let file = only("rename");
    assert_eq!(file.change(), FileChange::RenamedModified);
    assert_eq!(file.old_name(), Some("docs.md"));
    assert_eq!(file.new_name(), Some("guide.md"));
    assert_eq!((file.added(), file.removed()), (1, 1));
    assert_eq!(file.hunks().len(), 1);
}

#[test]
fn pure_rename_has_names_and_no_hunks() {
    let file = only("rename-pure");
    assert_eq!(file.change(), FileChange::Renamed);
    assert_eq!(file.old_name(), Some("keep.txt"));
    assert_eq!(file.new_name(), Some("kept.txt"));
    assert!(file.hunks().is_empty());
}

#[test]
fn mode_only_change_has_both_modes_and_no_hunks() {
    let file = only("mode");
    assert_eq!(file.change(), FileChange::Modified);
    assert_eq!(file.old_name(), Some("script.sh"));
    assert_eq!(file.new_name(), Some("script.sh"));
    assert_eq!(file.old_mode(), Some(0o100644));
    assert_eq!(file.new_mode(), Some(0o100755));
    assert!(file.hunks().is_empty());
}

#[test]
fn added_and_deleted_files_have_lines_on_one_side() {
    let [added, deleted] =
        <[FileDiff; 2]>::try_from(files("add-delete")).unwrap();
    assert_eq!(added.change(), FileChange::Added);
    assert_eq!(added.old_name(), None);
    assert_eq!(added.new_name(), Some("added.txt"));
    assert_eq!(added.old_mode(), None);
    assert_eq!(added.new_mode(), Some(0o100644));
    assert_eq!(headers(&added), ["@@ -0,0 +1,2 @@"]);
    assert_eq!((added.added(), added.removed()), (2, 0));
    assert!(added.old_lines().is_empty());
    assert_eq!(lines(added.new_lines()), ["new", "file"]);

    assert_eq!(deleted.change(), FileChange::Deleted);
    assert_eq!(deleted.old_name(), Some("doomed.txt"));
    assert_eq!(deleted.new_name(), None);
    assert_eq!(deleted.old_mode(), Some(0o100644));
    assert_eq!(headers(&deleted), ["@@ -1,3 +0,0 @@"]);
    assert_eq!((deleted.added(), deleted.removed()), (0, 3));
    assert!(deleted.new_lines().is_empty());
}

#[test]
fn empty_added_file_has_no_hunks() {
    let file = only("empty-add");
    assert_eq!(file.change(), FileChange::Added);
    assert_eq!(file.new_name(), Some("empty.txt"));
    assert!(file.hunks().is_empty());
}

#[test]
fn no_newline_marker_ends_the_side_it_follows() {
    let file = only("no-newline");
    assert!(file.old_lines().missing_newline());
    assert!(!file.new_lines().missing_newline());
    assert_eq!(file.old_lines().raw(1), "second");
    assert_eq!(file.new_lines().raw(1), "secondlast\n");

    let file = only("no-newline-both");
    assert!(file.old_lines().missing_newline());
    assert!(file.new_lines().missing_newline());
    assert_eq!(file.new_lines().raw(1), "changed");
    assert_eq!((file.added(), file.removed()), (1, 1));
}

#[test]
fn binary_files_have_no_hunks_and_no_lines() {
    for name in ["binary", "binary-full"] {
        let file = only(name);
        assert_eq!(file.change(), FileChange::Binary, "{name}");
        assert_eq!(file.old_name(), Some("logo.png"), "{name}");
        assert_eq!(file.new_name(), Some("logo.png"), "{name}");
        assert!(file.hunks().is_empty(), "{name}");
        assert!(file.old_lines().is_empty(), "{name}");
        assert!(file.new_lines().is_empty(), "{name}");
        assert_eq!((file.added(), file.removed()), (0, 0), "{name}");
    }
}

#[test]
fn copies_stay_apart_from_renames() {
    let file = only("copy");
    assert_eq!(file.change(), FileChange::Copied);
    assert_eq!(file.old_name(), Some("lib.rs"));
    assert_eq!(file.new_name(), Some("util.rs"));
    assert!(file.hunks().is_empty());

    let file = only("copy-edited");
    assert_eq!(file.change(), FileChange::Copied);
    assert_eq!(file.new_name(), Some("util.rs"));
    assert_eq!(headers(&file).len(), 1);
    assert_eq!((file.added(), file.removed()), (1, 1));
}

#[test]
fn quoted_and_spaced_names_are_decoded() {
    let [accent, spaced] =
        <[FileDiff; 2]>::try_from(files("quoted")).unwrap();
    assert_eq!(accent.old_name(), Some("h\u{e9}llo.txt"));
    assert_eq!(accent.new_name(), Some("h\u{e9}llo.txt"));
    assert_eq!(spaced.old_name(), Some("spaced name.txt"));
    assert_eq!(spaced.new_name(), Some("spaced name.txt"));
    assert_eq!(lines(spaced.new_lines()), ["spaced out"]);
}

#[test]
fn mail_headers_and_signature_around_files_are_skipped() {
    let [docs, lib] =
        <[FileDiff; 2]>::try_from(files("format-patch")).unwrap();
    assert_eq!(docs.new_name(), Some("docs.md"));
    assert_eq!(lib.new_name(), Some("lib.rs"));
    assert_eq!((docs.added(), docs.removed()), (1, 1));
    assert_eq!((lib.added(), lib.removed()), (1, 1));
    assert_eq!(lines(lib.new_lines())[3], "line three");
}

#[test]
fn plain_unified_diff_keeps_its_names_as_given() {
    let file = only("plain");
    assert_eq!(file.change(), FileChange::Modified);
    assert_eq!(file.old_name(), Some("old/plain.txt"));
    assert_eq!(file.new_name(), Some("new/plain.txt"));
    assert_eq!(headers(&file), ["@@ -1,7 +1,7 @@"]);
}

#[test]
fn git_header_names_are_kept_when_no_later_line_names_the_file() {
    let binary = only("noindex-binary");
    assert_eq!(binary.change(), FileChange::Binary);
    assert_eq!(binary.old_name(), Some("a/i.bin"));
    assert_eq!(binary.new_name(), Some("b/i.bin"));

    let mode = only("noindex-mode");
    assert_eq!(mode.change(), FileChange::Modified);
    assert_eq!(mode.old_name(), Some("a/m.sh"));
    assert_eq!(mode.new_name(), Some("b/m.sh"));
    assert_eq!(mode.new_mode(), Some(0o100755));

    let [accent, spaced] =
        <[FileDiff; 2]>::try_from(files("noindex-quoted")).unwrap();
    assert_eq!(accent.old_name(), Some("a/h\u{e9}llo.bin"));
    assert_eq!(accent.new_name(), Some("b/h\u{e9}llo.bin"));
    assert_eq!(spaced.old_name(), Some("a/spaced name.bin"));
    assert_eq!(spaced.new_name(), Some("b/spaced name.bin"));
}

#[test]
fn plain_binary_files_become_binary_entries() {
    let all = files("diff-r-binary");
    let found: Vec<_> = all
        .iter()
        .map(|f| (f.change(), f.old_name(), f.new_name()))
        .collect();
    assert_eq!(
        found,
        [
            (
                FileChange::Binary,
                Some("ru-old/added.bin"),
                Some("ru-new/added.bin")
            ),
            (
                FileChange::Modified,
                Some("ru-old/added.txt"),
                Some("ru-new/added.txt")
            ),
            (
                FileChange::Binary,
                Some("ru-old/gone.bin"),
                Some("ru-new/gone.bin")
            ),
            (
                FileChange::Binary,
                Some("ru-old/i.bin"),
                Some("ru-new/i.bin")
            ),
            (
                FileChange::Modified,
                Some("ru-old/t.txt"),
                Some("ru-new/t.txt")
            ),
        ]
    );
    assert_eq!((all[4].added(), all[4].removed()), (1, 1));

    let added = only("diff-null-binary");
    assert_eq!(added.change(), FileChange::Binary);
    assert_eq!(added.old_name(), None);
    assert_eq!(added.new_name(), Some("ru-new/added.bin"));
}

#[test]
fn empty_context_lines_parse_in_lf_and_crlf_patches() {
    let lf = only("blank-context-lf");
    let crlf = only("blank-context-crlf");
    assert_eq!((crlf.added(), crlf.removed()), (1, 1));
    assert_eq!(headers(&crlf), headers(&lf));
    assert_eq!(lines(crlf.new_lines()), lines(lf.new_lines()));
    assert_eq!(crlf.new_lines().raw(1), "\r\n");
}

#[test]
fn text_without_a_file_header_has_no_files() {
    for text in ["", "\n", "hello\nworld\n", "@@ -1 +1 @@\n-a\n+b\n"]
    {
        assert_eq!(parse_patch(text), Ok(Vec::new()), "{text:?}");
    }
}

#[test]
fn a_patch_of_many_files_gives_each_in_order() {
    let names = [
        "modified",
        "rename",
        "mode",
        "add-delete",
        "binary",
        "copy",
        "quoted",
        "format-patch",
        "plain",
    ];
    let text: String = names
        .iter()
        .map(|name| patch_text(&format!("{name}.patch")))
        .collect();
    let all = parse_patch(&text).unwrap();
    let each: Vec<_> = names.iter().flat_map(|n| files(n)).collect();
    assert_eq!(all, each);
    assert_eq!(all.len(), 12);
}

#[test]
fn crlf_patch_keeps_the_carriage_returns_in_its_lines() {
    let text = "diff --git a/f b/f\r\n--- a/f\r\n+++ b/f\r\n@@ -1,2 \
                +1,2 @@\r\n a\r\n-b\r\n+c\r\n";
    let file = &parse_patch(text).unwrap()[0];
    assert_eq!(file.old_name(), Some("f"));
    assert_eq!(file.new_lines().raw(1), "c\r\n");
    assert_eq!(file.new_lines().line(1), "c");
    assert_eq!(headers(file), ["@@ -1,2 +1,2 @@"]);
}

fn expect_error(text: &str, line: usize, kind: PatchErrorKind) {
    assert_eq!(
        parse_patch(text),
        Err(PatchError { line, kind }),
        "{text:?}"
    );
}

#[test]
fn errors_name_the_line_of_a_truncated_hunk() {
    let text = patch_text("modified.patch");
    let all: Vec<&str> = text.lines().collect();
    let at = |prefix: &str| {
        all.iter().position(|l| l.starts_with(prefix)).unwrap()
    };
    let (first, second) = (at("@@ -1,"), at("@@ -26,"));

    let mut cut = all.clone();
    cut.remove(second - 1);
    let cut = cut.join("\n");
    expect_error(&cut, first + 1, PatchErrorKind::HunkCount);

    let mut cut = all.clone();
    cut.pop();
    expect_error(
        &cut.join("\n"),
        second + 1,
        PatchErrorKind::HunkCount,
    );

    let mut extra = all.clone();
    extra.insert(second, "+one too many");
    expect_error(
        &extra.join("\n"),
        first + 1,
        PatchErrorKind::HunkCount,
    );
}

#[test]
fn errors_name_the_truncated_hunk_of_a_plain_multi_file_patch() {
    let text = patch_text("quilt.patch");
    let all: Vec<&str> = text.lines().collect();
    assert_eq!(parse_patch(&text).unwrap().len(), 2);
    let second =
        all.iter().rposition(|l| l.starts_with("--- ")).unwrap();
    let mut cut = all.clone();
    cut.remove(second - 1);
    expect_error(&cut.join("\n"), 3, PatchErrorKind::HunkCount);
}

#[test]
fn a_hunk_reads_by_its_counts_when_its_lines_look_like_file_headers()
{
    let parsed = files("dashes");
    let [file] = <[FileDiff; 1]>::try_from(parsed).unwrap();
    assert_eq!(
        headers(&file),
        ["@@ -2 +2 @@ top", "@@ -5 +5 @@ mid 2"]
    );
    let path = manifest().join("tests/patches/dashes.patch");
    assert_eq!(counts(&[file]), numstat(&repo(), &path));
}

#[test]
fn errors_name_the_line_of_a_bad_hunk_header() {
    let header = |bad: &str| {
        format!(
            "preamble\ndiff --git a/f b/f\n--- a/f\n+++ \
             b/f\n{bad}\n-a\n+b\n"
        )
    };
    for bad in [
        "@@ -1,x +1 @@",
        "@@ -1 +1",
        "@@ -0,2 +1 @@",
        "@@ -1 +0 @@",
        "@@ -1 +1,2",
        "@@ 1 +1 @@",
    ] {
        expect_error(&header(bad), 5, PatchErrorKind::HunkHeader);
    }
}

#[test]
fn errors_name_lines_that_break_the_diff_model() {
    let marker = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 \
                  @@\n\\ No newline at end of file\n-a\n+b\n";
    expect_error(
        marker,
        5,
        PatchErrorKind::Model(Invariant::Newline),
    );

    let overlap =
        "--- a\n+++ b\n@@ -3 +3 @@\n-a\n+b\n@@ -1 +1 @@\n-c\n+d\n";
    expect_error(
        overlap,
        6,
        PatchErrorKind::Model(Invariant::Overlap { hunk: 1 }),
    );

    let kind = "--- /dev/null\n+++ b\n@@ -1 +1 @@\n-a\n+b\n";
    expect_error(kind, 1, PatchErrorKind::Model(Invariant::Kind));
}

#[test]
fn errors_display_their_line() {
    let err = parse_patch("--- a\n+++ b\n@@ -1 +1\n").unwrap_err();
    assert_eq!(err.to_string(), "line 3: malformed hunk header");
}

fn repo() -> PathBuf {
    manifest().join("../..")
}

fn git(dir: &Path, args: &[&str]) -> (i32, String) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    (
        output.status.code().expect("git exits"),
        String::from_utf8(output.stdout).expect("git prints UTF-8"),
    )
}

/// Git ignores the files of a patch that are outside the directory
/// it runs in.
fn numstat(
    dir: &Path,
    patch: &Path,
) -> Vec<(Option<u32>, Option<u32>)> {
    let path = patch.to_str().unwrap();
    let (code, out) = git(dir, &["apply", "--numstat", path]);
    assert_eq!(code, 0, "git apply --numstat {path}");
    out.lines()
        .map(|line| {
            let mut cells = line.split('\t');
            let mut count = || cells.next().unwrap().parse().ok();
            (count(), count())
        })
        .collect()
}

fn counts(files: &[FileDiff]) -> Vec<(Option<u32>, Option<u32>)> {
    files
        .iter()
        .map(|file| match file.change() {
            FileChange::Binary => (None, None),
            _ => (Some(file.added()), Some(file.removed())),
        })
        .collect()
}

fn rebuild(old: &str, diff: &FileDiff) -> String {
    struct Rebuild<'a> {
        old: Vec<&'a str>,
        out: String,
        next_old: usize,
        lines_made: usize,
    }

    impl Rebuild<'_> {
        fn keep_until(&mut self, end: usize) {
            for line in &self.old[self.next_old..end] {
                self.out.push_str(line);
                self.lines_made += 1;
            }
            self.next_old = self.next_old.max(end);
        }

        fn take(&mut self, number: u32, raw: &str) {
            self.keep_until(number as usize - 1);
            assert_eq!(self.old[self.next_old], raw);
            self.next_old += 1;
        }

        fn push(&mut self, raw: &str) {
            self.out.push_str(raw);
            self.lines_made += 1;
        }
    }

    let mut build = Rebuild {
        old: old.split_inclusive('\n').collect(),
        out: String::new(),
        next_old: 0,
        lines_made: 0,
    };
    let table = rows(diff, Layout::Stacked, &Expansion::new());
    for row in table.range(..) {
        match row {
            Row::Context { old, new } => {
                build.take(
                    old.number.get(),
                    diff.old_lines().raw(old.index),
                );
                build.push(diff.new_lines().raw(new.index));
            }
            Row::Deletion { old } => {
                build.take(
                    old.number.get(),
                    diff.old_lines().raw(old.index),
                );
            }
            Row::Addition { new } => {
                let before = new.number.get() as usize - 1;
                build.keep_until(
                    build.next_old + before - build.lines_made,
                );
                build.push(diff.new_lines().raw(new.index));
            }
            Row::Gap { .. } => {}
            Row::Expanded { .. } | Row::Paired { .. } => {
                unreachable!("stacked rows of a partial diff")
            }
        }
    }
    build.keep_until(build.old.len());
    build.out
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn round_trip_rebuilds_the_new_file_and_matches_numstat() {
    let edits = scratch("r10-edits");
    let script = repo().join("bench/gen-edits.sh");
    let status = Command::new("bash")
        .arg(&script)
        .arg(&edits)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("bash runs");
    assert!(status.success(), "{}", script.display());

    let mut pairs: Vec<_> = fs::read_dir(&edits)
        .unwrap()
        .map(|entry| {
            entry.unwrap().file_name().into_string().unwrap()
        })
        .collect();
    pairs.sort();
    assert_eq!(pairs.len(), 10);
    for pair in pairs {
        let (old, new) = (
            format!("{pair}/old.rs.txt"),
            format!("{pair}/new.rs.txt"),
        );
        let (code, patch) = git(
            &edits,
            &["diff", "--no-index", "--no-color", &old, &new],
        );
        assert_eq!(code, 1, "{pair} differs");
        let patch_path = edits.join(format!("{pair}.patch"));
        fs::write(&patch_path, &patch).unwrap();

        let parsed = parse_patch(&patch).unwrap();
        let [file] = <[FileDiff; 1]>::try_from(parsed).unwrap();
        let rebuilt = rebuild(
            &fs::read_to_string(edits.join(&old)).unwrap(),
            &file,
        );
        let want = fs::read_to_string(edits.join(&new)).unwrap();
        assert!(rebuilt == want, "{pair} rebuilds the new file");
        let want = numstat(&repo(), &patch_path);
        assert_eq!(counts(&[file]), want, "{pair} counts");
    }
}

#[test]
fn bench_patches_match_numstat() {
    let dir = repo().join("bench/patch");
    for name in ["large.patch.txt", "stress.patch.txt"] {
        let path = dir.join(name);
        let text = fs::read_to_string(&path).unwrap();
        let parsed = parse_patch(&text).unwrap();
        for file in &parsed {
            assert_eq!(file.check(), Ok(()), "{name}");
        }
        assert_eq!(
            counts(&parsed),
            numstat(&repo(), &path),
            "{name}"
        );
    }
}
