//! The virtual list against real bevy_ui layout, headless: which rows
//! it builds, where they show, and what a change keeps still.

use core::ops::Range;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::app::{App, PostUpdate};
use bevy::camera::NormalizedRenderTarget;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::Query;
use bevy::ecs::world::World;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
use bevy::math::Vec2;
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, PointerScroll};
use bevy::picking::pointer::{Location, PointerId};
use bevy::ui::{
    ComputedNode, Display, Node, OverflowAxis, ScrollPosition,
    UiGlobalTransform, UiSystems, Val, percent, px,
};

use super::layout::{laid_out, laid_out_at};
use crate::tests::Plain;
use crate::views::{
    FrameProps, RowSlot, RowSpan, VirtualList, column, frame,
    row as hrow, virtual_list,
};
use crate::{
    AnyView, Bevy, Prop, ViewExt, derived, every_frame, keyed, mount,
    resource,
};

const ESTIMATE: f32 = 20.0;
const VIEW: f32 = 500.0;
const OVERSCAN: f32 = 1000.0;

#[derive(Resource)]
struct Count(usize);

#[derive(Resource)]
struct Tall(f32);

#[derive(Resource)]
struct Across(f32);

#[derive(Resource)]
struct Grown(usize, f32);

#[derive(Resource)]
struct Follow(bool);

#[derive(Resource)]
struct Sideways(bool);

#[derive(Component)]
struct Shows(usize);

type Height = fn(&World, usize) -> f32;

fn uniform(_: &World, _: usize) -> f32 {
    20.0
}

fn h19(_: &World, _: usize) -> f32 {
    19.0
}

fn tall_hundredths(_: &World, index: usize) -> f32 {
    if index.is_multiple_of(100) {
        60.0
    } else {
        20.0
    }
}

fn regions(_: &World, index: usize) -> f32 {
    match index {
        ..3_000 => 20.0,
        3_000..7_000 => 10.0,
        _ => 200.0,
    }
}

fn mixed(_: &World, index: usize) -> f32 {
    let x = (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 59;
    12.0 + x as f32 * 28.0 / 31.0
}

fn short_tail(_: &World, index: usize) -> f32 {
    if index >= 9_900 { 5.0 } else { 40.0 }
}

fn tall_tail(_: &World, index: usize) -> f32 {
    if index >= 9_960 { 60.0 } else { 17.0 }
}

fn one_huge(_: &World, index: usize) -> f32 {
    if index == 5_010 { 2_000.0 } else { 20.0 }
}

fn flat(_: &World, _: usize) -> f32 {
    0.0
}

fn grown(world: &World, index: usize) -> f32 {
    match world.get_resource::<Grown>() {
        Some(&Grown(at, height)) if at == index => height,
        _ => 20.0,
    }
}

fn row(height: Height) -> impl Fn(RowSlot) -> AnyView<Bevy, Plain> {
    move |slot| {
        AnyView::new(move |cx| {
            let index = slot.index();
            let node = cx.build(
                frame().height(
                    derived(move |world| {
                        px(height(world, index.get(world)))
                    })
                    .when(every_frame()),
                ),
            );
            cx.effect(
                node,
                slot.index().into(),
                |world, node, &index| {
                    world.entity_mut(node).insert(Shows(index));
                },
            );
            node
        })
    }
}

fn list(app: &mut App, count: usize, height: Height) -> Entity {
    list_in(app, count, row(height), px(VIEW))
}

fn list_in(
    app: &mut App,
    count: usize,
    row: impl Fn(RowSlot) -> AnyView<Bevy, Plain> + Send + Sync + 'static,
    view: impl Into<Prop<Val>>,
) -> Entity {
    let area = mounted(app, count, ESTIMATE, row, view);
    settle(app);
    area
}

fn mounted(
    app: &mut App,
    count: usize,
    estimate: f32,
    row: impl Fn(RowSlot) -> AnyView<Bevy, Plain> + Send + Sync + 'static,
    view: impl Into<Prop<Val>>,
) -> Entity {
    app.insert_resource(Count(count));
    if !app.world().contains_resource::<Follow>() {
        app.insert_resource(Follow(false));
    }
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            estimate,
            row,
        )
        .follow_bottom(resource::<Follow, _>(|follow| follow.0)),))
        .width(px(300.0))
        .height(view),
    );
    the_list(app)
}

fn the_list(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
}

fn updates(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn settle(app: &mut App) {
    updates(app, 6);
}

fn state(app: &App, area: Entity) -> &VirtualList {
    app.world().get::<VirtualList>(area).expect("a list")
}

fn scroll_to(app: &mut App, area: Entity, index: usize) {
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_to(index);
}

fn scroll(app: &App, area: Entity) -> f32 {
    app.world().get::<ScrollPosition>(area).expect("a scroll").y
}

fn wheel(app: &mut App, area: Entity, by: f32) {
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y += by;
    app.update();
}

fn to_the_end(app: &mut App, area: Entity) {
    for _ in 0..3 {
        app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
            1.0e9;
        updates(app, 3);
    }
}

fn reveal(app: &mut App, area: Entity, index: usize) {
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_into_view(index);
    settle(app);
}

fn reveal_range(
    app: &mut App,
    area: Entity,
    index: usize,
    range: Range<f32>,
) {
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_range_into_view(index, range);
    settle(app);
}

fn top(app: &App, node: Entity) -> f32 {
    let computed = app.world().get::<ComputedNode>(node).unwrap();
    let at = app.world().get::<UiGlobalTransform>(node).unwrap();
    (at.translation.y - computed.size().y / 2.0)
        * computed.inverse_scale_factor()
}

fn height(app: &App, node: Entity) -> f32 {
    let computed = app.world().get::<ComputedNode>(node).unwrap();
    computed.size().y * computed.inverse_scale_factor()
}

fn drawn(app: &mut App, area: Entity) -> Vec<(f32, f32)> {
    let origin = top(app, area);
    let built = app
        .world_mut()
        .query_filtered::<(Entity, &ChildOf), With<Shows>>()
        .iter(app.world())
        .map(|(row, slot)| (row, slot.parent()))
        .collect::<Vec<_>>();
    let mut spans = built
        .into_iter()
        .map(|(row, slot)| {
            let start = top(app, row) - origin;
            let end = start + height(app, row);
            let node = app.world().get::<Node>(slot).unwrap();
            if node.overflow.y.is_visible() {
                (start, end)
            } else {
                let from = top(app, slot) - origin;
                (start.max(from), end.min(from + height(app, slot)))
            }
        })
        .filter(|(start, end)| end > start)
        .collect::<Vec<_>>();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    spans
}

fn uncovered(app: &mut App, area: Entity) -> f32 {
    let mut at = 0.0f32;
    let mut worst = 0.0f32;
    for (start, end) in drawn(app, area) {
        if end <= at {
            continue;
        }
        worst = worst.max(start.min(VIEW) - at);
        at = end;
        if at >= VIEW {
            return worst;
        }
    }
    worst.max(VIEW - at)
}

fn rows(app: &mut App, area: Entity) -> Vec<(usize, Entity, f32)> {
    let origin = top(app, area);
    let mut built = app
        .world_mut()
        .query::<(Entity, &Shows)>()
        .iter(app.world())
        .map(|(node, shows)| (shows.0, node))
        .collect::<Vec<_>>();
    built.sort();
    built
        .into_iter()
        .map(|(index, node)| (index, node, top(app, node) - origin))
        .collect()
}

fn shown_at(
    app: &mut App,
    area: Entity,
    index: usize,
) -> Option<f32> {
    rows(app, area)
        .into_iter()
        .find(|&(shown, ..)| shown == index)
        .map(|(.., top)| top)
}

fn near(got: Option<f32>, want: f32) -> bool {
    got.is_some_and(|got| (got - want).abs() <= 1.0)
}

fn assert_cover(
    app: &mut App,
    area: Entity,
    from: f32,
    to: f32,
    when: &str,
) {
    let rows = rows(app, area);
    let indices = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let unbroken =
        indices.windows(2).all(|pair| pair[1] == pair[0] + 1);
    assert!(unbroken, "{when}: rows {indices:?}");
    let (_, first, start) = rows[0];
    let (_, last, end) = rows[rows.len() - 1];
    let end = end + height(app, last);
    assert!(
        start <= from,
        "{when}: the first row, {first}, starts at {start}"
    );
    assert!(end >= to, "{when}: the last row, {last}, ends at {end}");
}

fn assert_none(failed: Vec<String>, cases: usize) {
    assert!(
        failed.is_empty(),
        "{} of {cases} cases:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

#[test]
fn it_builds_the_rows_within_the_overscan() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    let most = ((VIEW + 2.0 * OVERSCAN) / ESTIMATE) as usize + 1;

    let built = rows(&mut app, area).len();
    assert_eq!(shown_at(&mut app, area, 0), Some(0.0), "at the top");
    assert_cover(
        &mut app,
        area,
        0.0,
        VIEW + 2.0 * OVERSCAN,
        "at the top",
    );
    assert!(built <= most, "at the top: {built} rows");

    scroll_to(&mut app, area, 5_000);
    settle(&mut app);
    assert_eq!(
        shown_at(&mut app, area, 5_000),
        Some(0.0),
        "in the middle"
    );
    assert_eq!(state(&app, area).first_visible(), 5_000);
    assert_cover(
        &mut app,
        area,
        -OVERSCAN,
        VIEW + OVERSCAN,
        "in the middle",
    );

    const NEAR: f32 = 100.0;
    let mut app = laid_out();
    app.insert_resource(Count(10_000));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row(uniform),
        )
        .overscan(NEAR),))
        .width(px(300.0))
        .height(px(VIEW)),
    );
    settle(&mut app);
    let area = the_list(&mut app);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), 5_000);
    assert_cover(&mut app, area, -NEAR, VIEW + NEAR, "100 px of it");
    let most = ((VIEW + 2.0 * NEAR) / ESTIMATE) as usize + 1;
    let built = rows(&mut app, area).len();
    assert!(built <= most, "100 px of it: {built} rows");
}

#[test]
fn scroll_to_puts_the_row_at_the_top_or_the_view_at_the_end() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);

    assert_eq!(shown_at(&mut app, area, 9_999), Some(VIEW - 20.0));
    assert_eq!(
        state(&app, area).spawned().map(|s| *s.end()),
        Some(9_999)
    );
    assert_cover(&mut app, area, -OVERSCAN, VIEW, "at the end");

    let mut app = laid_out();
    let area = list(&mut app, 10_000, uniform);
    scroll_to(&mut app, area, 5_000);
    app.update();
    let landed = scroll(&app, area);
    assert_eq!(landed, 5_000.0 * 20.0, "row 5000 at the top");

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y +=
        100.0;
    settle(&mut app);
    assert_eq!(
        scroll(&app, area),
        landed + 100.0,
        "a scroll on the frame after stands"
    );
}

#[test]
fn a_scroll_shows_new_rows_in_the_nodes_it_already_has() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, uniform);
    let nodes = |app: &mut App| {
        rows(app, area)
            .into_iter()
            .map(|(index, node, _)| (node, index))
            .collect::<HashMap<_, _>>()
    };
    let before = nodes(&mut app);
    let spawned = app.world().entities().count_spawned();

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        1_500.0;
    settle(&mut app);

    let after = nodes(&mut app);
    let mut kept = after.keys().collect::<Vec<_>>();
    kept.sort();
    let mut had = before.keys().collect::<Vec<_>>();
    had.sort();
    assert_eq!(kept, had, "the same row nodes");
    assert!(after.iter().any(|(node, index)| before[node] != *index));
    assert_eq!(app.world().entities().count_spawned(), spawned);
    assert_eq!(shown_at(&mut app, area, 75), Some(0.0));
}

#[test]
fn a_scroll_reads_only_the_rows_whose_slot_moves() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    let mut app = laid_out();
    let counted = |slot: RowSlot| {
        frame()
            .height(slot.index().map(|_| {
                READS.fetch_add(1, Ordering::Relaxed);
                px(20.0)
            }))
            .boxed()
    };
    let area = list_in(&mut app, 10_000, counted, px(VIEW));
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    READS.store(0, Ordering::Relaxed);
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y +=
        200.0;
    settle(&mut app);

    assert_eq!(
        READS.load(Ordering::Relaxed),
        10,
        "one read a moved row"
    );
}

#[test]
fn the_first_visible_row_holds_still_as_rows_above_change() {
    for scale in [1.0, 1.25] {
        let mut app = laid_out_at(scale);
        let area = list(&mut app, 10_000, tall_hundredths);
        scroll_to(&mut app, area, 5_001);
        for frame in 0..6 {
            app.update();
            let when =
                format!("{scale}x, row 5000 measured, frame {frame}");
            assert_eq!(
                state(&app, area).first_visible(),
                5_001,
                "{when}"
            );
            assert_eq!(
                shown_at(&mut app, area, 5_001),
                Some(0.0),
                "{when}"
            );
        }
        assert_eq!(
            shown_at(&mut app, area, 5_000),
            Some(-60.0),
            "{scale}x, row 5000 measured"
        );

        app.world_mut().resource_mut::<Count>().0 = 11_000;
        settle(&mut app);
        let when = format!("{scale}x, rows added");
        assert_eq!(
            state(&app, area).first_visible(),
            5_001,
            "{when}"
        );
        assert_eq!(
            shown_at(&mut app, area, 5_001),
            Some(0.0),
            "{when}"
        );

        let mut app = laid_out_at(scale);
        let area = list(&mut app, 10_000, grown);
        scroll_to(&mut app, area, 5_000);
        settle(&mut app);
        let before = scroll(&app, area);

        app.insert_resource(Grown(4_990, 100.0));
        for frame in 0..4 {
            app.update();
            assert_eq!(
                state(&app, area).first_visible(),
                5_000,
                "{scale}x, row 4990 grown, frame {frame}"
            );
        }
        let when = format!("{scale}x, row 4990 grown");
        assert_eq!(
            shown_at(&mut app, area, 5_000),
            Some(0.0),
            "{when}"
        );
        assert_eq!(scroll(&app, area), before + 80.0, "{when}");
    }
}

#[test]
fn growing_at_the_bottom_keeps_the_last_row_in_sight() {
    for scale in [1.0, 1.25] {
        let mut app = laid_out_at(scale);
        app.insert_resource(Follow(true));
        let area = list(&mut app, 10_000, h19);
        scroll_to(&mut app, area, 9_999);
        settle(&mut app);

        for count in (10_002..=10_100).step_by(2) {
            app.world_mut().resource_mut::<Count>().0 = count;
            app.update();
        }
        settle(&mut app);
        let last = shown_at(&mut app, area, 10_099);
        assert!(
            near(last, VIEW - 19.0),
            "{scale}x, two rows an update: row 10099 at {last:?}"
        );

        app.world_mut().resource_mut::<Count>().0 = 11_100;
        settle(&mut app);
        let last = shown_at(&mut app, area, 11_099);
        assert!(
            near(last, VIEW - 19.0),
            "{scale}x, 1,000 rows at once: row 11099 at {last:?}"
        );
        assert_eq!(
            state(&app, area).spawned().map(|s| *s.end()),
            Some(11_099),
            "{scale}x, 1,000 rows at once"
        );
    }
}

#[test]
fn growing_at_the_bottom_moves_no_list_that_does_not_follow() {
    let mut app = laid_out();
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    let up = scroll(&app, area) - 40.0;
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y = up;
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 2_000;
    settle(&mut app);
    assert_eq!(scroll(&app, area), up, "40 px above the bottom");

    scroll_to(&mut app, area, 1_999);
    settle(&mut app);
    let first = state(&app, area).first_visible();
    let at = scroll(&app, area);

    app.world_mut().resource_mut::<Count>().0 = 3_000;
    settle(&mut app);
    assert_eq!(scroll(&app, area), at, "at the bottom");
    assert_eq!(
        state(&app, area).first_visible(),
        first,
        "at the bottom"
    );
}

#[test]
fn a_list_at_its_end_keeps_it_in_sight_as_the_viewport_shrinks() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    app.insert_resource(Tall(VIEW));
    let area = list_in(
        &mut app,
        10_000,
        row(tall_hundredths),
        resource::<Tall, _>(|tall| px(tall.0)),
    );
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);

    app.insert_resource(Tall(300.0));
    settle(&mut app);

    assert_eq!(shown_at(&mut app, area, 9_999), Some(280.0));
}

#[test]
fn a_following_list_keeps_each_new_row_in_sight() {
    for height in [uniform as Height, tall_hundredths] {
        let mut app = laid_out();
        app.insert_resource(Follow(true));
        let area = list(&mut app, 0, height);
        assert!(rows(&mut app, area).is_empty(), "no rows");
        assert_eq!(state(&app, area).spawned(), None, "no rows");

        for count in (2..=200).step_by(2) {
            app.world_mut().resource_mut::<Count>().0 = count;
            app.update();
            let top =
                shown_at(&mut app, area, count.saturating_sub(3));
            assert!(
                count < 4 || top.is_some_and(|top| (0.0..VIEW).contains(&top)),
                "{count} rows: row {} at {top:?}",
                count - 3
            );
        }
        settle(&mut app);
        let last = shown_at(&mut app, area, 199).unwrap();
        assert_eq!(last + height(app.world(), 199), VIEW);
    }
}

#[test]
fn a_list_built_following_follows_rows_added_before_its_first_update()
{
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = mounted(&mut app, 0, ESTIMATE, row(uniform), px(VIEW));
    app.world_mut().resource_mut::<Count>().0 = 300;
    settle(&mut app);

    assert!(state(&app, area).following());
    assert_eq!(shown_at(&mut app, area, 299), Some(VIEW - 20.0));
}

#[test]
fn a_list_grown_from_empty_shows_its_first_row() {
    for (from, call) in [(0, None), (10, None), (10, Some(0))] {
        let mut app = laid_out();
        let area = list(&mut app, from, uniform);
        if let Some(index) = call {
            scroll_to(&mut app, area, index);
            settle(&mut app);
        }

        app.world_mut().resource_mut::<Count>().0 = 10_000;
        settle(&mut app);

        let when = format!("from {from} rows, scroll_to {call:?}");
        assert_eq!(scroll(&app, area), 0.0, "{when}");
        assert_eq!(state(&app, area).first_visible(), 0, "{when}");
    }
}

#[test]
fn scroll_to_moves_a_following_list_where_it_says() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 0, uniform);

    app.world_mut().resource_mut::<Count>().0 = 10_000;
    scroll_to(&mut app, area, 0);
    settle(&mut app);
    assert_eq!(scroll(&app, area), 0.0, "in the frame it grows");

    app.world_mut().resource_mut::<Count>().0 = 20_000;
    settle(&mut app);
    scroll_to(&mut app, area, 0);
    settle(&mut app);
    assert_eq!(scroll(&app, area), 0.0, "once it follows");
    assert!(!state(&app, area).following());
}

#[test]
fn shrinking_below_the_rows_built_keeps_the_rows_left() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 10;
    settle(&mut app);

    let indices = rows(&mut app, area)
        .into_iter()
        .map(|row| row.0)
        .collect::<Vec<_>>();
    assert_eq!(indices, (0..10).collect::<Vec<_>>());
    assert_eq!(scroll(&app, area), 0.0);
}

#[test]
fn rows_that_lay_out_at_nothing_keep_their_estimate() {
    let most = ((VIEW + 2.0 * OVERSCAN) / ESTIMATE) as usize + 1;
    let mut app = laid_out();
    let area = list(&mut app, 10_000, flat);
    let built = rows(&mut app, area);
    assert_eq!(built[0].0, 0, "in view");
    assert!(built.len() <= most, "in view: {} rows", built.len());

    let mut app = laid_out();
    app.insert_resource(Count(10_000));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row(flat),
        ),))
        .height(px(VIEW))
        .display(Display::None),
    );
    settle(&mut app);
    let built =
        app.world_mut().query::<&Shows>().iter(app.world()).count();
    assert!(built <= most, "in a hidden list: {built} rows");

    let mut app = laid_out();
    let area = list(&mut app, 10_000, |_, index| {
        if index.is_multiple_of(10) { 0.0 } else { 20.0 }
    });
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);
    let first = state(&app, area).first_visible();
    let at = scroll(&app, area);

    app.world_mut().resource_mut::<Count>().0 = 10_100;
    settle(&mut app);
    let when = "a scroll_to among them, then 100 rows";
    assert_eq!(state(&app, area).first_visible(), first, "{when}");
    assert_eq!(scroll(&app, area), at, "{when}");
}

struct Whole {
    name: &'static str,
    count: usize,
    estimate: f32,
    height: Height,
}

const WHOLE: [Whole; 3] = [
    Whole {
        name: "20 px rows under 2, as many slots as it may build",
        count: 10_000,
        estimate: 2.0,
        height: uniform,
    },
    Whole {
        name: "regions of 20, 10 and 200 px under 20, 100,000 rows",
        count: 100_000,
        estimate: 20.0,
        height: regions,
    },
    Whole {
        name: "one row of 2,000 px among 20 px under 200",
        count: 10_000,
        estimate: 200.0,
        height: one_huge,
    },
];

fn whole_failures(case: &Whole) -> Vec<String> {
    let mut app = laid_out();
    let area = mounted(
        &mut app,
        case.count,
        case.estimate,
        row(case.height),
        px(VIEW),
    );
    let mut failed = Vec::new();
    let mut check = |app: &mut App, when: String, cover: bool| {
        let slots = state(app, area).rows();
        let spans = drawn(app, area);
        let overlap = spans
            .windows(2)
            .find(|pair| pair[1].0 < pair[0].1 - 0.01)
            .map(|pair| (pair[0], pair[1]));
        let gap = if cover { uncovered(app, area) } else { 0.0 };
        if slots >= 200 || overlap.is_some() || gap > 0.0 {
            failed.push(format!(
                "{}, {when}: {slots} slots, {gap} px uncovered, \
                 overlap {overlap:?}",
                case.name
            ));
        }
    };
    for frame in 1..=10 {
        app.update();
        check(
            &mut app,
            format!("first paint, frame {frame}"),
            frame >= 2,
        );
    }
    let count = case.count;
    let thumb = |app: &mut App, area: Entity, at: f32| {
        let list = state(app, area);
        let end = list.row_span(list.count() - 1).unwrap().top;
        app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
            end * at;
    };
    type Land = Box<dyn Fn(&mut App, Entity)>;
    let lands: [(&str, Land); 6] = [
        (
            "scroll_to(5000)",
            Box::new(|app, area| scroll_to(app, area, 5_000)),
        ),
        (
            "a jump of 20,000 px",
            Box::new(|app, area| {
                app.world_mut()
                    .get_mut::<ScrollPosition>(area)
                    .unwrap()
                    .y += 20_000.0;
            }),
        ),
        (
            "scroll_to the last",
            Box::new(move |app, area| {
                scroll_to(app, area, count - 1)
            }),
        ),
        (
            "the thumb to 0.5",
            Box::new(move |app, area| thumb(app, area, 0.5)),
        ),
        (
            "scroll_to(7100)",
            Box::new(|app, area| scroll_to(app, area, 7_100)),
        ),
        (
            "the thumb to 0.31",
            Box::new(move |app, area| thumb(app, area, 0.31)),
        ),
    ];
    for (name, land) in lands {
        land(&mut app, area);
        for frame in 1..=6 {
            app.update();
            check(
                &mut app,
                format!("{name}, frame {frame}"),
                frame >= 2,
            );
        }
    }
    for step in 0..12 {
        thumb(&mut app, area, 0.1 + step as f32 / 40.0);
        app.update();
        check(&mut app, format!("drag step {step}"), false);
    }
    for frame in 2..=6 {
        app.update();
        check(&mut app, format!("drag end, frame {frame}"), true);
    }
    failed
}

#[test]
fn landings_stay_whole_under_any_estimate() {
    assert_none(
        WHOLE.iter().flat_map(whole_failures).collect(),
        WHOLE.len(),
    );
}

#[test]
fn a_content_y_maps_to_its_row_and_back() {
    let mut app = laid_out();
    let area = list(&mut app, 0, uniform);
    assert_eq!(state(&app, area).row_at(0.0), None, "no rows");
    assert_eq!(state(&app, area).row_span(0), None, "no rows");

    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    let mapped = state(&app, area);

    assert_eq!(
        mapped.row_span(0),
        Some(RowSpan {
            top: 0.0,
            height: 60.0
        })
    );
    assert_eq!(
        mapped.row_span(1),
        Some(RowSpan {
            top: 60.0,
            height: 20.0
        })
    );
    assert_eq!(mapped.row_at(59.9), Some(0));
    assert_eq!(mapped.row_at(60.0), Some(1));
    assert_eq!(mapped.row_at(85.0), Some(2));
    assert_eq!(mapped.row_at(-5.0), Some(0));
    assert_eq!(mapped.row_at(1.0e9), Some(9_999));
    assert_eq!(mapped.row_span(10_000), None);
    let span = mapped.row_span(5).unwrap();
    assert_eq!(mapped.row_at(span.top + span.height / 2.0), Some(5));
}

#[test]
fn scrolling_a_row_into_view_moves_the_least() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, uniform);

    reveal(&mut app, area, 40);
    assert_eq!(scroll(&app, area), 40.0 * 20.0 + 20.0 - VIEW);
    assert_eq!(shown_at(&mut app, area, 40), Some(VIEW - 20.0));

    reveal(&mut app, area, 30);
    assert_eq!(scroll(&app, area), 320.0, "a row in view stays");

    reveal(&mut app, area, 5);
    assert_eq!(scroll(&app, area), 100.0);
    assert_eq!(shown_at(&mut app, area, 5), Some(0.0));
}

#[test]
fn a_row_taller_than_the_view_scrolls_in_by_the_least() {
    let mut app = laid_out();
    app.insert_resource(Grown(50, 2_000.0));
    let area = list(&mut app, 10_000, grown);

    reveal(&mut app, area, 50);
    assert_eq!(shown_at(&mut app, area, 50), Some(0.0), "from above");

    let inside = scroll(&app, area) + 800.0;
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        inside;
    settle(&mut app);
    reveal(&mut app, area, 50);
    assert_eq!(scroll(&app, area), inside, "from inside it");

    scroll_to(&mut app, area, 60);
    settle(&mut app);
    reveal(&mut app, area, 50);
    let span = state(&app, area).row_span(50).unwrap();
    assert_eq!(
        scroll(&app, area),
        span.top + span.height - VIEW,
        "from below"
    );
}

#[test]
fn a_range_of_a_tall_row_scrolls_in_by_the_least() {
    let mut app = laid_out();
    app.insert_resource(Grown(50, 2_000.0));
    let area = list(&mut app, 10_000, grown);
    reveal(&mut app, area, 50);
    let top = state(&app, area).row_span(50).unwrap().top;
    assert_eq!(scroll(&app, area), top);

    reveal_range(&mut app, area, 50, 1_500.0..1_520.0);
    assert_eq!(
        scroll(&app, area),
        top + 1_520.0 - VIEW,
        "from above"
    );

    reveal_range(&mut app, area, 50, 600.0..620.0);
    assert_eq!(scroll(&app, area), top + 600.0, "from below");

    reveal_range(&mut app, area, 50, 700.0..720.0);
    assert_eq!(scroll(&app, area), top + 600.0, "in view");

    reveal_range(&mut app, area, 50, 100.0..900.0);
    assert_eq!(
        scroll(&app, area),
        top + 400.0,
        "taller than the view"
    );
}

fn list_across(app: &mut App, height: Height) -> Entity {
    app.insert_resource(Across(300.0));
    app.insert_resource(Count(10_000));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row(height),
        ),))
        .width(resource::<Across, _>(|across| px(across.0)))
        .height(px(VIEW)),
    );
    settle(app);
    let area = the_list(app);
    scroll_to(app, area, 5_000);
    settle(app);
    area
}

#[test]
fn a_width_change_reestimates_rows_out_of_sight() {
    let mut app = laid_out();
    let area = list_across(&mut app, |world, _| {
        if world.resource::<Across>().0 < 300.0 {
            40.0
        } else {
            20.0
        }
    });
    app.insert_resource(Across(200.0));
    settle(&mut app);

    assert_eq!(
        state(&app, area).row_span(50),
        Some(RowSpan {
            top: 2_000.0,
            height: 40.0
        }),
        "rows that wrap"
    );
    assert_eq!(
        shown_at(&mut app, area, 5_000),
        Some(0.0),
        "rows that wrap"
    );

    let mut app = laid_out();
    let area = list_across(&mut app, one_huge);
    app.insert_resource(Across(200.0));
    settle(&mut app);

    let when = "past one huge row";
    assert_eq!(
        state(&app, area).row_span(100).unwrap().height,
        20.0,
        "{when}"
    );
    assert_eq!(
        state(&app, area).row_span(9_999).unwrap().height,
        20.0,
        "{when}"
    );
}

#[test]
fn one_huge_row_barely_moves_the_estimate() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, |_, index| {
        if index == 3 { 2_000.0 } else { 20.0 }
    });

    let last = state(&app, area).row_span(9_999).unwrap();
    let real = 9_999.0 * 20.0 + 1_980.0;
    assert!(
        (last.top - real).abs() / real < 0.03,
        "row 9999 at {} of {real}",
        last.top
    );
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);
    assert_eq!(shown_at(&mut app, area, 9_999), Some(VIEW - 20.0));

    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    for index in [2_001, 9_999] {
        assert_eq!(
            state(&app, area).row_span(index).unwrap().height,
            20.0,
            "a few tall rows, row {index}"
        );
    }
}

#[derive(Component)]
struct Gutter;

fn wide(index: usize) -> f32 {
    if index == 7 { 900.0 } else { 100.0 }
}

fn sideways_list(app: &mut App) -> Entity {
    app.insert_resource(Count(10_000));
    app.insert_resource(Across(300.0));
    app.insert_resource(Sideways(true));
    let row = |slot: RowSlot| {
        let gutter = AnyView::new(|cx| {
            let gutter =
                cx.build(frame().width(px(40.0)).height(px(20.0)));
            cx.world.entity_mut(gutter).insert(Gutter);
            gutter
        });
        let index = slot.index();
        let text = frame()
            .width(
                derived(move |world| px(wide(index.get(world))))
                    .when(every_frame()),
            )
            .height(px(20.0));
        let text = AnyView::new(move |cx| {
            let node = cx.build(text);
            cx.effect(
                node,
                slot.index().into(),
                |world, node, &index| {
                    world.entity_mut(node).insert(Shows(index));
                },
            );
            node
        });
        hrow((gutter, slot.sideways(text))).boxed()
    };
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row,
        )
        .sideways(resource::<Sideways, _>(|on| on.0)),))
        .width(resource::<Across, _>(|across| px(across.0)))
        .height(px(VIEW)),
    );
    settle(app);
    the_list(app)
}

#[test]
fn a_sideways_list_leaves_a_parent_sized_by_its_content_its_width() {
    let mut app = laid_out();
    app.insert_resource(Count(100));
    let row = |slot: RowSlot| {
        let wide = frame().width(px(3000.0)).height(px(20.0));
        hrow((slot.sideways(wide.boxed()),)).boxed()
    };
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row,
        )
        .sideways(true),))
        .width(percent(100.0))
        .height(px(VIEW)),
    );
    settle(&mut app);
    let area = the_list(&mut app);

    let width =
        app.world().get::<ComputedNode>(area).unwrap().size().x;
    assert_eq!(width, 1000.0, "the window's width");
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().x =
        5000.0;
    settle(&mut app);
    assert_eq!(
        app.world().get::<ScrollPosition>(area).unwrap().x,
        2000.0,
        "as far as the rows overflow"
    );
}

fn left(app: &App, node: Entity) -> f32 {
    let computed = app.world().get::<ComputedNode>(node).unwrap();
    let at = app.world().get::<UiGlobalTransform>(node).unwrap();
    (at.translation.x - computed.size().x / 2.0)
        * computed.inverse_scale_factor()
}

fn right(app: &App, node: Entity) -> f32 {
    left(app, node) + {
        let computed = app.world().get::<ComputedNode>(node).unwrap();
        computed.size().x * computed.inverse_scale_factor()
    }
}

fn shows(app: &mut App, index: usize) -> Entity {
    app.world_mut()
        .query::<(Entity, &Shows)>()
        .iter(app.world())
        .find(|(_, shows)| shows.0 == index)
        .map(|(node, _)| node)
        .expect("a built row")
}

fn scroll_x(app: &mut App, area: Entity, by: f32) {
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().x += by;
    settle(app);
}

#[test]
fn sideways_rows_share_one_offset_and_the_widest_sets_its_end() {
    let mut app = laid_out();
    let area = sideways_list(&mut app);
    let gutters = |app: &mut App| {
        app.world_mut()
            .query_filtered::<Entity, With<Gutter>>()
            .iter(app.world())
            .map(|gutter| left(app, gutter))
            .collect::<Vec<_>>()
    };
    let before = gutters(&mut app);
    let (narrow, long) = (shows(&mut app, 3), shows(&mut app, 7));
    let (narrow_at, long_at) = (left(&app, narrow), left(&app, long));
    let cell = app.world().get::<ChildOf>(long).unwrap().parent();
    let cell = app.world().get::<ChildOf>(cell).unwrap().parent();

    scroll_x(&mut app, area, 50.0);
    assert_eq!(left(&app, narrow), narrow_at - 50.0);
    assert_eq!(left(&app, long), long_at - 50.0);
    assert_eq!(gutters(&mut app), before, "the gutters stay put");

    scroll_x(&mut app, area, 10_000.0);
    assert_eq!(right(&app, long), right(&app, cell), "the end shows");
    let by = long_at - left(&app, long);
    assert_eq!(left(&app, narrow), narrow_at - by);

    scroll_to(&mut app, area, 5_000);
    settle(&mut app);
    let x = app.world().get::<ScrollPosition>(area).unwrap().x;
    assert_eq!(x, by, "a wide row out of sight keeps the range");
}

#[test]
fn a_list_scrolls_sideways_only_when_asked() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, uniform);
    let node = app.world().get::<Node>(area).unwrap();

    assert_eq!(node.overflow.x, OverflowAxis::Clip);
}

#[test]
fn a_resize_forgets_the_range_of_rows_out_of_sight() {
    let mut app = laid_out();
    let area = sideways_list(&mut app);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    app.insert_resource(Across(600.0));
    settle(&mut app);
    scroll_x(&mut app, area, 10_000.0);
    let x = app.world().get::<ScrollPosition>(area).unwrap().x;
    assert_eq!(x, 0.0, "no row in sight overflows");

    scroll_to(&mut app, area, 0);
    settle(&mut app);
    scroll_x(&mut app, area, 10_000.0);
    let long = shows(&mut app, 7);
    let cell = app.world().get::<ChildOf>(long).unwrap().parent();
    let cell = app.world().get::<ChildOf>(cell).unwrap().parent();
    assert_eq!(right(&app, long), right(&app, cell), "the end shows");
}

#[test]
fn turning_sideways_off_and_on_keeps_the_rows() {
    let mut app = laid_out();
    let area = sideways_list(&mut app);
    scroll_to(&mut app, area, 3);
    settle(&mut app);
    scroll_x(&mut app, area, 10_000.0);
    let long = shows(&mut app, 7);
    let cell = app.world().get::<ChildOf>(long).unwrap().parent();
    let cell = app.world().get::<ChildOf>(cell).unwrap().parent();
    let spans = (0..20)
        .map(|index| state(&app, area).row_span(index))
        .collect::<Vec<_>>();

    app.insert_resource(Sideways(false));
    settle(&mut app);
    assert_eq!(state(&app, area).first_visible(), 3);
    assert_eq!(
        (0..20)
            .map(|index| state(&app, area).row_span(index))
            .collect::<Vec<_>>(),
        spans
    );
    assert_eq!(
        app.world().get::<ScrollPosition>(area).unwrap().x,
        0.0
    );
    let moved = app.world().get::<ChildOf>(long).unwrap().parent();
    assert_eq!(
        left(&app, moved),
        left(&app, cell),
        "back at the start"
    );
    assert_eq!(
        right(&app, moved),
        right(&app, cell),
        "the cell's width"
    );
    scroll_x(&mut app, area, 10_000.0);
    assert_eq!(
        app.world().get::<ScrollPosition>(area).unwrap().x,
        0.0
    );

    app.insert_resource(Sideways(true));
    settle(&mut app);
    scroll_x(&mut app, area, 10_000.0);
    assert_eq!(right(&app, long), right(&app, cell), "the end shows");
    assert_eq!(state(&app, area).first_visible(), 3);
}

fn wraps(world: &World, index: usize) -> f32 {
    let sideways = world.resource::<Sideways>().0;
    if index.is_multiple_of(50) && !sideways {
        200.0
    } else {
        20.0
    }
}

fn wrapping_list(app: &mut App, sideways: bool) -> Entity {
    app.insert_resource(Count(10_000));
    app.insert_resource(Sideways(sideways));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row(wraps),
        )
        .sideways(resource::<Sideways, _>(|on| on.0)),))
        .width(px(300.0))
        .height(px(VIEW)),
    );
    let area = the_list(app);
    settle(app);
    scroll_to(app, area, 5_003);
    settle(app);
    area
}

#[test]
fn turning_sideways_on_forgets_the_heights_rows_had_wrapped() {
    let mut built = laid_out();
    let area = wrapping_list(&mut built, true);
    let spans = (0..10_000)
        .map(|index| state(&built, area).row_span(index))
        .collect::<Vec<_>>();

    let mut app = laid_out();
    let area = wrapping_list(&mut app, false);
    let first = state(&app, area).first_visible();
    let top = shown_at(&mut app, area, first);
    app.insert_resource(Sideways(true));
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), first);
    assert_eq!(shown_at(&mut app, area, first), top);
    assert_eq!(
        state(&app, area).row_span(9_999).map(|span| span.top),
        Some(9_999.0 * 20.0)
    );
    assert!(
        (0..10_000)
            .map(|index| state(&app, area).row_span(index))
            .eq(spans),
        "as if built scrolling sideways"
    );
}

#[test]
fn turning_sideways_off_forgets_the_heights_it_measured() {
    let mut fresh = laid_out();
    let built = wrapping_list(&mut fresh, false);
    let mut app = laid_out();
    let area = wrapping_list(&mut app, true);
    let first = state(&app, area).first_visible();
    let top = shown_at(&mut app, area, first);

    app.insert_resource(Sideways(false));
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), first);
    assert_eq!(shown_at(&mut app, area, first), top);
    let spawned = state(&app, area).spawned().unwrap();
    for index in spawned.clone() {
        assert_eq!(
            state(&app, area).row_span(index).unwrap().height,
            state(&fresh, built).row_span(index).unwrap().height,
            "row {index}"
        );
    }
    let stale = (0..10_000)
        .filter(|index| !spawned.contains(index))
        .find(|&index| {
            state(&app, area).row_span(index).unwrap().height != 20.0
        });
    assert_eq!(stale, None);
}

fn notch(app: &mut App, area: Entity, lines: f32) {
    let location = Location {
        target: NormalizedRenderTarget::None {
            width: 1000,
            height: 1000,
        },
        position: Vec2::ZERO,
    };
    app.world_mut().trigger(PointerScroll {
        entity: area,
        pointer: Pointer::new(PointerId::Mouse, location),
        unit: MouseScrollUnit::Line,
        x: 0.0,
        y: lines,
        hit: HitData::new(Entity::PLACEHOLDER, 0.0, None, None),
        phase: TouchPhase::Moved,
    });
}

fn arrive(app: &mut App) {
    app.world_mut().resource_mut::<Count>().0 += 1;
    app.update();
}

#[test]
fn a_wheel_flung_to_the_end_follows_the_rows_arriving() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y -=
        100.0;
    settle(&mut app);
    assert!(!state(&app, area).following(), "scrolled up");

    notch(&mut app, area, -3.0);
    let mut frames = 0;
    while !state(&app, area).following() {
        frames += 1;
        assert!(
            frames <= 60,
            "following again within 60 frames, {} short of the end",
            crate::scroll::overflow(
                app.world().get::<ComputedNode>(area).unwrap()
            )
            .y - scroll(&app, area)
        );
        arrive(&mut app);
    }
    for frame in 1..=30 {
        arrive(&mut app);
        assert!(state(&app, area).following(), "frame {frame} after");
    }
    let last = app.world().resource::<Count>().0 - 1;
    assert_eq!(shown_at(&mut app, area, last), Some(VIEW - 20.0));
}

#[derive(Component)]
struct FitsRows;

fn fit_rows(
    mut fits: Query<(&mut Node, &Children), With<FitsRows>>,
    lists: Query<&VirtualList>,
) {
    for (mut node, children) in &mut fits {
        let Some(list) =
            children.iter().find_map(|&child| lists.get(child).ok())
        else {
            continue;
        };
        let rows = list
            .count()
            .checked_sub(1)
            .and_then(|last| list.row_span(last))
            .map_or(0.0, |last| last.top + last.height);
        node.height = px(rows.ceil());
    }
}

fn fitted(app: &mut App) {
    app.add_systems(PostUpdate, fit_rows.before(UiSystems::Layout));
}

fn fitted_list() -> AnyView<Bevy, Plain> {
    column((virtual_list(
        resource::<Count, _>(|count| count.0),
        15.6,
        row(|_, _| 15.6),
    )
    .overscan(0.0)
    .follow_bottom(true),))
    .width(px(300.0))
    .with(FitsRows)
    .boxed()
}

fn lists(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .iter(app.world())
        .collect()
}

fn built_under(app: &mut App, area: Entity) -> Vec<usize> {
    let world = app.world_mut();
    let mut built = world
        .query::<(Entity, &Shows)>()
        .iter(world)
        .filter(|&(node, _)| {
            core::iter::successors(Some(node), |&node| {
                world.get::<ChildOf>(node).map(ChildOf::parent)
            })
            .any(|above| above == area)
        })
        .map(|(_, shows)| shows.0)
        .collect::<Vec<_>>();
    built.sort_unstable();
    built
}

fn assert_builds_every_row(app: &mut App, area: Entity, at: &str) {
    let count = app.world().resource::<Count>().0;
    assert_eq!(
        built_under(app, area),
        (0..count).collect::<Vec<_>>(),
        "{at}, {count} rows in {} px",
        height(app, area)
    );
}

#[test]
fn a_following_list_its_parent_fits_draws_every_row_as_it_grows() {
    for scale_factor in [1.0, 1.25] {
        let mut app = laid_out_at(scale_factor);
        fitted(&mut app);
        app.insert_resource(Count(0));
        mount::<Plain>(app.world_mut(), fitted_list());
        let area = lists(&mut app)[0];
        settle(&mut app);
        for added in [17, 2, 1, 1, 3, 1, 2, 1] {
            app.world_mut().resource_mut::<Count>().0 += added;
            app.update();
            assert_builds_every_row(
                &mut app,
                area,
                &format!("{scale_factor}x"),
            );
        }
    }
}

#[derive(Resource, Clone, Copy, PartialEq)]
struct Pass(u32);

#[test]
fn a_list_its_parent_fits_draws_every_row_on_its_first_two_frames() {
    for scale_factor in [1.0, 1.25] {
        let mut app = laid_out_at(scale_factor);
        fitted(&mut app);
        app.insert_resource(Count(17));
        app.insert_resource(Pass(0));
        mount::<Plain>(
            app.world_mut(),
            keyed(resource::<Pass, _>(|pass| *pass), |_| {
                fitted_list()
            })
            .within(column(())),
        );
        let mut before = Vec::new();
        for pass in 0..2 {
            if pass > 0 {
                before = lists(&mut app);
                app.world_mut().resource_mut::<Count>().0 = 17;
                app.world_mut().resource_mut::<Pass>().0 = pass;
            }
            app.update();
            let area = lists(&mut app)
                .into_iter()
                .find(|list| !before.contains(list))
                .expect("a list built this frame");
            let at = format!("{scale_factor}x, build {pass}");
            assert_builds_every_row(&mut app, area, &at);
            app.world_mut().resource_mut::<Count>().0 = 19;
            app.update();
            assert_builds_every_row(&mut app, area, &at);
        }
    }
}

enum Start {
    Top,
    Middle,
    Bottom,
}

struct Landing {
    name: &'static str,
    height: Height,
    estimate: f32,
    scale: f32,
    follow: bool,
    start: Start,
    index: usize,
}

fn landed(
    app: &mut App,
    area: Entity,
    index: usize,
    when: &str,
    failed: &mut Vec<String>,
) -> bool {
    let list = state(app, area);
    let last = list.count() - 1;
    let target = list.row_span(index.min(last)).unwrap();
    let end = list.row_span(last).unwrap();
    let bottom = (end.top + end.height - VIEW).max(0.0);
    let at_bottom = target.top >= bottom - 0.5;
    let (shown, want) = if at_bottom {
        (last, (VIEW - end.height).min(end.top))
    } else {
        (index.min(last), 0.0)
    };
    let got = shown_at(app, area, shown);
    if !near(got, want) {
        failed.push(format!(
            "{when}: row {shown} at {got:?}, not {want}"
        ));
    }
    at_bottom
}

fn grown_by_100(
    app: &mut App,
    area: Entity,
    height: Height,
    failed: &mut Vec<String>,
) {
    let following = state(app, area).following();
    let first = state(app, area).first_visible();
    let top = shown_at(app, area, first);
    let count = state(app, area).count() + 100;
    app.world_mut().resource_mut::<Count>().0 = count;
    updates(app, 6);
    if following {
        let want = VIEW - height(app.world(), count - 1);
        let got = shown_at(app, area, count - 1);
        if !near(got, want) || !state(app, area).following() {
            failed.push(format!(
                "grown: row {} at {got:?}, not {want}, following {}",
                count - 1,
                state(app, area).following()
            ));
        }
    } else {
        let now = state(app, area).first_visible();
        let got = shown_at(app, area, first);
        if now != first || top.is_none_or(|top| !near(got, top)) {
            failed.push(format!(
                "grown: first visible {now}, row {first} at \
                 {got:?}, was at {top:?}"
            ));
        }
    }
}

fn landing_failures(case: &Landing) -> Vec<String> {
    let mut app = laid_out_at(case.scale);
    app.insert_resource(Follow(case.follow));
    let area = mounted(
        &mut app,
        10_000,
        case.estimate,
        row(case.height),
        px(VIEW),
    );
    settle(&mut app);
    match case.start {
        Start::Top => {}
        Start::Middle => {
            scroll_to(&mut app, area, 3_000);
            updates(&mut app, 4);
        }
        Start::Bottom => to_the_end(&mut app, area),
    }
    let mut failed = Vec::new();
    scroll_to(&mut app, area, case.index);
    updates(&mut app, 2);
    landed(
        &mut app,
        area,
        case.index,
        "by the second frame",
        &mut failed,
    );
    updates(&mut app, 4);
    let at_bottom =
        landed(&mut app, area, case.index, "settled", &mut failed);
    let following = state(&app, area).following();
    if following != (case.follow && at_bottom) {
        failed.push(format!("following() is {following}"));
    }
    grown_by_100(&mut app, area, case.height, &mut failed);
    failed
        .into_iter()
        .map(|f| format!("{}: {f}", case.name))
        .collect()
}

fn assert_lands(cases: &[Landing]) {
    assert_none(
        cases.iter().flat_map(landing_failures).collect(),
        cases.len(),
    );
}

#[test]
fn scroll_to_lands_on_uniform_rows() {
    assert_lands(&[
        Landing {
            name: "19 px under 17, from the top to row 9973, the \
                   last that can show first",
            height: h19,
            estimate: 17.0,
            scale: 1.0,
            follow: false,
            start: Start::Top,
            index: 9_973,
        },
        Landing {
            name: "19 px under 17 at 1.25x, following from the top \
                   to row 9975, past the bottom",
            height: h19,
            estimate: 17.0,
            scale: 1.25,
            follow: true,
            start: Start::Top,
            index: 9_975,
        },
        Landing {
            name: "19 px under 19, following from the bottom to row \
                   9940",
            height: h19,
            estimate: 19.0,
            scale: 1.0,
            follow: true,
            start: Start::Bottom,
            index: 9_940,
        },
        Landing {
            name: "19 px under 20 at 1.75x, from the bottom to past \
                   the last row",
            height: h19,
            estimate: 20.0,
            scale: 1.75,
            follow: false,
            start: Start::Bottom,
            index: 10_050,
        },
        Landing {
            name: "20 px under 20, following from the top to row \
                   9975, the first of the bottom view",
            height: uniform,
            estimate: 20.0,
            scale: 1.0,
            follow: true,
            start: Start::Top,
            index: 9_975,
        },
        Landing {
            name: "20 px under 5 at 1.25x, following from the \
                   bottom to row 5000",
            height: uniform,
            estimate: 5.0,
            scale: 1.25,
            follow: true,
            start: Start::Bottom,
            index: 5_000,
        },
        Landing {
            name: "20 px under 170, from the top to the last row",
            height: uniform,
            estimate: 170.0,
            scale: 1.0,
            follow: false,
            start: Start::Top,
            index: 9_999,
        },
        Landing {
            name: "20 px under 170 at 1.25x, from the bottom to row \
                   0",
            height: uniform,
            estimate: 170.0,
            scale: 1.25,
            follow: false,
            start: Start::Bottom,
            index: 0,
        },
    ]);
}

#[test]
fn scroll_to_lands_among_rows_of_other_heights() {
    assert_lands(&[
        Landing {
            name: "60 px every 100th under 20 at 1.75x, following \
                   from the top to the last row",
            height: tall_hundredths,
            estimate: 20.0,
            scale: 1.75,
            follow: true,
            start: Start::Top,
            index: 9_999,
        },
        Landing {
            name: "60 px every 100th under 17 at 1.5x, from the \
                   middle to row 9973",
            height: tall_hundredths,
            estimate: 17.0,
            scale: 1.5,
            follow: false,
            start: Start::Middle,
            index: 9_973,
        },
        Landing {
            name: "regions under 20, from the middle to row 5000",
            height: regions,
            estimate: 20.0,
            scale: 1.0,
            follow: false,
            start: Start::Middle,
            index: 5_000,
        },
        Landing {
            name: "regions under 20 at 1.25x, following from the \
                   middle to the last row",
            height: regions,
            estimate: 20.0,
            scale: 1.25,
            follow: true,
            start: Start::Middle,
            index: 9_999,
        },
        Landing {
            name: "regions under 20, following from the bottom to \
                   row 0",
            height: regions,
            estimate: 20.0,
            scale: 1.0,
            follow: true,
            start: Start::Bottom,
            index: 0,
        },
        Landing {
            name: "12 to 40 px under 17, following from the bottom \
                   to row 9975",
            height: mixed,
            estimate: 17.0,
            scale: 1.0,
            follow: true,
            start: Start::Bottom,
            index: 9_975,
        },
        Landing {
            name: "12 to 40 px under 26 at 1.25x, from the top to \
                   past the last row",
            height: mixed,
            estimate: 26.0,
            scale: 1.25,
            follow: false,
            start: Start::Top,
            index: 10_050,
        },
        Landing {
            name: "12 to 40 px under 26, following from the middle \
                   to row 9940",
            height: mixed,
            estimate: 26.0,
            scale: 1.0,
            follow: true,
            start: Start::Middle,
            index: 9_940,
        },
    ]);
}

#[test]
fn scroll_to_lands_on_a_tail_of_another_height() {
    assert_lands(&[
        Landing {
            name: "a 5 px tail under 40 at 1.25x, from the top to \
                   the last row",
            height: short_tail,
            estimate: 40.0,
            scale: 1.25,
            follow: false,
            start: Start::Top,
            index: 9_999,
        },
        Landing {
            name: "a 5 px tail under 40 at 1.5x, following from the \
                   top to past the last row",
            height: short_tail,
            estimate: 40.0,
            scale: 1.5,
            follow: true,
            start: Start::Top,
            index: 10_050,
        },
        Landing {
            name: "a 5 px tail under 20, following from the bottom \
                   to row 9900, where it starts",
            height: short_tail,
            estimate: 20.0,
            scale: 1.0,
            follow: true,
            start: Start::Bottom,
            index: 9_900,
        },
        Landing {
            name: "a 5 px tail under 20 at 1.25x, from the bottom \
                   to row 9973",
            height: short_tail,
            estimate: 20.0,
            scale: 1.25,
            follow: false,
            start: Start::Bottom,
            index: 9_973,
        },
        Landing {
            name: "a 60 px tail under 17, following from the top to \
                   the last row",
            height: tall_tail,
            estimate: 17.0,
            scale: 1.0,
            follow: true,
            start: Start::Top,
            index: 9_999,
        },
        Landing {
            name: "a 60 px tail under 17 at 1.25x, following from \
                   the bottom to row 9973",
            height: tall_tail,
            estimate: 17.0,
            scale: 1.25,
            follow: true,
            start: Start::Bottom,
            index: 9_973,
        },
        Landing {
            name: "a 60 px tail under 17 at 1.25x, from the top to \
                   past the last row",
            height: tall_tail,
            estimate: 17.0,
            scale: 1.25,
            follow: false,
            start: Start::Top,
            index: 10_050,
        },
    ]);
}

enum Call {
    Before,
    With,
    After,
}

struct CountChange {
    name: &'static str,
    height: Height,
    estimate: f32,
    follow: bool,
    from: usize,
    call: Call,
    index: usize,
}

const COUNT_CHANGES: [CountChange; 9] = [
    CountChange {
        name: "19 px under 17, 1000 rows, row 5000 called the \
               update before",
        height: h19,
        estimate: 17.0,
        follow: false,
        from: 1_000,
        call: Call::Before,
        index: 5_000,
    },
    CountChange {
        name: "19 px under 17 following, 9990 rows, row 9973 called \
               the update before",
        height: h19,
        estimate: 17.0,
        follow: true,
        from: 9_990,
        call: Call::Before,
        index: 9_973,
    },
    CountChange {
        name: "19 px under 17 following, no rows, row 9973 called \
               the update before",
        height: h19,
        estimate: 17.0,
        follow: true,
        from: 0,
        call: Call::Before,
        index: 9_973,
    },
    CountChange {
        name: "19 px under 17 following, 1000 rows, the last row \
               called in the update",
        height: h19,
        estimate: 17.0,
        follow: true,
        from: 1_000,
        call: Call::With,
        index: 9_999,
    },
    CountChange {
        name: "20 px under 20, no rows, row 5000 called the update \
               after",
        height: uniform,
        estimate: 20.0,
        follow: false,
        from: 0,
        call: Call::After,
        index: 5_000,
    },
    CountChange {
        name: "20 px under 20 following, 9990 rows, row 5000 called \
               in the update",
        height: uniform,
        estimate: 20.0,
        follow: true,
        from: 9_990,
        call: Call::With,
        index: 5_000,
    },
    CountChange {
        name: "20 px under 20, 9990 rows, the last row called the \
               update before",
        height: uniform,
        estimate: 20.0,
        follow: false,
        from: 9_990,
        call: Call::Before,
        index: 9_999,
    },
    CountChange {
        name: "60 px every 100th under 20 following, 1000 rows, the \
               last row called the update after",
        height: tall_hundredths,
        estimate: 20.0,
        follow: true,
        from: 1_000,
        call: Call::After,
        index: 9_999,
    },
    CountChange {
        name: "60 px every 100th under 20, no rows, row 9973 called \
               in the update",
        height: tall_hundredths,
        estimate: 20.0,
        follow: false,
        from: 0,
        call: Call::With,
        index: 9_973,
    },
];

fn count_change_failures(case: &CountChange) -> Vec<String> {
    let mut app = laid_out();
    app.insert_resource(Follow(case.follow));
    let area = mounted(
        &mut app,
        case.from,
        case.estimate,
        row(case.height),
        px(VIEW),
    );
    settle(&mut app);
    if let Call::Before = case.call {
        scroll_to(&mut app, area, case.index);
        app.update();
    }
    app.world_mut().resource_mut::<Count>().0 = 10_000;
    if let Call::With = case.call {
        scroll_to(&mut app, area, case.index);
    }
    app.update();
    if let Call::After = case.call {
        scroll_to(&mut app, area, case.index);
    }
    let mut failed = Vec::new();
    if !matches!(case.call, Call::Before) {
        app.update();
        landed(
            &mut app,
            area,
            case.index,
            "by the second frame",
            &mut failed,
        );
    }
    updates(&mut app, 6);
    let at_bottom =
        landed(&mut app, area, case.index, "settled", &mut failed);
    let following = state(&app, area).following();
    if following != (case.follow && at_bottom) {
        failed.push(format!("following() is {following}"));
    }
    failed
        .into_iter()
        .map(|f| format!("{}: {f}", case.name))
        .collect()
}

#[test]
fn scroll_to_lands_against_the_count_of_its_frames() {
    assert_none(
        COUNT_CHANGES
            .iter()
            .flat_map(count_change_failures)
            .collect(),
        COUNT_CHANGES.len(),
    );
}

struct Added {
    name: &'static str,
    height: Height,
    estimate: f32,
    follow: bool,
    from_bottom: bool,
    gap: usize,
    rows: usize,
    frames: usize,
}

const ADDED: [Added; 6] = [
    Added {
        name: "20 px, 100 rows on the call's second frame",
        height: uniform,
        estimate: 20.0,
        follow: true,
        from_bottom: false,
        gap: 1,
        rows: 100,
        frames: 1,
    },
    Added {
        name: "20 px from the bottom, 100 rows on the call's third \
               frame",
        height: uniform,
        estimate: 20.0,
        follow: true,
        from_bottom: true,
        gap: 2,
        rows: 100,
        frames: 1,
    },
    Added {
        name: "19 px under 17 from the bottom, 100 rows on the \
               call's second frame",
        height: h19,
        estimate: 17.0,
        follow: true,
        from_bottom: true,
        gap: 1,
        rows: 100,
        frames: 1,
    },
    Added {
        name: "19 px under 17, 100 rows on the call's third frame",
        height: h19,
        estimate: 17.0,
        follow: true,
        from_bottom: false,
        gap: 2,
        rows: 100,
        frames: 1,
    },
    Added {
        name: "19 px under 17, 2 rows a frame from the call's third",
        height: h19,
        estimate: 17.0,
        follow: true,
        from_bottom: false,
        gap: 2,
        rows: 2,
        frames: 30,
    },
    Added {
        name: "20 px not following, 2 rows a frame from the call's \
               second",
        height: uniform,
        estimate: 20.0,
        follow: false,
        from_bottom: false,
        gap: 1,
        rows: 2,
        frames: 30,
    },
];

fn added_failures(case: &Added) -> Vec<String> {
    let mut app = laid_out();
    app.insert_resource(Follow(case.follow));
    let area = mounted(
        &mut app,
        10_000,
        case.estimate,
        row(case.height),
        px(VIEW),
    );
    settle(&mut app);
    if case.from_bottom {
        to_the_end(&mut app, area);
    }
    scroll_to(&mut app, area, 9_999);
    for frame in 0..case.gap + case.frames + 6 {
        if (case.gap..case.gap + case.frames).contains(&frame) {
            app.world_mut().resource_mut::<Count>().0 += case.rows;
        }
        app.update();
    }
    let last = state(&app, area).count() - 1;
    let (shown, want, follows) = if case.gap <= 1 {
        (9_999, 0.0, false)
    } else if case.follow {
        (last, VIEW - (case.height)(app.world(), last), true)
    } else {
        (9_999, VIEW - (case.height)(app.world(), 9_999), false)
    };
    let got = shown_at(&mut app, area, shown);
    let following = state(&app, area).following();
    if !near(got, want) || following != follows {
        vec![format!(
            "{}: row {shown} at {got:?}, not {want}, following \
             {following}",
            case.name
        )]
    } else {
        Vec::new()
    }
}

#[test]
fn rows_added_after_a_scroll_to_land_or_follow() {
    assert_none(
        ADDED.iter().flat_map(added_failures).collect(),
        ADDED.len(),
    );
}

struct Shrink {
    name: &'static str,
    to: usize,
    grow: bool,
    settled: bool,
}

const SHRINKS: [Shrink; 6] = [
    Shrink {
        name: "to 5010 rows, under the view, once landed",
        to: 5_010,
        grow: false,
        settled: true,
    },
    Shrink {
        name: "to 5010 rows while landing, then growing",
        to: 5_010,
        grow: true,
        settled: false,
    },
    Shrink {
        name: "to 5025 rows, the view's end, while landing, then \
               growing",
        to: 5_025,
        grow: true,
        settled: false,
    },
    Shrink {
        name: "to 5030 rows, above the view's end, once landed, \
               then growing",
        to: 5_030,
        grow: true,
        settled: true,
    },
    Shrink {
        name: "to 4000 rows, above the first row, while landing",
        to: 4_000,
        grow: false,
        settled: false,
    },
    Shrink {
        name: "to 10 rows, all in view, once landed, then growing",
        to: 10,
        grow: true,
        settled: true,
    },
];

fn shrink_failures(case: &Shrink) -> Vec<String> {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 10_000, uniform);
    scroll_to(&mut app, area, 5_000);
    updates(&mut app, if case.settled { 6 } else { 1 });
    app.world_mut().resource_mut::<Count>().0 = case.to;
    for frame in 0..10 {
        if case.grow && frame >= 2 {
            app.world_mut().resource_mut::<Count>().0 += 3;
        }
        app.update();
    }
    let count = state(&app, area).count();
    let at_bottom = case.to as f32 * 20.0 - VIEW <= 5_000.0 * 20.0;
    let following = state(&app, area).following();
    let (shown, want) = if at_bottom {
        (count - 1, (VIEW - 20.0).min(count as f32 * 20.0 - 20.0))
    } else {
        (5_000, 0.0)
    };
    let got = shown_at(&mut app, area, shown);
    if following != at_bottom || !near(got, want) {
        vec![format!(
            "{}: row {shown} at {got:?}, not {want}, following \
             {following}",
            case.name
        )]
    } else {
        Vec::new()
    }
}

#[test]
fn a_following_list_that_shrinks_to_its_view_follows() {
    assert_none(
        SHRINKS.iter().flat_map(shrink_failures).collect(),
        SHRINKS.len(),
    );
}

#[test]
fn this_frames_count_is_placed_this_frame() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, uniform);

    app.world_mut().resource_mut::<Count>().0 = 10_100;
    app.update();

    assert_eq!(
        state(&app, area).row_span(10_099).map(|span| span.top),
        Some(10_099.0 * 20.0)
    );
}

#[test]
fn following_stops_on_a_move_up_and_resumes_at_the_bottom() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    assert!(state(&app, area).following(), "at the bottom");
    let bottom = scroll(&app, area);

    wheel(&mut app, area, -0.25);
    settle(&mut app);
    let up = scroll(&app, area);
    assert_eq!(up, bottom - 0.25, "a quarter pixel up");
    assert!(!state(&app, area).following(), "a quarter pixel up");
    app.world_mut().resource_mut::<Count>().0 = 1_100;
    settle(&mut app);
    assert_eq!(scroll(&app, area), up, "growth after leaving");

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        1.0e9;
    settle(&mut app);
    app.world_mut().resource_mut::<Count>().0 = 1_200;
    settle(&mut app);
    assert_eq!(
        shown_at(&mut app, area, 1_199),
        Some(VIEW - 20.0),
        "back at the bottom"
    );

    app.insert_resource(Follow(false));
    settle(&mut app);
    app.insert_resource(Follow(true));
    settle(&mut app);
    assert!(state(&app, area).following(), "on again at its bottom");
    app.world_mut().resource_mut::<Count>().0 = 1_300;
    settle(&mut app);
    assert_eq!(
        shown_at(&mut app, area, 1_299),
        Some(VIEW - 20.0),
        "on again at its bottom"
    );

    app.insert_resource(Follow(false));
    settle(&mut app);
    let at = scroll(&app, area);
    app.world_mut().resource_mut::<Count>().0 = 1_400;
    settle(&mut app);
    assert_eq!(scroll(&app, area), at, "follow switched off");
    assert!(!state(&app, area).following(), "follow switched off");

    wheel(&mut app, area, -100.0);
    app.insert_resource(Follow(false));
    settle(&mut app);
    app.insert_resource(Follow(true));
    settle(&mut app);
    let at = scroll(&app, area);
    app.world_mut().resource_mut::<Count>().0 = 1_500;
    settle(&mut app);
    assert_eq!(scroll(&app, area), at, "on again above its bottom");
}

#[test]
fn rows_arriving_with_a_move_to_the_end_are_followed() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 500);
    settle(&mut app);

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        1.0e9;
    app.world_mut().resource_mut::<Count>().0 = 1_002;
    app.update();
    for count in (1_004..=1_100).step_by(2) {
        app.world_mut().resource_mut::<Count>().0 = count;
        app.update();
    }
    settle(&mut app);

    assert!(state(&app, area).following());
    assert_eq!(shown_at(&mut app, area, 1_099), Some(VIEW - 20.0));
}

#[test]
fn a_move_to_the_bottom_follows_again() {
    for (to, steps) in [(990, 5), (500, 0)] {
        let mut app = laid_out();
        app.insert_resource(Follow(true));
        let area = list(&mut app, 1_000, uniform);
        scroll_to(&mut app, area, to);
        settle(&mut app);
        for _ in 0..steps {
            wheel(&mut app, area, 40.0);
        }
        if steps == 0 {
            scroll_to(&mut app, area, 975);
        }
        settle(&mut app);
        let when = if steps == 0 {
            "scroll_to(975) from row 500"
        } else {
            "a wheel down at the bottom"
        };
        assert_eq!(
            scroll(&app, area),
            1_000.0 * 20.0 - VIEW,
            "{when}"
        );
        assert!(state(&app, area).following(), "{when}");

        app.world_mut().resource_mut::<Count>().0 = 1_100;
        settle(&mut app);
        assert_eq!(
            shown_at(&mut app, area, 1_099),
            Some(VIEW - 20.0),
            "{when}"
        );
    }
}

#[test]
fn a_following_list_that_fits_or_is_empty_follows_a_stream() {
    for (from, index) in [(10, Some(5)), (0, Some(0)), (0, None)] {
        let mut app = laid_out();
        app.insert_resource(Follow(true));
        let area = list(&mut app, from, uniform);
        if let Some(index) = index {
            scroll_to(&mut app, area, index);
            settle(&mut app);
        }
        assert!(state(&app, area).following(), "{from} rows");
        for count in (from + 2..=100).step_by(2) {
            app.world_mut().resource_mut::<Count>().0 = count;
            app.update();
        }
        settle(&mut app);
        assert_eq!(
            shown_at(&mut app, area, 99),
            Some(VIEW - 20.0),
            "{from} rows, scroll_to {index:?}"
        );
    }
}

struct Reveal {
    name: &'static str,
    estimate: f32,
    height: f32,
    from: usize,
}

const REVEALS: [Reveal; 6] = [
    Reveal {
        name: "a 60 px row from far below, under 20",
        estimate: 20.0,
        height: 60.0,
        from: 9_000,
    },
    Reveal {
        name: "a 60 px row from far above, under 200",
        estimate: 200.0,
        height: 60.0,
        from: 1_000,
    },
    Reveal {
        name: "a 60 px row from just above, under 2",
        estimate: 2.0,
        height: 60.0,
        from: 4_900,
    },
    Reveal {
        name: "a 2000 px row from far below, under 2",
        estimate: 2.0,
        height: 2_000.0,
        from: 9_000,
    },
    Reveal {
        name: "a 2000 px row from far above, under 20",
        estimate: 20.0,
        height: 2_000.0,
        from: 1_000,
    },
    Reveal {
        name: "a 2000 px row from just below, under 200",
        estimate: 200.0,
        height: 2_000.0,
        from: 5_100,
    },
];

fn reveal_failures(case: &Reveal) -> Vec<String> {
    let mut app = laid_out();
    app.insert_resource(Grown(5_000, case.height));
    let area = mounted(
        &mut app,
        10_000,
        case.estimate,
        row(grown),
        px(VIEW),
    );
    settle(&mut app);
    scroll_to(&mut app, area, case.from);
    settle(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .unwrap()
        .scroll_into_view(5_000);
    updates(&mut app, 2);
    let second = scroll(&app, area);
    settle(&mut app);
    let span = state(&app, area).row_span(5_000).unwrap();
    let want = if (case.from > 5_000) == (span.height > VIEW) {
        span.top + span.height - VIEW
    } else {
        span.top
    };
    let settled = scroll(&app, area);
    if (second - want).abs() > 0.01 || (settled - want).abs() > 0.01 {
        vec![format!(
            "{}: second frame {second}, settled {settled}, not \
             {want}",
            case.name
        )]
    } else {
        Vec::new()
    }
}

#[test]
fn reveals_of_unmeasured_rows_scroll_the_least_by_the_second_frame() {
    assert_none(
        REVEALS.iter().flat_map(reveal_failures).collect(),
        REVEALS.len(),
    );
}

fn range_by_the_second_frame(
    app: &mut App,
    area: Entity,
    range: Range<f32>,
    when: &str,
) -> f32 {
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .unwrap()
        .scroll_range_into_view(5_000, range);
    updates(app, 2);
    let second = scroll(app, area);
    settle(app);
    assert_eq!(second, scroll(app, area), "{when}, settled");
    second
}

#[test]
fn ranges_of_an_unmeasured_tall_row_scroll_the_least() {
    let top = 5_000.0 * 20.0;
    let mut app = laid_out();
    app.insert_resource(Grown(5_000, 2_000.0));
    let area = list(&mut app, 10_000, grown);
    let when = "from above, unmeasured";
    assert_eq!(
        range_by_the_second_frame(
            &mut app,
            area,
            1_500.0..1_520.0,
            when
        ),
        top + 1_520.0 - VIEW,
        "{when}"
    );

    for measured in [false, true] {
        let mut app = laid_out();
        app.insert_resource(Grown(5_000, 2_000.0));
        let area = list(&mut app, 10_000, grown);
        if measured {
            scroll_to(&mut app, area, 5_000);
            settle(&mut app);
        }
        scroll_to(&mut app, area, 9_000);
        settle(&mut app);
        let when = format!("from below, measured {measured}");
        assert_eq!(
            range_by_the_second_frame(
                &mut app,
                area,
                1_500.0..1_520.0,
                &when
            ),
            top + 1_500.0,
            "{when}"
        );
        let when = format!("past its end, measured {measured}");
        assert_eq!(
            range_by_the_second_frame(
                &mut app,
                area,
                1_990.0..2_500.0,
                &when
            ),
            top + 2_000.0 - VIEW,
            "{when}"
        );
        scroll_to(&mut app, area, 4_000);
        settle(&mut app);
        let when = format!("empty, from above, measured {measured}");
        assert_eq!(
            range_by_the_second_frame(
                &mut app,
                area,
                1_000.0..1_000.0,
                &when
            ),
            top + 1_000.0 - VIEW,
            "{when}"
        );
    }
}

struct Physical {
    index: usize,
    top: f32,
    height: f32,
}

fn physical(app: &mut App) -> Vec<Physical> {
    let built = app
        .world_mut()
        .query::<(&Shows, &ChildOf)>()
        .iter(app.world())
        .map(|(shows, slot)| (shows.0, slot.parent()))
        .collect::<Vec<_>>();
    let mut spans = built
        .into_iter()
        .map(|(index, slot)| {
            let node = app.world().get::<ComputedNode>(slot).unwrap();
            let at =
                app.world().get::<UiGlobalTransform>(slot).unwrap();
            let height = node.size().y;
            Physical {
                index,
                top: at.translation.y - height / 2.0,
                height,
            }
        })
        .collect::<Vec<_>>();
    spans.sort_by_key(|span| span.index);
    spans
}

fn off_pixels(spans: &[Physical]) -> Option<String> {
    let whole = |y: f32| (y - y.round()).abs() <= 0.01;
    if let Some(span) = spans
        .iter()
        .find(|span| !whole(span.top) || !whole(span.height))
    {
        return Some(format!(
            "row {} at {}, {} high",
            span.index, span.top, span.height
        ));
    }
    spans
        .windows(2)
        .find(|pair| {
            pair[1].index == pair[0].index + 1
                && (pair[1].top - (pair[0].top + pair[0].height))
                    .abs()
                    > 0.01
        })
        .map(|pair| format!("a seam under row {}", pair[0].index))
}

struct Pixels {
    name: &'static str,
    scale: f32,
    estimate: f32,
}

const PIXELS: [Pixels; 4] = [
    Pixels {
        name: "1.25x, under 16.3",
        scale: 1.25,
        estimate: 16.3,
    },
    Pixels {
        name: "1.25x, under 20",
        scale: 1.25,
        estimate: 20.0,
    },
    Pixels {
        name: "1.5x, under 16.3",
        scale: 1.5,
        estimate: 16.3,
    },
    Pixels {
        name: "1.75x, under 20",
        scale: 1.75,
        estimate: 20.0,
    },
];

fn pixel_failures(case: &Pixels) -> Vec<String> {
    let mut app = laid_out_at(case.scale);
    let area = mounted(
        &mut app,
        10_000,
        case.estimate,
        row(regions),
        px(VIEW),
    );
    let mut failed = Vec::new();
    let mut check = |app: &mut App, when: String| {
        if let Some(off) = off_pixels(&physical(app)) {
            failed.push(format!("{}, {when}: {off}", case.name));
        }
    };
    for frame in 1..=3 {
        app.update();
        check(&mut app, format!("first paint, frame {frame}"));
    }
    for (step, index) in
        [5_000, 9_999, 100, 7_100].into_iter().enumerate()
    {
        scroll_to(&mut app, area, index);
        for frame in 1..=3 {
            app.update();
            check(
                &mut app,
                format!("scroll_to({index}), frame {frame}"),
            );
        }
        for k in 0..4 {
            wheel(
                &mut app,
                area,
                0.3 + step as f32 * 0.07 + k as f32 * 0.11,
            );
            check(&mut app, format!("scroll_to({index}), +{k}"));
        }
    }
    failed
}

#[test]
fn rows_sit_on_whole_physical_pixels_at_every_scale() {
    assert_none(
        PIXELS.iter().flat_map(pixel_failures).collect(),
        PIXELS.len(),
    );
}

#[test]
fn following_turned_on_at_the_end_of_a_long_list_follows() {
    let mut app = laid_out_at(1.25);
    let area = list(&mut app, 100_000, |_, _| 19.2);
    scroll_to(&mut app, area, 99_999);
    settle(&mut app);

    app.insert_resource(Follow(true));
    settle(&mut app);
    assert!(state(&app, area).following());
    app.world_mut().resource_mut::<Count>().0 = 100_100;
    settle(&mut app);

    assert!(near(shown_at(&mut app, area, 100_099), VIEW - 19.2));
}

#[test]
fn following_turned_off_with_rows_arriving_keeps_the_view() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    let at = scroll(&app, area);

    app.insert_resource(Follow(false));
    app.world_mut().resource_mut::<Count>().0 = 1_100;
    settle(&mut app);

    assert_eq!(scroll(&app, area), at);
}

#[test]
fn a_scroll_to_past_the_last_row_of_a_stream_lands_when_it_stops() {
    let mut app = laid_out();
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, usize::MAX);
    for count in (1_002..=1_040).step_by(2) {
        app.world_mut().resource_mut::<Count>().0 = count;
        app.update();
        assert_eq!(
            shown_at(&mut app, area, count - 1),
            Some(VIEW - 20.0)
        );
    }
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 1_100;
    settle(&mut app);
    assert_eq!(shown_at(&mut app, area, 1_039), Some(VIEW - 20.0));
}

#[test]
fn a_reveal_in_view_as_rows_arrive_keeps_a_list_following() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 1_001;
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .unwrap()
        .scroll_into_view(995);
    settle(&mut app);
    let when = "scroll_into_view(995)";
    assert!(state(&app, area).following(), "{when}");
    assert_eq!(
        shown_at(&mut app, area, 1_000),
        Some(VIEW - 20.0),
        "{when}"
    );

    app.world_mut().resource_mut::<Count>().0 = 1_002;
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .unwrap()
        .scroll_range_into_view(1_000, 0.0..5.0);
    settle(&mut app);
    let when = "a range of row 1000";
    assert!(state(&app, area).following(), "{when}");
    assert_eq!(
        shown_at(&mut app, area, 1_001),
        Some(VIEW - 20.0),
        "{when}"
    );
}
