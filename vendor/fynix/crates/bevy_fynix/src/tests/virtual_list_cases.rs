use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::world::World;
use bevy::ui::{ComputedNode, ScrollPosition, UiGlobalTransform, px};

use super::layout::laid_out_at;
use super::virtual_list::{
    Count, Follow, Grown, Height, Shows, Sideways, VIEW, grown, list,
    mounted, regions, reveal, row, scroll, scroll_to, settle,
    shown_at, state, tall_hundredths, uniform, wrapping_list,
};
use crate::views::VirtualList;

const SCALES: [f32; 3] = [1.0, 1.25, 1.5];
const EVERY_SCALE: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

fn h19(_: &World, _: usize) -> f32 {
    19.0
}

fn h15(_: &World, _: usize) -> f32 {
    15.0
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

struct Shape {
    name: &'static str,
    height: Height,
    estimates: &'static [f32],
}

const SHAPES: [Shape; 8] = [
    Shape {
        name: "19 px",
        height: h19,
        estimates: &[17.0, 19.0, 20.0],
    },
    Shape {
        name: "15 px",
        height: h15,
        estimates: &[17.0, 15.0],
    },
    Shape {
        name: "20 px",
        height: uniform,
        estimates: &[20.0, 5.0, 170.0],
    },
    Shape {
        name: "60 px every 100th",
        height: tall_hundredths,
        estimates: &[20.0, 17.0],
    },
    Shape {
        name: "regions",
        height: regions,
        estimates: &[20.0],
    },
    Shape {
        name: "12 to 40 px",
        height: mixed,
        estimates: &[17.0, 26.0],
    },
    Shape {
        name: "a 60 px tail",
        height: tall_tail,
        estimates: &[17.0],
    },
    Shape {
        name: "a 5 px tail",
        height: short_tail,
        estimates: &[40.0, 20.0],
    },
];

const INDICES: [usize; 12] = [
    0, 5_000, 9_700, 9_900, 9_940, 9_970, 9_973, 9_975, 9_990, 9_998,
    9_999, 10_050,
];

#[derive(Clone, Copy, Debug)]
enum Start {
    Top,
    Middle,
    Bottom,
}

fn near(got: Option<f32>, want: f32) -> bool {
    got.is_some_and(|got| (got - want).abs() <= 1.0)
}

fn updates(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn to_the_end(app: &mut App, area: Entity) {
    for _ in 0..3 {
        app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
            1.0e9;
        updates(app, 3);
    }
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

fn landing_case(
    shape: &Shape,
    estimate: f32,
    scale: f32,
    follow: bool,
    start: Start,
    index: usize,
) -> Vec<String> {
    let mut app = laid_out_at(scale);
    app.insert_resource(Follow(follow));
    let area = mounted(
        &mut app,
        10_000,
        estimate,
        row(shape.height),
        px(VIEW),
    );
    settle(&mut app);
    match start {
        Start::Top => {}
        Start::Middle => {
            scroll_to(&mut app, area, 3_000);
            updates(&mut app, 4);
        }
        Start::Bottom => to_the_end(&mut app, area),
    }
    let mut failed = Vec::new();
    scroll_to(&mut app, area, index);
    updates(&mut app, 2);
    landed(&mut app, area, index, "by the second frame", &mut failed);
    updates(&mut app, 4);
    let at_bottom =
        landed(&mut app, area, index, "settled", &mut failed);
    let following = state(&app, area).following();
    if following != (follow && at_bottom) {
        failed.push(format!("following() is {following}"));
    }
    grown_by_100(&mut app, area, shape.height, &mut failed);
    let case = format!(
        "{} under {estimate}, {scale}x, follow {follow}, from the \
         {start:?}, scroll_to({index})",
        shape.name
    );
    failed.into_iter().map(|f| format!("{case}: {f}")).collect()
}

fn assert_none(failed: Vec<String>, cases: usize) {
    assert!(
        failed.is_empty(),
        "{} of {cases} cases:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

fn landings(shape: &Shape) {
    let mut failed = Vec::new();
    let mut cases = 0;
    for &estimate in shape.estimates {
        for scale in SCALES {
            for follow in [true, false] {
                for start in
                    [Start::Top, Start::Middle, Start::Bottom]
                {
                    for index in INDICES {
                        cases += 1;
                        failed.extend(landing_case(
                            shape, estimate, scale, follow, start,
                            index,
                        ));
                    }
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[test]
fn scroll_to_lands_on_rows_of_19_px() {
    landings(&SHAPES[0]);
}

#[test]
fn scroll_to_lands_on_rows_of_15_px() {
    landings(&SHAPES[1]);
}

#[test]
fn scroll_to_lands_on_rows_of_20_px() {
    landings(&SHAPES[2]);
}

#[test]
fn scroll_to_lands_on_a_tall_row_every_100() {
    landings(&SHAPES[3]);
}

#[test]
fn scroll_to_lands_on_regions_of_other_heights() {
    landings(&SHAPES[4]);
}

#[test]
fn scroll_to_lands_on_mixed_heights() {
    landings(&SHAPES[5]);
}

#[test]
fn scroll_to_lands_on_a_tail_taller_than_the_estimate() {
    landings(&SHAPES[6]);
}

#[test]
fn scroll_to_lands_on_a_tail_shorter_than_the_estimate() {
    landings(&SHAPES[7]);
}

#[test]
fn scroll_to_the_last_row_lands_at_every_scale() {
    let mut failed = Vec::new();
    let mut cases = 0;
    for scale in EVERY_SCALE {
        for follow in [true, false] {
            for (shape, estimate) in [
                (&SHAPES[0], 17.0),
                (&SHAPES[1], 17.0),
                (&SHAPES[3], 20.0),
            ] {
                for index in [5_000, 9_973, 9_999] {
                    cases += 1;
                    failed.extend(landing_case(
                        shape,
                        estimate,
                        scale,
                        follow,
                        Start::Top,
                        index,
                    ));
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[derive(Clone, Copy, Debug)]
enum Call {
    Before,
    With,
    After,
}

fn count_change_case(
    shape: &Shape,
    estimate: f32,
    scale: f32,
    follow: bool,
    from: usize,
    call: Call,
    index: usize,
) -> Vec<String> {
    let mut app = laid_out_at(scale);
    app.insert_resource(Follow(follow));
    let area = mounted(
        &mut app,
        from,
        estimate,
        row(shape.height),
        px(VIEW),
    );
    settle(&mut app);
    if let Call::Before = call {
        scroll_to(&mut app, area, index);
        app.update();
    }
    app.world_mut().resource_mut::<Count>().0 = 10_000;
    if let Call::With = call {
        scroll_to(&mut app, area, index);
    }
    app.update();
    if let Call::After = call {
        scroll_to(&mut app, area, index);
    }
    let mut failed = Vec::new();
    if !matches!(call, Call::Before) {
        app.update();
        landed(
            &mut app,
            area,
            index,
            "by the second frame",
            &mut failed,
        );
    }
    updates(&mut app, 6);
    let at_bottom =
        landed(&mut app, area, index, "settled", &mut failed);
    let following = state(&app, area).following();
    if following != (follow && at_bottom) {
        failed.push(format!("following() is {following}"));
    }
    let case = format!(
        "{} under {estimate}, {scale}x, follow {follow}, {from} \
         rows to 10000, scroll_to({index}) {call:?}",
        shape.name
    );
    failed.into_iter().map(|f| format!("{case}: {f}")).collect()
}

#[test]
fn scroll_to_lands_against_the_count_of_its_frames() {
    let mut failed = Vec::new();
    let mut cases = 0;
    for (shape, estimate) in [
        (&SHAPES[0], 17.0),
        (&SHAPES[1], 17.0),
        (&SHAPES[2], 20.0),
        (&SHAPES[3], 20.0),
        (&SHAPES[5], 26.0),
        (&SHAPES[0], 19.0),
    ] {
        for scale in SCALES {
            for follow in [true, false] {
                for from in [0, 1_000, 9_990] {
                    for call in
                        [Call::Before, Call::With, Call::After]
                    {
                        for index in [5_000, 9_973, 9_999] {
                            cases += 1;
                            failed.extend(count_change_case(
                                shape, estimate, scale, follow, from,
                                call, index,
                            ));
                        }
                    }
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[test]
fn rows_added_after_a_scroll_to_land_or_follow() {
    let mut failed = Vec::new();
    let mut cases = 0;
    for (shape, estimate) in
        [(&SHAPES[2], 20.0), (&SHAPES[1], 17.0), (&SHAPES[0], 17.0)]
    {
        for gap in 0..4 {
            for at_bottom in [false, true] {
                cases += 1;
                let mut app = laid_out_at(1.0);
                app.insert_resource(Follow(true));
                let area = mounted(
                    &mut app,
                    10_000,
                    estimate,
                    row(shape.height),
                    px(VIEW),
                );
                settle(&mut app);
                if at_bottom {
                    to_the_end(&mut app, area);
                }
                scroll_to(&mut app, area, 9_999);
                for frame in 0..8 {
                    if frame == gap {
                        app.world_mut().resource_mut::<Count>().0 =
                            10_100;
                    }
                    app.update();
                }
                let case = format!(
                    "{} under {estimate}, at the bottom \
                     {at_bottom}, 100 rows {gap} updates on",
                    shape.name
                );
                let following = state(&app, area).following();
                let (shown, want) = if gap <= 1 {
                    (9_999, 0.0)
                } else {
                    (
                        10_099,
                        VIEW - (shape.height)(app.world(), 10_099),
                    )
                };
                let got = shown_at(&mut app, area, shown);
                if !near(got, want) || following != (gap > 1) {
                    failed.push(format!(
                        "{case}: row {shown} at {got:?}, not \
                         {want}, following {following}"
                    ));
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[test]
fn scroll_to_the_last_row_of_a_stream() {
    let mut failed = Vec::new();
    let mut cases = 0;
    for (shape, estimate) in
        [(&SHAPES[0], 17.0), (&SHAPES[2], 20.0), (&SHAPES[1], 17.0)]
    {
        for scale in SCALES {
            for gap in 0..3 {
                for follow in [true, false] {
                    cases += 1;
                    let mut app = laid_out_at(scale);
                    app.insert_resource(Follow(follow));
                    let area = mounted(
                        &mut app,
                        10_000,
                        estimate,
                        row(shape.height),
                        px(VIEW),
                    );
                    settle(&mut app);
                    scroll_to(&mut app, area, 9_999);
                    for frame in 0..30 {
                        if frame >= gap {
                            app.world_mut()
                                .resource_mut::<Count>()
                                .0 += 2;
                        }
                        app.update();
                    }
                    updates(&mut app, 4);
                    let count = state(&app, area).count();
                    let row = (shape.height)(app.world(), 0);
                    let (shown, want, follows) = match (gap, follow) {
                        (0 | 1, _) => (9_999, 0.0, false),
                        (_, true) => (count - 1, VIEW - row, true),
                        (_, false) => (9_999, VIEW - row, false),
                    };
                    let got = shown_at(&mut app, area, shown);
                    let following = state(&app, area).following();
                    if !near(got, want) || following != follows {
                        failed.push(format!(
                            "{} under {estimate}, {scale}x, follow \
                             {follow}, stream {gap} updates on: row \
                             {shown} at {got:?}, not {want}, \
                             following {following}",
                            shape.name
                        ));
                    }
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[test]
fn a_following_list_that_shrinks_to_its_view_follows() {
    let mut failed = Vec::new();
    let mut cases = 0;
    for to in [5_010, 5_025, 5_030, 4_000, 10] {
        for grow in [false, true] {
            for settled in [true, false] {
                cases += 1;
                let mut app = laid_out_at(1.0);
                app.insert_resource(Follow(true));
                let area = list(&mut app, 10_000, uniform);
                scroll_to(&mut app, area, 5_000);
                updates(&mut app, if settled { 6 } else { 1 });
                app.world_mut().resource_mut::<Count>().0 = to;
                for frame in 0..10 {
                    if grow && frame >= 2 {
                        app.world_mut().resource_mut::<Count>().0 +=
                            3;
                    }
                    app.update();
                }
                let count = state(&app, area).count();
                let at_bottom =
                    to as f32 * 20.0 - VIEW <= 5_000.0 * 20.0;
                let following = state(&app, area).following();
                let (shown, want) = if at_bottom {
                    (
                        count - 1,
                        (VIEW - 20.0).min(count as f32 * 20.0 - 20.0),
                    )
                } else {
                    (5_000, 0.0)
                };
                let got = shown_at(&mut app, area, shown);
                if following != at_bottom || !near(got, want) {
                    failed.push(format!(
                        "to {to}, grow {grow}, settled {settled}: \
                         row {shown} at {got:?}, not {want}, \
                         following {following}"
                    ));
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[test]
fn this_frames_count_is_placed_this_frame() {
    let mut app = laid_out_at(1.0);
    let area = list(&mut app, 10_000, uniform);

    app.world_mut().resource_mut::<Count>().0 = 10_100;
    app.update();

    assert_eq!(
        state(&app, area).row_span(10_099).map(|span| span.top),
        Some(10_099.0 * 20.0)
    );
}

fn wheel(app: &mut App, area: Entity, by: f32) {
    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y += by;
    app.update();
}

#[test]
fn following_stops_on_a_move_up_and_resumes_at_the_bottom() {
    let mut app = laid_out_at(1.0);
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);
    assert!(state(&app, area).following(), "at the bottom");

    wheel(&mut app, area, -0.25);
    settle(&mut app);
    let up = scroll(&app, area);
    assert!(!state(&app, area).following(), "a quarter pixel up");
    app.world_mut().resource_mut::<Count>().0 = 1_100;
    settle(&mut app);
    assert_eq!(scroll(&app, area), up, "growth after leaving");

    app.world_mut().get_mut::<ScrollPosition>(area).unwrap().y =
        1.0e9;
    settle(&mut app);
    app.world_mut().resource_mut::<Count>().0 = 1_200;
    settle(&mut app);
    assert_eq!(shown_at(&mut app, area, 1_199), Some(VIEW - 20.0));

    app.insert_resource(Follow(false));
    settle(&mut app);
    app.insert_resource(Follow(true));
    settle(&mut app);
    assert!(state(&app, area).following(), "on again at its bottom");
    app.world_mut().resource_mut::<Count>().0 = 1_300;
    settle(&mut app);
    assert_eq!(shown_at(&mut app, area, 1_299), Some(VIEW - 20.0));

    app.insert_resource(Follow(false));
    settle(&mut app);
    let at = scroll(&app, area);
    app.world_mut().resource_mut::<Count>().0 = 1_400;
    settle(&mut app);
    assert_eq!(scroll(&app, area), at, "follow switched off");
    assert!(!state(&app, area).following());

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
    let mut app = laid_out_at(1.0);
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
        let mut app = laid_out_at(1.0);
        app.insert_resource(Follow(true));
        let area = list(&mut app, 1_000, uniform);
        scroll_to(&mut app, area, to);
        settle(&mut app);
        for _ in 0..steps {
            wheel(&mut app, area, 40.0);
        }
        if steps == 0 {
            scroll_to(&mut app, area, 990);
        }
        settle(&mut app);
        assert!(state(&app, area).following(), "scroll_to({to})");

        app.world_mut().resource_mut::<Count>().0 = 1_100;
        settle(&mut app);
        assert_eq!(
            shown_at(&mut app, area, 1_099),
            Some(VIEW - 20.0)
        );
    }
}

#[test]
fn reveals_at_the_bottom_keep_a_list_following() {
    let mut app = laid_out_at(1.0);
    app.insert_resource(Follow(true));
    let area = list(&mut app, 1_000, uniform);
    scroll_to(&mut app, area, 999);
    settle(&mut app);

    reveal(&mut app, area, 995);
    assert!(state(&app, area).following(), "scroll_into_view(995)");
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .unwrap()
        .scroll_range_into_view(999, 0.0..5.0);
    settle(&mut app);
    assert!(state(&app, area).following(), "a range of 999");
}

#[test]
fn a_following_list_that_fits_or_is_empty_follows_a_stream() {
    for (from, index) in [(10, Some(5)), (0, Some(0)), (0, None)] {
        let mut app = laid_out_at(1.0);
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

#[test]
fn reveals_of_unmeasured_rows_scroll_the_least_by_the_second_frame() {
    let mut failed = Vec::new();
    let mut cases = 0;
    for estimate in [20.0, 2.0, 17.0, 200.0] {
        for scale in [1.0, 1.25] {
            for height in [2_000.0, 60.0] {
                for from in [9_000, 1_000, 5_100, 4_900] {
                    for follow in [false, true] {
                        cases += 1;
                        let mut app = laid_out_at(scale);
                        app.insert_resource(Follow(follow));
                        app.insert_resource(Grown(5_000, height));
                        let area = mounted(
                            &mut app,
                            10_000,
                            estimate,
                            row(grown),
                            px(VIEW),
                        );
                        settle(&mut app);
                        scroll_to(&mut app, area, from);
                        settle(&mut app);
                        app.world_mut()
                            .get_mut::<VirtualList>(area)
                            .unwrap()
                            .scroll_into_view(5_000);
                        updates(&mut app, 2);
                        let second = scroll(&app, area);
                        settle(&mut app);
                        let span = state(&app, area)
                            .row_span(5_000)
                            .unwrap();
                        let want = if (from > 5_000)
                            == (span.height > VIEW)
                        {
                            span.top + span.height - VIEW
                        } else {
                            span.top
                        };
                        let settled = scroll(&app, area);
                        if (second - want).abs() > 0.01
                            || (settled - want).abs() > 0.01
                        {
                            failed.push(format!(
                                "under {estimate}, {scale}x, \
                                 {height} px, from {from}, follow \
                                 {follow}: second frame {second}, \
                                 settled {settled}, not {want}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert_none(failed, cases);
}

#[test]
fn ranges_of_an_unmeasured_tall_row_scroll_the_least() {
    for measured in [false, true] {
        let mut app = laid_out_at(1.0);
        app.insert_resource(Grown(5_000, 2_000.0));
        let area = list(&mut app, 10_000, grown);
        if measured {
            scroll_to(&mut app, area, 5_000);
            settle(&mut app);
        }
        scroll_to(&mut app, area, 9_000);
        settle(&mut app);
        let range = |app: &mut App, range: core::ops::Range<f32>| {
            app.world_mut()
                .get_mut::<VirtualList>(area)
                .unwrap()
                .scroll_range_into_view(5_000, range);
            updates(app, 2);
            let second = scroll(app, area);
            settle(app);
            assert_eq!(
                second,
                scroll(app, area),
                "measured {measured}"
            );
            second
        };
        let top = 5_000.0 * 20.0;
        assert_eq!(
            range(&mut app, 1_500.0..1_520.0),
            top + 1_500.0,
            "from below, measured {measured}"
        );
        assert_eq!(
            range(&mut app, 1_990.0..2_500.0),
            top + 2_000.0 - VIEW,
            "past its end, measured {measured}"
        );
        scroll_to(&mut app, area, 4_000);
        settle(&mut app);
        assert_eq!(
            range(&mut app, 1_000.0..1_000.0),
            top + 1_000.0 - VIEW,
            "empty, from above, measured {measured}"
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

#[test]
fn rows_sit_on_whole_physical_pixels_at_every_scale() {
    let mut failed = Vec::new();
    for scale in EVERY_SCALE {
        for estimate in [16.3, 17.0, 20.0] {
            let mut app = laid_out_at(scale);
            let area = mounted(
                &mut app,
                10_000,
                estimate,
                row(regions),
                px(VIEW),
            );
            let mut check = |app: &mut App, when: String| {
                if let Some(off) = off_pixels(&physical(app)) {
                    failed.push(format!(
                        "{scale}x, under {estimate}, {when}: {off}"
                    ));
                }
            };
            for frame in 1..=3 {
                app.update();
                check(
                    &mut app,
                    format!("first paint, frame {frame}"),
                );
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
                    check(
                        &mut app,
                        format!("scroll_to({index}), +{k}"),
                    );
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn turning_sideways_off_forgets_the_heights_it_measured() {
    let mut fresh = laid_out_at(1.0);
    let built = wrapping_list(&mut fresh, false);
    let mut app = laid_out_at(1.0);
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

#[test]
fn a_scroll_to_among_rows_that_lay_out_at_nothing_lands() {
    let mut app = laid_out_at(1.0);
    let area = list(&mut app, 10_000, |_, index| {
        if index.is_multiple_of(10) { 0.0 } else { 20.0 }
    });
    scroll_to(&mut app, area, 9_999);
    settle(&mut app);
    let first = state(&app, area).first_visible();
    let at = scroll(&app, area);

    app.world_mut().resource_mut::<Count>().0 = 10_100;
    settle(&mut app);

    assert_eq!(state(&app, area).first_visible(), first);
    assert_eq!(scroll(&app, area), at);
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
    let mut app = laid_out_at(1.0);
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
    let mut app = laid_out_at(1.0);
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
    let mut app = laid_out_at(1.0);
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

    assert!(state(&app, area).following());
    assert_eq!(shown_at(&mut app, area, 1_000), Some(VIEW - 20.0));
}
