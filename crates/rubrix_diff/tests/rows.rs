use std::ops::Bound;

use rubrix_diff::{
    Anchor, Block, Edit, Expansion, FileDiff, Hunk, Layout, LineRef,
    Meta, PatchBuilder, Row, Rows, Side, diff_texts, realign, rows,
};

const LAYOUTS: [Layout; 2] = [Layout::Stacked, Layout::Split];

fn numbered(count: u32) -> Vec<String> {
    (1..=count).map(|n| format!("line {n}")).collect()
}

fn text(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn pair(count: u32, changed: &[u32]) -> (String, String) {
    let old = numbered(count);
    let mut new = old.clone();
    for &n in changed {
        new[n as usize - 1] = format!("edited {n}");
    }
    (text(&old), text(&new))
}

fn number(line: Option<LineRef>) -> String {
    line.map_or("-".into(), |line| line.number.to_string())
}

fn show(table: &Rows) -> Vec<String> {
    table
        .range(..)
        .map(|row| {
            let (old, new) =
                (number(row.old_line()), number(row.new_line()));
            match row {
                Row::Context { .. } => format!("C {old}/{new}"),
                Row::Expanded { .. } => format!("E {old}/{new}"),
                Row::Deletion { .. } => format!("D {old}"),
                Row::Addition { .. } => format!("A {new}"),
                Row::Paired { .. } => format!("P {old}/{new}"),
                Row::Gap {
                    hunk,
                    hidden,
                    up,
                    down,
                } => {
                    let up = if up { " up" } else { "" };
                    let down = if down { " down" } else { "" };
                    format!("G{hunk} {hidden}{up}{down}")
                }
            }
        })
        .collect()
}

fn expanded_count(table: &Rows) -> usize {
    table
        .range(..)
        .filter(|row| matches!(row, Row::Expanded { .. }))
        .count()
}

fn revealed(steps: &[(u32, Side, u32)]) -> Expansion {
    let mut expansion = Expansion::new();
    for &(hunk, side, lines) in steps {
        expansion.reveal(hunk, side, lines);
    }
    expansion
}

fn build(diff: &FileDiff, layout: Layout) -> Rows {
    rows(diff, layout, &Expansion::new())
}

fn expected_block_rows(diff: &FileDiff, layout: Layout) -> Vec<u32> {
    let rows = |block: &Block| {
        let (old, new) = (block.old_range(), block.new_range());
        let (old_len, new_len) =
            (old.end - old.start, new.end - new.start);
        match (block, layout) {
            (Block::Context { len, .. }, _) => *len,
            (Block::Change { .. }, Layout::Split) => {
                old_len.max(new_len)
            }
            (Block::Change { .. }, Layout::Stacked) => {
                old_len + new_len
            }
        }
    };
    let hunk_rows =
        |hunk: &Hunk| hunk.blocks().iter().map(rows).sum();
    diff.hunks().iter().map(hunk_rows).collect()
}

#[test]
fn change_blocks_give_old_plus_new_rows_stacked_and_max_split() {
    let old = "1\n2\n3\n4\n5\n6\n7\n8\n";
    let new = "1\nX\n3\n4\nY\nZ\n8\n";
    let diff = diff_texts(old, new, 0);
    assert_eq!(diff.hunks().len(), 2);

    let stacked = build(&diff, Layout::Stacked);
    assert_eq!(stacked.hunk_rows(0).len(), 2);
    assert_eq!(stacked.hunk_rows(1).len(), 5);
    let split = build(&diff, Layout::Split);
    assert_eq!(split.hunk_rows(0).len(), 1);
    assert_eq!(split.hunk_rows(1).len(), 3);

    assert_eq!(
        show(&stacked),
        [
            "E 1/1",
            "D 2",
            "A 2",
            "G1 2 up down",
            "D 5",
            "D 6",
            "D 7",
            "A 5",
            "A 6",
            "E 8/7",
        ]
    );
    assert_eq!(
        show(&split),
        [
            "E 1/1",
            "P 2/2",
            "G1 2 up down",
            "P 5/5",
            "P 6/6",
            "D 7",
            "E 8/7",
        ]
    );
}

#[test]
fn hunk_rows_match_the_block_counts_for_many_diffs() {
    let pairs = [
        pair(30, &[3, 4, 5]),
        pair(40, &[1, 20, 40]),
        pair(12, &[2, 4, 6, 8, 10]),
        ("a\nb\nc\nd\ne\nf\n".into(), "a\nx\ny\nz\nf\n".into()),
        ("a\nb\nc\n".into(), "".into()),
        ("".into(), "a\nb\nc\n".into()),
        ("a\nb\n".into(), "a\nb".into()),
    ];
    for (old, new) in &pairs {
        for context in [0, 1, 3] {
            let diff = diff_texts(old, new, context);
            for layout in LAYOUTS {
                let table = build(&diff, layout);
                let expected = expected_block_rows(&diff, layout);
                let got: Vec<u32> = (0..diff.hunks().len() as u32)
                    .map(|h| table.hunk_rows(h).len() as u32)
                    .collect();
                assert_eq!(got, expected, "{old:?} -> {new:?}");
            }
        }
    }
}

#[test]
fn rows_name_the_old_and_new_line_of_each_kind() {
    let diff = diff_texts("a\nb\nc\nd\ne\n", "a\nB\nc\nd\ne\nf\n", 3);
    assert_eq!(diff.hunks().len(), 1);

    let stacked = build(&diff, Layout::Stacked);
    assert_eq!(
        show(&stacked),
        ["C 1/1", "D 2", "A 2", "C 3/3", "C 4/4", "C 5/5", "A 6"]
    );
    let split = build(&diff, Layout::Split);
    assert_eq!(
        show(&split),
        ["C 1/1", "P 2/2", "C 3/3", "C 4/4", "C 5/5", "A 6"]
    );

    for table in [&stacked, &split] {
        for row in table.range(..) {
            for line in
                [row.old_line(), row.new_line()].into_iter().flatten()
            {
                assert_eq!(line.index + 1, line.number.get());
            }
            match row {
                Row::Context { .. } | Row::Paired { .. } => {
                    assert!(row.old_line().is_some());
                    assert!(row.new_line().is_some());
                }
                Row::Deletion { .. } => {
                    assert!(row.old_line().is_some());
                    assert!(row.new_line().is_none());
                }
                Row::Addition { .. } => {
                    assert!(row.old_line().is_none());
                    assert!(row.new_line().is_some());
                }
                Row::Expanded { .. } | Row::Gap { .. } => {
                    unreachable!()
                }
            }
        }
    }

    let paired = split.get(1).unwrap();
    assert_eq!(paired.anchor(), Anchor::Old(1));
    assert_eq!(stacked.get(1).unwrap().anchor(), Anchor::Old(1));
    assert_eq!(stacked.get(2).unwrap().anchor(), Anchor::New(1));
    assert_eq!(stacked.get(6).unwrap().anchor(), Anchor::New(5));
}

#[test]
fn expanded_rows_carry_both_line_numbers_and_indexes() {
    let (old, new) = pair(30, &[2, 29]);
    let diff = diff_texts(&old, &new, 1);
    let table = rows(
        &diff,
        Layout::Stacked,
        &revealed(&[(1, Side::Down, 2), (1, Side::Up, 1)]),
    );
    let shown: Vec<Row> = table
        .range(..)
        .filter(|row| matches!(row, Row::Expanded { .. }))
        .collect();
    assert_eq!(shown.len(), 3);
    for row in shown {
        let (old, new) =
            (row.old_line().unwrap(), row.new_line().unwrap());
        assert_eq!(old.number, new.number);
        assert_eq!(old.index + 1, old.number.get());
        assert_eq!(new.index + 1, new.number.get());
    }
}

#[test]
fn windowed_rows_equal_the_matching_slice_of_all_rows() {
    let (old, new) = pair(60, &[2, 3, 30, 31, 32, 58]);
    let diffs = [
        diff_texts(&old, &new, 2),
        diff_texts(&old, &new, 0),
        diff_texts("a\nb\nc\nd\n", "a\nx\ny\nc\nd\nz\n", 1),
    ];
    let expansions = [
        Expansion::new(),
        revealed(&[(1, Side::Up, 3), (1, Side::Down, 2)]),
        revealed(&[(0, Side::All, 0), (2, Side::Down, 1)]),
    ];
    for diff in &diffs {
        for layout in LAYOUTS {
            for expansion in &expansions {
                let table = rows(diff, layout, expansion);
                let all: Vec<Row> = table.range(..).collect();
                assert_eq!(all.len() as u32, table.len());
                let len = table.len();
                for start in 0..=len + 1 {
                    for end in start..=len + 2 {
                        let window = table.range(start..end);
                        let clamped_end = end.min(len) as usize;
                        let clamped_start = start.min(len) as usize;
                        let want = &all[clamped_start..clamped_end];
                        assert_eq!(window.len(), want.len());
                        assert!(window.eq(want.iter().copied()));
                    }
                }
                for row in 0..len {
                    assert_eq!(
                        table.get(row),
                        Some(all[row as usize])
                    );
                }
                assert_eq!(table.get(len), None);
            }
        }
    }
}

#[test]
fn range_clamps_every_kind_of_bound() {
    let (old, new) = pair(20, &[10]);
    let table = build(&diff_texts(&old, &new, 3), Layout::Stacked);
    let all: Vec<Row> = table.range(..).collect();
    let len = table.len();
    assert!(table.range(2..).eq(all[2..].iter().copied()));
    assert!(table.range(..=2).eq(all[..=2].iter().copied()));
    assert!(table.range(..2).eq(all[..2].iter().copied()));
    assert!(table.range(len + 5..).eq(std::iter::empty()));
    assert!(table.range(len + 5..len + 9).eq(std::iter::empty()));
    let reversed = (Bound::Included(5), Bound::Excluded(2));
    assert!(table.range(reversed).eq(std::iter::empty()));
    assert!(table.range(3..3).eq(std::iter::empty()));
    assert!(table.range(..=u32::MAX).eq(all.iter().copied()));
    assert!(table.range(u32::MAX..).eq(std::iter::empty()));
    let excluded =
        (std::ops::Bound::Excluded(1), std::ops::Bound::Unbounded);
    assert!(table.range(excluded).eq(all[2..].iter().copied()));
    assert_eq!(table.get(u32::MAX), None);
}

fn two_hunk_gap_pair() -> (FileDiff, u32) {
    let (old, new) = pair(600, &[10, 500]);
    (diff_texts(&old, &new, 3), 483)
}

#[test]
fn an_expansion_of_100_lines_reveals_exactly_100_expanded_rows() {
    let (diff, gap) = two_hunk_gap_pair();
    for layout in LAYOUTS {
        let before = build(&diff, layout);
        assert_eq!(expanded_count(&before), 0);
        let second = before.hunk_rows(1).start;
        let first_end = before.hunk_rows(0).end;
        assert_eq!(
            before.get(second - 1),
            Some(Row::Gap {
                hunk: 1,
                hidden: gap,
                up: true,
                down: true,
            })
        );

        let up =
            rows(&diff, layout, &revealed(&[(1, Side::Up, 100)]));
        assert_eq!(expanded_count(&up), 100);
        assert_eq!(up.len(), before.len() + 100);
        let second = up.hunk_rows(1).start;
        for row in second - 100..second {
            let Some(Row::Expanded { old, new }) = up.get(row) else {
                panic!("row {row} is not expanded");
            };
            assert_eq!(old.number, new.number);
            assert_eq!(old.number.get(), 497 - (second - row));
        }
        assert_eq!(
            up.get(second - 101),
            Some(Row::Gap {
                hunk: 1,
                hidden: gap - 100,
                up: true,
                down: true,
            })
        );

        let down =
            rows(&diff, layout, &revealed(&[(1, Side::Down, 100)]));
        assert_eq!(expanded_count(&down), 100);
        assert_eq!(down.len(), before.len() + 100);
        for row in first_end..first_end + 100 {
            let Some(Row::Expanded { old, .. }) = down.get(row)
            else {
                panic!("row {row} is not expanded");
            };
            assert_eq!(old.number.get(), 14 + (row - first_end));
        }
        assert_eq!(
            down.get(first_end + 100),
            Some(Row::Gap {
                hunk: 1,
                hidden: gap - 100,
                up: true,
                down: true,
            })
        );

        let both =
            revealed(&[(1, Side::Up, 100), (1, Side::Down, 100)]);
        let both = rows(&diff, layout, &both);
        assert_eq!(expanded_count(&both), 200);
        let twice =
            revealed(&[(1, Side::Up, 100), (1, Side::Up, 100)]);
        assert_eq!(expanded_count(&rows(&diff, layout, &twice)), 200);
    }
}

#[test]
fn all_shows_the_whole_gap_with_continuous_numbers_and_no_gap_row() {
    let (diff, gap) = two_hunk_gap_pair();
    let reveal_all = revealed(&[(1, Side::All, 0)]);
    for layout in LAYOUTS {
        let table = rows(&diff, layout, &reveal_all);
        assert_eq!(expanded_count(&table), gap as usize);
        let start = table.hunk_rows(0).end;
        for (k, row) in (0..).zip(table.range(start..start + gap)) {
            let Row::Expanded { old, new } = row else {
                panic!("row {k} of the gap is {row:?}");
            };
            assert_eq!(old.number.get(), 14 + k);
            assert_eq!(new.number.get(), 14 + k);
        }
        assert_eq!(table.hunk_rows(1).start, start + gap);
        assert_eq!(table.find(Anchor::Gap(1)), Some(start));
        let shown = show(&table);
        assert!(!shown.iter().any(|tag| tag.starts_with("G1")));
    }
}

#[test]
fn all_is_idempotent_and_ends_that_meet_or_cross_show_the_whole_gap()
{
    let (diff, _) = two_hunk_gap_pair();
    let whole =
        rows(&diff, Layout::Stacked, &revealed(&[(1, Side::All, 0)]));
    let steps: [&[(u32, Side, u32)]; 4] = [
        &[(1, Side::All, 0), (1, Side::All, 0)],
        &[(1, Side::Up, 242), (1, Side::Down, 241)],
        &[(1, Side::Up, 300), (1, Side::Down, 300)],
        &[(1, Side::Up, u32::MAX), (1, Side::Down, u32::MAX)],
    ];
    for steps in steps {
        let table = rows(&diff, Layout::Stacked, &revealed(steps));
        assert_eq!(table, whole);
    }

    let short = revealed(&[(1, Side::Up, 242), (1, Side::Down, 240)]);
    let table = rows(&diff, Layout::Stacked, &short);
    assert_eq!(expanded_count(&table), 482);
    assert!(show(&table).contains(&"G1 1 up down".to_string()));

    let one_side = revealed(&[(1, Side::Up, 1000)]);
    let table = rows(&diff, Layout::Split, &one_side);
    assert_eq!(
        table,
        rows(&diff, Layout::Split, &revealed(&[(1, Side::All, 0)]))
    );
}

#[test]
fn a_one_line_gap_is_always_shown() {
    let diff = diff_texts("a\nb\nc\n", "A\nb\nC\n", 0);
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        let tags = show(&table);
        assert!(tags.contains(&"E 2/2".to_string()), "{tags:?}");
        assert!(!tags.iter().any(|t| t.starts_with('G')), "{tags:?}");
    }

    let diff = diff_texts("a\nb\nc\nd\n", "A\nb\nc\nD\n", 0);
    let table = build(&diff, Layout::Stacked);
    assert_eq!(
        show(&table),
        ["D 1", "A 1", "G1 2 up down", "D 4", "A 4"]
    );
}

#[test]
fn revealing_a_one_line_gap_changes_nothing() {
    let (old, new) = pair(9, &[2, 4]);
    let diff = diff_texts(&old, &new, 0);
    let plain = build(&diff, Layout::Stacked);
    let revealed = revealed(&[(1, Side::Up, 5)]);
    let table = rows(&diff, Layout::Stacked, &revealed);
    assert_eq!(table, plain);
    assert!(show(&table).contains(&"E 3/3".to_string()));
}

#[test]
fn gap_rows_carry_their_hunk_and_which_buttons_apply() {
    let (old, new) = pair(30, &[5, 15, 25]);
    let diff = diff_texts(&old, &new, 1);
    assert_eq!(diff.hunks().len(), 3);
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        let gaps: Vec<Row> = table
            .range(..)
            .filter(|row| matches!(row, Row::Gap { .. }))
            .collect();
        let gap = |hunk, hidden, up, down| Row::Gap {
            hunk,
            hidden,
            up,
            down,
        };
        assert_eq!(
            gaps,
            [
                gap(0, 3, true, false),
                gap(1, 7, true, true),
                gap(2, 7, true, true),
                gap(3, 4, false, true),
            ]
        );
        for hunk in 0..4 {
            let at = table.find(Anchor::Gap(hunk)).unwrap();
            assert_eq!(table.get(at), Some(gaps[hunk as usize]));
            assert_eq!(
                gaps[hunk as usize].anchor(),
                Anchor::Gap(hunk)
            );
        }
        assert_eq!(table.find(Anchor::Gap(4)), None);
        assert_eq!(table.get(table.len() - 1), Some(gaps[3]));
        assert_eq!(table.get(0), Some(gaps[0]));
    }
}

#[test]
fn the_trailing_gap_expands_from_the_hunk_above() {
    let (old, new) = pair(30, &[5]);
    let diff = diff_texts(&old, &new, 1);
    let count = diff.hunks().len() as u32;
    assert_eq!(count, 1);

    let table = rows(
        &diff,
        Layout::Stacked,
        &revealed(&[(count, Side::Down, 2)]),
    );
    let tags = show(&table);
    let tail = &tags[tags.len() - 3..];
    assert_eq!(tail, ["E 7/7", "E 8/8", "G1 22 down"]);

    let table = rows(
        &diff,
        Layout::Stacked,
        &revealed(&[(count, Side::Up, 2)]),
    );
    let tags = show(&table);
    let tail = &tags[tags.len() - 3..];
    assert_eq!(tail, ["G1 22 down", "E 29/29", "E 30/30"]);

    let table = rows(
        &diff,
        Layout::Stacked,
        &revealed(&[(count, Side::All, 0)]),
    );
    assert_eq!(expanded_count(&table), 24);
    assert_eq!(
        table
            .get(table.len() - 1)
            .and_then(|row| row.old_line())
            .unwrap()
            .number
            .get(),
        30
    );
    assert!(!show(&table).iter().any(|t| t.starts_with("G1")));
}

#[test]
fn expansion_keys_past_the_gaps_are_ignored() {
    let (old, new) = pair(30, &[5]);
    let diff = diff_texts(&old, &new, 1);
    let table = build(&diff, Layout::Split);
    let ignored = revealed(&[(2, Side::All, 0), (99, Side::Up, 5)]);
    assert_eq!(rows(&diff, Layout::Split, &ignored), table);
}

#[test]
fn find_round_trips_between_layouts() {
    let (old, new) = pair(80, &[3, 4, 40, 70]);
    let uneven = diff_texts(
        "x\na\nb\nc\ny\nk\nl\nm\nn\nz\n",
        "x\nm1\ny\nk\nl\nm\nn\nz\nq\nr\n",
        1,
    );
    let diffs = [diff_texts(&old, &new, 2), uneven];
    let expansions = [
        Expansion::new(),
        revealed(&[(1, Side::Up, 5), (2, Side::Down, 4)]),
        revealed(&[(0, Side::All, 0), (3, Side::Down, 1)]),
    ];
    for diff in &diffs {
        for expansion in &expansions {
            let stacked = rows(diff, Layout::Stacked, expansion);
            let split = rows(diff, Layout::Split, expansion);
            for (from, to) in [(&stacked, &split), (&split, &stacked)]
            {
                for (index, row) in from.range(..).enumerate() {
                    let anchor = row.anchor();
                    assert_eq!(from.find(anchor), Some(index as u32));
                    let found = to.find(anchor).unwrap();
                    let there = to.get(found).unwrap();
                    match anchor {
                        Anchor::Old(i) => {
                            assert_eq!(
                                there.old_line().unwrap().index,
                                i
                            )
                        }
                        Anchor::New(i) => {
                            assert_eq!(
                                there.new_line().unwrap().index,
                                i
                            )
                        }
                        Anchor::Gap(h) => assert!(
                            matches!(there, Row::Gap { hunk, .. } if hunk == h)
                        ),
                    }
                    assert_eq!(to.find(there.anchor()), Some(found));
                }
            }
        }
    }
}

#[test]
fn find_returns_the_gap_row_for_hidden_lines_and_none_past_the_files()
{
    let (old, new) = pair(30, &[5, 25]);
    let diff = diff_texts(&old, &new, 1);
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        let gap = table.find(Anchor::Gap(1)).unwrap();
        for index in 6..23 {
            assert_eq!(table.find(Anchor::Old(index)), Some(gap));
            assert_eq!(table.find(Anchor::New(index)), Some(gap));
        }
        let before = table.find(Anchor::Old(5)).unwrap();
        assert!(before < gap);
        let after = table.find(Anchor::Old(23)).unwrap();
        assert!(after > gap);
        assert_eq!(
            table.find(Anchor::Old(29)),
            table.find(Anchor::Gap(2))
        );
        assert_eq!(table.find(Anchor::Old(100)), None);
        assert_eq!(table.find(Anchor::New(100)), None);
        assert_eq!(table.find(Anchor::Old(u32::MAX)), None);
    }
}

#[test]
fn find_never_lets_a_pure_insertion_capture_the_next_line() {
    let diff = diff_texts("a\nb\nc\n", "a\nx\nb\nc\n", 3);
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        let row =
            table.get(table.find(Anchor::Old(1)).unwrap()).unwrap();
        assert!(matches!(row, Row::Context { .. }));
        assert_eq!(row.old_line().unwrap().index, 1);
        let row =
            table.get(table.find(Anchor::New(1)).unwrap()).unwrap();
        assert!(matches!(row, Row::Addition { .. }));
    }

    let diff = diff_texts("a\nx\nb\nc\n", "a\nb\nc\n", 3);
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        let row =
            table.get(table.find(Anchor::New(1)).unwrap()).unwrap();
        assert!(matches!(row, Row::Context { .. }));
        assert_eq!(row.new_line().unwrap().index, 1);
    }
}

#[test]
fn hunk_rows_start_after_the_gap_above_each_hunk() {
    let (old, new) = pair(30, &[5, 15, 25]);
    let diff = diff_texts(&old, &new, 1);
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        let mut previous_end = 0;
        for hunk in 0..3 {
            let range = table.hunk_rows(hunk);
            assert!(!range.is_empty());
            assert_eq!(range.start, previous_end + 1);
            assert!(matches!(
                table.get(range.start - 1),
                Some(Row::Gap { hunk: h, .. }) if h == hunk
            ));
            assert!(!matches!(
                table.get(range.start),
                Some(Row::Gap { .. } | Row::Expanded { .. })
            ));
            previous_end = range.end;
        }
        assert_eq!(table.hunk_rows(3), table.len()..table.len());
        assert_eq!(table.hunk_rows(40), table.len()..table.len());
        assert_eq!(table.len(), previous_end + 1);
    }

    let expanded = revealed(&[(1, Side::Up, 2), (0, Side::All, 0)]);
    let table = rows(&diff, Layout::Stacked, &expanded);
    let range = table.hunk_rows(0);
    assert_eq!(range.start, 3);
    assert!(matches!(
        table.get(range.start - 1),
        Some(Row::Expanded { .. })
    ));
    let range = table.hunk_rows(1);
    assert!(matches!(
        table.get(range.start - 1),
        Some(Row::Expanded { .. })
    ));
}

#[test]
fn split_changes_pairs_the_middle_lines_and_stacked_rows_stay() {
    let mut diff = diff_texts("x\nd1\nd2\nd3\ny\n", "x\nm\ny\n", 1);
    let stacked_before = build(&diff, Layout::Stacked);
    let split_before = build(&diff, Layout::Split);
    assert_eq!(
        show(&split_before),
        ["C 1/1", "P 2/2", "D 3", "D 4", "C 5/3"]
    );

    diff.split_changes(|_, _, block, out| {
        if let Block::Change {
            old_len: 3,
            new_len: 1,
            ..
        } = block
        {
            out.extend([(1, 0), (1, 1), (1, 0)]);
        }
    })
    .unwrap();

    let stacked = build(&diff, Layout::Stacked);
    assert_eq!(stacked, stacked_before);
    assert_eq!(
        show(&stacked),
        ["C 1/1", "D 2", "D 3", "D 4", "A 2", "C 5/3"]
    );
    let split = build(&diff, Layout::Split);
    assert_eq!(split.len(), split_before.len());
    assert_eq!(
        show(&split),
        ["C 1/1", "D 2", "P 3/2", "D 4", "C 5/3"]
    );
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        for (index, row) in table.range(..).enumerate() {
            assert_eq!(table.find(row.anchor()), Some(index as u32));
        }
    }
}

#[test]
fn only_paired_rows_carry_word_ranges() {
    let mut diff = diff_texts(
        "x\nfirst one\nsecond two\nthird three\ny\n",
        "x\nsecond 2\ny\n",
        1,
    );
    realign(&mut diff);
    let marked = |layout| {
        let table = build(&diff, layout);
        let words: Vec<_> =
            table.range(..).map(|row| row.words(&diff)).collect();
        let shown = show(&table);
        shown.into_iter().zip(words).collect::<Vec<_>>()
    };
    let changed = Some((vec![7..10], vec![7..8]));

    let stacked = marked(Layout::Stacked);
    let expected = [
        ("C 1/1", None),
        ("D 2", None),
        ("D 3", changed.clone()),
        ("D 4", None),
        ("A 2", changed.clone()),
        ("C 5/3", None),
    ];
    assert_eq!(
        stacked,
        expected.map(|(row, words)| (row.to_owned(), words))
    );

    let split = marked(Layout::Split);
    let expected = [
        ("C 1/1", None),
        ("D 2", None),
        ("P 3/2", changed),
        ("D 4", None),
        ("C 5/3", None),
    ];
    assert_eq!(
        split,
        expected.map(|(row, words)| (row.to_owned(), words))
    );

    let added = diff_texts("", "a\nb\n", 3);
    let table = build(&added, Layout::Stacked);
    assert!(table.range(..).all(|row| row.words(&added).is_none()));
}

fn partial_diff() -> FileDiff {
    let mut builder = PatchBuilder::new();
    builder.hunk(10, 10, "").unwrap();
    for (edit, line) in [
        (Edit::Context, "c1"),
        (Edit::Delete, "old"),
        (Edit::Insert, "new"),
        (Edit::Context, "c2"),
    ] {
        builder.line(edit, line).unwrap();
    }
    builder.hunk(25, 30, "").unwrap();
    for (edit, line) in [
        (Edit::Context, "c3"),
        (Edit::Insert, "extra"),
        (Edit::Context, "c4"),
    ] {
        builder.line(edit, line).unwrap();
    }
    builder.hunk(44, 50, "").unwrap();
    builder.line(Edit::Delete, "last").unwrap();
    builder.finish(Meta::default()).unwrap()
}

#[test]
fn a_partial_diff_has_non_expandable_gap_rows_sized_on_the_new_side()
{
    let diff = partial_diff();
    assert_eq!(diff.check(), Ok(()));
    let mut every_gap = Expansion::new();
    for hunk in 0..4 {
        every_gap.reveal(hunk, Side::All, 0);
        every_gap.reveal(hunk, Side::Up, 3);
    }
    for layout in LAYOUTS {
        let table = build(&diff, layout);
        assert_eq!(rows(&diff, layout, &every_gap), table);
        let gaps: Vec<Row> = table
            .range(..)
            .filter(|row| matches!(row, Row::Gap { .. }))
            .collect();
        let gap = |hunk, hidden| Row::Gap {
            hunk,
            hidden,
            up: false,
            down: false,
        };
        assert_eq!(gaps, [gap(0, 10), gap(1, 17), gap(2, 17)]);
        assert!(!matches!(
            table.get(table.len() - 1),
            Some(Row::Gap { .. })
        ));
        assert_eq!(table.find(Anchor::Gap(3)), None);
        assert_eq!(expanded_count(&table), 0);
    }

    let stacked = build(&diff, Layout::Stacked);
    assert_eq!(
        show(&stacked),
        [
            "G0 10", "C 11/11", "D 12", "A 12", "C 13/13", "G1 17",
            "C 26/31", "A 32", "C 27/33", "G2 17", "D 45",
        ]
    );
}

#[test]
fn a_partial_diff_numbers_rows_by_file_position_and_indexes_by_list()
{
    let diff = partial_diff();
    let table = build(&diff, Layout::Stacked);
    let lines: Vec<(u32, u32, u32, u32)> = table
        .range(..)
        .filter(|row| !matches!(row, Row::Gap { .. }))
        .filter_map(|row| match (row.old_line(), row.new_line()) {
            (Some(old), Some(new)) => Some((
                old.index,
                old.number.get(),
                new.index,
                new.number.get(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines,
        [
            (0, 11, 0, 11),
            (2, 13, 2, 13),
            (3, 26, 3, 31),
            (4, 27, 5, 33)
        ]
    );
    for (index, row) in table.range(..).enumerate() {
        assert_eq!(table.find(row.anchor()), Some(index as u32));
    }
    let found = table.find(Anchor::Old(3)).unwrap();
    assert_eq!(
        table.get(found).unwrap().old_line().unwrap().number.get(),
        26
    );
    assert_eq!(table.find(Anchor::Old(99)), None);
}

#[test]
fn a_partial_diff_shows_a_one_line_gap_as_a_gap_row() {
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Context, "a").unwrap();
    builder.hunk(2, 2, "").unwrap();
    builder.line(Edit::Context, "c").unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    let table = build(&diff, Layout::Stacked);
    assert_eq!(show(&table), ["C 1/1", "G1 1", "C 3/3"]);
}

#[test]
fn a_partial_diff_without_a_leading_gap_starts_with_its_hunk() {
    let mut builder = PatchBuilder::new();
    builder.hunk(0, 0, "").unwrap();
    builder.line(Edit::Insert, "a").unwrap();
    let diff = builder.finish(Meta::default()).unwrap();
    assert_eq!(show(&build(&diff, Layout::Split)), ["A 1"]);
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

    fn below(&mut self, n: u64) -> u32 {
        (self.next() % n) as u32
    }
}

const ALPHABET: [&str; 4] = ["a", "b", "c", "d"];

fn random_text(rng: &mut Rng, base: Option<&str>) -> String {
    let mut out = String::new();
    match base {
        None => {
            for _ in 0..rng.below(60) {
                out.push_str(ALPHABET[rng.below(4) as usize]);
                out.push('\n');
            }
        }
        Some(base) => {
            for line in base.split_inclusive('\n') {
                match rng.below(12) {
                    0 => {}
                    1 => out.push_str("x\n"),
                    2 => {
                        out.push_str("y\n");
                        out.push_str(line);
                    }
                    _ => out.push_str(line),
                }
            }
        }
    }
    out
}

fn random_expansion(rng: &mut Rng, hunks: u32) -> Expansion {
    let mut expansion = Expansion::new();
    for hunk in 0..=hunks {
        let lines = rng.below(8);
        match rng.below(5) {
            0 => expansion.reveal(hunk, Side::Up, lines),
            1 => expansion.reveal(hunk, Side::Down, lines),
            2 => {
                expansion.reveal(hunk, Side::Up, lines);
                expansion.reveal(hunk, Side::Down, rng.below(8));
            }
            3 => expansion.reveal(hunk, Side::All, 0),
            _ => {}
        }
    }
    expansion
}

#[test]
fn property_rows_show_every_line_once_or_hide_it_in_a_gap() {
    let mut rng = Rng(0x7035);
    for round in 0..300 {
        let old = random_text(&mut rng, None);
        let new = random_text(&mut rng, Some(&old));
        let context = round % 4;
        let diff = diff_texts(&old, &new, context);
        let expansion =
            random_expansion(&mut rng, diff.hunks().len() as u32);
        let case = format!("{old:?} -> {new:?} context {context}");

        let mut shown = Vec::new();
        for layout in LAYOUTS {
            let table = rows(&diff, layout, &expansion);
            let all: Vec<Row> = table.range(..).collect();
            assert_eq!(all.len() as u32, table.len(), "{case}");
            if diff.hunks().is_empty() {
                assert!(all.is_empty(), "{case}");
                continue;
            }

            let mut hidden = 0;
            let (mut old_seen, mut new_seen) =
                (Vec::new(), Vec::new());
            for (index, row) in all.iter().enumerate() {
                assert_eq!(
                    table.get(index as u32),
                    Some(*row),
                    "{case}"
                );
                assert_eq!(
                    table.find(row.anchor()),
                    Some(index as u32),
                    "{case}"
                );
                if let Row::Gap { hidden: h, .. } = row {
                    hidden += h;
                }
                old_seen.extend(row.old_line().map(|l| l.index));
                new_seen.extend(row.new_line().map(|l| l.index));
                for line in [row.old_line(), row.new_line()]
                    .into_iter()
                    .flatten()
                {
                    assert_eq!(
                        line.index + 1,
                        line.number.get(),
                        "{case}"
                    );
                }
            }
            assert!(
                old_seen.windows(2).all(|w| w[0] < w[1]),
                "{case}"
            );
            assert!(
                new_seen.windows(2).all(|w| w[0] < w[1]),
                "{case}"
            );
            assert_eq!(
                old_seen.len() as u32 + hidden,
                diff.old_lines().len(),
                "{case}"
            );
            assert_eq!(
                new_seen.len() as u32 + hidden,
                diff.new_lines().len(),
                "{case}"
            );

            for &i in &old_seen {
                let row = table
                    .get(table.find(Anchor::Old(i)).unwrap())
                    .unwrap();
                assert_eq!(
                    row.old_line().unwrap().index,
                    i,
                    "{case}"
                );
            }
            for &i in &new_seen {
                let row = table
                    .get(table.find(Anchor::New(i)).unwrap())
                    .unwrap();
                assert_eq!(
                    row.new_line().unwrap().index,
                    i,
                    "{case}"
                );
            }
            for i in 0..diff.old_lines().len() {
                if !old_seen.contains(&i) {
                    let at = table.find(Anchor::Old(i)).unwrap();
                    assert!(
                        matches!(
                            table.get(at),
                            Some(Row::Gap { .. })
                        ),
                        "{case}"
                    );
                }
            }
            for i in 0..diff.new_lines().len() {
                if !new_seen.contains(&i) {
                    let at = table.find(Anchor::New(i)).unwrap();
                    assert!(
                        matches!(
                            table.get(at),
                            Some(Row::Gap { .. })
                        ),
                        "{case}"
                    );
                }
            }
            shown.push((old_seen, new_seen));
        }
        if let [stacked, split] = &shown[..] {
            assert_eq!(stacked, split, "{case}");
        }
    }
}
