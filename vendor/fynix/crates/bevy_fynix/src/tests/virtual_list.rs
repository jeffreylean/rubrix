//! The virtual list against real bevy_ui layout, headless: which rows
//! it builds, where they show, and what a change keeps still.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::app::App;
use bevy::camera::{Camera, RenderTargetInfo};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::math::UVec2;
use bevy::ui::{
    ComputedNode, Display, Node, OverflowAxis, ScrollPosition,
    UiGlobalTransform, Val, px,
};

use super::layout::laid_out;
use crate::tests::Plain;
use crate::views::{
    FrameProps, RowSlot, RowSpan, VirtualList, column, frame,
    row as hrow, virtual_list,
};
use crate::{
    AnyView, Bevy, Prop, ViewExt, derived, every_frame, mount,
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

#[derive(Component)]
struct Shows(usize);

type Height = fn(&World, usize) -> f32;

fn uniform(_: &World, _: usize) -> f32 {
    20.0
}

fn tall_hundredths(_: &World, index: usize) -> f32 {
    if index.is_multiple_of(100) {
        60.0
    } else {
        20.0
    }
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
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        app.update();
    }
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

fn reveal(app: &mut App, area: Entity, index: usize) {
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
        .sideways(),))
        .width(resource::<Across, _>(|across| px(across.0)))
        .height(px(VIEW)),
    );
    settle(app);
    app.world_mut()
        .query_filtered::<Entity, With<VirtualList>>()
        .single(app.world())
        .expect("one list")
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
fn scroll_to_the_top_of_a_following_list_that_fits_keeps_it_there() {
    let mut app = laid_out();
    app.insert_resource(Follow(true));
    let area = list(&mut app, 10, uniform);
    assert!(state(&app, area).following());
    scroll_to(&mut app, area, 0);
    settle(&mut app);

    app.world_mut().resource_mut::<Count>().0 = 10_000;
    settle(&mut app);

    assert_eq!(scroll(&app, area), 0.0);
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

fn regions(_: &World, index: usize) -> f32 {
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

fn at_scale(scale_factor: f32) -> App {
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
