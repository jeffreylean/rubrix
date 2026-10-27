//! A list that builds only the rows near its viewport.

use core::ops::{Range, RangeInclusive};
use std::sync::Arc;

use bevy::app::{App, Update};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::Query;
use bevy::ecs::world::World;
use bevy::math::Vec2;
use bevy::ui::{
    ComputedNode, FlexDirection, Node, Overflow, OverflowAxis,
    OverflowClipMargin, PositionType, ScrollPosition, Val, percent,
    px,
};

use crate::prop::{Prop, Signal, each};
use crate::scroll::{self, ScrollGoal};
use crate::tokens::SpacingTokens;
use crate::views::frame::{Frame, FrameProps};
use crate::{AnyView, Bevy, Cx, FynixSystems, Styled, View};

/// How far past each edge of the viewport rows stay built, Pierre's
/// figure. A row is laid out and measured this far before it shows,
/// and a scroll of 782 px in a frame still lands on built rows.
const OVERSCAN: f64 = 1000.0;

/// How near its end a list counts as scrolled to it, in logical px.
/// Bevy rounds the end a wheel can reach to whole physical pixels.
const AT_END: f64 = 1.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        place_rows.in_set(FynixSystems::Prepare).after(scroll::ease),
    );
}

/// A column of `count` rows, of which it builds only the ones within
/// [`OVERSCAN`] of what shows. See [`virtual_list`].
pub struct VirtualListView<R> {
    pub frame: Frame,
    count: Prop<usize>,
    estimate: f32,
    row: R,
}

/// A scroll area of `count` rows, each `estimate` pixels high until
/// it has been laid out, built by `row`.
///
/// A row that scrolls far out of sight is not despawned but shows
/// another row instead, so `row` builds the view of a [`RowSlot`]
/// and binds what it shows to [`RowSlot::index`]. Anything else the
/// row holds, such as hover or focus, stays with the slot.
///
/// The area takes the free space along its parent's direction and
/// can shrink below its rows. Its basis is 0, so in a column a
/// `height` does not size it: a parent of that height does. A row
/// takes the area's width, and the area takes no padding or border.
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
        row,
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
        let mut area = cx.world.entity_mut(list);
        area.insert((
            ScrollGoal::default(),
            VirtualList::new(self.estimate, spacer),
        ));
        // A grown area with its content as its basis grows to fit
        // it, and then has nothing to scroll.
        area.get_mut::<Node>().expect("a frame node").flex_basis =
            px(0.0);
        cx.effect(list, self.count, |world, list, &count| {
            if let Some(mut state) =
                world.get_mut::<VirtualList>(list)
            {
                state.count = count;
            }
        });
        // The slots read where to go as they are built, so the rows
        // are placed before them.
        let mut state =
            cx.world.get_mut::<VirtualList>(list).expect("the list");
        state.place(&[], 0.0, 0.0);
        let height = state.height();
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
}

/// A virtual list's rows: their heights, and which slot shows which.
/// It sits on the list's scroll area.
#[derive(Component)]
pub struct VirtualList {
    estimate: f32,
    count: usize,
    heights: Heights,
    /// The node as high as every row, which is what scrolls.
    spacer: Entity,
    slots: Vec<Placed>,
    spawned: Range<usize>,
    first_visible: usize,
    target: Option<usize>,
    /// Whether the list keeps its end in sight as it changes.
    pinned: bool,
    /// Where the last placement scrolled to.
    placed_at: f64,
}

/// Where one slot is, and the row it shows.
#[derive(Default)]
struct Placed {
    /// `None` until the slot's view is built.
    node: Option<Entity>,
    index: usize,
    top: f32,
}

/// What a placement asks of the scroll position.
struct Scroll {
    to: f32,
    /// Whether it jumps there, dropping where a wheel was heading.
    jump: bool,
}

impl VirtualList {
    fn new(estimate: f32, spacer: Entity) -> Self {
        Self {
            estimate,
            count: 0,
            heights: Heights::default(),
            spacer,
            slots: Vec::new(),
            spawned: 0..0,
            first_visible: 0,
            target: None,
            pinned: false,
            placed_at: 0.0,
        }
    }

    /// Scrolls so that the row `index` is the first to show, at the
    /// next update, or as near as the end of the list lets it.
    pub fn scroll_to(&mut self, index: usize) {
        self.target = Some(index);
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

    fn height(&self) -> Val {
        px(self.heights.total() as f32)
    }

    /// The row `y` px down the list falls in, half a pixel on, so a
    /// row of which only rounding shows does not count.
    fn shown_at(&self, y: f64) -> usize {
        self.heights.index_at(y + 0.5)
    }

    /// Takes the heights the slots were `measured` at, then places
    /// the rows for a viewport `view` px high scrolled `scroll` px
    /// down, keeping the first row that shows where it is, or the end
    /// in sight if it was.
    fn place(
        &mut self,
        measured: &[(usize, f32)],
        view: f32,
        scroll: f32,
    ) -> Scroll {
        let view = f64::from(view);
        let scroll = f64::from(scroll);
        let bottom = self.heights.total() - view;
        // Pinned until a scroll moves up from where it was placed.
        let pinned = bottom > 0.0
            && (scroll >= bottom - AT_END
                || self.pinned && scroll >= self.placed_at - AT_END);
        let mut anchor = self.shown_at(scroll);
        let mut within = scroll - self.heights.offset(anchor);
        for &(slot, height) in measured {
            // A slot not laid out yet measures 0, as does a row of a
            // hidden list and an empty row. Taken, 0 would put every
            // row inside the overscan.
            if height > 0.0
                && let Some(placed) = self.slots.get(slot)
                && placed.index < self.heights.len()
            {
                self.heights.set(placed.index, height);
            }
        }
        if self.heights.len() != self.count {
            self.heights.resize(self.count, self.estimate);
            if anchor >= self.count {
                anchor = self.count.saturating_sub(1);
                within = 0.0;
            }
        }
        let jump = self.target.take().is_some_and(|target| {
            anchor = target.min(self.count.saturating_sub(1));
            within = 0.0;
            true
        });
        let bottom = (self.heights.total() - view).max(0.0);
        let to = if pinned && !jump {
            bottom
        } else {
            (self.heights.offset(anchor) + within).clamp(0.0, bottom)
        };
        self.pinned = pinned && !jump;
        self.placed_at = to;
        self.fill(to, view);
        Scroll {
            to: to as f32,
            jump,
        }
    }

    /// Hands the rows within [`OVERSCAN`] of the viewport to slots,
    /// a slot keeping its row while the row stays within it.
    fn fill(&mut self, scroll: f64, view: f64) {
        let count = self.heights.len();
        // As deep at the top as anywhere, so the first scroll away
        // from it recycles rows rather than building more.
        let top = (scroll - OVERSCAN).max(0.0);
        let first = self.heights.index_at(top);
        let end =
            (self.heights.index_at(top + view + 2.0 * OVERSCAN) + 1)
                .min(count);
        let need = end.saturating_sub(first);
        // Kept past what is needed, unless that is less than half,
        // so a slot is not built and dropped as rows change height.
        let pool = if self.slots.len() > 2 * need {
            need
        } else {
            self.slots.len().max(need)
        }
        .min(count);
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
        for placed in &mut self.slots {
            placed.top = self.heights.offset(placed.index) as f32;
        }
        self.spawned = start..start + pool;
        self.first_visible = self.shown_at(scroll);
    }
}

/// Places every list's rows for where it is scrolled to, after the
/// wheel has moved it and before the rows read what to show.
fn place_rows(
    mut lists: Query<(
        &mut VirtualList,
        &ComputedNode,
        &mut ScrollPosition,
        &mut ScrollGoal,
    )>,
    mut nodes: Query<(&ComputedNode, &mut Node)>,
) {
    for (mut list, area, mut position, mut goal) in &mut lists {
        let view =
            area.content_box().height() * area.inverse_scale_factor();
        let measured = list
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, placed)| {
                let (node, _) = nodes.get(placed.node?).ok()?;
                Some((
                    slot,
                    node.size().y * node.inverse_scale_factor(),
                ))
            })
            .collect::<Vec<_>>();
        let scroll = list.place(&measured, view, position.y);
        if scroll.jump {
            *goal = ScrollGoal::default();
            position.y = scroll.to;
        } else if scroll.to != position.y {
            let delta = Vec2::Y * (scroll.to - position.y);
            goal.shift(&mut position, delta);
        }
        for placed in &list.slots {
            let top = px(placed.top);
            if let Some(slot) = placed.node
                && let Ok((_, mut slot)) = nodes.get_mut(slot)
                && slot.top != top
            {
                slot.top = top;
            }
        }
        let height = list.height();
        if let Ok((_, mut spacer)) = nodes.get_mut(list.spacer)
            && spacer.height != height
        {
            spacer.height = height;
        }
    }
}

/// The node the slots are placed in. Its own height is 0 whatever
/// the list's, so a change to the list's height leaves the slots'
/// layouts as they were.
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
        node
    })
}

/// One slot: a node with no element of its own, so no transition in
/// force can hold it, placed at its row's offset and holding the view
/// `row` builds for it.
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
        let top = cx
            .world
            .get_mut::<VirtualList>(list)
            .and_then(|mut state| {
                let placed = state.slots.get_mut(slot)?;
                placed.node = Some(node);
                Some(placed.top)
            })
            .unwrap_or_default();
        *cx.world.get_mut::<Node>(node).expect("a node") = Node {
            position_type: PositionType::Absolute,
            top: px(top),
            left: px(0.0),
            right: px(0.0),
            flex_direction: FlexDirection::Column,
            ..Node::default()
        };
        cx.under(node, |cx| cx.build(row(RowSlot { list, slot })));
        node
    })
}

/// A signal of what `read` takes from the list `list`, re-read when
/// that value changes. The list changes every frame it scrolls, so a
/// check on its change tick would re-read every slot every frame.
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

/// Row heights in logical pixels, and the offsets they add up to.
#[derive(Default)]
struct Heights {
    each: Vec<f32>,
    /// A Fenwick tree over `each`, 1-based: entry `k` sums the
    /// `lowest(k)` rows that end with row `k - 1`. The sums are f64,
    /// so 100,000 of them do not drift, though a node's top is still
    /// an f32, in steps of 0.125 px past 1,000,000 px.
    tree: Vec<f64>,
}

/// The lowest set bit of `k`.
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

    /// `count` rows, the ones past the old end `estimate` high. An
    /// entry sums only rows before it, so the tree is cut short or
    /// grown one entry at a time.
    fn resize(&mut self, count: usize, estimate: f32) {
        if self.tree.is_empty() {
            self.tree.push(0.0);
        }
        self.each.resize(count, estimate);
        self.tree.truncate(count + 1);
        for k in self.tree.len()..=count {
            let below =
                self.offset(k - 1) - self.offset(k - lowest(k));
            self.tree.push(f64::from(estimate) + below);
        }
    }

    fn set(&mut self, index: usize, height: f32) {
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

    /// The height of the rows before `index`.
    fn offset(&self, index: usize) -> f64 {
        let mut sum = 0.0;
        let mut k = index;
        while k > 0 {
            sum += self.tree[k];
            k &= k - 1;
        }
        sum
    }

    /// The row `y` falls in: row 0 for a `y` above the list, and the
    /// last row for one past its end.
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
