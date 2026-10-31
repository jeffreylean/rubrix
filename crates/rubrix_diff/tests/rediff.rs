use std::mem::{Discriminant, discriminant};
use std::ops::Range;
use std::time::Instant;

use rubrix_diff::{
    Block, Expansion, FileDiff, Hunk, Layout, LineRef, Lines, Rediff,
    Row, RowSplice, Rows, Side, diff_texts, realign, rows,
};

#[path = "../../bevy_rubrix/examples/gallery/fixtures/diff/\
          large_pairs.rs"]
mod large_pairs;

use large_pairs::{hundred_k_pair, with_review_edits};

const CONTEXT: u32 = 3;
const LAYOUTS: [Layout; 2] = [Layout::Stacked, Layout::Split];
const LARGE: &str =
    include_str!("../../../bench/fixtures/rust/large.rs.txt");

fn numbered(count: u32) -> Vec<String> {
    (0..count).map(|n| format!("line {n}\n")).collect()
}

fn pair(count: u32, changed: &[u32]) -> (String, String) {
    let old = numbered(count);
    let mut new = old.clone();
    for &n in changed {
        new[n as usize] = format!("edited {n}\n");
    }
    (old.concat(), new.concat())
}

fn buffer_len(lines: &Lines) -> u32 {
    let text = lines.text();
    lines.len() + u32::from(text.is_empty() || text.ends_with('\n'))
}

fn content(lines: &Lines, line: u32) -> &str {
    if line < lines.len() {
        let raw = lines.raw(line);
        raw.strip_suffix('\n').unwrap_or(raw)
    } else {
        ""
    }
}

fn change_spans(hunk: &Hunk) -> Range<u32> {
    let changes = || {
        hunk.blocks()
            .iter()
            .filter(|block| matches!(block, Block::Change { .. }))
    };
    let first = changes().next().expect("every hunk holds a change");
    let last = changes().next_back().unwrap_or(first);
    first.old_range().start..last.old_range().end
}

fn context_lens(hunk: &Hunk) -> (u32, u32) {
    let len = |block: &Block| match *block {
        Block::Context { len, .. } => len,
        Block::Change { .. } => 0,
    };
    let blocks = hunk.blocks();
    (len(&blocks[0]), len(&blocks[blocks.len() - 1]))
}

fn check_hunks(diff: &FileDiff) {
    let (old, new) = (diff.old_lines(), diff.new_lines());
    let mut out = String::with_capacity(new.text().len());
    let mut at = 0;
    for hunk in diff.hunks() {
        let blocks = hunk.blocks();
        let start = blocks[0].old_range().start;
        (at..start).for_each(|i| out.push_str(old.raw(i)));
        for block in blocks {
            if let Block::Context { .. } = block {
                let olds = block.old_range().map(|i| old.raw(i));
                let news = block.new_range().map(|i| new.raw(i));
                assert!(olds.eq(news), "context differs: {block:?}");
            }
            block.new_range().for_each(|i| out.push_str(new.raw(i)));
        }
        at = blocks[blocks.len() - 1].old_range().end;
    }
    (at..old.len()).for_each(|i| out.push_str(old.raw(i)));
    assert!(out == new.text(), "the hunks do not give the new side");

    let hunks = diff.hunks();
    for (h, hunk) in hunks.iter().enumerate() {
        let changes = change_spans(hunk);
        let (lead, trail) = context_lens(hunk);
        let want_lead = if h == 0 {
            CONTEXT.min(changes.start)
        } else {
            CONTEXT
        };
        let want_trail = if h + 1 == hunks.len() {
            CONTEXT.min(old.len() - changes.end)
        } else {
            CONTEXT
        };
        assert_eq!(
            (lead, trail),
            (want_lead, want_trail),
            "hunk {h}"
        );
    }
    for (h, pair) in hunks.windows(2).enumerate() {
        let run =
            change_spans(&pair[1]).start - change_spans(&pair[0]).end;
        assert!(run > 2 * CONTEXT, "hunks {h} and {} merge", h + 1);
    }
}

type Plain = (
    Discriminant<Row>,
    Option<(i64, u32)>,
    Option<(i64, u32)>,
    Option<(i64, u32, bool, bool)>,
);

fn plain(row: Row, lines: i64, hunks: i64) -> Plain {
    let line = |line: Option<LineRef>, by: i64| {
        line.map(|line| {
            (i64::from(line.index) + by, line.number.get())
        })
    };
    let gap = match row {
        Row::Gap {
            hunk,
            hidden,
            up,
            down,
        } => Some((i64::from(hunk) + hunks, hidden, up, down)),
        _ => None,
    };
    let new = line(row.new_line(), lines).map(|(index, number)| {
        (index, (i64::from(number) + lines) as u32)
    });
    (discriminant(&row), line(row.old_line(), 0), new, gap)
}

fn check_splice(
    before: &Rows,
    after: &Rows,
    splice: RowSplice,
    rediff: &Rediff,
) {
    let RowSplice {
        at,
        removed,
        inserted,
    } = splice;
    assert_eq!(before.len() - removed + inserted, after.len());
    let lines =
        i64::from(rediff.inserted) - rediff.lines.len() as i64;
    let hunks =
        i64::from(rediff.new_hunks) - rediff.hunks.len() as i64;
    assert!(before.range(..at).eq(after.range(..at)), "rows above");
    let below = before
        .range(at + removed..)
        .map(|row| plain(row, lines, hunks));
    let now =
        after.range(at + inserted..).map(|row| plain(row, 0, 0));
    assert!(below.eq(now), "rows below");
    if removed > 0 && inserted > 0 {
        assert_ne!(
            before.get(at),
            after.get(at),
            "a shared first row"
        );
        let last = |rows: &Rows, end: u32, by: (i64, i64)| {
            rows.get(end - 1).map(|row| plain(row, by.0, by.1))
        };
        assert_ne!(
            last(before, at + removed, (lines, hunks)),
            last(after, at + inserted, (0, 0)),
            "a shared last row"
        );
    }
}

fn shown_old_lines(table: &Rows, count: u32) -> Vec<bool> {
    let mut shown = vec![false; count as usize];
    for row in table.range(..) {
        if let Some(line) = row.old_line() {
            shown[line.index as usize] = true;
        }
    }
    shown
}

struct Doc {
    diff: FileDiff,
    expansion: Expansion,
    rows: [Rows; 2],
}

impl Doc {
    fn new(old: &str, new: &str) -> Doc {
        let mut diff = diff_texts(old, new, CONTEXT);
        realign(&mut diff);
        let expansion = Expansion::new();
        let rows =
            LAYOUTS.map(|layout| rows(&diff, layout, &expansion));
        Doc {
            diff,
            expansion,
            rows,
        }
    }

    fn edit(&mut self, lines: Range<u32>, text: &str) -> Rediff {
        let count = self.diff.old_lines().len();
        let shown = shown_old_lines(&self.rows[0], count);
        let before = self.rows.clone();
        let rediff = self.diff.edit_new(
            lines,
            text,
            CONTEXT,
            &mut self.expansion,
        );
        for (table, before) in self.rows.iter_mut().zip(&before) {
            let splice =
                table.patch(&self.diff, &self.expansion, &rediff);
            let fresh =
                rows(&self.diff, table.layout(), &self.expansion);
            assert_eq!(*table, fresh, "{:?} rows", table.layout());
            check_splice(before, table, splice, &rediff);
        }
        check_hunks(&self.diff);
        let now = shown_old_lines(&self.rows[0], count);
        let lost = (0..count as usize).find(|&i| shown[i] && !now[i]);
        assert_eq!(lost, None, "an old line shown before is hidden");
        rediff
    }

    fn replace(
        &mut self,
        lines: Range<u32>,
        with: &[&str],
    ) -> Rediff {
        let new = self.diff.new_lines();
        let mut text = with.join("\n");
        if lines.end < buffer_len(new) {
            text.push('\n');
        }
        let end = lines.end.min(new.len());
        self.edit(lines.start..end, &text)
    }

    fn type_at_end(&mut self, line: u32, key: char) -> Rediff {
        let typed =
            format!("{}{key}", content(self.diff.new_lines(), line));
        self.replace(line..line + 1, &[&typed])
    }

    fn reveal(&mut self, hunk: u32, side: Side, lines: u32) {
        self.expansion.reveal(hunk, side, lines);
        self.rows = LAYOUTS
            .map(|layout| rows(&self.diff, layout, &self.expansion));
    }

    fn table(&self) -> Vec<String> {
        show(&self.rows[0])
    }
}

fn show(table: &Rows) -> Vec<String> {
    let number = |line: Option<LineRef>| {
        line.map_or("-".into(), |line| line.number.to_string())
    };
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

struct Rng(u64);

impl Rng {
    fn from_env() -> Rng {
        let seed = std::env::var("RUBRIX_SEED")
            .ok()
            .and_then(|seed| seed.parse().ok())
            .unwrap_or(1u64);
        Rng(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n.max(1))) as u32
    }

    fn boundary(&mut self, text: &str) -> usize {
        let mut at: Vec<usize> =
            text.char_indices().map(|(i, _)| i).collect();
        at.push(text.len());
        at[self.below(at.len() as u32) as usize]
    }
}

fn target(doc: &Doc, rng: &mut Rng) -> u32 {
    let count = buffer_len(doc.diff.new_lines());
    let hunks = doc.diff.hunks();
    if hunks.is_empty() || rng.below(4) == 0 {
        return rng.below(count);
    }
    let hunk = &hunks[rng.below(hunks.len() as u32) as usize];
    let blocks = hunk.blocks();
    let reach = 2 * CONTEXT + 2;
    let start = blocks[0].new_range().start.saturating_sub(reach);
    let end =
        (blocks[blocks.len() - 1].new_range().end + reach).min(count);
    start + rng.below(end - start)
}

fn revert(doc: &mut Doc, rng: &mut Rng) -> bool {
    let (old, new) = (doc.diff.old_lines(), doc.diff.new_lines());
    let blocks: Vec<Block> = doc
        .diff
        .hunks()
        .iter()
        .flat_map(|hunk| hunk.blocks())
        .filter(|block| matches!(block, Block::Change { .. }))
        .copied()
        .collect();
    let Some(&block) =
        blocks.get(rng.below(blocks.len() as u32) as usize)
    else {
        return false;
    };
    let (olds, news) = (block.old_range(), block.new_range());
    let pairs = olds.len().min(news.len()) as u32;
    if pairs > 0 && rng.below(2) == 0 {
        let i = rng.below(pairs);
        let line = content(old, olds.start + i).to_owned();
        let at = news.start + i;
        doc.replace(at..at + 1, &[&line]);
        return true;
    }
    let mut text: String = olds.map(|i| old.raw(i)).collect();
    let len = new.len();
    if !text.is_empty() && !text.ends_with('\n') && news.end != len {
        text.push('\n');
    }
    if !text.is_empty() && news.start == len && new.missing_newline()
    {
        return false;
    }
    doc.edit(news, &text);
    true
}

fn random_edit(doc: &mut Doc, rng: &mut Rng) {
    if rng.below(20) == 0 {
        let gaps = doc.diff.hunks().len() as u32 + 1;
        let side =
            [Side::Up, Side::Down, Side::All][rng.below(3) as usize];
        let side = if side == Side::All && rng.below(4) != 0 {
            Side::Down
        } else {
            side
        };
        doc.reveal(rng.below(gaps), side, rng.below(150) + 1);
        return;
    }
    if rng.below(5) == 0 && revert(doc, rng) {
        return;
    }
    let line = target(doc, rng);
    let text = content(doc.diff.new_lines(), line).to_owned();
    match rng.below(4) {
        0 => {
            let at = rng.boundary(&text);
            let key =
                ['x', ' ', '{', '}', ';', 'é'][rng.below(6) as usize];
            let typed =
                format!("{}{key}{}", &text[..at], &text[at..]);
            doc.replace(line..line + 1, &[&typed]);
        }
        1 if !text.is_empty() => {
            let mut cut = text.clone();
            let chars = text.chars().count() as u32;
            let (at, _) = text
                .char_indices()
                .nth(rng.below(chars) as usize)
                .unwrap();
            cut.remove(at);
            doc.replace(line..line + 1, &[&cut]);
        }
        2 if line > 0 => {
            let above = content(doc.diff.new_lines(), line - 1);
            let joined = format!("{above}{text}");
            doc.replace(line - 1..line + 1, &[&joined]);
        }
        _ => {
            let at = rng.boundary(&text);
            doc.replace(line..line + 1, &[&text[..at], &text[at..]]);
        }
    }
}

#[test]
fn two_thousand_random_edits_keep_hunks_and_rows_right() {
    let mut doc = Doc::new(LARGE, &with_review_edits(LARGE));
    let mut rng = Rng::from_env();
    for _ in 0..2000 {
        random_edit(&mut doc, &mut rng);
    }
}

fn changed_lines(diff: &FileDiff) -> (Vec<u32>, Vec<u32>) {
    let changes = diff
        .hunks()
        .iter()
        .flat_map(|hunk| hunk.blocks())
        .filter(|block| matches!(block, Block::Change { .. }));
    let old = changes
        .clone()
        .flat_map(|block| block.old_range())
        .collect();
    let new = changes.flat_map(|block| block.new_range()).collect();
    (old, new)
}

#[test]
fn typing_sixty_keys_matches_a_full_diff_after_every_key() {
    let old = LARGE;
    let mut doc = Doc::new(old, &with_review_edits(LARGE));
    for key in "fn typed() { let sixty = \"keys at the end of a \
                line\"; }"
        .chars()
        .chain("!!!!!!!!".chars())
        .take(60)
    {
        doc.type_at_end(4000, key);
        let mut full =
            diff_texts(old, doc.diff.new_lines().text(), CONTEXT);
        realign(&mut full);
        assert_eq!(changed_lines(&doc.diff), changed_lines(&full));
        assert_eq!(doc.diff.hunks(), full.hunks());
    }
}

#[test]
fn an_edit_between_two_near_hunks_merges_them() {
    let (old, new) = pair(40, &[10, 20]);
    let mut doc = Doc::new(&old, &new);
    assert_eq!(doc.diff.hunks().len(), 2);
    let rediff = doc.replace(15..16, &["edited 15"]);
    assert_eq!(
        rediff,
        Rediff {
            hunks: 0..2,
            new_hunks: 1,
            lines: 15..16,
            inserted: 1,
        }
    );
    assert_eq!(doc.diff.hunks().len(), 1);
}

#[test]
fn an_insertion_in_a_run_of_equal_lines_re_diffs_to_the_runs_end() {
    let old: String = (0..10)
        .map(|n| format!("head {n}\n"))
        .chain((0..60).map(|_| "}\n".to_owned()))
        .chain((0..10).map(|n| format!("tail {n}\n")))
        .collect();
    let mut doc = Doc::new(&old, &old);
    doc.replace(40..41, &["}", "}"]);
    let full = diff_texts(&old, doc.diff.new_lines().text(), CONTEXT);
    assert_eq!(doc.diff.hunks(), full.hunks());
    assert_eq!(changed_lines(&doc.diff), (vec![], vec![70]));
}

#[test]
fn reverting_a_hunk_removes_it() {
    let (old, new) = pair(40, &[10, 30]);
    let mut doc = Doc::new(&old, &new);
    let rediff = doc.replace(10..11, &["line 10"]);
    assert_eq!((rediff.hunks, rediff.new_hunks), (0..1, 0));
    assert_eq!(changed_lines(&doc.diff), (vec![30], vec![30]));
}

#[test]
fn a_reverted_hunks_caret_line_stays_shown_in_the_merged_gap() {
    let (old, new) = pair(40, &[10, 30]);
    let mut doc = Doc::new(&old, &new);
    doc.replace(10..11, &["line 10"]);
    let table = doc.table();
    assert_eq!(&table[..3], ["G0 7", "E 8/8", "E 9/9"]);
    assert_eq!(
        table[3..8],
        ["E 10/10", "E 11/11", "E 12/12", "E 13/13", "E 14/14"]
    );
    assert_eq!(table[8], "G0 13 up");
    assert_eq!(table[9], "C 28/28");
    let caret = doc.rows[0].get(4);
    assert!(
        matches!(caret, Some(Row::Expanded { new, .. }) if new.index == 10)
    );
}

#[test]
fn an_edit_in_an_unchanged_region_adds_a_hunk_and_splits_its_gap() {
    let (old, new) = pair(300, &[20, 280]);
    let mut doc = Doc::new(&old, &new);
    doc.reveal(1, Side::Down, 10);
    doc.reveal(1, Side::Up, 10);
    let rediff = doc.replace(150..151, &["edited 150"]);
    assert_eq!((rediff.hunks, rediff.new_hunks), (1..1, 1));
    assert_eq!(doc.diff.hunks().len(), 3);
    let table = doc.table();
    let gaps: Vec<&String> =
        table.iter().filter(|row| row.starts_with('G')).collect();
    assert_eq!(
        gaps,
        [
            "G0 17 up",
            "G1 113 up down",
            "G2 113 up down",
            "G3 16 down"
        ]
    );
    let at = |row: &str| table.iter().position(|r| r == row).unwrap();
    assert_eq!(at("G1 113 up down") - at("E 25/25"), 10);
    assert_eq!(at("E 268/268") - at("G2 113 up down"), 1);
}

#[test]
fn two_line_edits_of_one_change_apply_in_order() {
    let (old, new) = pair(60, &[30]);
    let mut doc = Doc::new(&old, &new);
    doc.replace(5..6, &["line", " 5"]);
    doc.replace(41..42, &["line", " 40"]);
    let mut full =
        diff_texts(&old, doc.diff.new_lines().text(), CONTEXT);
    realign(&mut full);
    assert_eq!(doc.diff.hunks(), full.hunks());
    assert_eq!(doc.diff.hunks().len(), 3);
}

#[test]
fn typing_on_the_empty_last_line_appends_to_the_file() {
    let (old, new) = pair(20, &[2]);
    let mut doc = Doc::new(&old, &new);
    let rediff = doc.type_at_end(20, 'x');
    assert_eq!((rediff.lines, rediff.inserted), (20..20, 1));
    doc.type_at_end(20, 'y');
    assert!(doc.diff.new_lines().text().ends_with("line 19\nxy"));
    let full = diff_texts(&old, doc.diff.new_lines().text(), CONTEXT);
    assert_eq!(changed_lines(&doc.diff), changed_lines(&full));
}

#[test]
fn backspace_on_the_empty_last_line_joins_it_upward() {
    let (old, new) = pair(20, &[2]);
    let mut doc = Doc::new(&old, &new);
    let rediff = doc.replace(19..21, &["line 19"]);
    assert_eq!((rediff.lines, rediff.inserted), (19..20, 1));
    assert!(doc.diff.new_lines().text().ends_with("line 19"));
    assert!(doc.diff.new_lines().missing_newline());
    let full = diff_texts(&old, doc.diff.new_lines().text(), CONTEXT);
    assert_eq!(doc.diff.hunks(), full.hunks());
}

#[test]
fn an_edit_that_changes_nothing_in_equal_texts_leaves_no_rows() {
    let text = numbered(10).concat();
    let mut doc = Doc::new(&text, &text);
    let before = doc.rows.clone();
    let rediff = doc.replace(4..5, &["line 4"]);
    assert_eq!((rediff.hunks, rediff.new_hunks), (0..0, 0));
    assert!(doc.diff.hunks().is_empty());
    assert!(doc.rows.iter().all(Rows::is_empty));
    assert_eq!(doc.rows, before);
}

#[test]
#[ignore = "timing; run with --release -- --ignored --nocapture"]
fn rediff_timing() {
    let pairs = [
        ("10k", (LARGE.to_owned(), with_review_edits(LARGE))),
        ("100k", hundred_k_pair(LARGE)),
    ];
    for (name, (old, new)) in pairs {
        let mut diff = diff_texts(&old, &new, CONTEXT);
        realign(&mut diff);
        let hunks = diff.hunks().len();
        println!(
            "{name} pair: {} old lines, {hunks} hunks",
            diff.old_lines().len()
        );
        let mut expansion = Expansion::new();
        let mut table = rows(&diff, Layout::Split, &expansion);
        let line = diff.new_lines().len() / 2;
        let mut times = Vec::new();
        for key in 0..200 {
            let key = char::from(b'a' + (key % 26) as u8);
            let raw = diff.new_lines().raw(line);
            let typed =
                format!("{}{key}\n", raw.strip_suffix('\n').unwrap());
            let start = Instant::now();
            let rediff = diff.edit_new(
                line..line + 1,
                &typed,
                CONTEXT,
                &mut expansion,
            );
            table.patch(&diff, &expansion, &rediff);
            times.push(start.elapsed().as_secs_f64() * 1e6);
        }
        times.sort_by(f64::total_cmp);
        let at = |q: usize| {
            times[(times.len() * q / 100).min(times.len() - 1)]
        };
        println!(
            "rediff_timing {name} keys {} median {:.1} p95 {:.1}",
            times.len(),
            at(50),
            at(95)
        );
    }
}
