//! A highlighted text shown one row per line in a virtual list.

use std::borrow::Cow;
use std::sync::Arc;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::text::FontSource;
use bevy::ui::{AlignItems, UiRect, px};
use bevy_fynix::tokens::{SpacingTokens, Tone};
use bevy_fynix::views::{
    FrameProps, RowSlot, Span, Stack, column, rich, row, virtual_list,
};
use bevy_fynix::{
    AnyView, Bevy, Cx, Prop, Signal, View, ViewExt, component,
    derived, keyed,
};
use rubrix_highlight::{Highlights, Lang, Run, highlight, spans};

use crate::tokens::{Styles, SyntaxTokens};

/// Bevy's line height, as a factor of the font size.
const LINE_HEIGHT: f32 = 1.2;

/// The columns between tab stops.
const TAB: usize = 4;

/// How far past each end of the view the list builds rows. Each
/// frame draws every row built, and a code row has about five times
/// the glyphs of a label, so the list's default of 1,000 px costs a
/// scroll frame more than 3 ms.
const OVERSCAN: f32 = 400.0;

/// A text and its highlights. It is lexed once, when it is made, and
/// a clone shares both. Two sources are equal when one is a clone of
/// the other, or when they have the same language and text.
#[derive(Clone)]
pub struct CodeSource(Arc<Whole>);

struct Whole {
    text: Arc<str>,
    highlights: Highlights,
}

impl CodeSource {
    /// Lexes `text` as `lang`.
    ///
    /// # Panics
    ///
    /// Panics if `text` is 4 GiB or longer.
    pub fn new(lang: Lang, text: impl Into<Arc<str>>) -> CodeSource {
        let text = text.into();
        let highlights = highlight(lang, &text);
        CodeSource(Arc::new(Whole { text, highlights }))
    }

    /// Lexes `text` as the language that `path` or the text's first
    /// line names, or as plain text when neither names one.
    ///
    /// # Panics
    ///
    /// Panics if `text` is 4 GiB or longer.
    pub fn detect(
        path: &str,
        text: impl Into<Arc<str>>,
    ) -> CodeSource {
        let text = text.into();
        let first = text.lines().next().unwrap_or_default();
        CodeSource::new(Lang::detect(path, first), text)
    }

    /// No text, as plain text.
    pub fn empty() -> CodeSource {
        CodeSource::new(Lang::Plain, "")
    }

    pub fn lang(&self) -> Lang {
        self.0.highlights.lang()
    }

    pub fn text(&self) -> &str {
        &self.0.text
    }

    /// The rows a code view shows for it: a row per line, but none
    /// for the empty line after a final newline. An empty text shows
    /// one row.
    pub fn rows(&self) -> usize {
        self.0.highlights.line_count() as usize
            - usize::from(self.0.text.ends_with('\n'))
    }
}

impl PartialEq for CodeSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || (self.lang() == other.lang()
                && self.text() == other.text())
    }
}

/// What a code view does with a line wider than itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    /// The line stays on one row, and the view scrolls sideways, all
    /// rows by one offset, while the line numbers stay.
    #[default]
    Scroll,
    /// The line breaks onto more lines inside its row.
    Wrap,
}

/// A view of `source`, a row per line, each with its line number in
/// a gutter, drawn in the theme's [`SyntaxTokens::code_font`] at its
/// body size. It builds only the rows near what shows.
///
/// A new source builds the view's rows again and shows its first
/// line. A tab shows as spaces to the next stop of 4 columns, which
/// count chars, so a tab after a wide CJK char stops short. The
/// bundled font has no CJK or emoji glyphs: they show only in an app
/// that finds system fonts, through Bevy's `system_font_discovery`
/// feature.
///
/// Its colours come from the theme when it is built. A theme that
/// changes later colours it only once it is built again.
pub fn code_view(source: impl Into<Prop<CodeSource>>) -> CodeView {
    CodeView {
        source: source.into(),
        overflow: Prop::Value(Overflow::default()),
    }
}

/// See [`code_view`].
pub struct CodeView {
    source: Prop<CodeSource>,
    overflow: Prop<Overflow>,
}

impl CodeView {
    /// What it does with lines wider than itself,
    /// [`Overflow::Scroll`] when unset. A change keeps the first line
    /// that shows.
    pub fn overflow(
        mut self,
        overflow: impl Into<Prop<Overflow>>,
    ) -> CodeView {
        self.overflow = overflow.into();
        self
    }
}

/// What a code view shows, on the view's root entity.
///
/// Only the view writes it. A new source gives it a new revision, and
/// each revision builds its own list, which reads the rows once, so
/// its rows and their count always agree and nothing measured for
/// the old text stays.
#[derive(Component)]
pub struct CodeDoc {
    source: CodeSource,
    overflow: Overflow,
    revision: u64,
}

impl CodeDoc {
    pub fn overflow(&self) -> Overflow {
        self.overflow
    }

    /// See [`CodeSource::rows`].
    pub fn rows(&self) -> usize {
        self.source.rows()
    }

    /// The text of row `row`, from 0, without its line ending. `None`
    /// past the last row.
    pub fn line(&self, row: usize) -> Option<&str> {
        (row < self.rows()).then(|| {
            &self.source.text()
                [self.highlights().line_range(row as u32)]
        })
    }

    /// The runs of row `row`, from 0, as they were lexed. Empty for
    /// an empty line, and past the last row.
    pub fn runs(&self, row: usize) -> &[Run] {
        if row < self.rows() {
            self.highlights().stored_runs(row as u32)
        } else {
            &[]
        }
    }

    fn highlights(&self) -> &Highlights {
        &self.source.0.highlights
    }
}

/// On the rich text of each row of a code view: the view's root and
/// the row, from 0, it shows now.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeLine {
    pub view: Entity,
    pub row: usize,
}

/// How every row of one view is drawn, read from the theme once.
#[derive(Clone)]
struct RowStyle {
    styles: Styles,
    font: FontSource,
    size: f32,
    gap: f32,
}

impl<T> View<Bevy, T> for CodeView
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    fn build(self, cx: &mut Cx<'_, Bevy, T>) -> Entity {
        let theme = cx.theme();
        let style = RowStyle {
            styles: Styles::new(theme),
            font: theme.code_font(),
            size: theme.body_size(),
            gap: theme.gap(),
        };
        let root = cx.build(filling());
        cx.world.entity_mut(root).insert(CodeDoc {
            source: CodeSource::empty(),
            overflow: Overflow::default(),
            revision: 0,
        });
        cx.effect(root, self.source, |world, root, source| {
            let mut doc =
                world.get_mut::<CodeDoc>(root).expect("a code view");
            doc.source = source.clone();
            doc.revision += 1;
        });
        cx.effect(root, self.overflow, |world, root, &overflow| {
            world
                .get_mut::<CodeDoc>(root)
                .expect("a code view")
                .overflow = overflow;
        });
        let revision = component::<CodeDoc, u64>(root, |doc| {
            doc.map_or(0, |doc| doc.revision)
        });
        cx.under(root, |cx| {
            cx.build(
                keyed(revision, move |_| list(root, style.clone()))
                    .within(filling()),
            )
        });
        root
    }
}

/// A column that takes its parent's free space and can shrink below
/// what it holds. The list's basis is 0, so its parent sizes it.
fn filling() -> Stack<()> {
    column(()).grow(1.0).min_width(px(0.0)).min_height(px(0.0))
}

fn list<T>(root: Entity, style: RowStyle) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let count =
            cx.world.get::<CodeDoc>(root).map_or(1, CodeDoc::rows);
        let estimate = (style.size * LINE_HEIGHT).ceil();
        let sideways = component::<CodeDoc, bool>(root, |doc| {
            doc.is_none_or(|doc| doc.overflow == Overflow::Scroll)
        });
        cx.build(
            virtual_list(count, estimate, move |slot| {
                code_row(root, slot, &style)
            })
            .overscan(OVERSCAN)
            .sideways(sideways),
        )
    })
}

/// A line number and the line's text. The gutter stays outside the
/// sideways cell, so only the text moves when the view scrolls
/// sideways.
fn code_row<T>(
    root: Entity,
    slot: RowSlot,
    style: &RowStyle,
) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    let number = rich(at_row(root, slot, |doc, row| {
        let width = digits(doc.rows());
        vec![Span::new(format!("{:>width$}", row + 1))]
    }))
    .font(style.font.clone())
    .size(style.size)
    .tone(Tone::Faint)
    .wrap(false);
    let gutter = row((number,))
        .shrink(0.0)
        .padding(UiRect::right(px(style.gap)));
    let styles = style.styles.clone();
    let line = rich(at_row(root, slot, move |doc, row| {
        line_spans(
            doc.line(row).unwrap_or_default(),
            doc.runs(row),
            &styles,
        )
    }))
    .font(style.font.clone())
    .size(style.size)
    .wrap(component::<CodeDoc, bool>(root, |doc| {
        doc.is_some_and(|doc| doc.overflow == Overflow::Wrap)
    }));
    let text = AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let node = cx.build(line);
        cx.effect(
            node,
            slot.index().into(),
            move |world, node, &row| {
                world
                    .entity_mut(node)
                    .insert(CodeLine { view: root, row });
            },
        );
        node
    });
    row((gutter, slot.sideways(text)))
        .align(AlignItems::FlexStart)
        .gap(0.0)
        .boxed()
}

/// What `show` makes of the row that `slot` shows, read again when
/// the slot moves to another row. Within one list that is the only
/// time a row's content changes.
fn at_row(
    root: Entity,
    slot: RowSlot,
    show: impl Fn(&CodeDoc, usize) -> Vec<Span> + Send + Sync + 'static,
) -> Signal<Vec<Span>> {
    let row = slot.index();
    let mut moved = slot.index();
    derived(move |world| {
        world
            .get::<CodeDoc>(root)
            .map_or_else(Vec::new, |doc| show(doc, row.get(world)))
    })
    .when(move |world| moved.changed(world))
}

fn digits(rows: usize) -> usize {
    rows.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// A line as its row shows it, with each tab as spaces to the next
/// stop.
pub(crate) struct Shown<'a> {
    text: Cow<'a, str>,
    /// The byte of each tab in the line, and the spaces it became.
    tabs: Vec<(usize, usize)>,
}

impl<'a> Shown<'a> {
    pub(crate) fn new(line: &'a str) -> Shown<'a> {
        if !line.contains('\t') {
            return Shown {
                text: Cow::Borrowed(line),
                tabs: Vec::new(),
            };
        }
        let mut text = String::with_capacity(line.len() + TAB);
        let mut tabs = Vec::new();
        let mut column = 0;
        for (at, c) in line.char_indices() {
            if c == '\t' {
                let spaces = TAB - column % TAB;
                text.extend(core::iter::repeat_n(' ', spaces));
                tabs.push((at, spaces));
                column += spaces;
            } else {
                text.push(c);
                column += 1;
            }
        }
        Shown {
            text: Cow::Owned(text),
            tabs,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Where the line's byte `source` shows in [`Shown::text`].
    pub(crate) fn shown(&self, source: usize) -> usize {
        source
            + self
                .tabs
                .iter()
                .take_while(|&&(at, _)| at < source)
                .map(|&(_, spaces)| spaces - 1)
                .sum::<usize>()
    }
}

/// The spans that show `line`, one per run in the run's look, with
/// runs that look alike joined into one span.
pub(crate) fn line_spans(
    line: &str,
    runs: &[Run],
    styles: &Styles,
) -> Vec<Span> {
    let shown = Shown::new(line);
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
    out
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
    fn a_line_without_tabs_shows_as_it_is() {
        let shown = Shown::new("a 日本 b");
        assert!(matches!(shown.text, Cow::Borrowed(_)));
        assert_eq!(shown.shown(5), 5);
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
    fn rows_drop_the_empty_line_after_a_final_newline() {
        let rows =
            |text: &str| CodeSource::new(Lang::Plain, text).rows();
        assert_eq!(rows(""), 1);
        assert_eq!(rows("x"), 1);
        assert_eq!(rows("x\n"), 1);
        assert_eq!(rows("\n"), 1);
        assert_eq!(rows("x\ny"), 2);
        assert_eq!(rows("x\n\n"), 2);
    }

    #[test]
    fn a_source_equals_a_clone_and_a_twin() {
        let source = CodeSource::new(Lang::Rust, "fn main() {}\n");
        assert!(source == source.clone());
        assert!(
            source == CodeSource::new(Lang::Rust, "fn main() {}\n")
        );
        assert!(
            source != CodeSource::new(Lang::Plain, "fn main() {}\n")
        );
        assert!(
            source != CodeSource::new(Lang::Rust, "fn main() {}")
        );
    }

    #[test]
    fn digits_count_the_widest_line_number() {
        assert_eq!(
            [1, 9, 10, 99_999, 100_000].map(digits),
            [1, 1, 2, 5, 6]
        );
    }
}
