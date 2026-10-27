//! The virtual list against real bevy_ui layout, headless: which rows
//! it builds, where they show, and what a change keeps still.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::app::App;
use bevy::camera::{Camera, RenderTargetInfo};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::world::World;
use bevy::math::UVec2;
use bevy::ui::{
    ComputedNode, Display, ScrollPosition, UiGlobalTransform, Val, px,
};

use super::layout::laid_out;
use crate::tests::Plain;
use crate::views::{
    FrameProps, RowSlot, VirtualList, column, frame, virtual_list,
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

/// How high the list's column is.
#[derive(Resource)]
struct Tall(f32);

/// A row grown to a height of its own.
#[derive(Resource)]
struct Grown(usize, f32);

/// The index a row's node shows.
#[derive(Component)]
struct Shows(usize);

type Height = fn(&World, usize) -> f32;

fn uniform(_: &World, _: usize) -> f32 {
    20.0
}

/// 60 px for every hundredth row, 20 px for the rest.
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

/// 20 px, and the row [`Grown`] names at its height.
fn grown(world: &World, index: usize) -> f32 {
    match world.get_resource::<Grown>() {
        Some(&Grown(at, height)) if at == index => height,
        _ => 20.0,
    }
}

/// A row as high as `height` says, marked with the index it shows.
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

/// A list of [`Count`] rows in a column `VIEW` high at the top left
/// of the window, after it settles. Returns the list's scroll area.
fn list(app: &mut App, count: usize, height: Height) -> Entity {
    list_in(app, count, row(height), px(VIEW))
}

/// As [`list`], with rows `row` builds, in a column `view` high.
fn list_in(
    app: &mut App,
    count: usize,
    row: impl Fn(RowSlot) -> AnyView<Bevy, Plain> + Send + Sync + 'static,
    view: impl Into<Prop<Val>>,
) -> Entity {
    app.insert_resource(Count(count));
    mount::<Plain>(
        app.world_mut(),
        column((virtual_list(
            resource::<Count, _>(|count| count.0),
            ESTIMATE,
            row,
        ),))
        .width(px(300.0))
        .height(view),
    );
    settle(app);
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

/// Where `node` starts, in logical px down the window.
fn top(app: &App, node: Entity) -> f32 {
    let computed = app.world().get::<ComputedNode>(node).unwrap();
    let at = app.world().get::<UiGlobalTransform>(node).unwrap();
    (at.translation.y - computed.size().y / 2.0)
        * computed.inverse_scale_factor()
}

/// Every row built, by index: its node and how far below the top of
/// the scroll area it shows.
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

/// Asserts that the rows run unbroken from above `from` to below
/// `to`, in px from the top of the area.
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

/// Jumps to the row under a tall one, which nothing has measured, and
/// checks that it stays at the top while the tall row is measured.
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
fn growing_from_no_rows_shows_the_first() {
    let mut app = laid_out();
    let area = list(&mut app, 0, tall_hundredths);
    assert!(rows(&mut app, area).is_empty());
    assert_eq!(state(&app, area).spawned(), None);

    app.world_mut().resource_mut::<Count>().0 = 10_000;
    settle(&mut app);

    assert_eq!(scroll(&app, area), 0.0);
    assert_eq!(shown_at(&mut app, area, 0), Some(0.0));
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
