//! Where an editable view's rows put each byte, and the carets and
//! selection rects drawn from it.
//!
//! Everything stored or returned here is in logical pixels. Physical
//! pixels appear inside parley layouts, which Bevy lays out at the
//! window's scale, and in each caret bar's `UiGlobalTransform`.

use core::ops::Range;
use core::time::Duration;
use std::sync::OnceLock;

use bevy::camera::Camera;
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::{With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, SystemParam};
use bevy::input_focus::InputFocus;
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::text::{ComputedTextBlock, LineBreak, TextLayout};
use bevy::time::Time;
use bevy::ui::{
    BackgroundColor, CalculatedClip, ComputedNode, Node,
    PositionType, ScrollPosition, UiGlobalTransform, px,
};
use bevy_fynix::views::{VirtualList, drawn_layout, rich_point};
use parley::{Affinity, Cursor, Layout as Parley};
use rubrix_edit::{Buffer, Layout, graphemes};
use rubrix_highlight::LineEdit;

use crate::code_view::Overflow;
use crate::damage::{End, Shift, row_edits};
use crate::editor::{EditDoc, EditInput, EditLine, EditView};
use crate::keymap::content;
use crate::marks::{MarkKind, Marks};
use crate::rows::{ADVANCE, LINE_HEIGHT, Shown, inserts};
use crate::select::{self, Layer, Over, SelectionRects};

type Brush = bevy::text::TextBrush;

pub(crate) const CARET_WIDTH: f32 = 2.0;

/// A point resolved to the text: its line, its byte offset in the
/// buffer, and its x in logical pixels from the start of its visual
/// line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub line: u32,
    pub offset: usize,
    pub x: f32,
}

/// How carets blink: on for `period`, then off for `period`, and on
/// for `period` after each key or press. `None` keeps them solid, as
/// a scripted run wants for its screenshots.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretBlink {
    pub period: Option<Duration>,
}

impl Default for CaretBlink {
    fn default() -> CaretBlink {
        CaretBlink {
            period: Some(Duration::from_millis(500)),
        }
    }
}

/// A caret's box in its row: the visual line it falls on (0 unless
/// the row wraps), and x, top and height from the row's text origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaretBox {
    pub(crate) visual: u32,
    pub(crate) x: f32,
    pub(crate) top: f32,
    pub(crate) height: f32,
}

/// What a row not laid out is measured by: the code font's column
/// advance, the line height, and the width a wrapping row breaks at
/// (`None` under `Overflow::Scroll`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    pub(crate) advance: f32,
    pub(crate) line_height: f32,
    pub(crate) wrap: Option<f32>,
}

impl Metrics {
    /// The bundled font's, at `size`, before any row is laid out.
    pub(crate) fn of_size(size: f32) -> Metrics {
        Metrics {
            advance: ADVANCE * size,
            line_height: (size * LINE_HEIGHT).ceil(),
            wrap: None,
        }
    }
}

/// The built rows of one view: the line each shows, its text node,
/// and whether its layout is of the text it shows now. Collected from
/// the rows' `EditLine` tags, then moved through each change of the
/// frame, so a second key in a frame measures an edited line by
/// `Metrics` instead of reading the layout of its old text.
#[derive(Default)]
pub(crate) struct Built {
    /// Sorted by line.
    rows: Vec<BuiltRow>,
}

struct BuiltRow {
    line: u32,
    text: Entity,
    laid: bool,
    /// A wrapping row's layout broken at its node's width, made on
    /// first use in a frame.
    drawn: OnceLock<Parley<Brush>>,
}

impl Built {
    pub(crate) fn collect(
        &mut self,
        view: Entity,
        tags: &Query<(Entity, &EditLine)>,
    ) {
        self.rows.clear();
        self.rows.extend(
            tags.iter().filter(|(_, tag)| tag.view == view).map(
                |(text, tag)| BuiltRow {
                    line: tag.line,
                    text,
                    laid: true,
                    drawn: OnceLock::new(),
                },
            ),
        );
        self.rows.sort_unstable_by_key(|row| row.line);
    }

    /// Moves the rows through one change's line edits. Each edit's
    /// first line keeps its row with its layout out of date, and so
    /// does a row a 1:1 [`row_edits`] entry replaced; the other rows
    /// an edit replaced go.
    pub(crate) fn shift(&mut self, edits: &[LineEdit]) {
        let rows = row_edits(edits);
        let mut shift = Shift::new(&rows);
        self.rows.retain_mut(|row| match shift.inside(row.line) {
            Some(edit) if edit.removed == 1 && edit.inserted == 1 => {
                row.line = edit.first;
                row.laid = false;
                true
            }
            Some(_) => false,
            None => {
                row.line = shift.line(row.line, End::Start);
                true
            }
        });
        for row in &mut self.rows {
            if edits.iter().any(|edit| edit.first == row.line) {
                row.laid = false;
            }
        }
    }

    fn laid(&self, line: u32) -> Option<&BuiltRow> {
        let i = self.rows.partition_point(|row| row.line < line);
        self.rows.get(i).filter(|row| row.line == line && row.laid)
    }
}

type TextParts = (
    &'static ComputedNode,
    &'static UiGlobalTransform,
    &'static ComputedTextBlock,
    &'static TextLayout,
    Option<&'static CalculatedClip>,
);

type AreaParts = (
    &'static ComputedNode,
    &'static UiGlobalTransform,
    &'static ScrollPosition,
);

/// The parts of the world a [`Geometry`] reads.
#[derive(SystemParam)]
pub(crate) struct RowParts<'w, 's> {
    texts: Query<'w, 's, TextParts, Without<CaretBar>>,
    areas: Query<
        'w,
        's,
        AreaParts,
        (With<VirtualList>, Without<CaretBar>),
    >,
    cameras: Query<'w, 's, &'static Camera>,
}

/// A built row's layout, as drawn, with its node.
struct Laid<'a> {
    layout: &'a Parley<Brush>,
    node: &'a ComputedNode,
    transform: &'a UiGlobalTransform,
    clip: Option<&'a CalculatedClip>,
}

impl Laid<'_> {
    fn scale(&self) -> f32 {
        1.0 / self.node.inverse_scale_factor()
    }

    fn caret(&self, shown_byte: usize) -> CaretBox {
        let logical = self.node.inverse_scale_factor();
        let bbox = Cursor::from_byte_index(
            self.layout,
            shown_byte,
            Affinity::Downstream,
        )
        .geometry(self.layout, CARET_WIDTH * self.scale());
        let middle = ((bbox.y0 + bbox.y1) / 2.0) as f32;
        let lines = self.layout.len();
        let visual = (0..lines)
            .find(|&i| {
                self.layout.get(i).is_some_and(|line| {
                    middle < line.metrics().block_max_coord
                })
            })
            .unwrap_or(lines.saturating_sub(1));
        let (top, height) = self.layout.get(visual).map_or(
            (bbox.y0 as f32, (bbox.y1 - bbox.y0) as f32),
            |line| {
                let metrics = line.metrics();
                (
                    metrics.block_min_coord,
                    metrics.block_max_coord - metrics.block_min_coord,
                )
            },
        );
        CaretBox {
            visual: visual as u32,
            x: bbox.x0 as f32 * logical,
            top: top * logical,
            height: height * logical,
        }
    }

    /// The drawn byte nearest `x` on visual line `visual`.
    fn byte_at(&self, visual: u32, x: f32) -> usize {
        let Some(line) = self.layout.get(visual as usize) else {
            return 0;
        };
        let metrics = line.metrics();
        let y =
            (metrics.block_min_coord + metrics.block_max_coord) / 2.0;
        Cursor::from_point(self.layout, x * self.scale(), y).index()
    }
}

/// The visual lines a row of `text` breaks into at `columns`,
/// greedily at word starts, as the start of each in graphemes, and
/// the row's length in graphemes.
fn wrapped(
    text: &str,
    columns: Option<usize>,
) -> (Vec<usize>, usize) {
    let mut starts = vec![0];
    let mut column = 0;
    let mut word: Option<usize> = None;
    for (_, grapheme) in graphemes(text) {
        let space = grapheme.chars().all(char::is_whitespace);
        if let Some(columns) = columns.filter(|_| !space) {
            let line = *starts.last().expect("a first line");
            if column - line >= columns {
                let at = word
                    .filter(|&word| word > line)
                    .unwrap_or(column);
                starts.push(at);
            }
        }
        if space {
            word = None;
        } else if word.is_none() {
            word = Some(column);
        }
        column += 1;
    }
    (starts, column)
}

/// One view's rows as geometry: byte to caret box, point to byte, and
/// `rubrix_edit`'s `Layout`. Built rows answer from their parley
/// layouts; rows not built, or edited since their layout, from
/// `Metrics`. Hit-tests and vertical motion both go through
/// `byte_at`, and carets, reveals and `Layout::x` through `caret`.
pub(crate) struct Geometry<'a, 'w, 's> {
    parts: &'a RowParts<'w, 's>,
    list: &'a VirtualList,
    area: Entity,
    built: &'a Built,
    marks: Option<&'a Marks>,
    metrics: Metrics,
}

/// Where a reveal scrolls a list: a row, a y range inside it, and an
/// x range of its text.
pub(crate) struct Reveal {
    line: usize,
    y: Range<f32>,
    x: Range<f32>,
}

impl Reveal {
    pub(crate) fn to(self, list: &mut VirtualList) {
        list.scroll_range_into_view(self.line, self.y);
        list.scroll_x_into_view(self.x);
    }
}

impl<'a, 'w, 's> Geometry<'a, 'w, 's> {
    pub(crate) fn new(
        parts: &'a RowParts<'w, 's>,
        list: &'a VirtualList,
        area: Entity,
        built: &'a Built,
        marks: Option<&'a Marks>,
        metrics: Metrics,
    ) -> Geometry<'a, 'w, 's> {
        Geometry {
            parts,
            list,
            area,
            built,
            marks,
            metrics,
        }
    }

    fn laid(&self, line: u32) -> Option<Laid<'a>> {
        let row = self.built.laid(line)?;
        let (node, transform, block, text, clip) =
            self.parts.texts.get(row.text).ok()?;
        let layout = if text.linebreak == LineBreak::NoWrap {
            block.buffer()
        } else {
            row.drawn.get_or_init(|| {
                drawn_layout(block, text, node).into_owned()
            })
        };
        Some(Laid {
            layout,
            node,
            transform,
            clip,
        })
    }

    fn shown<'b>(&self, buffer: &'b Buffer, line: u32) -> Shown<'b> {
        let marks =
            self.marks.into_iter().flat_map(|marks| marks.on(line));
        Shown::with(content(buffer, line), &inserts(marks))
    }

    fn columns(&self) -> Option<usize> {
        self.metrics.wrap.map(|wrap| {
            ((wrap / self.metrics.advance.max(1.0)).floor() as usize)
                .max(1)
        })
    }

    /// The estimate of an unlaid row's caret at drawn byte `at`.
    fn estimate(&self, shown: &Shown, at: usize) -> CaretBox {
        let (starts, _) = wrapped(shown.text(), self.columns());
        let before = &shown.text()[..at.min(shown.text().len())];
        let column = graphemes(before).count();
        let visual = starts
            .partition_point(|&start| start <= column)
            .saturating_sub(1);
        CaretBox {
            visual: visual as u32,
            x: (column - starts[visual]) as f32
                * self.metrics.advance,
            top: visual as f32 * self.metrics.line_height,
            height: self.metrics.line_height,
        }
    }

    /// Where the caret at drawn byte `at` of `line` sits in its row.
    fn caret_in(
        &self,
        line: u32,
        shown: &Shown,
        at: usize,
    ) -> CaretBox {
        match self.laid(line) {
            Some(laid) => laid.caret(at),
            None => self.estimate(shown, at),
        }
    }

    /// Where the caret at `offset` sits in its row.
    pub(crate) fn caret(
        &self,
        buffer: &Buffer,
        offset: usize,
    ) -> CaretBox {
        let line = buffer.line_of(offset);
        let shown = self.shown(buffer, line);
        let at = shown.shown(offset - buffer.line_start(line));
        self.caret_in(line, &shown, at)
    }

    /// Visual lines in `line`'s row.
    fn visual_lines(&self, buffer: &Buffer, line: u32) -> u32 {
        match self.laid(line) {
            Some(laid) => laid.layout.len().max(1) as u32,
            None => {
                let shown = self.shown(buffer, line);
                wrapped(shown.text(), self.columns()).0.len() as u32
            }
        }
    }

    /// The byte offset nearest `x` on visual line `visual` of `line`.
    fn byte_at(
        &self,
        buffer: &Buffer,
        line: u32,
        visual: u32,
        x: f32,
    ) -> usize {
        let shown = self.shown(buffer, line);
        let drawn = match self.laid(line) {
            Some(laid) => laid.byte_at(visual, x),
            None => {
                let (starts, length) =
                    wrapped(shown.text(), self.columns());
                let visual = (visual as usize).min(starts.len() - 1);
                let end = starts
                    .get(visual + 1)
                    .map_or(length, |&next| next);
                let column = (x / self.metrics.advance.max(1.0))
                    .round()
                    .max(0.0) as usize;
                let column = (starts[visual] + column).min(end);
                graphemes(shown.text())
                    .nth(column)
                    .map_or(shown.text().len(), |(at, _)| at)
            }
        };
        buffer.line_start(line) + shown.source(drawn)
    }

    /// The text at `at`, logical window px, as a pointer would press
    /// it, and where it is against the list. `None` on a scrollbar or
    /// off the list. A press left of a row's visible text, on the
    /// gutter, lands at the line's start.
    pub(crate) fn hit(
        &self,
        buffer: &Buffer,
        at: Vec2,
    ) -> Option<(Hit, Over)> {
        let camera = self.parts.cameras.iter().next()?;
        let (node, transform, scroll) =
            self.parts.areas.get(self.area).ok()?;
        if select::on_scrollbars(node, transform, camera, at) {
            return None;
        }
        let over = select::over(
            self.list, node, transform, camera, scroll, at,
        )?;
        let line = (over.row as u32).min(buffer.line_count() - 1);
        let start = buffer.line_start(line);
        let offset = self.laid(line).map_or(start, |laid| {
            let Some(local) = rich_point(
                laid.node,
                laid.transform,
                camera,
                over.inside,
            ) else {
                return start;
            };
            let viewport = camera
                .physical_viewport_rect()
                .map_or(Vec2::ZERO, |viewport| {
                    viewport.min.as_vec2()
                });
            let physical = over.inside
                * camera.target_scaling_factor().unwrap_or(1.0)
                - viewport;
            if laid
                .clip
                .is_some_and(|clip| !clip.contains_point(physical))
            {
                return start;
            }
            let drawn =
                Cursor::from_point(laid.layout, local.x, local.y)
                    .index();
            start + self.shown(buffer, line).source(drawn)
        });
        let x = self.caret(buffer, offset).x;
        Some((Hit { line, offset, x }, over))
    }

    /// Where to scroll so the caret at `head` shows: its visual line
    /// in a laid-out row, or the whole row by its estimate.
    pub(crate) fn reveal(
        &self,
        buffer: &Buffer,
        head: usize,
    ) -> Reveal {
        let line = buffer.line_of(head);
        let caret = self.caret(buffer, head);
        let y = if self.laid(line).is_some() {
            caret.top..caret.top + caret.height
        } else {
            let lines = self.visual_lines(buffer, line) as f32;
            let height = self
                .list
                .row_span(line as usize)
                .map_or(0.0, |span| span.height);
            0.0..height.max(lines * self.metrics.line_height)
        };
        let margin = 2.0 * self.metrics.advance;
        Reveal {
            line: line as usize,
            y,
            x: caret.x - margin..caret.x + CARET_WIDTH + margin,
        }
    }

    fn view_height(&self) -> f32 {
        self.parts.areas.get(self.area).map_or(0.0, |(node, ..)| {
            (node.content_box().height() - node.scrollbar_size.y)
                * node.inverse_scale_factor()
        })
    }
}

impl Layout for Geometry<'_, '_, '_> {
    fn x(&self, buffer: &Buffer, offset: usize) -> f32 {
        self.caret(buffer, offset).x
    }

    /// Walks visual lines a row at a time, so Page Down walks one
    /// page.
    fn vertical(
        &self,
        buffer: &Buffer,
        from: usize,
        rows: i32,
        x: f32,
    ) -> Option<usize> {
        let mut line = buffer.line_of(from);
        let mut visual = i64::from(self.caret(buffer, from).visual)
            + i64::from(rows);
        loop {
            if visual < 0 {
                line = line.checked_sub(1)?;
                visual += i64::from(self.visual_lines(buffer, line));
                continue;
            }
            let lines = i64::from(self.visual_lines(buffer, line));
            if visual >= lines {
                if line + 1 >= buffer.line_count() {
                    return None;
                }
                visual -= lines;
                line += 1;
                continue;
            }
            return Some(self.byte_at(
                buffer,
                line,
                visual as u32,
                x,
            ));
        }
    }

    fn page_rows(&self) -> u32 {
        ((self.view_height() / self.metrics.line_height.max(1.0))
            .floor() as u32)
            .max(1)
    }
}

/// The overlay a view's carets are drawn in: a child of the view's
/// root over all of it, above the rows, clipping to it. One bar per
/// caret, reused by index.
#[derive(Component)]
pub(crate) struct CaretLayer {
    pub(crate) color: Color,
}

/// One caret bar in a [`CaretLayer`].
#[derive(Component)]
pub(crate) struct CaretBar;

/// Shows each view's carets while it holds the input focus, blinking
/// by [`CaretBlink`]. Gaining the focus restarts the blink.
pub(crate) fn blink(
    time: Res<Time>,
    focus: Option<Res<InputFocus>>,
    blink: Res<CaretBlink>,
    mut views: Query<(Entity, &EditView, &mut EditInput)>,
    mut layers: Query<&mut Visibility, With<CaretLayer>>,
) {
    let now = time.elapsed();
    let focused = focus.and_then(|focus| focus.get());
    for (view, edit, mut input) in &mut views {
        let input = input.bypass_change_detection();
        let has = focused == Some(view);
        if has && !input.focused {
            input.keyed_at = now;
        }
        input.focused = has;
        let on = has
            && blink.period.is_none_or(|period| {
                let since = now.saturating_sub(input.keyed_at);
                since.as_nanos() / period.as_nanos().max(1) % 2 == 0
            });
        let shown = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Ok(mut visibility) = layers.get_mut(edit.carets()) {
            visibility.set_if_neq(shown);
        }
    }
}

/// `Metrics` from the first built row with plain ASCII text, or
/// `fallback`.
fn measure(
    geometry: &Geometry,
    buffer: &Buffer,
    overflow: Overflow,
    fallback: Metrics,
) -> Metrics {
    for row in &geometry.built.rows {
        let text = content(buffer, row.line);
        if text.is_empty() || !text.is_ascii() || text.contains('\t')
        {
            continue;
        }
        let Some(laid) = geometry.laid(row.line) else {
            continue;
        };
        let Some(first) = laid.layout.get(0) else {
            continue;
        };
        let logical = laid.node.inverse_scale_factor();
        let metrics = first.metrics();
        let bytes = first.text_range().len().max(1) as f32;
        // Parley rounds a line's block extent outward, so a 16.8 px
        // line spans 18; the node is the pitch the list stacks rows
        // at.
        let lines = laid.layout.len().max(1) as f32;
        return Metrics {
            advance: metrics.advance / bytes * logical,
            line_height: laid.node.content_box().height() * logical
                / lines,
            wrap: (overflow == Overflow::Wrap)
                .then(|| laid.node.content_box().width() * logical),
        };
    }
    Metrics {
        wrap: fallback.wrap.filter(|_| overflow == Overflow::Wrap),
        ..fallback
    }
}

type Bars<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut UiGlobalTransform),
    (With<CaretBar>, Without<Layer>),
>;

/// Where a spare bar waits, outside the overlay, which clips it. It
/// keeps its size, so its `ComputedNode` is right in the frame it is
/// placed again: a bar made visible after Bevy propagates visibility
/// would show a frame late.
const PARKED: f32 = -1.0e5;

type Rects<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut BackgroundColor),
    (Without<Layer>, Without<CaretBar>),
>;

type Overlays<'w, 's> = Query<
    'w,
    's,
    (
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static CaretLayer,
        Option<&'static Children>,
    ),
    Without<CaretBar>,
>;

/// Draws every editable view's selections and carets from this
/// frame's layout, after `UiSystems::PostLayout`.
///
/// Selection rects go per row into the row's selection layer, below
/// the text, and lay out next frame. A caret bar writes its `Node`
/// for the next layout and its `UiGlobalTransform` for this frame's
/// render, so it moves in the frame its text changes.
#[expect(
    clippy::too_many_arguments,
    reason = "one drawer over disjoint queries"
)]
pub(crate) fn draw(
    parts: RowParts,
    tags: Query<(Entity, &EditLine)>,
    mut views: Query<(Entity, &mut EditView, &mut EditInput, &Marks)>,
    docs: Query<&EditDoc>,
    mut lists: Query<&mut VirtualList>,
    mut rows: Query<(&mut SelectionRects, &ChildOf), With<EditLine>>,
    mut layers: Query<(&mut Node, Option<&Children>), With<Layer>>,
    mut nodes: Rects,
    overlays: Overlays,
    mut bars: Bars,
    mut commands: Commands,
) {
    for (view, mut edit, mut input, marks) in &mut views {
        let (Some(area), Ok(doc)) =
            (edit.list(), docs.get(edit.doc()))
        else {
            continue;
        };
        let Ok(list) = lists.get(area) else {
            continue;
        };
        let editor = doc.editor();
        let buffer = editor.buffer();
        let mut built = Built::default();
        built.collect(view, &tags);
        let seed = edit.drawn.metrics;
        let metrics = measure(
            &Geometry::new(
                &parts,
                list,
                area,
                &built,
                Some(marks),
                seed,
            ),
            buffer,
            edit.overflow(),
            seed,
        );
        let geometry = Geometry::new(
            &parts,
            list,
            area,
            &built,
            Some(marks),
            metrics,
        );

        let ranges: Vec<Range<usize>> = editor
            .selections()
            .iter()
            .filter(|selection| !selection.is_caret())
            .map(|selection| selection.range())
            .collect();
        for row in &built.rows {
            let Ok((mut rects, cell)) = rows.get_mut(row.text) else {
                continue;
            };
            let start = buffer.line_start(row.line);
            let end = start + content(buffer, row.line).len();
            let shown = geometry.shown(buffer, row.line);
            let laid = geometry.laid(row.line);
            let color = rects.color();
            let mut boxes = Vec::new();
            let first =
                ranges.partition_point(|range| range.end < start);
            for range in ranges[first..]
                .iter()
                .take_while(|range| range.start <= end)
            {
                let from = range.start.max(start) - start;
                let to = range.end.min(end) - start;
                if from < to {
                    let Some(laid) = &laid else {
                        continue;
                    };
                    let drawn = shown.shown(from)..shown.shown(to);
                    let logical = laid.node.inverse_scale_factor();
                    boxes.extend(select::rects(
                        laid.layout,
                        drawn,
                        logical,
                        color,
                    ));
                } else if start == end
                    && range.start <= start
                    && range.end > end
                {
                    let caret =
                        geometry.caret_in(row.line, &shown, 0);
                    let half = metrics.advance / 2.0;
                    let rect = [
                        0.0,
                        caret.top,
                        half,
                        caret.top + caret.height,
                    ];
                    boxes.push((rect, color));
                }
            }
            select::paint(
                &boxes,
                &mut rects,
                cell,
                &mut layers,
                &mut nodes,
                &mut commands,
            );
        }

        let Ok((overlay, overlay_at, layer, kids)) =
            overlays.get(edit.carets())
        else {
            continue;
        };
        let logical = overlay.inverse_scale_factor();
        let origin =
            overlay_at.translation + overlay.border_box().min;
        let kids = kids.map_or(&[][..], |kids| &kids[..]);
        let primary = editor.selections().primary();
        let mut caret = None;
        let mut used = 0;
        for selection in editor.selections().iter() {
            let head = selection.head;
            let line = buffer.line_of(head);
            let Some(laid) = geometry.laid(line) else {
                continue;
            };
            let rel = head - buffer.line_start(line);
            let mut at = geometry.shown(buffer, line).shown(rel);
            if selection == primary {
                let insert =
                    marks.on(line).find_map(|mark| {
                        match &mark.kind {
                            MarkKind::Insert {
                                at, caret, ..
                            } if *at == rel => Some(*caret),
                            _ => None,
                        }
                    });
                match insert {
                    Some(Some(into)) => at += into,
                    Some(None) => continue,
                    None => {}
                }
            }
            let scale = laid.scale();
            let bbox = Cursor::from_byte_index(
                laid.layout,
                at,
                Affinity::Downstream,
            )
            .geometry(laid.layout, CARET_WIDTH * scale);
            let corner = (laid.transform.translation
                + laid.node.content_box().min
                + Vec2::new(bbox.x0 as f32, bbox.y0 as f32))
            .round();
            let size = Vec2::new(
                CARET_WIDTH * scale,
                (bbox.y1 - bbox.y0) as f32,
            )
            .round();
            let middle = corner + Vec2::new(0.5, size.y / 2.0);
            if laid
                .clip
                .is_some_and(|clip| !clip.contains_point(middle))
            {
                continue;
            }
            let placed = Node {
                position_type: PositionType::Absolute,
                left: px((corner.x - origin.x) * logical),
                top: px((corner.y - origin.y) * logical),
                width: px(size.x * logical),
                height: px(size.y * logical),
                ..Node::default()
            };
            let centre = UiGlobalTransform::from_translation(
                corner + size / 2.0,
            );
            match kids
                .get(used)
                .and_then(|&kid| bars.get_mut(kid).ok())
            {
                Some((mut node, mut transform)) => {
                    node.set_if_neq(placed);
                    *transform = centre;
                }
                None => {
                    commands.spawn((
                        placed,
                        centre,
                        BackgroundColor(layer.color),
                        Pickable::IGNORE,
                        CaretBar,
                        ChildOf(edit.carets()),
                    ));
                }
            }
            if selection == primary {
                caret = Some(Rect::from_corners(
                    corner * logical,
                    (corner + size) * logical,
                ));
            }
            used += 1;
        }
        for &kid in kids.iter().skip(used) {
            if let Ok((mut node, mut transform)) = bars.get_mut(kid) {
                if node.left != px(PARKED) {
                    node.left = px(PARKED);
                    node.top = px(PARKED);
                }
                *transform = UiGlobalTransform::from_translation(
                    Vec2::splat(PARKED),
                );
            }
        }

        let shown = caret.is_some_and(|caret| {
            parts.areas.get(area).is_ok_and(|(node, at, _)| {
                let logical = node.inverse_scale_factor();
                let box_ = node.content_box();
                let min = (at.translation + box_.min) * logical;
                let max = min
                    + (box_.size() - node.scrollbar_size) * logical;
                let centre = caret.center();
                centre.cmpge(min).all() && centre.cmple(max).all()
            })
        });
        let again = (input.acted && !shown)
            .then(|| geometry.reveal(buffer, primary.head));
        let drawn = &mut edit.bypass_change_detection().drawn;
        drawn.caret = caret;
        drawn.shown = shown;
        drawn.metrics = metrics;
        input.bypass_change_detection().acted = false;
        if let Some(reveal) = again
            && let Ok(mut list) = lists.get_mut(area)
        {
            reveal.to(&mut list);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_wraps_before_the_word_that_overflows() {
        assert_eq!(wrapped("let x = 1;", None), (vec![0], 10));
        assert_eq!(
            wrapped("aaa bbb ccc", Some(5)),
            (vec![0, 4, 8], 11)
        );
        assert_eq!(
            wrapped("abcdefgh", Some(3)).0,
            [0, 3, 6],
            "a long word breaks at the column"
        );
        assert_eq!(
            wrapped("ab    cd", Some(4)).0,
            [0, 6],
            "spaces hang past the edge"
        );
    }
}
