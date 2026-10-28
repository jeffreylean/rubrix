use core::ops::Range;
use std::borrow::Cow;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Query;
use bevy::ecs::world::World;
use bevy::picking::Pickable;
use bevy::text::FontSource;
use bevy::ui::{AlignItems, Node, PositionType, UiRect, px};
use bevy_fynix::tokens::{SpacingTokens, Tone};
use bevy_fynix::views::{
    FrameProps, RowSlot, Span, Stack, column, rich, row, virtual_list,
};
use bevy_fynix::{
    AnyView, Bevy, Cx, Prop, Signal, View, ViewExt, component,
    derived, keyed,
};
use rubrix_highlight::{
    Highlights, Lang, Run, Stream, highlight, spans,
};

use crate::select::{self, CodeSelection, Highlight};
use crate::tokens::{Styles, SyntaxTokens};

/// Bevy's line height, as a factor of the font size.
const LINE_HEIGHT: f32 = 1.2;

const TAB: usize = 4;

const OVERSCAN: f32 = 400.0;

const SELECTED_ALPHA: f32 = 0.35;

const STREAM_OVERSCAN: f32 = 0.0;

/// A text and its highlights, or a stream of text still arriving.
///
/// A whole text is lexed once, when it is made, and a clone shares
/// both. Two whole sources are equal when one is a clone of the
/// other, or when they have the same language and text. A streaming
/// source, from [`CodeSource::streaming`], equals only its clones.
#[derive(Clone)]
pub struct CodeSource(Feed);

#[derive(Clone)]
enum Feed {
    Whole(Arc<Whole>),
    Stream(Arc<Inbox>),
}

struct Whole {
    text: Arc<str>,
    highlights: Highlights,
}

/// What a [`CodeStream`] sent, which every view of its source copies
/// from at its own pace.
struct Inbox {
    lang: Lang,
    sent: Mutex<Sent>,
}

#[derive(Default)]
struct Sent {
    text: String,
    lines: usize,
    finished: bool,
}

impl Inbox {
    fn sent(&self) -> MutexGuard<'_, Sent> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner)
    }
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
        CodeSource(Feed::Whole(Arc::new(Whole { text, highlights })))
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

    /// A source with no text yet, lexed as `lang` as the returned
    /// handle sends it text. Show [`CodeStream::source`] in a code
    /// view. Each line is coloured once its `\n` arrives, and the
    /// unfinished last line shows with the colours it would have if
    /// the text ended there. The view follows its last line while
    /// it is scrolled to the bottom.
    pub fn streaming(lang: Lang) -> CodeStream {
        CodeStream(Arc::new(Inbox {
            lang,
            sent: Mutex::default(),
        }))
    }

    pub fn lang(&self) -> Lang {
        match &self.0 {
            Feed::Whole(whole) => whole.highlights.lang(),
            Feed::Stream(inbox) => inbox.lang,
        }
    }

    /// The text it was made with. A streaming source was made with
    /// none: its text goes to the views that show it.
    pub fn text(&self) -> &str {
        match &self.0 {
            Feed::Whole(whole) => &whole.text,
            Feed::Stream(_) => "",
        }
    }

    /// The rows a code view shows for it: a row per line, but none
    /// for the empty line after a final newline. An empty text shows
    /// one row. A streaming source counts the text sent so far, and
    /// has no row before its first byte.
    pub fn rows(&self) -> usize {
        match &self.0 {
            Feed::Whole(whole) => {
                whole.highlights.line_count() as usize
                    - usize::from(whole.text.ends_with('\n'))
            }
            Feed::Stream(inbox) => {
                let sent = inbox.sent();
                if sent.text.is_empty() {
                    0
                } else {
                    sent.lines + 1
                        - usize::from(sent.text.ends_with('\n'))
                }
            }
        }
    }
}

impl PartialEq for CodeSource {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Feed::Whole(a), Feed::Whole(b)) => {
                Arc::ptr_eq(a, b)
                    || (a.highlights.lang() == b.highlights.lang()
                        && a.text == b.text)
            }
            (Feed::Stream(a), Feed::Stream(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// Sends text to the views of a streaming source, from
/// [`CodeSource::streaming`]. A clone sends to the same source.
#[derive(Clone)]
pub struct CodeStream(Arc<Inbox>);

impl CodeStream {
    /// The source to show in a code view. Every view of it shows all
    /// the text sent, whenever it was built.
    pub fn source(&self) -> CodeSource {
        CodeSource(Feed::Stream(self.0.clone()))
    }

    /// Sends `chunk`, which the views show from their next update.
    /// A chunk sent after [`CodeStream::finish`] is dropped.
    ///
    /// # Panics
    ///
    /// Panics when the text grows to 4 GiB or more.
    pub fn push(&self, chunk: &str) {
        let mut sent = self.0.sent();
        if sent.finished {
            return;
        }
        assert!(
            sent.text.len() + chunk.len() < u32::MAX as usize,
            "text of 4 GiB or more"
        );
        sent.lines += chunk.bytes().filter(|&b| b == b'\n').count();
        sent.text.push_str(chunk);
    }

    /// Ends the text, so its last line is lexed as the end of it, and
    /// the views end with the rows and runs a whole source of the
    /// same text gives.
    pub fn finish(&self) {
        self.0.sent().finished = true;
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
#[derive(Component)]
pub struct CodeDoc {
    source: CodeSource,
    text: Text,
    overflow: Overflow,
    revision: u64,
    edits: u64,
    list: Option<Entity>,
}

enum Text {
    Whole(Arc<Whole>),
    Growing(Box<Growing>),
}

/// A streaming source's text as far as this view has copied it.
struct Growing {
    text: String,
    /// The byte of each committed line's `\n`.
    ends: Vec<usize>,
    stream: Stream,
    /// The runs of the unfinished last line.
    partial: Vec<Run>,
}

impl Growing {
    fn new(lang: Lang) -> Growing {
        Growing {
            text: String::new(),
            ends: Vec::new(),
            stream: Stream::new(lang),
            partial: Vec::new(),
        }
    }

    fn push(&mut self, chunk: &str) {
        let base = self.text.len();
        self.text.push_str(chunk);
        self.stream.push(chunk);
        self.ends.extend(
            chunk.match_indices('\n').map(|(at, _)| base + at),
        );
        self.partial = self.stream.partial();
    }

    fn finish(&mut self) -> Whole {
        let stream =
            mem::replace(&mut self.stream, Stream::new(Lang::Plain));
        Whole {
            text: mem::take(&mut self.text).into(),
            highlights: stream.finish(),
        }
    }

    fn start(&self, row: usize) -> usize {
        row.checked_sub(1).map_or(0, |above| self.ends[above] + 1)
    }

    fn line(&self, row: usize) -> &str {
        let end =
            self.ends.get(row).copied().unwrap_or(self.text.len());
        let line = &self.text[self.start(row)..end];
        line.strip_suffix('\r').unwrap_or(line)
    }
}

impl CodeDoc {
    fn new(source: &CodeSource) -> CodeDoc {
        CodeDoc {
            source: source.clone(),
            text: match &source.0 {
                Feed::Whole(whole) => Text::Whole(whole.clone()),
                Feed::Stream(inbox) => {
                    Text::Growing(Box::new(Growing::new(inbox.lang)))
                }
            },
            overflow: Overflow::default(),
            revision: 0,
            edits: 0,
            list: None,
        }
    }

    /// See [`CodeSource::rows`]. A streaming source counts the text
    /// this view has shown so far, and shows no row before its first
    /// byte, so its view starts at the bottom and follows it.
    pub fn rows(&self) -> usize {
        match &self.text {
            Text::Whole(whole) => {
                whole.highlights.line_count() as usize
                    - usize::from(whole.text.ends_with('\n'))
            }
            Text::Growing(growing) if growing.text.is_empty() => 0,
            Text::Growing(growing) => {
                growing.ends.len() + 1
                    - usize::from(growing.text.ends_with('\n'))
            }
        }
    }

    /// The rows whose runs are final: every row, unless the last is
    /// a streaming source's unfinished line.
    pub fn committed(&self) -> usize {
        match &self.text {
            Text::Whole(_) => self.rows(),
            Text::Growing(growing) => growing.ends.len(),
        }
    }

    /// The text of row `row`, from 0, without its line ending. `None`
    /// past the last row.
    pub fn line(&self, row: usize) -> Option<&str> {
        (row < self.rows()).then(|| match &self.text {
            Text::Whole(whole) => {
                &whole.text[whole.highlights.line_range(row as u32)]
            }
            Text::Growing(growing) => growing.line(row),
        })
    }

    /// The runs of row `row`, from 0, as they were lexed. Empty for
    /// an empty line, and past the last row. An unfinished last line
    /// has the runs it would have if the text ended there.
    pub fn runs(&self, row: usize) -> &[Run] {
        if row >= self.rows() {
            return &[];
        }
        match &self.text {
            Text::Whole(whole) => {
                whole.highlights.stored_runs(row as u32)
            }
            Text::Growing(growing) if row < growing.ends.len() => {
                growing.stream.runs(row as u32)
            }
            Text::Growing(growing) => &growing.partial,
        }
    }

    /// Shows `source` from its first line, in a new list.
    fn show(&mut self, source: &CodeSource) {
        let shown = CodeDoc::new(source);
        self.source = shown.source;
        self.text = shown.text;
        self.revision += 1;
    }

    /// The byte range of row `row`'s text in [`CodeDoc::text`],
    /// without its line ending. Empty past the last row.
    pub fn line_range(&self, row: usize) -> Range<usize> {
        let Some(line) = self.line(row) else {
            let end = self.text().len();
            return end..end;
        };
        let start = match &self.text {
            Text::Whole(whole) => {
                whole.highlights.line_range(row as u32).start
            }
            Text::Growing(growing) => growing.start(row),
        };
        start..start + line.len()
    }

    /// The whole text, as far as this view has shown it.
    pub fn text(&self) -> &str {
        match &self.text {
            Text::Whole(whole) => &whole.text,
            Text::Growing(growing) => &growing.text,
        }
    }

    /// The list of the document shown now.
    pub(crate) fn list(&self) -> Option<Entity> {
        self.list
    }

    fn streamed(&self) -> bool {
        matches!(self.source.0, Feed::Stream(_))
    }

    /// Copies what the source sent since the last call, and returns
    /// whether there was any.
    fn take_sent(&mut self) -> bool {
        let (Feed::Stream(inbox), Text::Growing(growing)) =
            (&self.source.0, &mut self.text)
        else {
            return false;
        };
        let sent = inbox.sent();
        let read = growing.text.len();
        if sent.text.len() == read && !sent.finished {
            return false;
        }
        growing.push(&sent.text[read..]);
        let finished = sent.finished;
        drop(sent);
        if finished {
            let whole = growing.finish();
            self.text = Text::Whole(Arc::new(whole));
        }
        self.edits += 1;
        true
    }
}

/// Copies what each streaming source sent into the views showing it,
/// before the lists read their counts.
pub(crate) fn take_sent(mut docs: Query<&mut CodeDoc>) {
    for mut doc in &mut docs {
        if doc.streamed() && doc.bypass_change_detection().take_sent()
        {
            doc.set_changed();
        }
    }
}

/// On the rich text of each row of a code view: the view's root and
/// the row, from 0, it shows now.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeLine {
    pub view: Entity,
    pub row: usize,
}

#[derive(Clone)]
struct RowStyle {
    styles: Styles,
    font: FontSource,
    size: f32,
    gap: f32,
    selected: Color,
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
            selected: theme
                .tone(Tone::Accent)
                .with_alpha(SELECTED_ALPHA),
        };
        let root = cx.build(filling());
        cx.world
            .entity_mut(root)
            .insert((
                CodeDoc::new(&CodeSource::empty()),
                CodeSelection::default(),
            ))
            .observe(select::press)
            .observe(select::drag)
            .observe(select::release);
        cx.effect(root, self.source, |world, root, source| {
            let mut view = world.entity_mut(root);
            view.get_mut::<CodeDoc>()
                .expect("a code view")
                .show(source);
            view.get_mut::<CodeSelection>()
                .expect("a code view")
                .clear();
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

fn filling() -> Stack<()> {
    column(()).grow(1.0).min_width(px(0.0)).min_height(px(0.0))
}

fn list<T>(root: Entity, style: RowStyle) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let estimate = (style.size * LINE_HEIGHT).ceil();
        let count = component::<CodeDoc, usize>(root, |doc| {
            doc.map_or(1, CodeDoc::rows)
        });
        let follow = component::<CodeDoc, bool>(root, |doc| {
            doc.is_some_and(CodeDoc::streamed)
        });
        let sideways = component::<CodeDoc, bool>(root, |doc| {
            doc.is_none_or(|doc| doc.overflow == Overflow::Scroll)
        });
        let streamed = cx
            .world
            .get::<CodeDoc>(root)
            .is_some_and(CodeDoc::streamed);
        let overscan =
            if streamed { STREAM_OVERSCAN } else { OVERSCAN };
        let list = cx.build(
            virtual_list(count, estimate, move |slot| {
                code_row(root, slot, &style)
            })
            .overscan(overscan)
            .follow_bottom(follow)
            .sideways(sideways),
        );
        if let Some(mut doc) = cx.world.get_mut::<CodeDoc>(root) {
            doc.list = Some(list);
        }
        list
    })
}

fn code_row<T>(
    root: Entity,
    slot: RowSlot,
    style: &RowStyle,
) -> AnyView<Bevy, T>
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    let number = rich(gutter(root, slot))
        .font(style.font.clone())
        .size(style.size)
        .tone(Tone::Faint)
        .wrap(false);
    let gutter = row((number,))
        .shrink(0.0)
        .padding(UiRect::right(px(style.gap)));
    let styles = style.styles.clone();
    let line = rich(line(root, slot, move |doc, row| {
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
    let color = style.selected;
    let text = AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let cell = cx.spawn();
        *cx.world.get_mut::<Node>(cell).expect("a node") = Node {
            flex_grow: 1.0,
            min_width: px(0.0),
            ..Node::default()
        };
        let layer = cx.under(cell, Cx::spawn);
        let node = cx.under(cell, |cx| cx.build(line));
        cx.world.entity_mut(layer).insert((
            Node {
                position_type: PositionType::Absolute,
                ..Node::default()
            },
            Pickable::IGNORE,
            Highlight { text: node, color },
        ));
        cx.effect(
            node,
            slot.index().into(),
            move |world, node, &row| {
                world
                    .entity_mut(node)
                    .insert(CodeLine { view: root, row });
            },
        );
        cell
    });
    row((gutter, slot.sideways(text)))
        .align(AlignItems::FlexStart)
        .gap(0.0)
        .boxed()
}

/// The row's number, padded to the widest number. It is re-read
/// when the slot moves or the widest number gains a digit.
fn gutter(root: Entity, slot: RowSlot) -> Signal<Vec<Span>> {
    let width = move |world: &World| {
        world
            .get::<CodeDoc>(root)
            .map_or(1, |doc| digits(doc.rows()))
    };
    let row = slot.index();
    let mut index = slot.index();
    let mut shown = None;
    derived(move |world| {
        let width = width(world);
        vec![Span::new(format!("{:>width$}", row.get(world) + 1))]
    })
    .when(move |world| {
        let moved = index.changed(world);
        let now = Some(width(world));
        moved | (mem::replace(&mut shown, now) != now)
    })
}

/// The spans `show` makes of the row. It is re-read when the slot
/// moves, or when a streaming source adds text while the row shows
/// a line that is not yet committed, so a committed row is read once.
fn line(
    root: Entity,
    slot: RowSlot,
    show: impl Fn(&CodeDoc, usize) -> Vec<Span> + Send + Sync + 'static,
) -> Signal<Vec<Span>> {
    let row = slot.index();
    let mut index = slot.index();
    let mut read: Option<Read> = None;
    derived(move |world| {
        world
            .get::<CodeDoc>(root)
            .map_or_else(Vec::new, |doc| show(doc, row.get(world)))
    })
    .when(move |world| {
        let moved = index.changed(world);
        let doc = world.get::<CodeDoc>(root);
        let edits = doc.map(|doc| doc.edits);
        let fires = moved
            || read.is_none_or(|read| {
                !read.committed && read.edits != edits
            });
        if fires {
            let row = index.get(world);
            read = Some(Read {
                edits,
                committed: doc
                    .is_some_and(|doc| row < doc.committed()),
            });
        }
        fires
    })
}

/// What a row's last read saw.
#[derive(Clone, Copy)]
struct Read {
    edits: Option<u64>,
    committed: bool,
}

fn digits(rows: usize) -> usize {
    rows.checked_ilog10().map_or(1, |log| log as usize + 1)
}

pub(crate) struct Shown<'a> {
    text: Cow<'a, str>,
    tabs: Vec<Tab>,
}

struct Tab {
    byte: usize,
    spaces: usize,
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
                tabs.push(Tab { byte: at, spaces });
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

    /// The source byte of a shown byte. A byte inside a tab's spaces
    /// is the tab's start, or its end past its middle.
    pub(crate) fn source(&self, shown_byte: usize) -> usize {
        let mut extra = 0;
        for tab in &self.tabs {
            let at = tab.byte + extra;
            if shown_byte < at {
                break;
            }
            if shown_byte < at + tab.spaces {
                return tab.byte
                    + usize::from(shown_byte - at > tab.spaces / 2);
            }
            extra += tab.spaces - 1;
        }
        shown_byte - extra
    }

    pub(crate) fn shown(&self, source_byte: usize) -> usize {
        source_byte
            + self
                .tabs
                .iter()
                .take_while(|tab| tab.byte < source_byte)
                .map(|tab| tab.spaces - 1)
                .sum::<usize>()
    }
}

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
    fn a_shown_byte_maps_back_to_its_source_byte() {
        let shown = Shown::new("\tab\tc");
        let back =
            (0..=9).map(|at| shown.source(at)).collect::<Vec<_>>();
        assert_eq!(back, [0, 0, 0, 1, 1, 2, 3, 4, 4, 5]);
        assert_eq!(Shown::new("a 日").source(5), 5);
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
