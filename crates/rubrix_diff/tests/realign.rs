use rubrix_diff::{
    Expansion, FileDiff, Layout, Row, Rows, diff_texts, realign, rows,
};

fn split(diff: &FileDiff) -> Rows {
    rows(diff, Layout::Split, &Expansion::new())
}

fn texts(diff: &FileDiff, table: &Rows) -> Vec<(String, String)> {
    let text = |line: Option<&str>| line.unwrap_or("").to_owned();
    table
        .range(..)
        .map(|row| {
            let old = row
                .old_line()
                .map(|l| diff.old_lines().line(l.index));
            let new = row
                .new_line()
                .map(|l| diff.new_lines().line(l.index));
            (text(old), text(new))
        })
        .collect()
}

#[test]
fn a_one_line_addition_pairs_with_the_middle_of_three_deletions() {
    let old = "fn main() {\n    open_the_door();\n    let total = a \
               + b;\n    close_the_window();\n}\n";
    let new = "fn main() {\n    let total = a + b + c;\n}\n";
    let unaligned = diff_texts(old, new, 3);
    let mut diff = unaligned.clone();
    let before = split(&diff);
    assert_eq!(
        texts(&diff, &before)[1].1,
        "    let total = a + b + c;"
    );
    assert_eq!(texts(&diff, &before)[1].0, "    open_the_door();");

    realign(&mut diff);
    let after = split(&diff);
    assert_eq!(after.len(), before.len());
    assert_eq!(
        texts(&diff, &after),
        [
            ("fn main() {", "fn main() {"),
            ("    open_the_door();", ""),
            ("    let total = a + b;", "    let total = a + b + c;"),
            ("    close_the_window();", ""),
            ("}", "}"),
        ]
        .map(|(old, new)| (old.to_owned(), new.to_owned()))
    );
    assert!(matches!(after.get(2), Some(Row::Paired { .. })));
    let stacked =
        |diff| rows(diff, Layout::Stacked, &Expansion::new());
    assert_eq!(stacked(&diff), stacked(&unaligned));
}

#[test]
fn additions_slide_along_the_shorter_deletions() {
    let old = "a\nlet x = compute(1);\nb\n";
    let new = "a\n\n\nlet x = compute(2);\nb\n";
    let mut diff = diff_texts(old, new, 3);
    realign(&mut diff);
    let table = split(&diff);
    let paired: Vec<_> = table
        .range(..)
        .filter(|row| matches!(row, Row::Paired { .. }))
        .collect();
    assert_eq!(paired.len(), 1);
    let new = paired[0].new_line().unwrap();
    assert_eq!(
        diff.new_lines().line(new.index),
        "let x = compute(2);"
    );
}

#[test]
fn a_near_tie_keeps_the_positional_pairs() {
    let old = "a\nuse std::fmt;\nb\n";
    let new = "a\nuse std::io;\nuse std::fs;\nb\n";
    let mut diff = diff_texts(old, new, 3);
    let before = split(&diff);
    realign(&mut diff);
    assert_eq!(split(&diff), before);
}

#[test]
fn balanced_blocks_stay() {
    let old = "a\nb\nc\n";
    let new = "x\ny\nz\n";
    let mut diff = diff_texts(old, new, 3);
    let unaligned = diff.clone();
    realign(&mut diff);
    assert_eq!(diff, unaligned);
}
