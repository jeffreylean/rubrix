use rubrix_diff::{
    Algorithm, Block, Edit, Expansion, FileChange, FileDiff, Hunk,
    Invariant, Layout, Lines, Meta, PatchBuilder, Row, Side,
    diff_texts, diff_texts_with, header_start, rows,
};

fn ctx(len: u32, old_start: u32, new_start: u32) -> Block {
    Block::Context {
        len,
        old_start,
        new_start,
    }
}

fn chg(
    old_start: u32,
    old_len: u32,
    new_start: u32,
    new_len: u32,
) -> Block {
    Block::Change {
        old_start,
        old_len,
        new_start,
        new_len,
    }
}

fn blocks(diff: &FileDiff) -> Vec<Vec<Block>> {
    let blocks = |hunk: &Hunk| hunk.blocks().to_vec();
    diff.hunks().iter().map(blocks).collect()
}

fn headers(diff: &FileDiff) -> Vec<String> {
    let header = |hunk: &Hunk| hunk.header().to_string();
    diff.hunks().iter().map(header).collect()
}

fn raws(lines: &Lines) -> Vec<&str> {
    (0..lines.len()).map(|i| lines.raw(i)).collect()
}

fn replay(diff: &FileDiff) -> String {
    let (old, new) = (diff.old_lines(), diff.new_lines());
    let mut out = String::new();
    let mut at = 0;
    for block in diff.hunks().iter().flat_map(Hunk::blocks) {
        let (olds, news) = (block.old_range(), block.new_range());
        (at..olds.start).for_each(|i| out.push_str(old.raw(i)));
        match block {
            Block::Context { .. } => {
                olds.clone().for_each(|i| out.push_str(old.raw(i)))
            }
            Block::Change { .. } => {
                news.for_each(|i| out.push_str(new.raw(i)))
            }
        }
        at = olds.end;
    }
    (at..old.len()).for_each(|i| out.push_str(old.raw(i)));
    out
}

#[test]
fn identical_texts_have_no_hunks_and_no_rows() {
    for text in ["", "a\n", "a\nb", "a\r\nb\r\n", "\n\n"] {
        let diff = diff_texts(text, text, 3);
        assert_eq!(diff.check(), Ok(()));
        assert!(diff.hunks().is_empty());
        assert_eq!((diff.added(), diff.removed()), (0, 0));
        assert_eq!(replay(&diff), text);
        let mut expansion = Expansion::new();
        expansion.reveal(0, Side::All, 0);
        for layout in [Layout::Stacked, Layout::Split] {
            let table = rows(&diff, layout, &expansion);
            assert!(table.is_empty());
            assert_eq!(table.len(), 0);
            assert_eq!(table.range(..).count(), 0);
            assert_eq!(table.range(3..9).len(), 0);
            assert_eq!(table.get(0), None);
            assert_eq!(table.find(rubrix_diff::Anchor::Old(0)), None);
            assert_eq!(table.hunk_rows(0), 0..0);
        }
    }
}

#[test]
fn empty_old_side_is_one_addition_block() {
    let diff = diff_texts("", "a\nb\n", 3);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(0, 0, 0, 2)]]);
    assert_eq!(headers(&diff), ["@@ -0,0 +1,2 @@"]);
    assert_eq!((diff.removed(), diff.added()), (0, 2));
    assert!(diff.old_lines().is_empty());
    assert!(!diff.old_lines().missing_newline());
    assert_eq!(replay(&diff), "a\nb\n");
    let table = rows(&diff, Layout::Stacked, &Expansion::new());
    for (row, number) in table.range(..).zip(1..) {
        assert!(matches!(row, Row::Addition { .. }));
        assert_eq!(row.old_line(), None);
        assert_eq!(row.new_line().unwrap().number.get(), number);
    }
    assert_eq!(table.len(), 2);
}

#[test]
fn empty_new_side_is_one_deletion_block() {
    let diff = diff_texts("a\nb\n", "", 3);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(0, 2, 0, 0)]]);
    assert_eq!(headers(&diff), ["@@ -1,2 +0,0 @@"]);
    assert_eq!((diff.removed(), diff.added()), (2, 0));
    assert!(diff.new_lines().is_empty());
    assert_eq!(replay(&diff), "");
    let table = rows(&diff, Layout::Split, &Expansion::new());
    for (row, number) in table.range(..).zip(1..) {
        assert!(matches!(row, Row::Deletion { .. }));
        assert_eq!(row.new_line(), None);
        assert_eq!(row.old_line().unwrap().number.get(), number);
    }
    assert_eq!(table.len(), 2);
}

#[test]
fn change_at_first_line() {
    let diff = diff_texts("a\nb\nc\nd\n", "x\nb\nc\nd\n", 1);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(0, 1, 0, 1), ctx(1, 1, 1)]]);
    assert_eq!(headers(&diff), ["@@ -1,2 +1,2 @@"]);
    assert_eq!(diff.old_lines().line(0), "a");
    assert_eq!(diff.new_lines().line(0), "x");
}

#[test]
fn change_at_last_line_without_trailing_newline() {
    let diff = diff_texts("b", "b\n", 3);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(0, 1, 0, 1)]]);
    assert_eq!((diff.removed(), diff.added()), (1, 1));
    assert!(diff.old_lines().missing_newline());
    assert!(!diff.new_lines().missing_newline());
    assert_eq!(diff.old_lines().line(0), "b");
    assert_eq!(diff.new_lines().line(0), "b");

    let diff = diff_texts("a\nb\n", "a\nb", 3);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[ctx(1, 0, 0), chg(1, 1, 1, 1)]]);
    assert!(!diff.old_lines().missing_newline());
    assert!(diff.new_lines().missing_newline());
    assert_eq!(replay(&diff), "a\nb");

    let diff = diff_texts("a\nb", "a\nc", 3);
    assert_eq!(blocks(&diff), [[ctx(1, 0, 0), chg(1, 1, 1, 1)]]);
    assert!(diff.old_lines().missing_newline());
    assert!(diff.new_lines().missing_newline());
}

#[test]
fn crlf_texts_diff_like_their_lf_copies() {
    let (old, new) = ("a\nb\nc\nd\ne\n", "a\nb\nX\nd\ne\n");
    let lf = diff_texts(old, new, 1);
    let crlf = diff_texts(
        &old.replace('\n', "\r\n"),
        &new.replace('\n', "\r\n"),
        1,
    );
    assert_eq!(crlf.check(), Ok(()));
    assert_eq!(blocks(&crlf), blocks(&lf));
    assert_eq!(headers(&crlf), headers(&lf));
    assert_eq!(crlf.new_lines().line(2), "X");
    assert_eq!(crlf.new_lines().raw(2), "X\r\n");
    assert_eq!(crlf.old_lines().line(2), "c");

    let mixed = diff_texts("a\r\nb\r\n", "a\nb\n", 3);
    assert_eq!(mixed.check(), Ok(()));
    assert_eq!((mixed.removed(), mixed.added()), (2, 2));
    assert_eq!(blocks(&mixed), [[chg(0, 2, 0, 2)]]);
}

#[test]
fn lines_split_after_each_newline_and_keep_terminators() {
    let diff = diff_texts("a\r\n\nb\r\r\nc\rd", "", 3);
    let old = diff.old_lines();
    assert_eq!(old.text(), "a\r\n\nb\r\r\nc\rd");
    assert_eq!(raws(old), ["a\r\n", "\n", "b\r\r\n", "c\rd"]);
    let contents: Vec<_> =
        (0..old.len()).map(|i| old.line(i)).collect();
    assert_eq!(contents, ["a", "", "b\r", "c\rd"]);
    assert!(old.missing_newline());

    let diff = diff_texts("\n", "", 3);
    assert_eq!(diff.old_lines().len(), 1);
    assert_eq!(diff.old_lines().line(0), "");
    assert!(!diff.old_lines().missing_newline());

    let empty = diff_texts("", "", 3);
    assert!(empty.old_lines().is_empty());
    assert_eq!(empty.old_lines().text(), "");
    assert!(!empty.old_lines().missing_newline());
}

#[test]
fn context_0_leaves_no_empty_blocks() {
    let diff =
        diff_texts("a\nb\nc\nd\ne\nf\n", "a\nB\nc\nd\nE\nf\n", 0);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(1, 1, 1, 1)], [chg(4, 1, 4, 1)]]);
    assert_eq!(headers(&diff), ["@@ -2 +2 @@", "@@ -5 +5 @@"]);

    let diff = diff_texts("a\nb\n", "a\nx\nb\n", 0);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(1, 0, 1, 1)]]);
    assert_eq!(headers(&diff), ["@@ -1,0 +2 @@"]);

    let diff = diff_texts("a\nx\nb\n", "a\nb\n", 0);
    assert_eq!(blocks(&diff), [[chg(1, 1, 1, 0)]]);
    assert_eq!(headers(&diff), ["@@ -2 +1,0 @@"]);

    let diff = diff_texts("a\nb\nc\n", "A\nb\nC\n", 0);
    assert_eq!(blocks(&diff), [[chg(0, 1, 0, 1)], [chg(2, 1, 2, 1)]]);
}

#[test]
fn hunks_at_most_twice_the_context_apart_merge() {
    let old = "a\nb\nc\nd\ne\n";
    let new = "A\nb\nc\nd\nE\n";
    assert_eq!(diff_texts(old, new, 1).hunks().len(), 2);
    assert_eq!(diff_texts(old, new, 2).hunks().len(), 1);

    let (old, new) = ("a\nb\nc\nd\n", "A\nb\nc\nD\n");
    let merged = diff_texts(old, new, 1);
    assert_eq!(merged.hunks().len(), 1);
    assert_eq!(
        blocks(&merged),
        [[chg(0, 1, 0, 1), ctx(2, 1, 1), chg(3, 1, 3, 1)]]
    );
}

#[test]
fn huge_context_is_clamped_to_the_text() {
    let diff = diff_texts("a\nb\nc\n", "a\nB\nc\n", u32::MAX);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(
        blocks(&diff),
        [[ctx(1, 0, 0), chg(1, 1, 1, 1), ctx(1, 2, 2)]]
    );
}

#[test]
fn header_text_writes_git_form() {
    let diff = diff_texts("a\nb\nc\nd\n", "a\nB\nc\nd\n", 1);
    assert_eq!(headers(&diff), ["@@ -1,3 +1,3 @@"]);

    let one = diff_texts("a\nb\nc\n", "a\nB\nc\n", 0);
    assert_eq!(headers(&one), ["@@ -2 +2 @@"]);

    let added = diff_texts("", "a\n", 3);
    assert_eq!(headers(&added), ["@@ -0,0 +1 @@"]);
    let deleted = diff_texts("a\nb\n", "", 3);
    assert_eq!(headers(&deleted), ["@@ -1,2 +0,0 @@"]);

    let after_five =
        diff_texts("1\n2\n3\n4\n5\n", "1\n2\n3\n4\n5\n6\n", 0);
    assert_eq!(headers(&after_five), ["@@ -5,0 +6 @@"]);

    let mut builder = PatchBuilder::new();
    builder.hunk(5, 5, "fn main()").unwrap();
    builder.line(Edit::Context, "x").unwrap();
    builder.line(Edit::Insert, "y").unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.hunks()[0].heading(), "fn main()");
    assert_eq!(headers(&diff), ["@@ -6 +6,2 @@ fn main()"]);
}

#[test]
fn header_start_edge_cases() {
    assert_eq!(header_start(1, 3), Some(0));
    assert_eq!(header_start(6, 2), Some(5));
    assert_eq!(header_start(5, 0), Some(5));
    assert_eq!(header_start(0, 0), Some(0));
    assert_eq!(header_start(1, 0), Some(1));
    assert_eq!(header_start(0, 3), None);
    assert_eq!(header_start(0, 1), None);
    assert_eq!(header_start(u32::MAX, 2), Some(u32::MAX - 1));
}

#[test]
fn added_and_removed_count_change_block_lines() {
    let diff = diff_texts("a\nb\nc\nd\n", "a\nX\nY\nZ\nd\n", 1);
    assert_eq!((diff.removed(), diff.added()), (2, 3));
    assert_eq!(
        (diff.removed(), diff.added()),
        (diff.old_lines().len() - 2, diff.new_lines().len() - 2)
    );
}

fn patch(builder: &mut PatchBuilder, lines: &[(Edit, &str)]) {
    for &(edit, text) in lines {
        builder.line(edit, text).unwrap();
    }
}

#[test]
fn patch_builder_happy_path() {
    use Edit::{Context, Delete, Insert};
    let mut builder = PatchBuilder::new();
    assert_eq!(builder.hunk_lens(), (0, 0));
    builder.hunk(2, 2, "fn a").unwrap();
    assert_eq!(builder.hunk_lens(), (0, 0));
    patch(
        &mut builder,
        &[
            (Context, "c1"),
            (Delete, "old"),
            (Insert, "new1"),
            (Insert, "new2"),
            (Context, "c2"),
        ],
    );
    assert_eq!(builder.hunk_lens(), (3, 4));
    builder.hunk(20, 21, "").unwrap();
    patch(&mut builder, &[(Delete, "gone"), (Insert, "last")]);
    builder.no_newline().unwrap();
    let diff = builder.finish(Meta::default()).unwrap();

    assert_eq!(diff.check(), Ok(()));
    assert!(diff.is_partial());
    assert_eq!(diff.change(), FileChange::Modified);
    assert_eq!(
        blocks(&diff),
        [
            vec![ctx(1, 0, 0), chg(1, 1, 1, 2), ctx(1, 2, 3)],
            vec![chg(3, 1, 4, 1)],
        ]
    );
    assert_eq!(
        headers(&diff),
        ["@@ -3,3 +3,4 @@ fn a", "@@ -21 +22 @@"]
    );
    assert_eq!(diff.old_lines().text(), "c1\nold\nc2\ngone\n");
    assert_eq!(diff.new_lines().text(), "c1\nnew1\nnew2\nc2\nlast");
    assert!(!diff.old_lines().missing_newline());
    assert!(diff.new_lines().missing_newline());
    assert_eq!(diff.new_lines().line(4), "last");
    assert_eq!((diff.removed(), diff.added()), (2, 3));
}

#[test]
fn patch_builder_marker_after_a_context_line_ends_both_sides() {
    use Edit::{Context, Delete, Insert};
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    patch(&mut builder, &[(Delete, "a"), (Context, "z")]);
    builder.no_newline().unwrap();
    assert_eq!(builder.line(Context, "q"), Err(Invariant::Newline));
    assert_eq!(builder.line(Delete, "q"), Err(Invariant::Newline));
    assert_eq!(builder.line(Insert, "q"), Err(Invariant::Newline));
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.check(), Ok(()));
    assert!(diff.old_lines().missing_newline());
    assert!(diff.new_lines().missing_newline());
}

#[test]
fn patch_builder_marker_after_a_deletion_ends_the_old_side_only() {
    use Edit::{Context, Delete, Insert};
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    patch(&mut builder, &[(Delete, "a")]);
    builder.no_newline().unwrap();
    assert_eq!(builder.line(Delete, "b"), Err(Invariant::Newline));
    assert_eq!(builder.line(Context, "b"), Err(Invariant::Newline));
    builder.line(Insert, "c").unwrap();
    builder.no_newline().unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(blocks(&diff), [[chg(0, 1, 0, 1)]]);
    assert_eq!(diff.old_lines().text(), "a");
    assert_eq!(diff.new_lines().text(), "c");
}

#[test]
fn patch_builder_keeps_a_carriage_return_before_the_stripped_newline()
{
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Insert, "a\r").unwrap();
    builder.no_newline().unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.new_lines().raw(0), "a\r");
    assert_eq!(diff.new_lines().line(0), "a");
}

#[test]
fn patch_builder_rejects_an_embedded_newline() {
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    assert_eq!(
        builder.line(Edit::Context, "a\nb"),
        Err(Invariant::Newline)
    );
    assert_eq!(builder.hunk_lens(), (0, 0));
    builder.line(Edit::Context, "a").unwrap();
    assert_eq!(builder.hunk_lens(), (1, 1));
}

#[test]
fn patch_builder_no_newline_needs_a_line_with_content() {
    let mut builder = PatchBuilder::new();
    assert_eq!(builder.no_newline(), Err(Invariant::Newline));
    builder.hunk(0, 0, "").unwrap();
    assert_eq!(builder.no_newline(), Err(Invariant::Newline));

    for edit in [Edit::Context, Edit::Delete, Edit::Insert] {
        builder.line(edit, "").unwrap();
        assert_eq!(builder.no_newline(), Err(Invariant::Newline));
    }
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.check(), Ok(()));
}

#[test]
fn patch_builder_no_newline_fails_at_the_start_of_a_hunk() {
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Insert, "x").unwrap();
    builder.hunk(5, 5, "").unwrap();
    assert_eq!(builder.no_newline(), Err(Invariant::Newline));
}

#[test]
fn patch_builder_no_newline_fails_when_the_side_is_already_ended() {
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Delete, "ab").unwrap();
    builder.no_newline().unwrap();
    assert_eq!(builder.no_newline(), Err(Invariant::Newline));
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.old_lines().text(), "ab");
}

#[test]
fn patch_builder_rejects_an_empty_hunk() {
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    assert_eq!(
        builder.hunk(5, 5, ""),
        Err(Invariant::EmptyHunk { hunk: 0 })
    );

    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Context, "a").unwrap();
    builder.hunk(5, 5, "").unwrap();
    assert_eq!(
        builder.finish(Meta::default()).map(|_| ()),
        Err(Invariant::EmptyHunk { hunk: 1 })
    );
}

#[test]
fn patch_builder_rejects_overlapping_hunks() {
    let mut builder = PatchBuilder::new();
    builder.hunk(4, 6, "").unwrap();
    patch(&mut builder, &[(Edit::Context, "a"), (Edit::Delete, "b")]);
    assert_eq!(builder.hunk_lens(), (2, 1));
    assert_eq!(
        builder.hunk(5, 20, ""),
        Err(Invariant::Overlap { hunk: 1 })
    );
    assert_eq!(
        builder.hunk(20, 6, ""),
        Err(Invariant::Overlap { hunk: 1 })
    );
    builder.hunk(6, 7, "").unwrap();
    builder.line(Edit::Insert, "c").unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(diff.hunks().len(), 2);
}

#[test]
fn patch_builder_rejects_positions_past_the_largest_line() {
    let mut builder = PatchBuilder::new();
    let max = u32::MAX;
    assert_eq!(
        builder.hunk(max, 0, ""),
        Err(Invariant::Range { hunk: 0 })
    );
    assert_eq!(
        builder.hunk(0, max, ""),
        Err(Invariant::Range { hunk: 0 })
    );

    builder.hunk(max - 2, 0, "").unwrap();
    builder.line(Edit::Delete, "a").unwrap();
    assert_eq!(
        builder.line(Edit::Delete, "b"),
        Err(Invariant::Range { hunk: 0 })
    );
    assert_eq!(
        builder.line(Edit::Context, "b"),
        Err(Invariant::Range { hunk: 0 })
    );
    assert_eq!(builder.hunk_lens(), (1, 0));
    builder.line(Edit::Insert, "c").unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(headers(&diff), ["@@ -4294967294 +1 @@"]);

    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Insert, "a").unwrap();
    assert_eq!(
        builder.hunk(max - 1, max, ""),
        Err(Invariant::Range { hunk: 1 })
    );
    builder.hunk(max - 1, max - 1, "").unwrap();
    assert_eq!(
        builder.line(Edit::Context, "a"),
        Err(Invariant::Range { hunk: 1 })
    );
    builder.line(Edit::Insert, "b").unwrap_err();
}

#[test]
#[should_panic(expected = "no hunk is open")]
fn patch_builder_line_panics_without_a_hunk() {
    let mut builder = PatchBuilder::new();
    let _ = builder.line(Edit::Context, "a");
}

#[test]
fn patch_builder_diffs_with_no_lines_are_valid() {
    let diff = PatchBuilder::new().finish(Meta::default()).unwrap();
    assert_eq!(diff.check(), Ok(()));
    assert!(diff.is_partial());
    assert!(diff.hunks().is_empty());
}

#[test]
fn with_meta_sets_names_and_modes() {
    let diff = diff_texts("a\n", "b\n", 3);
    let meta = Meta {
        change: FileChange::RenamedModified,
        old_name: Some("old.rs".into()),
        new_name: Some("new.rs".into()),
        old_mode: Some(0o100644),
        new_mode: Some(0o100755),
    };
    let diff = diff.with_meta(meta).unwrap();
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(diff.change(), FileChange::RenamedModified);
    assert_eq!(diff.old_name(), Some("old.rs"));
    assert_eq!(diff.new_name(), Some("new.rs"));
    assert_eq!(diff.old_mode(), Some(0o100644));
    assert_eq!(diff.new_mode(), Some(0o100755));
}

fn kind(change: FileChange) -> Meta {
    Meta {
        change,
        ..Meta::default()
    }
}

#[test]
fn with_meta_kind_errors() {
    use FileChange::*;
    let edited = || diff_texts("a\n", "b\n", 3);
    let same = || diff_texts("a\n", "a\n", 3);
    let empty = || diff_texts("", "", 3);
    let added = || diff_texts("", "a\n", 3);
    let deleted = || diff_texts("a\n", "", 3);

    let cases = [
        ("binary with hunks", edited(), Binary, false),
        ("binary with lines", same(), Binary, false),
        ("binary with nothing", empty(), Binary, true),
        ("added with old lines", edited(), Added, false),
        ("added", added(), Added, true),
        ("deleted with new lines", edited(), Deleted, false),
        ("deleted", deleted(), Deleted, true),
        ("renamed with hunks", edited(), Renamed, false),
        ("renamed", same(), Renamed, true),
        (
            "renamed and modified without hunks",
            same(),
            RenamedModified,
            false,
        ),
        ("renamed and modified", edited(), RenamedModified, true),
        ("copied", same(), Copied, true),
        ("modified copy", edited(), Copied, true),
        ("modified", same(), Modified, true),
    ];
    for (name, diff, change, ok) in cases {
        let result = diff.with_meta(kind(change)).map(|_| ());
        let expected = if ok { Ok(()) } else { Err(Invariant::Kind) };
        assert_eq!(result, expected, "{name}");
    }
}

#[test]
fn patch_builder_finish_checks_the_kind_like_with_meta() {
    let build = |change| {
        let mut builder = PatchBuilder::new();
        builder.hunk(0, 0, "").unwrap();
        builder.line(Edit::Insert, "a").unwrap();
        builder.finish(kind(change)).map(|_| ())
    };
    assert_eq!(build(FileChange::Added), Ok(()));
    assert_eq!(build(FileChange::RenamedModified), Ok(()));
    assert_eq!(build(FileChange::Renamed), Err(Invariant::Kind));
    assert_eq!(build(FileChange::Binary), Err(Invariant::Kind));
    assert_eq!(build(FileChange::Deleted), Err(Invariant::Kind));
}

#[test]
fn patch_builder_binary_and_mode_only_files_have_no_hunks() {
    let binary = PatchBuilder::new()
        .finish(kind(FileChange::Binary))
        .unwrap();
    assert_eq!(binary.check(), Ok(()));
    let renamed = PatchBuilder::new()
        .finish(kind(FileChange::Renamed))
        .unwrap();
    assert_eq!(renamed.check(), Ok(()));
    let renamed_edit = PatchBuilder::new()
        .finish(kind(FileChange::RenamedModified))
        .map(|_| ());
    assert_eq!(renamed_edit, Err(Invariant::Kind));
}

fn split_with(
    diff: &mut FileDiff,
    parts: &[(u32, u32)],
) -> Result<(), Invariant> {
    diff.split_changes(|_, _, _, out| out.extend_from_slice(parts))
}

#[test]
fn split_changes_replaces_each_change_block_with_its_parts() {
    let mut diff = diff_texts("x\na\nb\nc\ny\n", "x\nm\ny\n", 1);
    assert_eq!(
        blocks(&diff),
        [[ctx(1, 0, 0), chg(1, 3, 1, 1), ctx(1, 4, 2)]]
    );
    let mut seen = Vec::new();
    diff.split_changes(|old, new, block, out| {
        seen.push((old.len(), new.len(), block));
        out.extend([(1, 0), (1, 1), (1, 0)]);
    })
    .unwrap();
    assert_eq!(seen, [(5, 3, chg(1, 3, 1, 1))]);
    assert_eq!(diff.check(), Ok(()));
    assert_eq!(
        blocks(&diff),
        [[
            ctx(1, 0, 0),
            chg(1, 1, 1, 0),
            chg(2, 1, 1, 1),
            chg(3, 1, 2, 0),
            ctx(1, 4, 2),
        ]]
    );
    assert_eq!((diff.removed(), diff.added()), (3, 1));
}

#[test]
fn split_changes_keeps_a_block_when_the_callback_pushes_nothing() {
    let mut diff =
        diff_texts("a\nb\nc\nd\ne\n", "a\nX\nc\nY\ne\n", 1);
    let before = diff.clone();
    diff.split_changes(|_, _, _, _| {}).unwrap();
    assert_eq!(diff, before);
}

#[test]
fn split_changes_rejects_bad_parts_and_leaves_the_diff_alone() {
    let mut diff =
        diff_texts("a\nb\nc\nd\ne\nf\n", "a\nX\nc\nY\nZ\nf\n", 0);
    assert_eq!(diff.hunks().len(), 2);
    let before = diff.clone();

    assert_eq!(
        split_with(&mut diff, &[(1, 0), (0, 0), (0, 1)]),
        Err(Invariant::Split { hunk: 0, block: 0 })
    );
    assert_eq!(
        split_with(&mut diff, &[(1, 0)]),
        Err(Invariant::Split { hunk: 0, block: 0 })
    );
    assert_eq!(
        split_with(&mut diff, &[(1, 1), (1, 1)]),
        Err(Invariant::Split { hunk: 0, block: 0 })
    );
    assert_eq!(diff, before);

    let second_hunk_fails =
        diff.split_changes(|_, _, block, out| match block {
            Block::Change {
                old_len: 1,
                new_len: 1,
                ..
            } => out.extend([(1, 0), (0, 1)]),
            _ => out.push((9, 9)),
        });
    assert_eq!(
        second_hunk_fails,
        Err(Invariant::Split { hunk: 1, block: 0 })
    );
    assert_eq!(diff, before);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const ALPHABET: [&str; 6] = ["a", "b", "c", "d", "e", ""];

fn random_line(rng: &mut Rng) -> &'static str {
    ALPHABET[rng.below(ALPHABET.len() as u64) as usize]
}

fn random_lines(rng: &mut Rng) -> Vec<&'static str> {
    let len = match rng.below(8) {
        0 => 0,
        _ => rng.below(40),
    };
    (0..len).map(|_| random_line(rng)).collect()
}

fn edited(
    rng: &mut Rng,
    lines: &[&'static str],
) -> Vec<&'static str> {
    let mut out = Vec::new();
    for &line in lines {
        match rng.below(20) {
            0..=1 => {}
            2..=4 => out.push(random_line(rng)),
            5..=6 => {
                out.push(random_line(rng));
                out.push(line);
            }
            _ => out.push(line),
        }
    }
    out
}

fn render(rng: &mut Rng, lines: &[&str]) -> String {
    let mode = rng.below(3);
    let drop_last = rng.below(4) == 0;
    let mut text = String::new();
    for (i, line) in lines.iter().enumerate() {
        text.push_str(line);
        if i + 1 == lines.len() && drop_last {
            break;
        }
        let crlf = match mode {
            0 => false,
            1 => true,
            _ => rng.below(2) == 0,
        };
        text.push_str(if crlf { "\r\n" } else { "\n" });
    }
    text
}

fn check_random_pairs(algorithm: Algorithm) {
    let mut rng = Rng(0x5eed_d1ff);
    for round in 0..500u32 {
        let old_lines = random_lines(&mut rng);
        let new_lines = match rng.below(10) {
            0 => random_lines(&mut rng),
            _ => edited(&mut rng, &old_lines),
        };
        let old = render(&mut rng, &old_lines);
        let new = render(&mut rng, &new_lines);
        let context = round % 5;
        let diff = diff_texts_with(&old, &new, context, algorithm);
        let case = format!("{old:?} -> {new:?} context {context}");

        assert_eq!(diff.check(), Ok(()), "{case}");
        assert_eq!(diff.old_lines().text(), old, "{case}");
        assert_eq!(diff.new_lines().text(), new, "{case}");
        assert_eq!(replay(&diff), new, "{case}");
        assert_eq!(
            i64::from(diff.new_lines().len()),
            i64::from(diff.old_lines().len())
                + i64::from(diff.added())
                - i64::from(diff.removed()),
            "{case}"
        );

        let mut prev_end = None;
        for hunk in diff.hunks() {
            let list = hunk.blocks();
            for (i, block) in list.iter().enumerate() {
                let Block::Context { len, .. } = *block else {
                    continue;
                };
                let (old_run, new_run) =
                    (block.old_range(), block.new_range());
                for (o, n) in old_run.zip(new_run) {
                    let raw = diff.old_lines().raw(o);
                    assert_eq!(
                        raw,
                        diff.new_lines().raw(n),
                        "{case}"
                    );
                }
                let edge = i == 0 || i + 1 == list.len();
                let most = if edge { context } else { 2 * context };
                assert!(len <= most, "{case}");
            }
            let start = list[0].old_range().start;
            if let Some(end) = prev_end {
                assert!(start > end, "{case}");
            }
            prev_end = Some(list[list.len() - 1].old_range().end);
        }
    }
}

#[test]
fn property_blocks_rebuild_the_new_text_with_myers() {
    check_random_pairs(Algorithm::Myers);
}

#[test]
fn property_blocks_rebuild_the_new_text_with_histogram() {
    check_random_pairs(Algorithm::Histogram);
}

#[test]
fn algorithms_agree_on_what_changed_for_an_unambiguous_edit() {
    let old: String =
        (0..200).map(|i| format!("line {i}\n")).collect();
    let new = old.replace("line 50\n", "edited\n");
    let myers = diff_texts_with(&old, &new, 3, Algorithm::Myers);
    let histogram =
        diff_texts_with(&old, &new, 3, Algorithm::Histogram);
    assert_eq!(blocks(&myers), blocks(&histogram));
    assert_eq!(diff_texts(&old, &new, 3), myers);
}
