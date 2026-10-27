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

/// How near its end a list counts as scrolled to it, in logical px.
/// Bevy rounds the end a wheel can reach to whole physical pixels.
const AT_END: f64 = 1.0;

/// How much of a row, in logical px, has to show for it to count as
/// showing, so that a row only rounding shows does not.
const SHOWN: f64 = 0.5;

/// The most row slots a list builds. When the overscan holds more
/// rows, it is cut on both sides of the view.
const MOST: usize = 199;

/// How far, as a ratio, the mean laid-out row height may drift from
/// the estimate before it takes the estimate's place.
const DRIFT: core::ops::RangeInclusive<f32> = 0.8..=1.25;

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
    sideways: bool,
    row: R,
}

/// A scroll area of `count` rows, each `estimate` pixels high until
/// it has been laid out, built by `row`. Once the mean height of the
/// rows laid out is more than a quarter off `estimate`, the mean
/// takes its place.
///
/// A row that scrolls far out of sight is not despawned but shows
/// another row instead, so `row` builds the view of a [`RowSlot`]
/// and binds what it shows to [`RowSlot::index`]. Anything else the
/// row holds, such as hover or focus, stays with the slot.
///
/// A row is clipped to the height it is known to have, so on the
/// frame a row first shows, before it is laid out, it shows only as
/// much of itself as its estimate.
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
/// - Positions are `f32` pixels, as in Bevy's layout. Past 2^24 px,
///   about 880,000 rows of 19 px, the gaps between rows round to
///   whole pixels, and seams of up to 2 px show. 100,000 rows of 19
///   px show none.
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
        sideways: false,
        row,
    }
}

impl<R> VirtualListView<R> {
    /// Lets the list scroll sideways, by one offset for every row and
    /// with one bar. Only the [`RowSlot::sideways`] cells of a row
    /// move. The list scrolls as far as the content of the widest
    /// such cell of any row laid out so far overflows its cell, so a
    /// wide row that scrolls out of sight keeps the range.
    pub fn sideways(mut self) -> Self {
        self.sideways = true;
        self.frame = self.frame.overflow(Overflow::scroll());
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
        let mut state =
            VirtualList::new(self.estimate, count, spacer);
        if self.sideways {
            state.widths = Some(Widths::default());
        }
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
    /// leaves it, while `content` keeps its own width. In a list that
    /// scrolls [sideways](VirtualListView::sideways), `content` moves
    /// with the list's sideways offset and the rest of the row stays.
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
            let moved = cx.under(cell, Cx::spawn);
            let mut node = cx.world.entity_mut(moved);
            node.insert(Sideways(self));
            node.get_mut::<Node>().expect("a node").flex_shrink = 0.0;
            cx.under(moved, |cx| cx.build(content));
            cell
        })
    }
}

/// The content of a [`RowSlot::sideways`] cell.
#[derive(Component)]
struct Sideways(RowSlot);

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
    pinned: bool,
    placed_at: f64,
    widths: Option<Widths>,
}

#[derive(Clone, Copy)]
enum Target {
    /// The row to show first.
    First(usize),
    /// The row to show whole, and whether it has been placed at its
    /// estimate already.
    Whole { index: usize, again: bool },
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
            pinned: false,
            placed_at: 0.0,
            widths: None,
        };
        list.place(&[], 0.0, 0.0);
        list
    }

    /// Scrolls so that the row `index` is the first to show, at the
    /// next update, or as near as the end of the list lets it.
    pub fn scroll_to(&mut self, index: usize) {
        self.target = Some(Target::First(index));
    }

    /// Scrolls as little as shows all of the row `index`, at the next
    /// update, or its top when it is higher than the view. A row in
    /// view does not move.
    pub fn scroll_into_view(&mut self, index: usize) {
        self.target = Some(Target::Whole {
            index,
            again: false,
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

    fn height(&self) -> Val {
        px(self.heights.total() as f32)
    }

    fn shown_at(&self, y: f64) -> usize {
        self.heights.index_at(y + SHOWN)
    }

    fn place(
        &mut self,
        measured: &[(usize, f32)],
        view: f32,
        scroll: f32,
    ) -> Scroll {
        let view = f64::from(view);
        let scroll = f64::from(scroll);
        // A view of no height has not been laid out, or is hidden,
        // so it says nothing about where the user left it.
        let pinned = if view > 0.0 {
            let bottom = self.heights.total() - view;
            scroll >= bottom - AT_END
                || self.pinned && scroll >= self.placed_at - AT_END
        } else {
            self.pinned
        };
        let mut anchor = self.shown_at(scroll);
        let mut within = scroll - self.heights.offset(anchor);
        for &(index, height) in measured {
            // A slot not laid out yet measures 0, as does a row of a
            // hidden list and an empty row. Taken, 0 would put every
            // row inside the overscan.
            if height > 0.0 {
                self.heights.set(index, height);
            }
        }
        if let Some(mean) = self.heights.mean()
            && !DRIFT.contains(&(mean / self.estimate))
        {
            self.estimate = mean;
            self.heights.reestimate(mean);
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
            Some(Target::First(index)) => {
                self.heights.offset(index.min(last))
            }
            Some(Target::Whole { .. }) if self.count == 0 => 0.0,
            Some(Target::Whole { index, again }) => {
                let index = index.min(last);
                if !again && !self.heights.known[index] {
                    self.target =
                        Some(Target::Whole { index, again: true });
                }
                let top = self.heights.offset(index);
                let end = self.heights.offset(index + 1);
                if top < at || end - top > view {
                    top
                } else {
                    at.max(end - view)
                }
            }
        }
        .clamp(0.0, bottom);
        let jump = target.is_some() && to != at;
        self.pinned = pinned && !jump;
        self.placed_at = to;
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
        if end.saturating_sub(first) > MOST {
            let shown = self.heights.index_at(scroll);
            let below = self.heights.index_at(scroll + view) + 1;
            let spare = MOST.saturating_sub(below - shown);
            first = first.max(shown.saturating_sub(spare / 2));
            end = end.min(first + MOST);
            first = first.max(end.saturating_sub(MOST));
        }
        let need = end.saturating_sub(first);
        let pool = if self.slots.len() > 2 * need {
            need
        } else {
            self.slots.len().max(need)
        }
        .min(count)
        .min(MOST);
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
            placed.height = self.heights.each[placed.index];
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
        &Sideways,
        &ChildOf,
        &ComputedNode,
        &mut UiTransform,
    )>,
) {
    for (entity, mut list, area, mut position, mut goal) in &mut lists
    {
        let scale = area.inverse_scale_factor();
        let view = area.content_box().height() * scale;
        let wide = list.widths.is_some().then(|| {
            overflows(entity, &list.slots, &cells, &nodes, scale)
        });
        let measured = list
            .slots
            .iter()
            .filter_map(|placed| {
                let (node, _) = nodes.get(placed.node?).ok()?;
                let height = node.content_size().y
                    * node.inverse_scale_factor();
                Some((placed.index, height))
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
        if let Some(widths) = &mut list.widths {
            for (index, by) in wide.into_iter().flatten() {
                widths.set(index, by);
            }
            position.x = position.x.clamp(0.0, widths.widest);
            width =
                px(area.content_box().width() * scale
                    + widths.widest);
            let shift = Val2::px(-position.x, 0.0);
            for (cell, _, _, mut moved) in &mut cells {
                if cell.0.list == entity && moved.translation != shift
                {
                    moved.translation = shift;
                }
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

/// How far the content of the sideways cells in each slot of the
/// list overflows them, at most, by the row the slot shows.
fn overflows(
    list: Entity,
    slots: &[Placed],
    cells: &Query<(
        &Sideways,
        &ChildOf,
        &ComputedNode,
        &mut UiTransform,
    )>,
    nodes: &Query<(&ComputedNode, &mut Node)>,
    scale: f32,
) -> Vec<(usize, f32)> {
    let mut wide = vec![None::<f32>; slots.len()];
    for (cell, room, content, _) in cells {
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
        // The rows stay as the area scrolls sideways, and only their
        // sideways cells move.
        cx.world
            .entity_mut(node)
            .insert(IgnoreScroll(BVec2::new(true, false)));
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
        // The slot is as high as its row is known to be and clips it,
        // so a row taller than its estimate cannot draw over the next
        // before it is measured. In a block its row keeps its height
        // whatever the slot's, and that is what the slot measures.
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

/// How far the widest sideways cell of each row overflows it, and
/// the most any row does.
#[derive(Default)]
struct Widths {
    each: Vec<f32>,
    widest: f32,
}

impl Widths {
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

#[derive(Default)]
struct Heights {
    each: Vec<f32>,
    known: Vec<bool>,
    tree: Vec<f64>,
    /// The sum of the heights laid out, and how many there are.
    laid_out: (f64, usize),
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

    /// The mean height of the rows laid out, if any are.
    fn mean(&self) -> Option<f32> {
        let (sum, rows) = self.laid_out;
        (rows > 0).then(|| (sum / rows as f64) as f32)
    }

    fn resize(&mut self, count: usize, estimate: f32) {
        if self.tree.is_empty() {
            self.tree.push(0.0);
        }
        for index in count..self.len() {
            if self.known[index] {
                self.laid_out.0 -= f64::from(self.each[index]);
                self.laid_out.1 -= 1;
            }
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
        if !self.known[index] {
            self.known[index] = true;
            self.laid_out.1 += 1;
            self.laid_out.0 += f64::from(self.each[index]);
        }
        if self.each[index] == height {
            return;
        }
        let by = f64::from(height) - f64::from(self.each[index]);
        self.laid_out.0 += by;
        self.each[index] = height;
        let mut k = index + 1;
        while k < self.tree.len() {
            self.tree[k] += by;
            k += lowest(k);
        }
    }

    /// Gives every row not laid out yet the height `estimate`.
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
