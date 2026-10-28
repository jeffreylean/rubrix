mod support;

use bevy::app::App;
use bevy::asset::Handle;
use bevy::color::Alpha;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::math::Vec2;
use bevy::ui::widget::{ImageNode, Text};
use bevy_rubrix::{DiffRow, DiffSource, Layout, Row};
use support::diff::*;
use support::{copy, drag_on, hold_on, point_in, release_on};

fn split(source: DiffSource) -> App {
    let mut app = showing(source);
    set(&mut app, |showing| showing.layout = Layout::Split);
    app
}

fn hatched(app: &App, root: Entity) -> [bool; 2] {
    let world = app.world();
    let mut sides = [false; 2];
    let mut texts = 0;
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if world.get::<ImageNode>(node).is_some_and(|image| {
            image.image != Handle::default()
                && !image.color.is_fully_transparent()
        }) {
            sides[texts / 2] = true;
        }
        if world.get::<Text>(node).is_some() {
            texts += 1;
        }
        if let Some(children) = world.get::<Children>(node) {
            stack.extend(children.iter().rev());
        }
    }
    sides
}

#[test]
fn each_spawned_row_matches_the_split_row_for_its_index() {
    let source =
        DiffSource::patch(LARGE_PATCH).expect("the patch parses");
    let want = expected(&source, Layout::Split, &[]);
    let files = source.files().collect::<Vec<_>>();
    let mut app = split(source.clone());
    assert_eq!(doc(&mut app).len(), want.len());
    let mut lines = 0;
    for top in [0, 3_000, 7_000] {
        scroll_to(&mut app, top);
        for (index, root) in slots(&mut app) {
            let row = doc(&mut app).row(index);
            assert_eq!(row, want.get(index).copied(), "row {index}");
            let Some(DiffRow::Body { file, row }) = row else {
                continue;
            };
            if matches!(row, Row::Gap { .. }) {
                continue;
            }
            lines += 1;
            let diff = files[file];
            let shown = texts(&app, root);
            let number = |line: Option<rubrix_diff::LineRef>| {
                line.map(|line| line.number.to_string())
                    .unwrap_or_default()
            };
            assert_eq!(
                shown[0].trim(),
                number(row.old_line()),
                "row {index}"
            );
            assert_eq!(
                shown[2].trim(),
                number(row.new_line()),
                "row {index}"
            );
            assert_eq!(
                shown[1],
                old_text(diff, row).unwrap_or_default()
            );
            assert_eq!(
                shown[3],
                new_text(diff, row).unwrap_or_default()
            );
            assert_eq!(
                hatched(&app, root),
                [row.old_line().is_none(), row.new_line().is_none()],
                "row {index}"
            );
        }
    }
    assert!(lines > 20, "{lines} line rows built");
}

#[test]
fn a_realigned_block_pairs_the_similar_lines() {
    let old = "top\nfirst removed\nlet total = count + 1;\nlast \
               removed\nend\n";
    let new = "top\nlet total = count + 2;\nend\n";
    let mut app = split(DiffSource::texts("total.rs", old, new));
    let rows = (2..5)
        .map(|index| {
            let root = slot(&mut app, index);
            (texts(&app, root), hatched(&app, root))
        })
        .collect::<Vec<_>>();
    let side = |text: &str| text.to_owned();
    assert_eq!(rows[0].0[1], side("first removed"));
    assert_eq!(rows[0].1, [false, true]);
    assert_eq!(rows[1].0[1], side("let total = count + 1;"));
    assert_eq!(rows[1].0[3], side("let total = count + 2;"));
    assert_eq!(rows[1].1, [false, false]);
    assert_eq!(rows[2].0[1], side("last removed"));
    assert_eq!(rows[2].1, [false, true]);
}

fn first_lines(app: &mut App) -> (Option<u32>, Option<u32>) {
    let first = first_visible(app);
    match doc(app).row(first) {
        Some(DiffRow::Body { row, .. }) => (
            row.old_line().map(|line| line.index),
            row.new_line().map(|line| line.index),
        ),
        other => panic!("row {first} is {other:?}"),
    }
}

fn keeps(
    before: (Option<u32>, Option<u32>),
    after: (Option<u32>, Option<u32>),
) -> bool {
    before.0.is_some_and(|old| after.0 == Some(old))
        || before.1.is_some_and(|new| after.1 == Some(new))
}

#[test]
fn switching_layouts_keeps_the_first_visible_source_line() {
    let source =
        DiffSource::patch(LARGE_PATCH).expect("the patch parses");
    let mut app = showing(source);
    for row in [3_000, 3_004, 7_321] {
        set(&mut app, |showing| showing.layout = Layout::Stacked);
        scroll_to(&mut app, row);
        let stacked = first_lines(&mut app);
        set(&mut app, |showing| showing.layout = Layout::Split);
        let split = first_lines(&mut app);
        assert!(
            keeps(stacked, split),
            "row {row}: {stacked:?} {split:?}"
        );
        set(&mut app, |showing| showing.layout = Layout::Stacked);
        let back = first_lines(&mut app);
        assert!(keeps(split, back), "row {row}: {split:?} {back:?}");
    }
}

const REALIGN_OLD: &str =
    include_str!("../examples/gallery/fixtures/diff/realign-old.rs");
const REALIGN_NEW: &str =
    include_str!("../examples/gallery/fixtures/diff/realign-new.rs");

#[test]
fn a_drag_that_starts_on_the_left_copies_only_old_text() {
    let mut app = laid_out(
        DiffSource::texts("main.rs", REALIGN_OLD, REALIGN_NEW),
        800.0,
        400.0,
    );
    set(&mut app, |showing| showing.layout = Layout::Split);
    let started = slot(&mut app, 2);
    let old = rich_texts(&app, started)[1];
    let from = point_in(&app, old, 0);
    hold_on(&mut app, old, from, 1);
    let paired = slot(&mut app, 3);
    let new = rich_texts(&app, paired)[3];
    let printed = slot(&mut app, 4);
    let below = rich_texts(&app, printed)[1];
    let to = Vec2::new(
        point_in(&app, new, 10).x,
        point_in(&app, below, 0).y,
    );
    drag_on(&mut app, old, from, to);
    release_on(&mut app, old, to);
    let lines = REALIGN_OLD.lines().collect::<Vec<_>>();
    assert_eq!(copy(&mut app), lines[1..4].join("\n"));
}
