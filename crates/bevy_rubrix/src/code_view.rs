use core::ops::Range;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::Query;
use bevy_fynix::tokens::SpacingTokens;
use bevy_fynix::views::virtual_list;
use bevy_fynix::{AnyView, Bevy, Cx, Prop, View, component, keyed};
use rubrix_highlight::{Highlights, Lang, Run, Stream, highlight};

use crate::rows::{
    LINE_HEIGHT, RowStyle, ViewFeed, code_row, filling,
};
use crate::select::{
    self, CodeSelection, Column, RowText, Selectable,
};
use crate::tokens::SyntaxTokens;

pub(crate) const OVERSCAN: f32 = 400.0;

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

impl Whole {
    fn rows(&self) -> usize {
        self.highlights.line_count() as usize
            - usize::from(self.text.ends_with('\n'))
    }
}

fn streamed_rows(lines: usize, text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        lines + 1 - usize::from(text.ends_with('\n'))
    }
}

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
    /// has no row before its first byte until it is finished.
    pub fn rows(&self) -> usize {
        match &self.0 {
            Feed::Whole(whole) => whole.rows(),
            Feed::Stream(inbox) => {
                let sent = inbox.sent();
                if sent.finished && sent.text.is_empty() {
                    1
                } else {
                    streamed_rows(sent.lines, &sent.text)
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
/// body size, or at [`CodeView::size`]. It builds only the rows near
/// what shows.
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
        size: None,
    }
}

/// See [`code_view`].
pub struct CodeView {
    source: Prop<CodeSource>,
    overflow: Prop<Overflow>,
    size: Option<f32>,
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

    /// The font size of its text and line numbers, in logical
    /// pixels, the theme's body size when unset. It is read when the
    /// view is built.
    pub fn size(mut self, size: f32) -> CodeView {
        self.size = Some(size);
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

struct Growing {
    text: String,
    newlines: Vec<usize>,
    stream: Stream,
    partial: Vec<Run>,
}

impl Growing {
    fn new(lang: Lang) -> Growing {
        Growing {
            text: String::new(),
            newlines: Vec::new(),
            stream: Stream::new(lang),
            partial: Vec::new(),
        }
    }

    fn push(&mut self, chunk: &str) {
        let base = self.text.len();
        self.text.push_str(chunk);
        self.stream.push(chunk);
        self.newlines.extend(
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
        row.checked_sub(1)
            .map_or(0, |above| self.newlines[above] + 1)
    }

    fn line(&self, row: usize) -> &str {
        let end = self
            .newlines
            .get(row)
            .copied()
            .unwrap_or(self.text.len());
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
            Text::Whole(whole) => whole.rows(),
            Text::Growing(growing) => {
                streamed_rows(growing.newlines.len(), &growing.text)
            }
        }
    }

    /// The rows whose runs are final: every row, unless the last is
    /// a streaming source's unfinished line.
    pub fn committed(&self) -> usize {
        match &self.text {
            Text::Whole(_) => self.rows(),
            Text::Growing(growing) => growing.newlines.len(),
        }
    }

    pub(crate) fn overflow(&self) -> Overflow {
        self.overflow
    }

    /// Rises each time a stream sends text.
    pub(crate) fn edits(&self) -> u64 {
        self.edits
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
            Text::Growing(growing)
                if row < growing.newlines.len() =>
            {
                growing.stream.runs(row as u32)
            }
            Text::Growing(growing) => &growing.partial,
        }
    }

    fn show(&mut self, source: &CodeSource) {
        let shown = CodeDoc::new(source);
        self.source = shown.source;
        self.text = shown.text;
        self.take_sent();
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

    fn streamed(&self) -> bool {
        matches!(self.source.0, Feed::Stream(_))
    }

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

pub(crate) fn take_sent(mut docs: Query<&mut CodeDoc>) {
    for mut doc in &mut docs {
        if doc.streamed() && doc.bypass_change_detection().take_sent()
        {
            doc.set_changed();
        }
    }
}

impl Selectable for CodeDoc {
    type At = usize;
    type Line = CodeLine;

    fn line_of(line: &CodeLine) -> (Entity, usize, Column) {
        (line.view, line.row, Column::Only)
    }

    fn list(&self) -> Option<Entity> {
        self.list
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn column(&self, _: f32) -> Column {
        Column::Only
    }

    fn text(&self, _: Column, row: usize) -> Option<RowText<'_>> {
        self.line(row).map(|line| RowText { lead: "", line })
    }

    fn at(&self, row: usize, byte: usize) -> usize {
        self.line_range(row).start + byte
    }

    fn on_row(
        &self,
        _: Column,
        row: usize,
        range: &Range<usize>,
    ) -> Range<usize> {
        let line = self.line_range(row);
        let from = range.start.max(line.start);
        let to = range.end.min(line.end).max(from);
        from - line.start..to - line.start
    }

    fn all(&self) -> Range<usize> {
        0..self.text().len()
    }

    fn copy(&self, _: Column, range: Range<usize>) -> String {
        self.text()[range].to_owned()
    }
}

/// On the rich text of each row of a code view: the view's root and
/// the row, from 0, it shows now.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeLine {
    pub view: Entity,
    pub row: usize,
}

impl<T> View<Bevy, T> for CodeView
where
    T: SyntaxTokens + SpacingTokens + Send + Sync + 'static,
{
    fn build(self, cx: &mut Cx<'_, Bevy, T>) -> Entity {
        let style = RowStyle::new(cx.theme(), self.size);
        let root = cx.build(filling());
        let mut view = cx.world.entity_mut(root);
        view.insert(CodeDoc::new(&CodeSource::empty()));
        select::attach::<CodeDoc>(&mut view);
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
                code_row(ViewFeed { view: root }, slot, &style)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
