//! The row builder a read-only and an editable code view share.

use core::ops::Range;
use std::borrow::Cow;
use std::mem;

use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::world::World;
use bevy::text::FontSource;
use bevy::ui::{AlignItems, UiRect, px};
use bevy_fynix::tokens::{SpacingTokens, Tone};
use bevy_fynix::views::{
    FrameProps, LineColor, RowSlot, Span, Stack, column, rich, row,
};
use bevy_fynix::{
    AnyView, Bevy, Cx, Signal, ViewExt, component, derived,
};
use rubrix_edit::TAB;
use rubrix_highlight::{Run, spans};

use crate::code_view::{CodeDoc, CodeLine, Overflow};
use crate::marks::{Mark, MarkKind};
use crate::select::{self, selectable};
use crate::tokens::{Styles, SyntaxTokens};

/// Bevy's line height, as a factor of the font size.
pub(crate) const LINE_HEIGHT: f32 = 1.2;

/// JetBrains Mono's advance, as a factor of the font size.
pub(crate) const ADVANCE: f32 = 0.6;

/// What every row of a code list draws with, read from the theme when
/// the view is built.
#[derive(Clone)]
pub(crate) struct RowStyle {
    pub(crate) styles: Styles,
    pub(crate) font: FontSource,
    pub(crate) size: f32,
    pub(crate) gap: f32,
    pub(crate) selected: Color,
    pub(crate) caret: Color,
    pub(crate) faint: Color,
    /// Room right of each row's text, logical px.
    pub(crate) pad: f32,
}

impl RowStyle {
    pub(crate) fn new(
        theme: &(impl SyntaxTokens + SpacingTokens),
        size: Option<f32>,
    ) -> RowStyle {
        RowStyle {
            styles: Styles::new(theme),
            font: theme.code_font(),
            size: size.unwrap_or_else(|| theme.body_size()),
            gap: theme.gap(),
            selected: select::selected_colour(theme),
            caret: theme.tone(Tone::Accent),
            faint: theme.tone(Tone::Faint),
            pad: 0.0,
        }
    }
}

/// Where a row of a code list reads its text and when it reads again.
pub(crate) trait RowFeed:
    Copy + Send + Sync + 'static
{
    /// The component on each row's text naming the row it shows.
    type Tag: Component;
    /// What a row remembers of its last read.
    type Read: Copy + Send + Sync + 'static;

    fn tag(self, row: usize) -> Self::Tag;

    /// The view whose [`Numbers`](crate::numbers::Numbers) draw the
    /// rows' line numbers, or `None` when each row draws its own.
    fn numbers(self) -> Option<Entity> {
        None
    }

    fn rows(self, world: &World) -> usize;

    fn wrap(self) -> Signal<bool>;

    fn spans(
        self,
        world: &World,
        row: usize,
        styles: &Styles,
    ) -> Vec<Span>;

    /// Whether a row that read at `read` reads again, and what it
    /// reads at now. Called at every Mount, and kept either way.
    fn reread(
        self,
        world: &World,
        row: usize,
        read: Option<Self::Read>,
    ) -> (bool, Self::Read);
}

/// A read-only code view's rows: a committed row reads again only
/// when its slot moves, and an unfinished one also when the stream
/// sends more.
#[derive(Clone, Copy)]
pub(crate) struct ViewFeed {
    pub(crate) view: Entity,
}

#[derive(Clone, Copy)]
pub(crate) struct ViewRead {
    edits: Option<u64>,
    committed: bool,
}

impl RowFeed for ViewFeed {
    type Tag = CodeLine;
    type Read = ViewRead;

    fn tag(self, row: usize) -> CodeLine {
        CodeLine {
            view: self.view,
            row,
        }
    }

    fn rows(self, world: &World) -> usize {
        world.get::<CodeDoc>(self.view).map_or(1, CodeDoc::rows)
    }

    fn wrap(self) -> Signal<bool> {
        component::<CodeDoc, bool>(self.view, |doc| {
            doc.is_some_and(|doc| doc.overflow() == Overflow::Wrap)
        })
    }

    fn spans(
        self,
        world: &World,
        row: usize,
        styles: &Styles,
    ) -> Vec<Span> {
        world
            .get::<CodeDoc>(self.view)
            .map_or_else(Vec::new, |doc| {
                line_spans(
                    doc.line(row).unwrap_or_default(),
                    doc.runs(row),
                    styles,
                )
            })
    }

    fn reread(
        self,
        world: &World,
        row: usize,
        read: Option<ViewRead>,
    ) -> (bool, ViewRead) {
        let doc = world.get::<CodeDoc>(self.view);
        let edits = doc.map(CodeDoc::edits);
        let fires = read.is_none_or(|read| {
            !read.committed && read.edits != edits
        });
        let now = ViewRead {
            edits,
            committed: doc.is_some_and(|doc| row < doc.committed()),
        };
        (fires, now)
    }
}

/// One row of a code list: its line number in a gutter, then its text
/// in a cell that scrolls sideways with the list.
pub(crate) fn code_row<T, F: RowFeed>(
    feed: F,
    slot: RowSlot,
    style: &RowStyle,
) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    let number = rich(gutter(feed, slot))
        .font(style.font.clone())
        .size(style.size)
        .tone(Tone::Faint)
        .wrap(false);
    let number = match feed.numbers() {
        None => number.boxed(),
        Some(_) => AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
            let node = cx.build(number);
            cx.world.entity_mut(node).insert(Visibility::Hidden);
            node
        }),
    };
    let gutter = row((number,))
        .shrink(0.0)
        .padding(UiRect::right(px(style.gap)));
    let line = rich(line(feed, slot, style.styles.clone()))
        .font(style.font.clone())
        .size(style.size)
        .wrap(feed.wrap());
    let mut text =
        selectable(line, slot, style.selected, move |row| {
            feed.tag(row)
        });
    if style.pad > 0.0 {
        text = row((text,))
            .min_width(px(0.0))
            .padding(UiRect::right(px(style.pad)))
            .boxed();
    }
    let whole = row((gutter, slot.sideways(text)))
        .align(AlignItems::FlexStart)
        .gap(0.0);
    match feed.numbers() {
        None => whole.boxed(),
        Some(view) => {
            AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
                let node = cx.build(whole);
                select::tag_row(cx, node, slot, move |line| {
                    NumberedRow { view, line }
                });
                node
            })
        }
    }
}

#[derive(Component, Clone, Copy)]
pub(crate) struct NumberedRow {
    pub(crate) view: Entity,
    pub(crate) line: usize,
}

/// A row's line number, or, when [`RowFeed::numbers`] draws them, as
/// many digits as the widest number: the room the number takes, held
/// while the slot moves to other lines.
fn gutter<F: RowFeed>(feed: F, slot: RowSlot) -> Signal<Vec<Span>> {
    let width = move |world: &World| digits(feed.rows(world));
    let own = feed.numbers().is_none();
    let row = slot.index();
    let mut index = slot.index();
    let mut shown = None;
    derived(move |world| {
        let width = width(world);
        let text = if own {
            format!("{:>width$}", row.get(world) + 1)
        } else {
            "0".repeat(width)
        };
        vec![Span::new(text)]
    })
    .when(move |world| {
        let moved = own && index.changed(world);
        let now = Some(width(world));
        moved | (mem::replace(&mut shown, now) != now)
    })
}

fn line<F: RowFeed>(
    feed: F,
    slot: RowSlot,
    styles: Styles,
) -> Signal<Vec<Span>> {
    let row = slot.index();
    let mut index = slot.index();
    let mut read: Option<F::Read> = None;
    derived(move |world| feed.spans(world, row.get(world), &styles))
        .when(move |world| {
            let moved = index.changed(world);
            let (fires, now) =
                feed.reread(world, index.get(world), read);
            read = Some(now);
            moved || fires
        })
}

pub(crate) fn filling() -> Stack<()> {
    column(()).grow(1.0).min_width(px(0.0)).min_height(px(0.0))
}

pub(crate) fn digits(number: usize) -> usize {
    number.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// A line as drawn: each tab as spaces to the next stop, and each
/// inserted text (an IME preedit) at its byte. Neither has source
/// bytes of its own, so `source` and `shown` map past both.
pub(crate) struct Shown<'a> {
    text: Cow<'a, str>,
    /// In drawn order.
    gaps: Vec<Gap>,
}

/// Drawn text that stands for `source` source bytes at `byte`: a tab
/// (one byte) or an insert (none).
struct Gap {
    byte: usize,
    source: usize,
    drawn: usize,
}

impl<'a> Shown<'a> {
    pub(crate) fn new(line: &'a str) -> Shown<'a> {
        Shown::with(line, &[])
    }

    /// `line` with `inserts`, each `(byte, text)`, sorted by byte.
    pub(crate) fn with(
        line: &'a str,
        inserts: &[(usize, &str)],
    ) -> Shown<'a> {
        if inserts.is_empty() && !line.contains('\t') {
            return Shown {
                text: Cow::Borrowed(line),
                gaps: Vec::new(),
            };
        }
        let mut text = String::with_capacity(line.len() + TAB);
        let mut gaps = Vec::new();
        let mut inserts = inserts.iter().peekable();
        let mut chars = line.char_indices().peekable();
        let mut column = 0;
        loop {
            let at = chars.peek().map_or(line.len(), |&(at, _)| at);
            while let Some((_, insert)) =
                inserts.next_if(|(byte, _)| *byte <= at)
            {
                text.push_str(insert);
                column += insert.chars().count();
                gaps.push(Gap {
                    byte: at,
                    source: 0,
                    drawn: insert.len(),
                });
            }
            let Some((at, c)) = chars.next() else {
                break;
            };
            if c == '\t' {
                let spaces = TAB - column % TAB;
                text.extend(core::iter::repeat_n(' ', spaces));
                gaps.push(Gap {
                    byte: at,
                    source: 1,
                    drawn: spaces,
                });
                column += spaces;
            } else {
                text.push(c);
                column += 1;
            }
        }
        Shown {
            text: Cow::Owned(text),
            gaps,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The source byte a drawn byte shows, the nearer end of a tab
    /// inside one, and an insert's byte inside an insert.
    pub(crate) fn source(&self, shown_byte: usize) -> usize {
        let mut extra = 0;
        for gap in &self.gaps {
            let at = gap.byte + extra;
            if shown_byte < at {
                break;
            }
            if shown_byte < at + gap.drawn {
                return gap.byte
                    + usize::from(
                        gap.source > 0
                            && shown_byte - at > gap.drawn / 2,
                    );
            }
            extra += gap.drawn - gap.source;
        }
        shown_byte - extra
    }

    /// Where a source byte is drawn: before an insert at it.
    pub(crate) fn shown(&self, source_byte: usize) -> usize {
        source_byte
            + self
                .gaps
                .iter()
                .take_while(|gap| gap.byte < source_byte)
                .map(|gap| gap.drawn - gap.source)
                .sum::<usize>()
    }
}

pub(crate) fn line_spans(
    line: &str,
    runs: &[Run],
    styles: &Styles,
) -> Vec<Span> {
    shown_spans(&Shown::new(line), runs, styles)
}

/// The spans of `shown` coloured by `runs`, whose ends are source
/// bytes. Text after the last run (an insert at the line's end) takes
/// the last span's look.
pub(crate) fn shown_spans(
    shown: &Shown,
    runs: &[Run],
    styles: &Styles,
) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(runs.len());
    let mut start = 0;
    for (range, kind) in spans(runs) {
        let end = shown.shown(range.end);
        let text = &shown.text()[start..end];
        let look = styles.look(kind);
        match out.last_mut() {
            Some(last) if last.look == *look => {
                last.text.push_str(text)
            }
            _ => out.push(Span {
                text: text.to_owned(),
                look: look.clone(),
            }),
        }
        start = end;
    }
    let rest = &shown.text()[start..];
    if !rest.is_empty() {
        match out.last_mut() {
            Some(last) => last.text.push_str(rest),
            None => out.push(Span::new(rest)),
        }
    }
    out
}

/// The inserts among `marks`, as [`Shown::with`] takes them.
pub(crate) fn inserts<'m>(
    marks: impl Iterator<Item = &'m Mark>,
) -> Vec<(usize, &'m str)> {
    let mut inserts: Vec<(usize, &str)> = marks
        .filter_map(|mark| match &mark.kind {
            MarkKind::Insert { at, text, .. } => {
                Some((*at, text.as_str()))
            }
            _ => None,
        })
        .collect();
    inserts.sort_by_key(|&(at, _)| at);
    inserts
}

/// `spans` of `shown` with a row's marks applied: a tint as a
/// background, an underline, and an insert's underline over its text.
pub(crate) fn marked<'m>(
    spans: Vec<Span>,
    shown: &Shown,
    marks: impl Iterator<Item = &'m Mark>,
) -> Vec<Span> {
    let looks: Vec<(Range<usize>, &MarkKind)> = marks
        .filter_map(|mark| {
            let drawn = match &mark.kind {
                MarkKind::Tint { bytes, .. }
                | MarkKind::Underline { bytes, .. } => {
                    shown.shown(bytes.start)..shown.shown(bytes.end)
                }
                MarkKind::Insert {
                    at,
                    text,
                    underline: Some(_),
                    ..
                } => {
                    let at = shown.shown(*at);
                    at..at + text.len()
                }
                MarkKind::Insert { .. } => return None,
            };
            (!drawn.is_empty()).then_some((drawn, &mark.kind))
        })
        .collect();
    if looks.is_empty() {
        return spans;
    }
    let mut cuts: Vec<usize> = looks
        .iter()
        .flat_map(|(drawn, _)| [drawn.start, drawn.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Vec::with_capacity(spans.len() + cuts.len());
    let mut start = 0;
    for span in spans {
        let end = start + span.text.len();
        let mut at = start;
        let inner = cuts
            .iter()
            .copied()
            .filter(|&cut| start < cut && cut < end);
        for cut in inner.chain([end]) {
            let mut look = span.look.clone();
            for (drawn, kind) in &looks {
                if drawn.start <= at && cut <= drawn.end {
                    restyle(&mut look, kind);
                }
            }
            out.push(Span {
                text: span.text[at - start..cut - start].to_owned(),
                look,
            });
            at = cut;
        }
        start = end;
    }
    out
}

fn restyle(look: &mut bevy_fynix::views::Look, kind: &MarkKind) {
    match kind {
        MarkKind::Tint { color, .. } => {
            look.background = Some(*color)
        }
        MarkKind::Underline { color, .. }
        | MarkKind::Insert {
            underline: Some(color),
            ..
        } => look.underline = Some(LineColor::Own(*color)),
        MarkKind::Insert { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use bevy::color::Color;
    use bevy_fynix::tokens::TextTokens;
    use rubrix_highlight::Kind;

    use super::*;

    #[test]
    fn tabs_expand_to_four_column_stops() {
        let shown = Shown::new("\tab\tc");
        assert_eq!(shown.text(), "    ab  c");
        assert_eq!(shown.shown(0), 0);
        assert_eq!(shown.shown(1), 4);
        assert_eq!(shown.shown(3), 6);
        assert_eq!(shown.shown(4), 8);
        assert_eq!(shown.shown(5), 9);
    }

    #[test]
    fn a_shown_byte_maps_back_to_its_source_byte() {
        let shown = Shown::new("\tab\tc");
        let back =
            (0..=9).map(|at| shown.source(at)).collect::<Vec<_>>();
        assert_eq!(back, [0, 0, 0, 1, 1, 2, 3, 3, 4, 5]);
        assert_eq!(Shown::new("a 日").source(5), 5);
    }

    #[test]
    fn a_line_without_tabs_shows_as_it_is() {
        let shown = Shown::new("a 日本 b");
        assert!(matches!(shown.text, Cow::Borrowed(_)));
        assert_eq!(shown.shown(5), 5);
    }

    #[test]
    fn an_insert_draws_at_its_byte_and_owns_no_source_byte() {
        let shown = Shown::with("ab\tc", &[(1, "XY")]);
        assert_eq!(
            shown.text(),
            "aXYb    c",
            "the tab stops at a drawn column"
        );
        assert_eq!(
            shown.shown(1),
            1,
            "a caret at the insert sits before it"
        );
        assert_eq!(shown.shown(2), 4);
        assert_eq!(shown.shown(3), 8);
        let back =
            (0..=9).map(|at| shown.source(at)).collect::<Vec<_>>();
        assert_eq!(back, [0, 1, 1, 1, 2, 2, 2, 3, 3, 4]);
    }

    #[test]
    fn an_insert_at_the_end_of_a_line_is_drawn() {
        let styles = Styles::new(&Theme);
        let runs = [Run {
            end: 2,
            kind: Kind::Plain,
        }];
        let shown = Shown::with("ab", &[(2, "cd")]);
        let spans = shown_spans(&shown, &runs, &styles);
        assert_eq!(
            spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            "abcd"
        );
    }

    #[test]
    fn marks_restyle_the_drawn_bytes_they_cover() {
        let styles = Styles::new(&Theme);
        let runs = [Run {
            end: 6,
            kind: Kind::Plain,
        }];
        let red = Color::srgb(1.0, 0.0, 0.0);
        let marks = [
            Mark {
                line: 0,
                kind: MarkKind::Tint {
                    bytes: 1..3,
                    color: red,
                },
            },
            Mark {
                line: 0,
                kind: MarkKind::Insert {
                    at: 4,
                    text: "ZZ".to_owned(),
                    underline: Some(red),
                    caret: None,
                },
            },
        ];
        let shown = Shown::with("\tbcdef", &inserts(marks.iter()));
        assert_eq!(shown.text(), "    bcdZZef");
        let spans = marked(
            shown_spans(&shown, &runs, &styles),
            &shown,
            marks.iter(),
        );
        let pieces = spans
            .iter()
            .map(|span| {
                (
                    span.text.as_str(),
                    span.look.background.is_some(),
                    span.look.underline.is_some(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            pieces,
            [
                ("    ", false, false),
                ("bc", true, false),
                ("d", false, false),
                ("ZZ", false, true),
                ("ef", false, false),
            ]
        );
    }

    struct Theme;

    impl TextTokens for Theme {
        fn tone(&self, _: Tone) -> Color {
            Color::WHITE
        }

        fn body_size(&self) -> f32 {
            14.0
        }

        fn small_size(&self) -> f32 {
            11.0
        }
    }

    impl SyntaxTokens for Theme {}

    #[test]
    fn equal_looks_merge_into_one_span() {
        let styles = Styles::new(&Theme);
        let runs = [
            Run {
                end: 1,
                kind: Kind::Keyword,
            },
            Run {
                end: 3,
                kind: Kind::String,
            },
            Run {
                end: 5,
                kind: Kind::Plain,
            },
        ];
        let spans = line_spans("a\tbcd", &runs, &styles);
        let texts = spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["a   bcd"]);
    }

    #[test]
    fn digits_count_the_widest_line_number() {
        assert_eq!(
            [0, 9, 10, 99_999, 100_000].map(digits),
            [1, 1, 2, 5, 6]
        );
    }
}
