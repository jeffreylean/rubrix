//! The virtual list against real bevy_ui layout, headless: which rows
//! it builds, where they show, and what a change keeps still.

use core::ops::Range;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::app::{App, PostUpdate};
use bevy::camera::{
    Camera, NormalizedRenderTarget, RenderTargetInfo,
};
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
use bevy::math::{UVec2, Vec2};
use bevy::picking::backend::HitData;
use bevy::picking::events::{Pointer, PointerScroll};
use bevy::picking::pointer::{Location, PointerId};
use bevy::ui::{
    ComputedNode, Display, Node, OverflowAxis, ScrollPosition,
    UiGlobalTransform, UiSystems, Val, percent, px,
};

use super::layout::laid_out;
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
pub(super) const VIEW: f32 = 500.0;
const OVERSCAN: f32 = 1000.0;

#[derive(Resource)]
pub(super) struct Count(pub(super) usize);

#[derive(Resource)]
struct Tall(f32);

#[derive(Resource)]
struct Across(f32);

#[derive(Resource)]
pub(super) struct Grown(pub(super) usize, pub(super) f32);

#[derive(Resource)]
pub(super) struct Follow(pub(super) bool);

#[derive(Resource)]
pub(super) struct Sideways(pub(super) bool);

#[derive(Component)]
pub(super) struct Shows(pub(super) usize);

pub(super) type Height = fn(&World, usize) -> f32;

pub(super) fn uniform(_: &World, _: usize) -> f32 {
    20.0
}

pub(super) fn tall_hundredths(_: &World, index: usize) -> f32 {
    if index.is_multiple_of(100) {
        60.0
    } else {
        20.0
    }
}

fn flat(_: &World, _: usize) -> f32 {
    0.0
}

pub(super) fn grown(world: &World, index: usize) -> f32 {
    match world.get_resource::<Grown>() {
        Some(&Grown(at, height)) if at == index => height,
        _ => 20.0,
    }
}

pub(super) fn row(
    height: Height,
) -> impl Fn(RowSlot) -> AnyView<Bevy, Plain> {
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

pub(super) fn list(
    app: &mut App,
    count: usize,
    height: Height,
) -> Entity {
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

pub(super) fn mounted(
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
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
}

pub(super) fn settle(app: &mut App) {
    for _ in 0..6 {
        app.update();
    }
}

pub(super) fn state(app: &App, area: Entity) -> &VirtualList {
    app.world().get::<VirtualList>(area).expect("a list")
}

pub(super) fn scroll_to(app: &mut App, area: Entity, index: usize) {
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_to(index);
}

pub(super) fn scroll(app: &App, area: Entity) -> f32 {
    app.world().get::<ScrollPosition>(area).expect("a scroll").y
}

fn top(app: &App, node: Entity) -> f32 {
    let computed = app.world().get::<ComputedNode>(node).unwrap();
    let at = app.world().get::<UiGlobalTransform>(node).unwrap();
    (at.translation.y - computed.size().y / 2.0)
        * computed.inverse_scale_factor()
}

pub(super) fn height(app: &App, node: Entity) -> f32 {
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

fn assert_apart(app: &mut App, area: Entity, when: &str) {
    let spans = drawn(app, area);
    for pair in spans.windows(2) {
        assert!(
            pair[1].0 >= pair[0].1 - 0.01,
            "{when}: a row drawn at {:?} and one at {:?}",
            pair[0],
            pair[1]
        );
    }
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

pub(super) fn shown_at(
    app: &mut App,
    area: Entity,
    index: usize,
) -> Option<f32> {
    rows(app, area)
        .into_iter()
        .find(|&(shown, ..)| shown == index)
        .map(|(.., top)| top)
}

fn assert_cover(app: &mut App, area: Entity, from: f32, to: f32) {
    let rows = rows(app, area);
    let indices = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let unbroken =
        indices.windows(2).all(|pair| pair[1] == pair[0] + 1);
    assert!(unbroken, "rows {indices:?}");
    let (_, first, start) = rows[0];
    let (_, last, end) = rows[rows.len() - 1];
    let end = end
        + app.world().get::<ComputedNode>(last).unwrap().size().y
            * app
                .world()
                .get::<ComputedNode>(last)
                .unwrap()
                .inverse_scale_factor();
    assert!(
        start <= from,
        "the first row, {first}, starts at {start}"
    );
    assert!(end >= to, "the last row, {last}, ends at {end}");
}

#[test]
fn at_the_top_it_builds_the_rows_to_twice_the_overscan_below() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);

    let rows = rows(&mut app, area);
    assert_eq!(rows[0].0, 0);
    assert_cover(&mut app, area, 0.0, VIEW + 2.0 * OVERSCAN);
    let most = ((VIEW + 2.0 * OVERSCAN) / ESTIMATE) as usize + 1;
    assert!(rows.len() <= most, "{} rows", rows.len());
}

#[test]
fn in_the_middle_it_builds_the_rows_within_the_overscan() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    assert_eq!(shown_at(&mut app, area, 5_000), Some(0.0));
    assert_eq!(state(&app, area).first_visible(), 5_000);
    assert_cover(&mut app, area, -OVERSCAN, VIEW + OVERSCAN);
}

#[test]
fn an_overscan_of_100_px_builds_only_the_rows_within_it() {
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
    let area = app
        .world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list");
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), 5_000);
    assert_cover(&mut app, area, -NEAR, VIEW + NEAR);
    let most = ((VIEW + 2.0 * NEAR) / ESTIMATE) as usize + 1;
    assert!(rows(&mut app, area).len() <= most);
}

#[test]
fn at_the_end_the_last_row_ends_the_viewport() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);

    assert_eq!(shown_at(&mut app, area, 9_999), Some(VIEW - 20.0));
    assert_eq!(
        state(&app, area).spawned().map(|s| *s.end()),
        Some(9_999)
    );
    assert_cover(&mut app, area, -OVERSCAN, VIEW);
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
fn a_measured_height_replaces_the_estimate() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);

    assert_eq!(shown_at(&mut app, area, 1), Some(60.0));
    assert_eq!(shown_at(&mut app, area, 2), Some(80.0));
}

fn the_first_row_holds_still_while_the_row_above_is_measured_at(
    scale_factor: f32,
) {
    let mut app = laid_out();
    app.world_mut()
        .query::<&mut Camera>()
        .single_mut(app.world_mut())
        .unwrap()
        .computed
        .target_info = Some(RenderTargetInfo {
        physical_size: UVec2::new(1000, 1000),
        scale_factor,
    });
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 5_001);

    for frame in 0..6 {
        app.update();
        assert_eq!(
            state(&app, area).first_visible(),
            5_001,
            "{frame}"
        );
        assert_eq!(
            shown_at(&mut app, area, 5_001),
            Some(0.0),
            "frame {frame}"
        );
    }
    assert_eq!(shown_at(&mut app, area, 5_000), Some(-60.0));
}

#[test]
fn the_first_row_holds_still_while_the_row_above_is_measured() {
    the_first_row_holds_still_while_the_row_above_is_measured_at(1.0);
}

#[test]
fn the_first_row_holds_still_at_a_scale_factor_of_2() {
    the_first_row_holds_still_while_the_row_above_is_measured_at(2.0);
}

#[test]
fn the_first_row_holds_still_when_a_row_above_grows() {
    let mut app = laid_out();
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
            "{frame}"
        );
    }
    assert_eq!(shown_at(&mut app, area, 5_000), Some(0.0));
    assert_eq!(scroll(&app, area), before + 80.0);
}

#[test]
fn growing_at_the_bottom_keeps_the_last_row_in_sight() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 11_000;
    settle(&mut app);

    assert_eq!(shown_at(&mut app, area, 10_999), Some(VIEW - 20.0));
    assert_eq!(
        state(&app, area).spawned().map(|s| *s.end()),
        Some(10_999)
    );
}

#[test]
fn growing_in_the_middle_keeps_the_first_row_still() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 11_000;
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), 5_000);
    assert_eq!(shown_at(&mut app, area, 5_000), Some(0.0));
}

#[test]
fn growing_from_no_rows_follows_the_bottom() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 0, uniform);
    assert!(rows(&mut app, area).is_empty());
    assert_eq!(state(&app, area).spawned(), None);

    for count in (2..=200).step_by(2) {
        app.world_mut().resource_mut::<Count>().0 = count;
        app.update();
    }
    settle(&mut app);

    assert_eq!(shown_at(&mut app, area, 199), Some(VIEW - 20.0));
    assert_eq!(state(&app, area).count(), 200);
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
fn growing_while_scrolled_up_from_the_bottom_keeps_the_view() {
    let mut app = laid_out();
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    let up = scroll(&app, area) - 40.0;
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y = up;
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 2_000;
    settle(&mut app);

    assert_eq!(scroll(&app, area), up);
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
    let mut app = laid_out();
    let area = list(&mut app, 10_000, flat);

    let rows = rows(&mut app, area);
    assert_eq!(rows[0].0, 0);
    let most = ((VIEW + 2.0 * OVERSCAN) / ESTIMATE) as usize + 1;
    assert!(rows.len() <= most, "{} rows", rows.len());
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
fn a_hidden_list_builds_no_more_than_its_band() {
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

    let most = ((VIEW + 2.0 * OVERSCAN) / ESTIMATE) as usize + 1;
    let built =
        app.world_mut().query::<&Shows>().iter(app.world()).count();
    assert!(built <= most, "{built} rows");
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
fn no_frame_draws_an_unmeasured_row_over_the_next() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);

    scroll_to(&mut app, area, 5_000);
    for frame in 1..=4 {
        app.update();
        assert_apart(
            &mut app,
            area,
            &format!("scroll_to, frame {frame}"),
        );
    }
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y +=
        20_000.0;
    for frame in 1..=4 {
        app.update();
        assert_apart(
            &mut app,
            area,
            &format!("a jump, frame {frame}"),
        );
    }
}

fn under_a_wrong_estimate(count: usize, estimate: f32) {
    let mut app = laid_out();
    let area =
        mounted(&mut app, count, estimate, row(uniform), px(VIEW));
    let check = |app: &mut App, when: &str| {
        let slots = state(app, area).rows();
        assert!(slots < 200, "{when}: {slots} slots");
        assert_apart(app, area, when);
    };
    for frame in 1..=30 {
        app.update();
        check(&mut app, &format!("first paint, frame {frame}"));
    }
    type Jump = fn(&mut App, Entity);
    let jumps: [(&str, Jump); 2] = [
        ("scroll_to", |app, area| scroll_to(app, area, 5_000)),
        ("a jump", |app, area| {
            app.world_mut()
                .get_mut::<ScrollPosition>(area)
                .unwrap()
                .y += 20_000.0;
        }),
    ];
    for (name, jump) in jumps {
        jump(&mut app, area);
        for frame in 1..=8 {
            app.update();
            let when =
                format!("{estimate} px, {name}, frame {frame}");
            check(&mut app, &when);
            if frame >= 2 {
                let gap = uncovered(&mut app, area);
                assert_eq!(gap, 0.0, "{when}: {gap} px uncovered");
            }
        }
    }
}

#[test]
fn an_estimate_ten_times_too_small_stays_in_bounds() {
    under_a_wrong_estimate(10_000, ESTIMATE / 10.0);
    under_a_wrong_estimate(100_000, ESTIMATE / 10.0);
}

#[test]
fn a_right_estimate_stays_in_bounds() {
    under_a_wrong_estimate(10_000, ESTIMATE);
    under_a_wrong_estimate(100_000, ESTIMATE);
}

#[test]
fn an_estimate_ten_times_too_large_stays_in_bounds() {
    under_a_wrong_estimate(10_000, ESTIMATE * 10.0);
    under_a_wrong_estimate(100_000, ESTIMATE * 10.0);
}

#[test]
fn a_content_y_maps_to_its_row_and_back() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    let list = state(&app, area);

    assert_eq!(
        list.row_span(0),
        Some(RowSpan {
            top: 0.0,
            height: 60.0
        })
    );
    assert_eq!(
        list.row_span(1),
        Some(RowSpan {
            top: 60.0,
            height: 20.0
        })
    );
    assert_eq!(list.row_at(59.9), Some(0));
    assert_eq!(list.row_at(60.0), Some(1));
    assert_eq!(list.row_at(85.0), Some(2));
    assert_eq!(list.row_at(-5.0), Some(0));
    assert_eq!(list.row_at(1.0e9), Some(9_999));
    assert_eq!(list.row_span(10_000), None);
    let span = list.row_span(5).unwrap();
    assert_eq!(list.row_at(span.top + span.height / 2.0), Some(5));
}

#[test]
fn an_empty_list_has_no_rows_to_map() {
    let mut app = laid_out();
    let area = list(&mut app, 0, uniform);

    assert_eq!(state(&app, area).row_at(0.0), None);
    assert_eq!(state(&app, area).row_span(0), None);
}

pub(super) fn reveal(app: &mut App, area: Entity, index: usize) {
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_into_view(index);
    settle(app);
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
fn a_row_scrolled_into_view_shows_whole_once_measured() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);

    reveal(&mut app, area, 300);
    assert_eq!(shown_at(&mut app, area, 300), Some(VIEW - 60.0));
}

#[test]
fn a_row_taller_than_the_view_scrolls_in_at_its_top() {
    let mut app = laid_out();
    app.insert_resource(Grown(50, 2_000.0));
    let area = list(&mut app, 10_000, grown);

    reveal(&mut app, area, 50);
    assert_eq!(shown_at(&mut app, area, 50), Some(0.0));
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
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
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
    let area = app
        .world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list");

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
fn small_scrolls_up_leave_the_bottom() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    let bottom = scroll(&app, area);

    for _ in 0..4 {
        app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y -=
            0.5;
        app.update();
    }
    settle(&mut app);

    assert_eq!(scroll(&app, area), bottom - 2.0);
}

fn growing_at_the_bottom_at(scale_factor: f32) {
    let mut app = laid_out();
    app.world_mut()
        .query::<&mut Camera>()
        .single_mut(app.world_mut())
        .unwrap()
        .computed
        .target_info = Some(RenderTargetInfo {
        physical_size: UVec2::new(1000, 1000),
        scale_factor,
    });
    app.insert_resource(Follow(true));
    let area = list(&mut app, 10_000, |_, _| 19.0);
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);

    for count in (10_002..=10_100).step_by(2) {
        app.world_mut().resource_mut::<Count>().0 = count;
        app.update();
    }
    settle(&mut app);

    let last =
        shown_at(&mut app, area, 10_099).expect("the last row");
    assert!(
        (last - (VIEW - 19.0)).abs() <= 1.0,
        "{scale_factor}: {last}"
    );
}

#[test]
fn growing_at_the_bottom_follows_at_fractional_scales() {
    for scale_factor in [1.0, 1.25, 1.5, 2.0] {
        growing_at_the_bottom_at(scale_factor);
    }
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
fn a_width_change_reestimates_rows_out_of_sight() {
    let mut app = laid_out();
    app.insert_resource(Across(300.0));
    let wraps = |world: &World, _: usize| {
        if world.resource::<Across>().0 < 300.0 {
            40.0
        } else {
            20.0
        }
    };
    app.insert_resource(Count(10_000));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row(wraps),
        ),))
        .width(resource::<Across, _>(|across| px(across.0)))
        .height(px(VIEW)),
    );
    settle(&mut app);
    let area = app
        .world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list");
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    app.insert_resource(Across(200.0));
    settle(&mut app);

    assert_eq!(
        state(&app, area).row_span(50),
        Some(RowSpan {
            top: 2_000.0,
            height: 40.0
        })
    );
    assert_eq!(shown_at(&mut app, area, 5_000), Some(0.0));
}

#[test]
fn a_list_grown_from_empty_shows_its_first_row() {
    for from in [0, 10] {
        let mut app = laid_out();
        let area = list(&mut app, from, uniform);

        app.world_mut().resource_mut::<Count>().0 = 10_000;
        settle(&mut app);

        assert_eq!(scroll(&app, area), 0.0, "from {from}");
        assert_eq!(
            state(&app, area).first_visible(),
            0,
            "from {from}"
        );
    }
}

#[test]
fn growing_at_the_bottom_moves_no_list_that_does_not_follow() {
    let mut app = laid_out();
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    let first = state(&app, area).first_visible();
    scroll_to(&mut app, area, first);
    settle(&mut app);
    let at = scroll(&app, area);

    app.world_mut().resource_mut::<Count>().0 = 2_000;
    settle(&mut app);

    assert_eq!(scroll(&app, area), at);
    assert_eq!(state(&app, area).first_visible(), first);
}

#[test]
fn scroll_to_the_top_of_a_list_that_fits_keeps_it_there() {
    let mut app = laid_out();
    let area = list(&mut app, 10, uniform);
    scroll_to(&mut app, area, 0);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 10_000;
    settle(&mut app);

    assert_eq!(scroll(&app, area), 0.0);
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
fn a_following_list_follows_again_back_at_the_bottom() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 500);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 2_000;
    settle(&mut app);
    assert_eq!(
        state(&app, area).first_visible(),
        500,
        "away from it"
    );

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        1.0e9;
    settle(&mut app);
    assert!(state(&app, area).following());
    app.world_mut().resource_mut::<Count>().0 = 3_000;
    settle(&mut app);
    assert_eq!(shown_at(&mut app, area, 2_999), Some(VIEW - 20.0));
}

#[test]
fn a_following_list_keeps_each_new_row_in_sight() {
    for height in [uniform as Height, tall_hundredths] {
        let mut app = laid_out();
        app.insert_resource(Follow(true));
        let area = list(&mut app, 0, height);
        for count in (2..=600).step_by(2) {
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
        let last = shown_at(&mut app, area, 599).unwrap();
        assert_eq!(last + height(app.world(), 599), VIEW);
    }
}

#[test]
fn a_scroll_to_that_ends_at_the_bottom_keeps_a_list_following() {
    for index in [975, 990] {
        let mut app = laid_out();
        app.insert_resource(Follow(true));
        let area = list(&mut app, 1_000, uniform);
        scroll_to(&mut app, area, index);
        settle(&mut app);
        assert_eq!(scroll(&app, area), 1_000.0 * 20.0 - VIEW);
        assert!(state(&app, area).following(), "row {index}");

        app.world_mut().resource_mut::<Count>().0 = 1_100;
        settle(&mut app);

        assert_eq!(
            shown_at(&mut app, area, 1_099),
            Some(VIEW - 20.0),
            "row {index}"
        );
    }
}

#[test]
fn a_scroll_to_in_a_following_list_that_fits_keeps_it_following() {
    for (from, index) in [(10, 5), (0, 0)] {
        let mut app = laid_out();
        app.insert_resource(Follow(true));
        let area = list(&mut app, from, uniform);
        scroll_to(&mut app, area, index);
        settle(&mut app);
        assert!(state(&app, area).following(), "from {from}");

        for count in (from + 2..=100).step_by(2) {
            app.world_mut().resource_mut::<Count>().0 = count;
            app.update();
        }
        settle(&mut app);

        assert_eq!(
            shown_at(&mut app, area, 99),
            Some(VIEW - 20.0),
            "from {from}"
        );
    }
}

fn scroll_to_failures(
    scale_factor: f32,
    follow: bool,
    height: Height,
    index: usize,
) -> Vec<String> {
    let mut app = at_scale(scale_factor);
    app.insert_resource(Follow(follow));
    let area = mounted(&mut app, 10_000, 17.0, row(height), px(VIEW));
    settle(&mut app);
    let case = format!(
        "{scale_factor}x, rows of {} px, following {follow}, \
         scroll_to({index})",
        height(app.world(), 0)
    );
    let mut failed = Vec::new();
    let mut at_bottom = false;
    scroll_to(&mut app, area, index);
    for (when, frames) in [("by the second frame", 2), ("settled", 6)]
    {
        for _ in 0..frames {
            app.update();
        }
        let list = state(&app, area);
        let top = list.row_span(index).unwrap().top;
        let last = list.row_span(9_999).unwrap();
        let bottom = last.top + last.height - VIEW;
        at_bottom = top >= bottom;
        let (shown, want) = if top <= bottom {
            (index, 0.0)
        } else {
            (9_999, VIEW - last.height)
        };
        let got = shown_at(&mut app, area, shown);
        if !got.is_some_and(|got| (got - want).abs() <= 1.0) {
            failed.push(format!(
                "{case}, {when}: row {shown} at {got:?}, not {want}"
            ));
        }
    }
    let following = state(&app, area).following();
    if following != (follow && at_bottom) {
        failed.push(format!("{case}: following() is {following}"));
    }
    let first = state(&app, area).first_visible();
    app.world_mut().resource_mut::<Count>().0 = 10_100;
    settle(&mut app);
    if following {
        let want = VIEW - height(app.world(), 10_099);
        let got = shown_at(&mut app, area, 10_099);
        if !got.is_some_and(|got| (got - want).abs() <= 1.0) {
            failed.push(format!(
                "{case}, grown: row 10099 at {got:?}, not {want}"
            ));
        }
    } else if state(&app, area).first_visible() != first {
        failed.push(format!(
            "{case}, grown: first visible {} not {first}",
            state(&app, area).first_visible()
        ));
    }
    failed
}

#[test]
fn scroll_to_lands_where_it_says_once_the_rows_are_measured() {
    let mut failed = Vec::new();
    for scale_factor in [1.0, 1.25, 1.5] {
        for follow in [true, false] {
            for height in [(|_, _| 19.0) as Height, |_, _| 15.0] {
                for index in [5_000, 9_973, 9_990, 9_999] {
                    failed.extend(scroll_to_failures(
                        scale_factor,
                        follow,
                        height,
                        index,
                    ));
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn a_scroll_on_the_frame_after_a_scroll_to_stands() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, uniform);
    scroll_to(&mut app, area, 5_000);
    app.update();
    let landed = scroll(&app, area);

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y +=
        100.0;
    settle(&mut app);

    assert_eq!(scroll(&app, area), landed + 100.0);
}

pub(super) fn regions(_: &World, index: usize) -> f32 {
    match index {
        ..3_000 => 20.0,
        3_000..7_000 => 10.0,
        _ => 200.0,
    }
}

fn one_huge(_: &World, index: usize) -> f32 {
    if index == 5_010 { 2_000.0 } else { 20.0 }
}

fn landing_failures(
    count: usize,
    estimate: f32,
    height: Height,
) -> Vec<String> {
    let mut app = laid_out();
    let area =
        mounted(&mut app, count, estimate, row(height), px(VIEW));
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
                "{estimate} px, {count} rows, {when}: {slots} \
                 slots, {gap} px uncovered, overlap {overlap:?}"
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

const REAL_TO_TEN_TIMES: [f32; 9] =
    [2.0, 4.0, 10.0, 16.0, 20.0, 25.0, 50.0, 100.0, 200.0];

fn assert_landings(height: Height, count: usize) {
    let failed = REAL_TO_TEN_TIMES
        .into_iter()
        .flat_map(|estimate| {
            landing_failures(count, estimate, height)
        })
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn regions_of_other_heights_show_whole_by_the_second_frame() {
    assert_landings(regions, 10_000);
}

#[test]
fn regions_of_other_heights_show_whole_at_100k() {
    assert_landings(regions, 100_000);
}

#[test]
fn one_huge_row_leaves_landings_whole_by_the_second_frame() {
    assert_landings(one_huge, 10_000);
    assert_landings(one_huge, 100_000);
}

#[test]
fn three_line_rows_show_whole_by_the_second_frame() {
    assert_landings(tall_hundredths, 10_000);
    assert_landings(tall_hundredths, 100_000);
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
}

#[test]
fn a_right_estimate_holds_through_a_few_tall_rows() {
    let mut app = laid_out();
    let area = list(&mut app, 10_000, tall_hundredths);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    assert_eq!(
        state(&app, area).row_span(9_999).unwrap().height,
        20.0
    );
    assert_eq!(
        state(&app, area).row_span(2_001).unwrap().height,
        20.0
    );
}

#[test]
fn a_width_change_keeps_a_right_estimate_past_one_huge_row() {
    let mut app = laid_out();
    app.insert_resource(Across(300.0));
    app.insert_resource(Count(10_000));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row(one_huge),
        ),))
        .width(resource::<Across, _>(|across| px(across.0)))
        .height(px(VIEW)),
    );
    settle(&mut app);
    let area = app
        .world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list");
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    app.insert_resource(Across(200.0));
    settle(&mut app);

    assert_eq!(state(&app, area).row_span(100).unwrap().height, 20.0);
    assert_eq!(
        state(&app, area).row_span(9_999).unwrap().height,
        20.0
    );
}

pub(super) fn at_scale(scale_factor: f32) -> App {
    let mut app = laid_out();
    app.world_mut()
        .query::<&mut Camera>()
        .single_mut(app.world_mut())
        .unwrap()
        .computed
        .target_info = Some(RenderTargetInfo {
        physical_size: UVec2::new(1000, 1000),
        scale_factor,
    });
    app
}

#[test]
fn rows_never_overlap_at_any_scale() {
    for scale_factor in [1.0, 1.25, 1.5, 2.0] {
        let mut app = at_scale(scale_factor);
        let area =
            mounted(&mut app, 10_000, 16.3, row(regions), px(VIEW));
        for (index, frames) in [(9_999, 4), (5_000, 4), (100, 4)] {
            scroll_to(&mut app, area, index);
            for frame in 1..=frames {
                app.update();
                assert_apart(
                    &mut app,
                    area,
                    &format!(
                        "{scale_factor}x, row {index}, frame {frame}"
                    ),
                );
            }
        }
    }
}

#[test]
fn a_view_inside_a_row_taller_than_it_does_not_move() {
    let mut app = laid_out();
    app.insert_resource(Grown(50, 2_000.0));
    let area = list(&mut app, 10_000, grown);
    reveal(&mut app, area, 50);
    let inside = scroll(&app, area) + 800.0;
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        inside;
    settle(&mut app);

    reveal(&mut app, area, 50);

    assert_eq!(scroll(&app, area), inside);
}

#[test]
fn a_row_taller_than_the_view_above_it_scrolls_in_at_its_bottom() {
    let mut app = laid_out();
    app.insert_resource(Grown(50, 2_000.0));
    let area = list(&mut app, 10_000, grown);
    scroll_to(&mut app, area, 60);
    settle(&mut app);

    reveal(&mut app, area, 50);

    let span = state(&app, area).row_span(50).unwrap();
    assert_eq!(scroll(&app, area), span.top + span.height - VIEW);
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

#[test]
fn an_unmeasured_tall_row_scrolls_in_by_the_least() {
    let mut app = laid_out();
    app.insert_resource(Grown(5_000, 2_000.0));
    let area = list(&mut app, 10_000, grown);
    reveal_range(&mut app, area, 5_000, 1_500.0..1_520.0);
    let span = state(&app, area).row_span(5_000).unwrap();
    assert_eq!(
        scroll(&app, area),
        span.top + 1_520.0 - VIEW,
        "above"
    );

    let mut app = laid_out();
    app.insert_resource(Grown(5_000, 2_000.0));
    let area = list(&mut app, 10_000, grown);
    scroll_to(&mut app, area, 9_000);
    settle(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_into_view(5_000);
    app.update();
    app.update();
    let span = state(&app, area).row_span(5_000).unwrap();
    let bottom = span.top + span.height - VIEW;
    assert_eq!(span.height, 2_000.0);
    assert_eq!(scroll(&app, area), bottom, "by the second frame");
    settle(&mut app);
    assert_eq!(scroll(&app, area), bottom, "below");

    reveal_range(&mut app, area, 5_000, 1_990.0..2_500.0);
    assert_eq!(scroll(&app, area), bottom, "past its end");

    scroll_to(&mut app, area, 4_000);
    settle(&mut app);
    reveal_range(&mut app, area, 5_000, 1_000.0..1_000.0);
    assert_eq!(
        scroll(&app, area),
        span.top + 1_000.0 - VIEW,
        "empty"
    );
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

pub(super) fn wrapping_list(app: &mut App, sideways: bool) -> Entity {
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
    let area = app
        .world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list");
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
    let mut app = at_scale(1.0);
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

#[test]
fn a_following_list_its_parent_fits_draws_every_row_as_it_grows() {
    for scale_factor in [1.0, 1.25, 2.0] {
        let mut app = at_scale(scale_factor);
        fitted(&mut app);
        app.insert_resource(Count(0));
        mount::<Plain>(app.world_mut(), fitted_list());
        let area = lists(&mut app)[0];
        settle(&mut app);
        for added in [1, 2, 1, 1, 3, 1, 2, 1] {
            app.world_mut().resource_mut::<Count>().0 += added;
            app.update();
            let count = app.world().resource::<Count>().0;
            let built = rows(&mut app, area)
                .into_iter()
                .map(|(index, ..)| index)
                .collect::<Vec<_>>();
            assert_eq!(
                built,
                (0..count).collect::<Vec<_>>(),
                "{scale_factor}x, {count} rows in {} px",
                height(&app, area)
            );
        }
    }
}

#[derive(Resource, Clone, Copy, PartialEq)]
struct Pass(u32);

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
fn a_list_its_parent_fits_draws_every_row_on_its_first_two_frames() {
    for scale_factor in [1.0, 1.25, 2.0] {
        let mut app = at_scale(scale_factor);
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
        for pass in 0..3 {
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

#[test]
fn a_list_its_parent_fits_laid_out_empty_draws_the_rows_it_gets() {
    for scale_factor in [1.0, 1.25, 2.0] {
        let mut app = at_scale(scale_factor);
        fitted(&mut app);
        app.insert_resource(Count(0));
        mount::<Plain>(app.world_mut(), fitted_list());
        settle(&mut app);
        let area = lists(&mut app)[0];
        app.world_mut().resource_mut::<Count>().0 = 17;
        app.update();
        assert_builds_every_row(
            &mut app,
            area,
            &format!("{scale_factor}x"),
        );
    }
}

#[derive(Resource)]
struct Model(Vec<(u32, f32)>);

#[derive(Component)]
struct Id(u32);

fn model(count: usize) -> Vec<(u32, f32)> {
    (0..count as u32)
        .map(|id| (id, 16.0 + (id * 37 % 9) as f32))
        .collect()
}

fn modelled(slot: RowSlot) -> AnyView<Bevy, Plain> {
    AnyView::new(move |cx| {
        let index = slot.index();
        let node = cx.build(
            frame().height(
                derived(move |world| {
                    px(world.resource::<Model>().0[index.get(world)]
                        .1)
                })
                .when(every_frame()),
            ),
        );
        let index = slot.index();
        let shows = derived(move |world| {
            let index = index.get(world);
            (index, world.resource::<Model>().0[index].0)
        })
        .when(every_frame());
        cx.effect(node, shows.into(), |world, node, &(index, id)| {
            world.entity_mut(node).insert((Shows(index), Id(id)));
        });
        node
    })
}

fn modelled_list(app: &mut App, count: usize) -> Entity {
    app.insert_resource(Model(model(count)));
    let area = mounted(app, count, ESTIMATE, modelled, px(VIEW));
    settle(app);
    area
}

/// Splices the rows of the model, its count and the list alike. The
/// rows inserted are 22 px high, which the list does not know yet.
fn splice_rows(
    app: &mut App,
    area: Entity,
    at: usize,
    removed: usize,
    inserted: usize,
) {
    let mut model = app.world_mut().resource_mut::<Model>();
    let next =
        model.0.iter().map(|&(id, _)| id + 1).max().unwrap_or(0);
    let new = (next..next + inserted as u32).map(|id| (id, 22.0));
    model.0.splice(at..at + removed, new);
    let count = model.0.len();
    app.world_mut().resource_mut::<Count>().0 = count;
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .splice(at, removed, inserted);
}

/// Each built row's id, with the index it shows, its node and its
/// top relative to the area.
fn ids(
    app: &mut App,
    area: Entity,
) -> HashMap<u32, (usize, Entity, f32)> {
    let origin = top(app, area);
    let built = app
        .world_mut()
        .query::<(Entity, &Id, &Shows)>()
        .iter(app.world())
        .map(|(node, id, shows)| (id.0, shows.0, node))
        .collect::<Vec<_>>();
    built
        .into_iter()
        .map(|(id, index, node)| {
            (id, (index, node, top(app, node) - origin))
        })
        .collect()
}

fn spans(list: &VirtualList) -> Vec<f32> {
    (0..list.count())
        .map(|index| list.row_span(index).expect("a row").height)
        .collect()
}

fn fresh(heights: &[f32]) -> VirtualList {
    let mut fresh = VirtualList::detached(heights.len(), ESTIMATE);
    for (index, &height) in heights.iter().enumerate() {
        fresh.measure(index, height);
    }
    fresh
}

fn assert_fresh(list: &VirtualList, heights: &[f32], when: &str) {
    assert_eq!(list.count(), heights.len(), "{when}: the count");
    let fresh = fresh(heights);
    for index in 0..heights.len() {
        assert_eq!(
            list.row_span(index),
            fresh.row_span(index),
            "{when}: row {index}"
        );
    }
}

fn splice_case(
    view: usize,
    at: usize,
    removed: usize,
    inserted: usize,
) {
    const COUNT: usize = 2_100;
    let when = format!("{removed} for {inserted} at {at}");
    let removed = removed.min(COUNT - at);
    let mut app = laid_out();
    let area = modelled_list(&mut app, COUNT);
    scroll_to(&mut app, area, view);
    settle(&mut app);
    let before = ids(&mut app, area);
    let mut heights = spans(state(&app, area));
    heights.splice(
        at..at + removed,
        core::iter::repeat_n(ESTIMATE, inserted),
    );

    splice_rows(&mut app, area, at, removed, inserted);
    assert_fresh(state(&app, area), &heights, &when);
    settle(&mut app);

    let list = state(&app, area);
    assert_fresh(list, &spans(list), &format!("{when}, settled"));
    let after = ids(&mut app, area);
    let model = &app.world().resource::<Model>().0;
    let list = state(&app, area);
    let mut kept = 0;
    for (&id, &(index, node, _)) in &after {
        assert_eq!(model[index].0, id, "{when}: row {index}'s id");
        assert_eq!(
            list.row_span(index).unwrap().height,
            model[index].1,
            "{when}: row {index}'s height"
        );
        if let Some(&(_, was, _)) = before.get(&id) {
            assert_eq!(node, was, "{when}: the view of {id}");
            kept += 1;
        }
    }
    assert!(kept > 0, "{when}: no row kept its view");
}

#[test]
fn a_splice_at_the_first_middle_or_last_row_moves_only_the_rows_after()
 {
    for (removed, inserted) in [(0, 3), (3, 0), (2, 3)] {
        splice_case(0, 0, removed, inserted);
        splice_case(1_040, 1_050, removed, inserted);
        splice_case(2_099, 2_099, removed, inserted);
    }
}

#[test]
fn a_splice_that_frees_slots_measured_this_frame_leaves_no_gap() {
    let mut app = laid_out();
    let area = modelled_list(&mut app, 2_100);

    splice_rows(&mut app, area, 10, 30, 0);
    app.update();
    assert_eq!(uncovered(&mut app, area), 0.0, "after the remove");
    splice_rows(&mut app, area, 5, 20, 2);
    app.update();
    assert_eq!(uncovered(&mut app, area), 0.0, "after the replace");
    settle(&mut app);
    assert_apart(&mut app, area, "settled");
    assert_eq!(state(&app, area).count(), 2_100 - 30 - 18);
}

#[derive(Resource)]
struct Wide(HashMap<u32, f32>);

fn sideways_modelled(slot: RowSlot) -> AnyView<Bevy, Plain> {
    let gutter = frame().width(px(40.0)).height(px(20.0));
    let index = slot.index();
    let text = frame()
        .width(
            derived(move |world| {
                let id =
                    world.resource::<Model>().0[index.get(world)].0;
                let wide = &world.resource::<Wide>().0;
                px(wide.get(&id).copied().unwrap_or(100.0))
            })
            .when(every_frame()),
        )
        .height(px(20.0));
    let text = AnyView::new(move |cx| {
        let node = cx.build(text);
        let index = slot.index();
        let shows = derived(move |world| {
            let index = index.get(world);
            (index, world.resource::<Model>().0[index].0)
        })
        .when(every_frame());
        cx.effect(node, shows.into(), |world, node, &(index, id)| {
            world.entity_mut(node).insert((Shows(index), Id(id)));
        });
        node
    });
    hrow((gutter.boxed(), slot.sideways(text))).boxed()
}

fn sideways_modelled_list(
    app: &mut App,
    rows: Vec<(u32, f32)>,
    wide: HashMap<u32, f32>,
) -> Entity {
    app.insert_resource(Count(rows.len()));
    app.insert_resource(Model(rows));
    app.insert_resource(Wide(wide));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            sideways_modelled,
        )
        .sideways(true),))
        .width(px(300.0))
        .height(px(VIEW)),
    );
    settle(app);
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
}

/// The width of a row's sideways cell.
fn room(app: &mut App) -> f32 {
    let (row, _) = app
        .world_mut()
        .query::<(Entity, &Id)>()
        .iter(app.world())
        .next()
        .expect("a built row");
    let moved = app.world().get::<ChildOf>(row).unwrap().parent();
    let cell = app.world().get::<ChildOf>(moved).unwrap().parent();
    right(app, cell) - left(app, cell)
}

fn widest(app: &mut App, area: Entity) -> f32 {
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().x =
        1.0e6;
    settle(app);
    app.world().get::<ScrollPosition>(area).unwrap().x
}

#[test]
fn a_splice_leaves_the_widest_row_of_a_fresh_list() {
    let wide = HashMap::from([(3, 600.0), (7, 900.0)]);
    let mut app = laid_out();
    let area =
        sideways_modelled_list(&mut app, model(2_100), wide.clone());
    let room = room(&mut app);
    assert_eq!(widest(&mut app, area), 900.0 - room);

    for (at, removed, inserted, most) in [
        (2, 0, 5, 900.0),
        (12, 1, 0, 600.0),
        (0, 3, 2, 600.0),
        (7, 1, 0, 100.0),
    ] {
        splice_rows(&mut app, area, at, removed, inserted);
        let spliced = widest(&mut app, area);
        let rows = app.world().resource::<Model>().0.clone();
        let mut fresh = laid_out();
        let fresh_area =
            sideways_modelled_list(&mut fresh, rows, wide.clone());
        assert_eq!(spliced, widest(&mut fresh, fresh_area));
        assert_eq!(spliced, (most - room).max(0.0), "at {at}");
    }
}

#[test]
fn a_scroll_to_past_a_splice_lands_on_the_row_it_named() {
    let mut app = laid_out();
    let area = modelled_list(&mut app, 10_000);

    scroll_to(&mut app, area, 9_000);
    splice_rows(&mut app, area, 1_000, 0, 20);
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), 9_020);
    let named = ids(&mut app, area)[&9_000];
    assert_eq!((named.0, named.2), (9_020, 0.0));
}

fn on_screen(app: &mut App, area: Entity) -> Vec<(u32, f32)> {
    let mut shown = ids(app, area)
        .into_iter()
        .filter(|&(_, (_, _, top))| (0.0..VIEW).contains(&top))
        .map(|(id, (_, _, top))| (id, top))
        .collect::<Vec<_>>();
    shown.sort_by(|a, b| a.1.total_cmp(&b.1));
    shown
}

#[test]
fn a_splice_above_the_view_leaves_the_rows_on_screen_still() {
    for (at, removed, inserted) in [
        (1_000, 0, 20),
        (1_000, 20, 0),
        (1_000, 5, 7),
        (4_970, 10, 3),
        (4_970, 3, 10),
    ] {
        let when = format!("{removed} for {inserted} at {at}");
        let mut app = laid_out();
        let area = modelled_list(&mut app, 10_000);
        scroll_to(&mut app, area, 5_000);
        settle(&mut app);
        app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y +=
            7.0;
        settle(&mut app);
        let shown = on_screen(&mut app, area);
        let first = state(&app, area).first_visible();

        splice_rows(&mut app, area, at, removed, inserted);
        let moved = first - removed + inserted;
        assert_eq!(
            state(&app, area).first_visible(),
            moved,
            "{when}"
        );
        app.update();
        assert_eq!(on_screen(&mut app, area), shown, "{when}");
        assert_eq!(
            state(&app, area).first_visible(),
            moved,
            "{when}"
        );
        settle(&mut app);
        assert_eq!(
            on_screen(&mut app, area),
            shown,
            "{when}, settled"
        );
    }
}

#[test]
fn a_following_list_follows_through_a_splice_above() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = modelled_list(&mut app, 2_100);
    for _ in 0..3 {
        app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
            1.0e9;
        settle(&mut app);
    }
    assert!(state(&app, area).following());
    let bottom = |app: &App| {
        let list = state(app, area);
        let last = list.row_span(list.count() - 1).unwrap();
        last.top + last.height - VIEW
    };

    for (at, removed, inserted) in [(100, 0, 20), (100, 40, 0)] {
        splice_rows(&mut app, area, at, removed, inserted);
        app.update();
        assert!(state(&app, area).following(), "at {at}");
        assert_eq!(scroll(&app, area), bottom(&app), "at {at}");
        settle(&mut app);
        assert!(state(&app, area).following(), "at {at}, settled");
        assert_eq!(
            scroll(&app, area),
            bottom(&app),
            "at {at}, settled"
        );
    }
}

#[test]
fn a_splice_of_the_first_visible_row_leaves_no_gap() {
    let mut app = laid_out();
    let area = modelled_list(&mut app, 10_000);
    scroll_to(&mut app, area, 5_000);
    settle(&mut app);

    splice_rows(&mut app, area, 4_990, 20, 0);
    assert_eq!(state(&app, area).first_visible(), 4_990);
    app.update();
    assert_eq!(uncovered(&mut app, area), 0.0);
    settle(&mut app);
    assert_eq!(uncovered(&mut app, area), 0.0, "settled");
    assert_apart(&mut app, area, "settled");
}

/// splitmix64.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn below(state: &mut u64, bound: usize) -> usize {
    (next(state) % bound as u64) as usize
}

#[test]
fn random_splices_match_a_fresh_list() {
    let seed = std::env::var("RUBRIX_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(1);
    let mut random = seed;
    let mut heights = vec![ESTIMATE; 2_500];
    let mut list = VirtualList::detached(heights.len(), ESTIMATE);
    for step in 0..1_000 {
        let len = heights.len();
        let most = if below(&mut random, 8) == 0 { 1_500 } else { 8 };
        let at = below(&mut random, len + 1);
        let removed = below(&mut random, most.min(len - at) + 1);
        let mut inserted = below(&mut random, most + 1);
        if len - removed + inserted > 6_000 {
            inserted = 0;
        }
        list.splice(at, removed, inserted);
        heights.splice(
            at..at + removed,
            core::iter::repeat_n(ESTIMATE, inserted),
        );
        for _ in 0..below(&mut random, 20).min(heights.len()) {
            let index = below(&mut random, heights.len());
            let height = 8.0 + below(&mut random, 256) as f32 / 8.0;
            list.measure(index, height);
            heights[index] = height;
        }

        let when = format!("seed {seed}, step {step}");
        assert_fresh(&list, &heights, &when);
        let fresh = fresh(&heights);
        let total = heights.iter().sum::<f32>();
        for _ in 0..50 {
            let y = below(&mut random, total as usize * 8 + 800)
                as f32
                / 8.0;
            assert_eq!(
                list.row_at(y),
                fresh.row_at(y),
                "{when}: y {y}"
            );
        }
    }
}
