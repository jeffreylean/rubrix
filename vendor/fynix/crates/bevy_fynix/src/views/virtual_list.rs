//! A list that builds only the rows near its viewport.

use core::ops::{Range, RangeInclusive};
use std::sync::Arc;

use bevy::app::{App, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::Query;
use bevy::ecs::world::World;
use bevy::math::{BVec2, Vec2};
use bevy::ui::{
    ComputedNode, Display, FlexDirection, IgnoreScroll, Node,
    Overflow, OverflowAxis, OverflowClipMargin, PositionType,
    ScrollPosition, UiTransform, Val, Val2, percent, px,
};

use crate::prop::{Prop, Signal, each};
use crate::scroll::{self, ScrollGoal};
use crate::tokens::SpacingTokens;
use crate::views::frame::{Frame, FrameProps};
use crate::{AnyView, Bevy, Cx, FynixSystems, Styled, View};

const OVERSCAN: f64 = 1000.0;

/// How much of a row, in logical px, has to show for it to count as
/// showing, so that a row only rounding shows does not.
const SHOWN: f64 = 0.5;

const MAX_SLOTS: usize = 199;

const ESTIMATE_TOLERANCE: core::ops::RangeInclusive<f32> = 0.8..=1.25;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        place_rows.in_set(FynixSystems::Prepare).after(scroll::ease),
    );
}

/// A column of `count` rows, of which it builds only the ones near
/// what shows. See [`virtual_list`].
pub struct VirtualListView<R> {
    pub frame: Frame,
    count: Prop<usize>,
    estimate: f32,
    sideways: Prop<bool>,
    follow: Prop<bool>,
    row: R,
}

/// A scroll area of `count` rows, each `estimate` pixels high until
/// it has been laid out, built by `row`. Once the median height of
/// the rows laid out in a frame is below 0.8 or above 1.25 times
/// `estimate`, the median takes its place, so a few rows much taller
/// than the rest leave it as it is.
///
/// Unless the view [follows the
/// bottom](VirtualListView::follow_bottom), rows added keep its first
/// visible row in place, so a list that grows from no rows shows its
/// first.
///
/// A row that scrolls far out of sight is not despawned but shows
/// another row instead, so `row` builds the view of a [`RowSlot`]
/// and binds what it shows to [`RowSlot::index`]. Anything else the
/// row holds, such as hover or focus, stays with the slot.
///
/// A row is clipped to the height it is known to have, so on the
/// frame a row first shows, before it is laid out, it shows only as
/// much of itself as its estimate. Each row's top and bottom sit on
/// whole physical pixels, so rows meet without overlapping at any
/// scale factor.
///
/// The area takes the free space along its parent's direction and
/// can shrink below its rows. Its basis is 0, so in a column a
/// `height` does not size it: a parent of that height does. A row
/// takes the area's width, and the area takes no padding or border.
///
/// # Limits
///
/// - It builds at most 199 rows, cutting the 1,000 px of overscan on
///   each side first. A view more than 199 rows high shows a blank
///   band below them.
/// - Positions are `f32` pixels, as in Bevy's layout. Past 2^23 px,
///   about 440,000 rows of 19 px, a
///   [`scroll_to`](VirtualList::scroll_to) of the last row leaves it
///   just below the view. Past 2^24 px, about 880,000 rows of 19 px,
///   seams of 2 to 4 px also show between rows. 100,000 rows of 19 px
///   and 1,000,000 rows of 20 px show neither.
pub fn virtual_list<R>(
    count: impl Into<Prop<usize>>,
    estimate: f32,
    row: R,
) -> VirtualListView<R> {
    assert!(
        estimate.is_finite() && estimate > 0.0,
        "a row estimate of {estimate} px"
    );
    VirtualListView {
        frame: Frame::unset()
            .direction(FlexDirection::Column)
            .overflow(Overflow {
                x: OverflowAxis::Clip,
                y: OverflowAxis::Scroll,
            })
            .clip_margin(OverflowClipMargin::content_box())
            .min_width(px(0.0))
            .min_height(px(0.0))
            .grow(1.0),
        count: count.into(),
        estimate,
        sideways: Prop::Value(false),
        follow: Prop::Value(false),
        row,
    }
}

impl<R> VirtualListView<R> {
    /// Lets the list scroll sideways while `on` holds, by one offset
    /// for every row and with one bar. Only the [`RowSlot::sideways`]
    /// cells of a row move. The list scrolls as far as the content of
    /// the widest such cell of any row laid out so far overflows its
    /// cell, so a wide row that scrolls out of sight keeps the range.
    /// A change in the area's width forgets the rows out of sight
    /// until they show again.
    ///
    /// Turning it off or on keeps the first visible row in place and
    /// forgets how far rows overflow and the heights they were laid
    /// out at, which wrapping changes.
    pub fn sideways(mut self, on: impl Into<Prop<bool>>) -> Self {
        self.sideways = on.into();
        self
    }

    /// Keeps the view at the bottom while `on` holds and the view is
    /// at the bottom, as rows are added or grow, including while
    /// every row fits the view. A move that leaves the view above the
    /// bottom stops it: a scroll up by the user, or a
    /// [`scroll_to`](VirtualList::scroll_to),
    /// [`scroll_into_view`](VirtualList::scroll_into_view) or
    /// [`scroll_range_into_view`](VirtualList::scroll_range_into_view)
    /// that ends above the bottom. A move that ends at the bottom
    /// starts it again.
    pub fn follow_bottom(
        mut self,
        on: impl Into<Prop<bool>>,
    ) -> Self {
        self.follow = on.into();
        self
    }
}

impl<R> FrameProps for VirtualListView<R> {
    fn frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }
}

impl<T, R> View<Bevy, T> for VirtualListView<R>
where
    T: SpacingTokens + Send + Sync + 'static,
    R: Fn(RowSlot) -> AnyView<Bevy, T> + Send + Sync + 'static,
{
    fn build(self, cx: &mut Cx<'_, Bevy, T>) -> Entity {
        let list = cx.build(self.frame);
        let spacer = cx.under(list, Cx::spawn);
        let count = self.count.get(cx.world).unwrap_or(0);
        let state = VirtualList::new(self.estimate, count, spacer);
        let height = state.height();
        let mut area = cx.world.entity_mut(list);
        area.insert((ScrollGoal::default(), state));
        area.get_mut::<Node>().expect("a frame node").flex_basis =
            px(0.0);
        cx.effect(list, self.count, |world, list, &count| {
            if let Some(mut state) =
                world.get_mut::<VirtualList>(list)
            {
                state.count = count;
            }
        });
        cx.effect(list, self.follow, |world, list, &on| {
            if let Some(mut state) =
                world.get_mut::<VirtualList>(list)
            {
                state.follow = on;
            }
        });
        cx.effect(list, self.sideways, |world, list, &on| {
            let mut area = world.entity_mut(list);
            if let Some(mut state) = area.get_mut::<VirtualList>()
                && state.widths.is_some() != on
            {
                state.widths = on.then(Widths::default);
                state.sideways_switched = true;
            }
            if let Some(mut node) = area.get_mut::<Node>() {
                node.overflow.x = if on {
                    OverflowAxis::Scroll
                } else {
                    OverflowAxis::Clip
                };
            }
        });
        *cx.world.get_mut::<Node>(spacer).expect("a node") = Node {
            width: percent(100.0),
            height,
            flex_shrink: 0.0,
            ..Node::default()
        };
        let row = Arc::new(self.row);
        let slots =
            follow(list, |state| state.map_or(0, |s| s.slots.len()))
                .map(|len| (0..len).collect::<Vec<_>>());
        cx.under(list, |cx| {
            cx.build(
                each(
                    slots,
                    |&slot| slot,
                    move |&slot| slot_view(list, slot, row.clone()),
                )
                .within(track()),
            )
        });
        list
    }
}

/// The slot of a [`virtual_list`] that one row view is built in.
/// Which row it shows changes as the list scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSlot {
    list: Entity,
    slot: usize,
}

impl RowSlot {
    /// The index of the row the slot shows. It is re-read only when
    /// the slot moves to another row, so a slot that keeps its row
    /// reads and writes nothing.
    pub fn index(self) -> Signal<usize> {
        follow(self.list, move |state| {
            state
                .and_then(|s| s.slots.get(self.slot))
                .map_or(0, |placed| placed.index)
        })
    }

    /// A cell of the row that clips `content` to the room the row
    /// leaves it. While the list scrolls
    /// [sideways](VirtualListView::sideways), `content` keeps its own
    /// width and moves with the list's sideways offset, and the rest
    /// of the row stays. Otherwise `content` takes the cell's width,
    /// so text that wraps wraps inside it.
    pub fn sideways<T: 'static>(
        self,
        content: AnyView<Bevy, T>,
    ) -> AnyView<Bevy, T> {
        AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
            let cell = cx.spawn();
            *cx.world.get_mut::<Node>(cell).expect("a node") = Node {
                flex_grow: 1.0,
                flex_basis: px(0.0),
                min_width: px(0.0),
                overflow: Overflow::clip_x(),
                ..Node::default()
            };
            let sideways = cx
                .world
                .get::<VirtualList>(self.list)
                .is_some_and(|list| list.widths.is_some());
            let moved = cx.under(cell, Cx::spawn);
            let mut node = cx.world.entity_mut(moved);
            node.insert(SidewaysContent(self));
            fit(
                &mut node.get_mut::<Node>().expect("a node"),
                sideways,
            );
            cx.under(moved, |cx| cx.build(content));
            cell
        })
    }
}

#[derive(Component)]
struct SidewaysContent(RowSlot);

fn fit(node: &mut Node, sideways: bool) {
    let (width, shrink) = if sideways {
        (Val::Auto, 0.0)
    } else {
        (percent(100.0), 1.0)
    };
    if node.width != width || node.flex_shrink != shrink {
        node.width = width;
        node.flex_shrink = shrink;
    }
}

/// Where a row sits in its list's content, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowSpan {
    /// From the top of the first row.
    pub top: f32,
    /// The row's height, or its estimate until it is laid out.
    pub height: f32,
}

/// A virtual list's rows: their heights, and which slot shows which.
/// It sits on the list's scroll area.
#[derive(Component)]
pub struct VirtualList {
    estimate: f32,
    count: usize,
    heights: Heights,
    spacer: Entity,
    slots: Vec<Placed>,
    spawned: Range<usize>,
    first_visible: usize,
    target: Option<Target>,
    follow: bool,
    pin: Pin,
    placed_at: f32,
    widths: Option<Widths>,
    sideways_switched: bool,
    /// The area's width when its rows were measured.
    across: f32,
    physical_per_logical: f64,
}

#[derive(Clone, Copy)]
enum Target {
    First {
        index: usize,
        remeasured: bool,
    },
    Reveal {
        index: usize,
        from_top: (f32, f32),
        anchor_before: Option<(usize, f64)>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pin {
    AtBottom,
    Unpinned,
    HeldByTarget,
}

#[derive(Default)]
struct Placed {
    node: Option<Entity>,
    index: usize,
    top: f32,
    height: f32,
}

struct Scroll {
    to: f32,
    jump: bool,
}

impl VirtualList {
    fn new(estimate: f32, count: usize, spacer: Entity) -> Self {
        let mut list = Self {
            estimate,
            count,
            heights: Heights::default(),
            spacer,
            slots: Vec::new(),
            spawned: 0..0,
            first_visible: 0,
            target: None,
            follow: false,
            pin: Pin::Unpinned,
            placed_at: 0.0,
            widths: None,
            sideways_switched: false,
            across: 0.0,
            physical_per_logical: 1.0,
        };
        list.place(&mut [], 0.0, 0.0, 0.0);
        list
    }

    /// Scrolls so that the row `index` is the first to show, at the
    /// next update, or as near as the end of the list lets it. Near
    /// the end, it lands again a frame later, once the rows it shows
    /// are measured.
    pub fn scroll_to(&mut self, index: usize) {
        self.target = Some(Target::First {
            index,
            remeasured: false,
        });
    }

    /// Scrolls as little as shows all of the row `index`, at the next
    /// update. A row higher than the view scrolls as little as fills
    /// the view with it, so a view already inside it does not move.
    pub fn scroll_into_view(&mut self, index: usize) {
        self.scroll_range_into_view(index, 0.0..f32::INFINITY);
    }

    /// Scrolls as little as shows `range` of the row `index`, in
    /// pixels from the row's top, at the next update, the same way
    /// as [`scroll_into_view`](Self::scroll_into_view) shows a whole
    /// row. `range` is cut to the row's height.
    pub fn scroll_range_into_view(
        &mut self,
        index: usize,
        range: Range<f32>,
    ) {
        self.target = Some(Target::Reveal {
            index,
            from_top: (range.start, range.end),
            anchor_before: None,
        });
    }

    /// The row at `y` in the list's content, or the nearest row when
    /// `y` is above the first or below the last. `None` while there
    /// are no rows. A point's content `y` is how far below the top of
    /// the area's content box it is, plus the area's
    /// [`ScrollPosition`].
    pub fn row_at(&self, y: f32) -> Option<usize> {
        (self.heights.len() > 0)
            .then(|| self.heights.index_at(f64::from(y.max(0.0))))
    }

    /// Where the row `index` sits in the list's content, `None` past
    /// the last row.
    pub fn row_span(&self, index: usize) -> Option<RowSpan> {
        (index < self.heights.len()).then(|| RowSpan {
            top: self.heights.offset(index) as f32,
            height: self.heights.each[index],
        })
    }

    /// The first row more than half a pixel of which shows.
    pub fn first_visible(&self) -> usize {
        self.first_visible
    }

    /// The rows built, `None` while there are none.
    pub fn spawned(&self) -> Option<RangeInclusive<usize>> {
        (!self.spawned.is_empty())
            .then(|| self.spawned.start..=self.spawned.end - 1)
    }

    /// How many row slots are built.
    pub fn rows(&self) -> usize {
        self.slots.len()
    }

    /// How many rows the list has.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Whether the view follows the bottom as rows are added, as
    /// [`follow_bottom`](VirtualListView::follow_bottom) describes.
    pub fn following(&self) -> bool {
        self.follow && self.pin == Pin::AtBottom
    }

    /// Rows out of sight cannot be measured again, so a change in
    /// the area's width leaves each with the height it had only as an
    /// estimate, until it shows or the median moves it.
    fn measured_across(&mut self, across: f32) {
        if self.across != across {
            self.across = across;
            self.heights.forget();
            if let Some(widths) = &mut self.widths {
                widths.forget();
            }
        }
    }

    fn height(&self) -> Val {
        px(self.heights.total() as f32)
    }

    fn shown_at(&self, y: f64) -> usize {
        self.heights.index_at(y + SHOWN)
    }

    fn place(
        &mut self,
        mut measured: &mut [(usize, f32)],
        view: f32,
        scroll: f32,
        end: f32,
    ) -> Scroll {
        let view = f64::from(view);
        let held =
            self.pin == Pin::HeldByTarget && scroll == self.placed_at;
        let pinned = self.follow
            && !held
            && if view > 0.0 {
                scroll >= end
                    || self.pin == Pin::AtBottom
                        && scroll >= self.placed_at
            } else {
                self.pin == Pin::AtBottom
            };
        let scroll = f64::from(scroll);
        let mut anchor = self.shown_at(scroll);
        let mut within = scroll - self.heights.offset(anchor);
        if core::mem::take(&mut self.sideways_switched) {
            measured = &mut [];
            self.heights.forget();
            self.heights.reestimate(self.estimate);
        }
        for &(index, height) in &*measured {
            self.heights.set(index, height);
        }
        if let Some(typical) = lower_median(measured)
            && !ESTIMATE_TOLERANCE
                .contains(&(typical / self.estimate))
        {
            self.estimate = typical;
            self.heights.reestimate(typical);
        }
        if let Some(widths) = &mut self.widths {
            widths.resize(self.count);
        }
        if self.heights.len() != self.count {
            self.heights.resize(self.count, self.estimate);
            if anchor >= self.count {
                anchor = self.count.saturating_sub(1);
                within = 0.0;
            }
        }
        let bottom = (self.heights.total() - view).max(0.0);
        let at =
            (self.heights.offset(anchor) + within).clamp(0.0, bottom);
        let last = self.count.saturating_sub(1);
        let target = self.target.take();
        let to = match target {
            None if pinned => bottom,
            None => at,
            Some(Target::First { index, remeasured }) => {
                let top = self.heights.offset(index.min(last));
                if !remeasured && top > bottom {
                    self.target = Some(Target::First {
                        index,
                        remeasured: true,
                    });
                }
                top
            }
            Some(Target::Reveal { .. }) if self.count == 0 => 0.0,
            Some(Target::Reveal {
                index,
                from_top,
                anchor_before,
            }) => {
                let index = index.min(last);
                let (row, into) =
                    anchor_before.unwrap_or((anchor, within));
                if anchor_before.is_none()
                    && !self.heights.known[index]
                {
                    self.target = Some(Target::Reveal {
                        index,
                        from_top,
                        anchor_before: Some((row, into)),
                    });
                }
                let at = (self.heights.offset(row.min(self.count))
                    + into)
                    .clamp(0.0, bottom);
                let top = self.heights.offset(index);
                let height = f64::from(self.heights.each[index]);
                let from = f64::from(from_top.0).clamp(0.0, height);
                let to = f64::from(from_top.1).clamp(from, height);
                let (lo, hi) = (top + from, top + to - view);
                at.clamp(lo.min(hi), lo.max(hi))
            }
        }
        .clamp(0.0, bottom);
        let scrolled_to_bottom = at as f32 >= bottom as f32;
        self.pin = match target {
            None if pinned => Pin::AtBottom,
            None if held && self.follow && scrolled_to_bottom => {
                Pin::AtBottom
            }
            None => Pin::Unpinned,
            Some(_) => Pin::HeldByTarget,
        };
        let jump = target.is_some() && to != at;
        self.fill(to, view);
        Scroll {
            to: to as f32,
            jump,
        }
    }

    fn fill(&mut self, scroll: f64, view: f64) {
        let count = self.heights.len();
        let top = (scroll - OVERSCAN).max(0.0);
        let mut first = self.heights.index_at(top);
        let mut end =
            (self.heights.index_at(top + view + 2.0 * OVERSCAN) + 1)
                .min(count);
        if end.saturating_sub(first) > MAX_SLOTS {
            let shown = self.heights.index_at(scroll);
            let below = self.heights.index_at(scroll + view) + 1;
            let spare = MAX_SLOTS.saturating_sub(below - shown);
            first = first.max(shown.saturating_sub(spare / 2));
            end = end.min(first + MAX_SLOTS);
            first = first.max(end.saturating_sub(MAX_SLOTS));
        }
        let need = end.saturating_sub(first);
        let pool = if self.slots.len() > 2 * need {
            need
        } else {
            self.slots.len().max(need)
        }
        .min(count)
        .min(MAX_SLOTS);
        let start = first.min(count - pool);
        self.slots.truncate(pool);
        let mut shown = vec![false; pool];
        let mut free = Vec::new();
        for (slot, placed) in self.slots.iter().enumerate() {
            match placed.index.checked_sub(start) {
                Some(at) if at < pool && !shown[at] => {
                    shown[at] = true
                }
                _ => free.push(slot),
            }
        }
        free.extend(self.slots.len()..pool);
        self.slots.resize_with(pool, Placed::default);
        let missing = (start..start + pool)
            .filter(|&index| !shown[index - start]);
        for (slot, index) in free.into_iter().zip(missing) {
            self.slots[slot].index = index;
        }
        let whole = |y: f64| {
            (y * self.physical_per_logical).round()
                / self.physical_per_logical
        };
        for placed in &mut self.slots {
            let top = whole(self.heights.offset(placed.index));
            let bottom = whole(self.heights.offset(placed.index + 1));
            placed.top = top as f32;
            placed.height = (bottom - top) as f32;
        }
        self.spawned = start..start + pool;
        self.first_visible = self.shown_at(scroll);
    }
}

fn place_rows(
    mut lists: Query<(
        Entity,
        &mut VirtualList,
        &ComputedNode,
        &mut ScrollPosition,
        &mut ScrollGoal,
    )>,
    mut nodes: Query<(&ComputedNode, &mut Node)>,
    mut cells: Query<(
        Entity,
        &SidewaysContent,
        &ChildOf,
        &ComputedNode,
        &mut UiTransform,
    )>,
) {
    for (entity, mut list, area, mut position, mut goal) in &mut lists
    {
        let scale = area.inverse_scale_factor();
        if scale > 0.0 {
            list.physical_per_logical = f64::from(scale.recip());
        }
        let view = area.content_box().height() * scale;
        let wide = list.widths.is_some().then(|| {
            overflows(entity, &list.slots, &cells, &nodes, scale)
        });
        let mut measured = list
            .slots
            .iter()
            .filter_map(|placed| {
                let (node, _) = nodes.get(placed.node?).ok()?;
                let height = node.content_size().y
                    * node.inverse_scale_factor();
                (height > 0.0).then_some((placed.index, height))
            })
            .collect::<Vec<_>>();
        let across = area.content_box().width() * scale;
        list.measured_across(across);
        let end = scroll::overflow(area).y;
        let scroll = list.place(&mut measured, view, position.y, end);
        if scroll.jump {
            *goal = ScrollGoal::default();
            position.y = scroll.to;
        } else if scroll.to != position.y {
            let delta = Vec2::Y * (scroll.to - position.y);
            goal.shift(&mut position, delta);
        }
        list.placed_at = position.y;
        for placed in &list.slots {
            let (top, height) = (px(placed.top), px(placed.height));
            if let Some(slot) = placed.node
                && let Ok((_, mut slot)) = nodes.get_mut(slot)
                && (slot.top != top || slot.height != height)
            {
                slot.top = top;
                slot.height = height;
            }
        }
        let mut width = percent(100.0);
        let mut x = 0.0;
        if let Some(widths) = &mut list.widths {
            for (index, by) in wide.into_iter().flatten() {
                widths.set(index, by);
            }
            x = position.x.clamp(0.0, widths.widest);
            width = px(across + widths.widest);
        }
        if position.x != x {
            position.x = x;
        }
        let shift = Val2::px(-x, 0.0);
        let sideways = list.widths.is_some();
        for (moved, cell, _, _, mut transform) in &mut cells {
            if cell.0.list != entity {
                continue;
            }
            if transform.translation != shift {
                transform.translation = shift;
            }
            if let Ok((_, mut node)) = nodes.get_mut(moved) {
                fit(&mut node, sideways);
            }
        }
        let height = list.height();
        if let Ok((_, mut spacer)) = nodes.get_mut(list.spacer)
            && (spacer.height != height || spacer.width != width)
        {
            spacer.height = height;
            spacer.width = width;
        }
    }
}

fn overflows(
    list: Entity,
    slots: &[Placed],
    cells: &Query<(
        Entity,
        &SidewaysContent,
        &ChildOf,
        &ComputedNode,
        &mut UiTransform,
    )>,
    nodes: &Query<(&ComputedNode, &mut Node)>,
    scale: f32,
) -> Vec<(usize, f32)> {
    let mut wide = vec![None::<f32>; slots.len()];
    for (_, cell, room, content, _) in cells {
        let RowSlot { list: of, slot } = cell.0;
        let Ok((room, _)) = nodes.get(room.parent()) else {
            continue;
        };
        if of != list || room.size().x <= 0.0 {
            continue;
        }
        let by = (content.size().x - room.content_box().width())
            .max(0.0)
            * scale;
        if let Some(most) = wide.get_mut(slot) {
            *most = Some(most.map_or(by, |most| most.max(by)));
        }
    }
    wide.into_iter()
        .zip(slots)
        .filter_map(|(by, placed)| Some((placed.index, by?)))
        .collect()
}

fn track<T: 'static>() -> AnyView<Bevy, T> {
    AnyView::new(|cx: &mut Cx<'_, Bevy, T>| {
        let node = cx.spawn();
        *cx.world.get_mut::<Node>(node).expect("a node") = Node {
            position_type: PositionType::Absolute,
            top: px(0.0),
            left: px(0.0),
            right: px(0.0),
            ..Node::default()
        };
        cx.world
            .entity_mut(node)
            .insert(IgnoreScroll(BVec2 { x: true, y: false }));
        node
    })
}

fn slot_view<T, R>(
    list: Entity,
    slot: usize,
    row: Arc<R>,
) -> AnyView<Bevy, T>
where
    T: 'static,
    R: Fn(RowSlot) -> AnyView<Bevy, T> + Send + Sync + 'static,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let node = cx.spawn();
        let mut state =
            cx.world.get_mut::<VirtualList>(list).expect("the list");
        let placed = &mut state.slots[slot];
        placed.node = Some(node);
        let (top, height) = (placed.top, placed.height);
        // In a block its row keeps its height whatever the slot's,
        // and that is what the slot measures.
        *cx.world.get_mut::<Node>(node).expect("a node") = Node {
            display: Display::Block,
            position_type: PositionType::Absolute,
            top: px(top),
            left: px(0.0),
            right: px(0.0),
            height: px(height),
            overflow: Overflow::clip(),
            ..Node::default()
        };
        cx.under(node, |cx| cx.build(row(RowSlot { list, slot })));
        node
    })
}

fn follow<T>(
    list: Entity,
    read: impl Fn(Option<&VirtualList>) -> T + Send + Sync + 'static,
) -> Signal<T>
where
    T: PartialEq + Send + Sync + 'static,
{
    let read = Arc::new(move |world: &World| {
        read(world.get::<VirtualList>(list))
    });
    let check = read.clone();
    let mut seen = None;
    Signal::new(
        move |world| read(world),
        move |world| {
            let now = check(world);
            let changed = seen.as_ref() != Some(&now);
            seen = Some(now);
            changed
        },
    )
}

#[derive(Default)]
struct Widths {
    each: Vec<f32>,
    widest: f32,
}

impl Widths {
    fn forget(&mut self) {
        self.each.fill(0.0);
        self.widest = 0.0;
    }

    fn set(&mut self, index: usize, by: f32) {
        let Some(at) = self.each.get_mut(index) else {
            return;
        };
        let was = core::mem::replace(at, by);
        if by >= self.widest {
            self.widest = by;
        } else if was == self.widest {
            self.widest = self.most();
        }
    }

    fn resize(&mut self, count: usize) {
        let shrinks = count < self.each.len();
        self.each.resize(count, 0.0);
        if shrinks {
            self.widest = self.most();
        }
    }

    fn most(&self) -> f32 {
        self.each.iter().copied().fold(0.0, f32::max)
    }
}

fn lower_median(heights: &mut [(usize, f32)]) -> Option<f32> {
    let middle = heights.len().checked_sub(1)? / 2;
    let (_, &mut (_, height), _) = heights
        .select_nth_unstable_by(middle, |a, b| a.1.total_cmp(&b.1));
    Some(height)
}

#[derive(Default)]
struct Heights {
    each: Vec<f32>,
    known: Vec<bool>,
    tree: Vec<f64>,
}

fn lowest(k: usize) -> usize {
    k & k.wrapping_neg()
}

impl Heights {
    fn len(&self) -> usize {
        self.each.len()
    }

    fn total(&self) -> f64 {
        self.offset(self.len())
    }

    fn resize(&mut self, count: usize, estimate: f32) {
        if self.tree.is_empty() {
            self.tree.push(0.0);
        }
        self.known.resize(count, false);
        self.each.resize(count, estimate);
        self.tree.truncate(count + 1);
        for k in self.tree.len()..=count {
            let below =
                self.offset(k - 1) - self.offset(k - lowest(k));
            self.tree.push(f64::from(estimate) + below);
        }
    }

    fn set(&mut self, index: usize, height: f32) {
        self.known[index] = true;
        if self.each[index] == height {
            return;
        }
        let by = f64::from(height) - f64::from(self.each[index]);
        self.each[index] = height;
        let mut k = index + 1;
        while k < self.tree.len() {
            self.tree[k] += by;
            k += lowest(k);
        }
    }

    fn forget(&mut self) {
        self.known.fill(false);
    }

    fn reestimate(&mut self, estimate: f32) {
        for (height, &known) in self.each.iter_mut().zip(&self.known)
        {
            if !known {
                *height = estimate;
            }
        }
        let len = self.len();
        self.tree.truncate(1);
        self.tree.extend(self.each.iter().map(|&h| f64::from(h)));
        for k in 1..=len {
            let up = k + lowest(k);
            if up <= len {
                self.tree[up] += self.tree[k];
            }
        }
    }

    fn offset(&self, index: usize) -> f64 {
        let mut sum = 0.0;
        let mut k = index;
        while k > 0 {
            sum += self.tree[k];
            k &= k - 1;
        }
        sum
    }

    fn index_at(&self, y: f64) -> usize {
        let len = self.len();
        let mut at = 0;
        let mut left = y;
        let mut step = len.checked_ilog2().map_or(0, |log| 1 << log);
        while step > 0 {
            let next = at + step;
            if next <= len && self.tree[next] <= left {
                at = next;
                left -= self.tree[next];
            }
            step >>= 1;
        }
        at.min(len.saturating_sub(1))
    }
}
