use std::ops::Range;
use std::sync::Arc;

use bevy::asset::{Assets, Handle, uuid_handle};
use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::system::ResMut;
use bevy::ecs::world::{EntityWorldMut, Mut, World};
use bevy::image::{Image, ImageSampler};
use bevy::text::FontSource;
use bevy::ui::widget::{ImageNode, NodeImageMode};
use bevy::ui::{AlignItems, Display, JustifyContent, percent, px};
use bevy_fynix::patch::Paint;
use bevy_fynix::tokens::{
    MotionTokens, SpacingTokens, SurfaceTokens, Tone,
};
use bevy_fynix::views::{
    BehaviorExt, FrameProps, Rich, RowSlot, Span, VirtualList,
    button, column, ghost, label, rich, row, virtual_list,
};
use bevy_fynix::{
    AnyView, Bevy, Cx, Prop, ScopedExt, Signal, View, ViewExt,
    component, derived, keyed,
};
use rubrix_diff::{
    Anchor, Expansion, FileChange, FileDiff, Hunk, Layout, LineRef,
    Lines, Meta, PatchError, Row, Rows, Side, diff_texts,
    parse_patch, realign, rows,
};
use rubrix_highlight::{Highlights, Lang, Run, Stream, highlight};

use crate::code_view::{
    LINE_HEIGHT, Overflow, Shown, digits, filling, line_spans,
};
use crate::select::{self, Column, RowText, Selectable, selectable};
use crate::tokens::{Styles, SyntaxTokens, Tint};

const OVERSCAN: f32 = 100.0;

const CONTEXT: u32 = 3;

const HATCH_SIZE: u32 = 8;

const HATCH_ALPHA: f32 = 0.35;

pub(crate) const HATCH: Handle<Image> =
    uuid_handle!("5c0e1d39-8a7f-4a5e-9a51-2b7f3f4d6e21");

/// A diff to show: the files of a patch, or the old and new text of
/// one file. It is parsed, diffed and highlighted once, when it is
/// made, and a clone shares all of it. Two sources are equal only
/// when one is a clone of the other.
#[derive(Clone)]
pub struct DiffSource(Arc<[SourceFile]>);

struct SourceFile {
    diff: FileDiff,
    old: Runs,
    new: Runs,
    digits: usize,
}

/// A side's runs. A patch lexes each hunk apart, from the start
/// state, because the lines between its hunks are missing.
struct Runs {
    starts: Vec<u32>,
    parts: Vec<Highlights>,
}

impl Runs {
    fn new(lang: Lang, lines: &Lines, hunks: &[u32]) -> Runs {
        let mut runs = Runs {
            starts: Vec::new(),
            parts: Vec::new(),
        };
        let mut at = 0;
        for &len in hunks.iter().filter(|&&len| len > 0) {
            let mut stream = Stream::new(lang);
            for line in at..at + len {
                stream.push(lines.raw(line));
            }
            runs.starts.push(at);
            runs.parts.push(stream.finish());
            at += len;
        }
        runs
    }

    fn whole(lang: Lang, lines: &Lines) -> Runs {
        Runs {
            starts: vec![0],
            parts: vec![highlight(lang, lines.text())],
        }
    }

    fn line(&self, index: u32) -> &[Run] {
        let part =
            self.starts.partition_point(|&start| start <= index) - 1;
        self.parts[part].stored_runs(index - self.starts[part])
    }
}

impl DiffSource {
    /// The files of `patch`, as [`parse_patch`] reads them. Each
    /// hunk's lines on each side are streamed through the highlighter
    /// in the language its file's name names, each hunk from the
    /// start of a text, since the lines between hunks are missing. A
    /// patch holds only its hunk lines, so its gaps cannot expand.
    ///
    /// # Errors
    ///
    /// Fails where [`parse_patch`] fails.
    pub fn patch(patch: &str) -> Result<DiffSource, PatchError> {
        let files = parse_patch(patch)?
            .into_iter()
            .map(SourceFile::new)
            .collect();
        Ok(DiffSource(files))
    }

    /// The change from `old` to `new`, the two texts of the file at
    /// `path`, with 3 unchanged lines around each change. Each side
    /// is highlighted whole, in the language `path` names, and every
    /// gap can expand.
    ///
    /// # Panics
    ///
    /// Panics if either text is 4 GiB or longer.
    pub fn texts(path: &str, old: &str, new: &str) -> DiffSource {
        let change = match (old.is_empty(), new.is_empty()) {
            (true, false) => FileChange::Added,
            (false, true) => FileChange::Deleted,
            _ => FileChange::Modified,
        };
        let meta = Meta {
            change,
            old_name: Some(path.to_owned()),
            new_name: Some(path.to_owned()),
            ..Meta::default()
        };
        let diff = diff_texts(old, new, CONTEXT)
            .with_meta(meta)
            .expect("the change kind follows the empty sides");
        DiffSource(Arc::new([SourceFile::new(diff)]))
    }

    /// No files.
    pub fn empty() -> DiffSource {
        DiffSource(Arc::new([]))
    }

    /// Each file's diff, in order, after [`realign`].
    pub fn files(&self) -> impl ExactSizeIterator<Item = &FileDiff> {
        self.0.iter().map(|file| &file.diff)
    }
}

impl PartialEq for DiffSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl SourceFile {
    fn new(mut diff: FileDiff) -> SourceFile {
        realign(&mut diff);
        let path =
            diff.new_name().or(diff.old_name()).unwrap_or_default();
        let shown = [diff.new_lines(), diff.old_lines()]
            .into_iter()
            .find(|lines| !lines.is_empty());
        let first = shown.map_or("", |lines| lines.line(0));
        let lang = Lang::detect(path, first);
        let digits = digits(last_number(&diff) as usize);
        let side = |lines: &Lines, len: fn(&Hunk) -> u32| {
            if diff.is_partial() {
                let hunks =
                    diff.hunks().iter().map(len).collect::<Vec<_>>();
                Runs::new(lang, lines, &hunks)
            } else {
                Runs::whole(lang, lines)
            }
        };
        SourceFile {
            old: side(diff.old_lines(), Hunk::old_len),
            new: side(diff.new_lines(), Hunk::new_len),
            diff,
            digits,
        }
    }

    fn side(&self, lane: Lane) -> (&Lines, &Runs) {
        match lane {
            Lane::Old => (self.diff.old_lines(), &self.old),
            Lane::New => (self.diff.new_lines(), &self.new),
        }
    }
}

fn last_number(diff: &FileDiff) -> u32 {
    if !diff.is_partial() {
        return diff.old_lines().len().max(diff.new_lines().len());
    }
    let rows = rows(diff, Layout::Stacked, &Expansion::new());
    rows.range(..)
        .flat_map(|row| [row.old_line(), row.new_line()])
        .flatten()
        .map(|line| line.number.get())
        .max()
        .unwrap_or(0)
}

/// A view of `source`: a header row per file, then its rows as
/// [`rows()`] derives them, all in one virtual list that builds only
/// the rows near what shows. Each line row has its old and new line
/// numbers in a gutter. A removed row is tinted with the theme's
/// [`SyntaxTokens::deletion`] and an added one with its
/// [`SyntaxTokens::addition`], mixed into its
/// [`SurfaceTokens::panel`], and the words that changed between a
/// removed line and the added line it pairs with take a stronger
/// tint. A click on a header collapses or expands its file, and a
/// gap between hunks of a text pair has buttons that reveal its
/// lines.
///
/// A new source builds the rows again, with every file expanded and
/// every gap collapsed, and shows its first row. Its colours come
/// from the theme when it is built.
pub fn diff_view(source: impl Into<Prop<DiffSource>>) -> DiffView {
    DiffView {
        source: source.into(),
        layout: Prop::Value(Layout::Stacked),
        compact: Prop::Value(false),
        overflow: Prop::Value(Overflow::Scroll),
        limit: Prop::Value(None),
    }
}

/// See [`diff_view`].
pub struct DiffView {
    source: Prop<DiffSource>,
    layout: Prop<Layout>,
    compact: Prop<bool>,
    overflow: Prop<Overflow>,
    limit: Prop<Option<u32>>,
}

impl DiffView {
    /// Stacked rows, or split rows with the old line on the left
    /// and the new line on the right, [`Layout::Stacked`] when unset.
    /// A change keeps the first source line that shows at the top.
    pub fn layout(mut self, layout: impl Into<Prop<Layout>>) -> Self {
        self.layout = layout.into();
        self
    }

    /// Whether it draws as a compact block: stacked rows with no
    /// gutter, each line's text after a `+ `, `- ` or two-space
    /// prefix, and wrapped. It wins over [`layout`](Self::layout) and
    /// [`overflow`](Self::overflow) while it holds.
    pub fn compact(mut self, compact: impl Into<Prop<bool>>) -> Self {
        self.compact = compact.into();
        self
    }

    /// What it does with lines wider than itself, as a code view
    /// does. [`Overflow::Scroll`] scrolls both sides of a split row
    /// sideways together, and [`Overflow::Wrap`] makes each row as
    /// tall as its taller side.
    pub fn overflow(
        mut self,
        overflow: impl Into<Prop<Overflow>>,
    ) -> Self {
        self.overflow = overflow.into();
        self
    }

    /// Shows only the first `n` rows that show a line, then a
    /// `... M more lines` button that shows the rest. Headers and
    /// gap rows do not count. `None`, as when unset, shows every
    /// row.
    pub fn limit(
        mut self,
        limit: impl Into<Prop<Option<u32>>>,
    ) -> Self {
        self.limit = limit.into();
        self
    }
}

/// One row of a diff view, by its index from the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffRow {
    /// File `file`'s header.
    Header { file: usize },
    /// A row of file `file`, as [`rows()`] derives it for the view's
    /// layout and the file's expansion.
    Body { file: usize, row: Row },
    /// The button that shows the `lines` rows a limit hides.
    More { lines: u32 },
}

/// What a diff view shows, on the view's root entity. The view's
/// buttons call its methods, and so may an app.
#[derive(Component)]
pub struct DiffDoc {
    source: DiffSource,
    files: Vec<FileRows>,
    layout: Layout,
    compact: bool,
    overflow: Overflow,
    limit: Option<u32>,
    header_rows: Vec<usize>,
    len: usize,
    more: Option<More>,
    lists_built: u64,
    revision: u64,
    list: Option<Entity>,
    pending_scroll: Option<usize>,
}

struct FileRows {
    rows: Rows,
    expansion: Expansion,
    collapsed: bool,
}

#[derive(Clone, Copy)]
struct More {
    row: usize,
    lines: u32,
}

#[derive(Clone, Copy)]
enum Place {
    Header(usize),
    Body(usize, Anchor),
    More,
}

impl DiffDoc {
    fn new() -> DiffDoc {
        DiffDoc {
            source: DiffSource::empty(),
            files: Vec::new(),
            layout: Layout::Stacked,
            compact: false,
            overflow: Overflow::Scroll,
            limit: None,
            header_rows: Vec::new(),
            len: 0,
            more: None,
            lists_built: 0,
            revision: 0,
            list: None,
            pending_scroll: None,
        }
    }

    pub fn source(&self) -> &DiffSource {
        &self.source
    }

    /// The layout its rows have: stacked while it is compact.
    pub fn layout(&self) -> Layout {
        if self.compact {
            Layout::Stacked
        } else {
            self.layout
        }
    }

    pub fn is_compact(&self) -> bool {
        self.compact
    }

    pub fn overflow(&self) -> Overflow {
        if self.compact {
            Overflow::Wrap
        } else {
            self.overflow
        }
    }

    /// Every row it lays out.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Row `index`, `None` past the last.
    pub fn row(&self, index: usize) -> Option<DiffRow> {
        if let Some(more) = self.more
            && index >= more.row
        {
            return (index == more.row)
                .then_some(DiffRow::More { lines: more.lines });
        }
        if index >= self.len {
            return None;
        }
        let file =
            self.header_rows.partition_point(|&s| s <= index) - 1;
        match index - self.header_rows[file] {
            0 => Some(DiffRow::Header { file }),
            at => {
                let row = self.files[file].rows.get(at as u32 - 1)?;
                Some(DiffRow::Body { file, row })
            }
        }
    }

    pub fn is_collapsed(&self, file: usize) -> bool {
        self.files.get(file).is_some_and(|file| file.collapsed)
    }

    /// Collapses file `file` to its header, or expands it again.
    pub fn toggle(&mut self, file: usize) {
        if let Some(shown) = self.files.get_mut(file) {
            shown.collapsed = !shown.collapsed;
            self.index();
        }
    }

    /// Reveals [`Expansion::STEP`] more lines of file `file`'s gap
    /// above hunk `hunk` from the `side` end, or all of it, as
    /// [`Expansion::reveal`] does. A gap row names its `hunk`.
    pub fn reveal(&mut self, file: usize, hunk: u32, side: Side) {
        let layout = self.layout();
        let Some(shown) = self.files.get_mut(file) else {
            return;
        };
        shown.expansion.reveal(hunk, side, Expansion::STEP);
        shown.rows =
            rows(&self.source.0[file].diff, layout, &shown.expansion);
        self.index();
    }

    /// Shows the rows a limit hides.
    pub fn show_all(&mut self) {
        self.limit = None;
        self.index();
    }

    fn show(&mut self, source: DiffSource) {
        let layout = self.layout();
        self.files = source
            .0
            .iter()
            .map(|file| FileRows {
                rows: rows(&file.diff, layout, &Expansion::new()),
                expansion: Expansion::new(),
                collapsed: false,
            })
            .collect();
        self.source = source;
        self.pending_scroll = None;
        self.lists_built += 1;
        self.index();
    }

    fn relayout_keeping(
        &mut self,
        first: Option<usize>,
        set: impl FnOnce(&mut DiffDoc),
    ) {
        let place = first.and_then(|first| self.place(first));
        set(self);
        let layout = self.layout();
        for (shown, file) in
            self.files.iter_mut().zip(self.source.0.iter())
        {
            shown.rows = rows(&file.diff, layout, &shown.expansion);
        }
        self.index();
        self.pending_scroll =
            place.and_then(|place| self.find(place));
        self.lists_built += 1;
    }

    fn set_limit(&mut self, limit: Option<u32>) {
        self.limit = limit;
        self.index();
    }

    fn index(&mut self) {
        self.revision += 1;
        self.header_rows.clear();
        let mut at = 0;
        for file in &self.files {
            self.header_rows.push(at);
            at += 1;
            if !file.collapsed {
                at += file.rows.len() as usize;
            }
        }
        self.len = at;
        self.more =
            self.limit.and_then(|limit| self.more_after(limit));
        if let Some(more) = self.more {
            self.len = more.row + 1;
        }
    }

    fn more_after(&self, limit: u32) -> Option<More> {
        let mut lines = 0;
        let mut cut = None;
        for (file, shown) in self.files.iter().enumerate() {
            if shown.collapsed {
                continue;
            }
            for (at, row) in shown.rows.range(..).enumerate() {
                if matches!(row, Row::Gap { .. }) {
                    continue;
                }
                if lines == limit && cut.is_none() {
                    cut = Some(self.header_rows[file] + 1 + at);
                }
                lines += 1;
            }
        }
        cut.map(|row| More {
            row,
            lines: lines - limit,
        })
    }

    fn place(&self, index: usize) -> Option<Place> {
        Some(match self.row(index)? {
            DiffRow::Header { file } => Place::Header(file),
            DiffRow::Body { file, row } => {
                Place::Body(file, row.anchor())
            }
            DiffRow::More { .. } => Place::More,
        })
    }

    fn find(&self, place: Place) -> Option<usize> {
        let index = match place {
            Place::Header(file) => *self.header_rows.get(file)?,
            Place::Body(file, _) if self.is_collapsed(file) => {
                *self.header_rows.get(file)?
            }
            Place::Body(file, anchor) => {
                let row = self.files.get(file)?.rows.find(anchor)?;
                self.header_rows[file] + 1 + row as usize
            }
            Place::More => self.more?.row,
        };
        (index < self.len).then_some(index)
    }

    fn file(&self, file: usize) -> &SourceFile {
        &self.source.0[file]
    }

    fn body(&self, index: usize) -> Option<(&SourceFile, Row)> {
        match self.row(index)? {
            DiffRow::Body { file, row } => {
                Some((self.file(file), row))
            }
            DiffRow::Header { .. } | DiffRow::More { .. } => None,
        }
    }

    fn sideways(&self) -> bool {
        self.overflow() == Overflow::Scroll
    }
}

/// A place in a diff view's text: byte `byte` of row `row`'s text.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord,
)]
pub(crate) struct Spot {
    row: usize,
    byte: usize,
}

/// On the rich text of each line row of a diff view, a side's in a
/// split row: the view's root, the row's index, and the column.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffLine {
    view: Entity,
    index: usize,
    column: Column,
}

/// The bytes of a row's text of `len` bytes that `range` covers.
fn covered(
    row: usize,
    len: usize,
    range: &Range<Spot>,
) -> Range<usize> {
    if row < range.start.row || row > range.end.row {
        return 0..0;
    }
    let from = if row == range.start.row {
        range.start.byte.min(len)
    } else {
        0
    };
    let to = if row == range.end.row {
        range.end.byte.min(len)
    } else {
        len
    };
    from..to.max(from)
}

/// A selection takes a line row's text: in compact rows with its
/// prefix, in split rows from its column's side. Header, gap and
/// `more` rows, and a split side with no line, have none.
impl Selectable for DiffDoc {
    type At = Spot;
    type Line = DiffLine;

    fn line_of(line: &DiffLine) -> (Entity, usize, Column) {
        (line.view, line.index, line.column)
    }

    fn list(&self) -> Option<Entity> {
        self.list
    }

    fn revision(&self) -> u64 {
        self.revision
    }

    fn column(&self, across: f32) -> Column {
        match self.layout() {
            Layout::Stacked => Column::Only,
            Layout::Split if across < 0.5 => Column::Old,
            Layout::Split => Column::New,
        }
    }

    fn text(
        &self,
        column: Column,
        index: usize,
    ) -> Option<RowText<'_>> {
        let (file, row) = self.body(index)?;
        let (lane, line, lead) = match column {
            Column::Only => {
                let (lane, line, prefix) = stacked_line(row)?;
                (lane, line, if self.compact { prefix } else { "" })
            }
            Column::Old => (Lane::Old, row.old_line()?, ""),
            Column::New => (Lane::New, row.new_line()?, ""),
        };
        let line = file.side(lane).0.line(line.index);
        Some(RowText { lead, line })
    }

    fn at(&self, row: usize, byte: usize) -> Spot {
        Spot { row, byte }
    }

    fn on_row(
        &self,
        column: Column,
        row: usize,
        range: &Range<Spot>,
    ) -> Range<usize> {
        if row < range.start.row || row > range.end.row {
            return 0..0;
        }
        let len = self.text(column, row).map_or(0, |text| text.len());
        covered(row, len, range)
    }

    fn all(&self) -> Range<Spot> {
        Spot::default()..Spot {
            row: self.len,
            byte: 0,
        }
    }

    fn copy(&self, column: Column, range: Range<Spot>) -> String {
        let mut out = String::new();
        let rows = (range.start.row..=range.end.row)
            .take_while(|&row| row < self.len)
            .filter_map(|row| Some((row, self.text(column, row)?)));
        for (at, (row, text)) in rows.enumerate() {
            if at > 0 {
                out.push('\n');
            }
            text.push(covered(row, text.len(), &range), &mut out);
        }
        out
    }
}

/// On each row of a diff view: the view's root and the index of the
/// row it shows now, for [`DiffDoc::row`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffRowAt {
    pub view: Entity,
    pub index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    Old,
    New,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Stacked,
    Split,
    Compact,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Header,
    Line,
    Gap,
    More,
}

impl Kind {
    fn of(row: DiffRow) -> Kind {
        match row {
            DiffRow::Header { .. } => Kind::Header,
            DiffRow::Body {
                row: Row::Gap { .. },
                ..
            } => Kind::Gap,
            DiffRow::Body { .. } => Kind::Line,
            DiffRow::More { .. } => Kind::More,
        }
    }
}

trait Tokens:
    SyntaxTokens
    + SpacingTokens
    + SurfaceTokens
    + MotionTokens
    + Send
    + Sync
    + 'static
{
}

impl<T> Tokens for T where
    T: SyntaxTokens
        + SpacingTokens
        + SurfaceTokens
        + MotionTokens
        + Send
        + Sync
        + 'static
{
}

struct RowStyle {
    styles: Styles,
    font: FontSource,
    size: f32,
    gap: f32,
    added: Color,
    removed: Color,
    add: Tint,
    delete: Tint,
    faint: Color,
    header: Color,
    selected: Color,
}

impl RowStyle {
    fn new<T: SyntaxTokens + SpacingTokens + SurfaceTokens>(
        theme: &T,
    ) -> RowStyle {
        let (added, removed) = (theme.addition(), theme.deletion());
        let panel = theme.panel();
        RowStyle {
            styles: Styles::new(theme),
            font: theme.code_font(),
            size: theme.body_size(),
            gap: theme.gap(),
            added,
            removed,
            add: Tint::new(panel, added),
            delete: Tint::new(panel, removed),
            faint: theme.tone(Tone::Faint),
            header: theme.fill(),
            selected: select::selected_colour(theme),
        }
    }

    fn estimate(&self) -> f32 {
        (self.size * LINE_HEIGHT).ceil()
    }

    fn tint(&self, lane: Lane) -> &Tint {
        match lane {
            Lane::Old => &self.delete,
            Lane::New => &self.add,
        }
    }
}

impl<T> View<Bevy, T> for DiffView
where
    T: SyntaxTokens
        + SpacingTokens
        + SurfaceTokens
        + MotionTokens
        + Send
        + Sync
        + 'static,
{
    fn build(self, cx: &mut Cx<'_, Bevy, T>) -> Entity {
        let style = Arc::new(RowStyle::new(cx.theme()));
        let root = cx.build(filling());
        let mut view = cx.world.entity_mut(root);
        view.insert(DiffDoc::new());
        select::attach::<DiffDoc>(&mut view);
        cx.effect(root, self.source, |world, root, source| {
            doc_mut(world, root).show(source.clone());
        });
        cx.effect(root, self.layout, |world, root, &layout| {
            reshape(world, root, |doc| doc.layout = layout);
        });
        cx.effect(root, self.compact, |world, root, &compact| {
            reshape(world, root, |doc| doc.compact = compact);
        });
        cx.effect(root, self.overflow, |world, root, &overflow| {
            doc_mut(world, root).overflow = overflow;
        });
        cx.effect(root, self.limit, |world, root, &limit| {
            doc_mut(world, root).set_limit(limit);
        });
        let lists_built = component::<DiffDoc, u64>(root, |doc| {
            doc.map_or(0, |doc| doc.lists_built)
        });
        cx.under(root, |cx| {
            cx.build(
                keyed(lists_built, move |_| {
                    list(root, style.clone())
                })
                .within(filling()),
            )
        });
        root
    }
}

fn doc_mut(world: &mut World, root: Entity) -> Mut<'_, DiffDoc> {
    world.get_mut::<DiffDoc>(root).expect("a diff view")
}

fn reshape(
    world: &mut World,
    root: Entity,
    set: impl FnOnce(&mut DiffDoc),
) {
    let list = world.get::<DiffDoc>(root).and_then(|doc| doc.list);
    let first = list
        .and_then(|list| world.get::<VirtualList>(list))
        .map(VirtualList::first_visible);
    doc_mut(world, root).relayout_keeping(first, set);
}

fn list<T>(root: Entity, style: Arc<RowStyle>) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let doc = cx.world.get::<DiffDoc>(root).expect("a diff view");
        let shape = match (doc.compact, doc.layout) {
            (true, _) => Shape::Compact,
            (false, Layout::Stacked) => Shape::Stacked,
            (false, Layout::Split) => Shape::Split,
        };
        let count = component::<DiffDoc, usize>(root, |doc| {
            doc.map_or(0, DiffDoc::len)
        });
        let sideways = component::<DiffDoc, bool>(root, |doc| {
            doc.is_some_and(DiffDoc::sideways)
        });
        let rows = style.clone();
        let list = cx.build(
            virtual_list(count, style.estimate(), move |slot| {
                slot_view(root, slot, &rows, shape)
            })
            .overscan(OVERSCAN)
            .sideways(sideways),
        );
        let mut doc = doc_mut(cx.world, root);
        doc.list = Some(list);
        if let Some(pending_scroll) = doc.pending_scroll.take()
            && let Some(mut list) =
                cx.world.get_mut::<VirtualList>(list)
        {
            list.scroll_to(pending_scroll);
        }
        list
    })
}

fn row_signal<V: Default + 'static>(
    root: Entity,
    slot: RowSlot,
    read: impl Fn(&DiffDoc, usize) -> V + Send + Sync + 'static,
) -> Signal<V> {
    let index = slot.index();
    let mut moved = slot.index();
    let mut changed = component::<DiffDoc, ()>(root, |_| ());
    derived(move |world| {
        world
            .get::<DiffDoc>(root)
            .map(|doc| read(doc, index.get(world)))
            .unwrap_or_default()
    })
    .when(move |world| moved.changed(world) | changed.changed(world))
}

fn slot_view<T>(
    root: Entity,
    slot: RowSlot,
    style: &Arc<RowStyle>,
    shape: Shape,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let kind = row_signal(root, slot, |doc, index| {
        doc.row(index).map(Kind::of)
    });
    let style = style.clone();
    let content = keyed(kind, move |kind| {
        let style = &style;
        match kind {
            Some(Kind::Header) => header(root, slot, style),
            Some(Kind::Gap) => gap(root, slot, style),
            Some(Kind::More) => more(root, slot),
            Some(Kind::Line) => match shape {
                Shape::Stacked => stacked(root, slot, style, false),
                Shape::Compact => stacked(root, slot, style, true),
                Shape::Split => split(root, slot, style),
            },
            None => column(()).boxed(),
        }
    })
    .within(column(()).width(percent(100.0)));
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let node = cx.build(content);
        cx.effect(
            node,
            slot.index().into(),
            move |world, node, &index| {
                world
                    .entity_mut(node)
                    .insert(DiffRowAt { view: root, index });
            },
        );
        node
    })
}

fn code(spans: Signal<Vec<Span>>, style: &RowStyle) -> Rich {
    rich(spans).font(style.font.clone()).size(style.size)
}

/// The line numbers of a row, faint and still while the text beside
/// them scrolls sideways. A space pads them from the text, which
/// costs a frame less than a padded node around them.
fn gutter(numbers: Signal<String>, style: &RowStyle) -> Rich {
    rich(numbers.map(|numbers| vec![Span::new(numbers + " ")]))
        .font(style.font.clone())
        .size(style.size)
        .tone(Tone::Faint)
        .wrap(false)
}

fn wraps(root: Entity) -> Signal<bool> {
    component::<DiffDoc, bool>(root, |doc| {
        doc.is_some_and(|doc| doc.overflow() == Overflow::Wrap)
    })
}

fn stacked<T>(
    root: Entity,
    slot: RowSlot,
    style: &Arc<RowStyle>,
    compact: bool,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let spans = style.clone();
    let text = code(
        row_signal(root, slot, move |doc, index| {
            doc.body(index).map_or_else(Vec::new, |(file, row)| {
                stacked_spans(file, row, &spans, compact)
            })
        }),
        style,
    )
    .wrap(wraps(root));
    let tints = style.clone();
    let fill = row_signal(root, slot, move |doc, index| {
        match doc.body(index) {
            Some((_, Row::Deletion { .. })) => tints.delete.row,
            Some((_, Row::Addition { .. })) => tints.add.row,
            _ => Color::NONE,
        }
    });
    let text = slot.sideways(selectable(
        text,
        slot,
        style.selected,
        move |index| DiffLine {
            view: root,
            index,
            column: Column::Only,
        },
    ));
    if compact {
        return row((text,)).gap(0.0).fill(fill).boxed();
    }
    let numbers = row_signal(root, slot, |doc, index| {
        doc.body(index).map_or_else(String::new, |(file, row)| {
            let width = file.digits;
            let old = number(row.old_line(), width);
            let new = number(row.new_line(), width);
            format!("{old} {new}")
        })
    });
    row((gutter(numbers, style), text))
        .align(AlignItems::FlexStart)
        .gap(0.0)
        .fill(fill)
        .boxed()
}

fn split<T>(
    root: Entity,
    slot: RowSlot,
    style: &Arc<RowStyle>,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    row((
        side::<T>(root, slot, style, Lane::Old),
        side::<T>(root, slot, style, Lane::New),
    ))
    .gap(style.gap)
    .boxed()
}

fn side<T>(
    root: Entity,
    slot: RowSlot,
    style: &Arc<RowStyle>,
    lane: Lane,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let numbers = row_signal(root, slot, move |doc, index| {
        doc.body(index).map_or_else(String::new, |(file, row)| {
            number(line_of(row, lane), file.digits)
        })
    });
    let spans = style.clone();
    let text = code(
        row_signal(root, slot, move |doc, index| {
            doc.body(index).map_or_else(Vec::new, |(file, row)| {
                side_spans(file, row, lane, &spans)
            })
        }),
        style,
    )
    .wrap(wraps(root));
    let tints = style.clone();
    let fill = row_signal(root, slot, move |doc, index| {
        match doc.body(index).map(|(_, row)| (row, lane)) {
            Some((
                Row::Paired { .. } | Row::Deletion { .. },
                Lane::Old,
            ))
            | Some((
                Row::Paired { .. } | Row::Addition { .. },
                Lane::New,
            )) => tints.tint(lane).row,
            _ => Color::NONE,
        }
    });
    let column = match lane {
        Lane::Old => Column::Old,
        Lane::New => Column::New,
    };
    let text = selectable(text, slot, style.selected, move |index| {
        DiffLine {
            view: root,
            index,
            column,
        }
    });
    let cell = row((gutter(numbers, style), slot.sideways(text)))
        .align(AlignItems::FlexStart)
        .gap(0.0)
        .grow(1.0)
        .width(px(0.0))
        .min_width(px(0.0))
        .fill(fill);
    let hatch = style.faint.with_alpha(HATCH_ALPHA);
    let empty = row_signal(root, slot, move |doc, index| {
        doc.body(index).is_some_and(|(_, row)| {
            !matches!(row, Row::Gap { .. })
                && line_of(row, lane).is_none()
        })
    });
    AnyView::new(move |cx: &mut Cx<'_, Bevy, T>| {
        let node = cx.build(cell);
        cx.effect(node, empty.into(), move |world, node, &empty| {
            let mut node = world.entity_mut(node);
            if empty {
                show_hatch(&mut node, hatch);
            } else {
                node.remove::<ImageNode>();
            }
        });
        node
    })
}

/// Tiles the hatch over `cell` in `colour`, through the cell's
/// [`Paint`] tint as well, which each fill change repaints its image
/// from.
fn show_hatch(cell: &mut EntityWorldMut, colour: Color) {
    let mut paint = cell.get::<Paint>().copied().unwrap_or_default();
    paint.tint = Some(colour);
    cell.insert((
        paint,
        ImageNode {
            image: HATCH,
            color: colour,
            image_mode: NodeImageMode::Tiled {
                tile_x: true,
                tile_y: true,
                stretch_value: 1.0,
            },
            ..ImageNode::default()
        },
    ));
}

fn line_of(row: Row, lane: Lane) -> Option<LineRef> {
    match lane {
        Lane::Old => row.old_line(),
        Lane::New => row.new_line(),
    }
}

fn number(line: Option<LineRef>, width: usize) -> String {
    match line {
        Some(line) => format!("{:>width$}", line.number.get()),
        None => " ".repeat(width),
    }
}

/// The side a stacked row's line is on, the line, and the prefix a
/// compact row writes before it. `None` for a gap row.
fn stacked_line(row: Row) -> Option<(Lane, LineRef, &'static str)> {
    match row {
        Row::Deletion { old } => Some((Lane::Old, old, "- ")),
        Row::Addition { new } => Some((Lane::New, new, "+ ")),
        Row::Context { new, .. } | Row::Expanded { new, .. } => {
            Some((Lane::New, new, "  "))
        }
        Row::Paired { .. } | Row::Gap { .. } => None,
    }
}

fn stacked_spans(
    file: &SourceFile,
    row: Row,
    style: &RowStyle,
    compact: bool,
) -> Vec<Span> {
    let Some((lane, line, prefix)) = stacked_line(row) else {
        return Vec::new();
    };
    let mut spans = line_text(file, row, lane, line, style);
    if compact {
        let colour = match row {
            Row::Deletion { .. } => Some(style.removed),
            Row::Addition { .. } => Some(style.added),
            _ => None,
        };
        let mut mark = Span::new(prefix);
        mark.look.color = colour;
        spans.insert(0, mark);
    }
    spans
}

fn side_spans(
    file: &SourceFile,
    row: Row,
    lane: Lane,
    style: &RowStyle,
) -> Vec<Span> {
    line_of(row, lane).map_or_else(Vec::new, |line| {
        line_text(file, row, lane, line, style)
    })
}

fn line_text(
    file: &SourceFile,
    row: Row,
    lane: Lane,
    line: LineRef,
    style: &RowStyle,
) -> Vec<Span> {
    let (lines, highlights) = file.side(lane);
    let text = lines.line(line.index);
    let spans =
        line_spans(text, highlights.line(line.index), &style.styles);
    let marks = match row {
        Row::Paired { .. }
        | Row::Deletion { .. }
        | Row::Addition { .. } => row.words(&file.diff),
        _ => None,
    };
    let Some((old, new)) = marks else {
        return spans;
    };
    let marks = match lane {
        Lane::Old => old,
        Lane::New => new,
    };
    let shown = Shown::new(text);
    let marks = marks
        .iter()
        .map(|range| {
            shown.shown(range.start as usize)
                ..shown.shown(range.end as usize)
        })
        .collect::<Vec<_>>();
    mark(spans, &marks, style.tint(lane).word)
}

fn mark(
    spans: Vec<Span>,
    marks: &[Range<usize>],
    fill: Color,
) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len() + 2 * marks.len());
    let mut marks = marks.iter().peekable();
    let mut start = 0;
    for span in spans {
        let end = start + span.text.len();
        let mut at = start;
        while at < end {
            while marks.next_if(|mark| mark.end <= at).is_some() {}
            let (to, marked) = match marks.peek() {
                Some(mark) if mark.start <= at => {
                    (mark.end.min(end), true)
                }
                Some(mark) => (mark.start.min(end), false),
                None => (end, false),
            };
            let mut piece = Span {
                text: span.text[at - start..to - start].to_owned(),
                look: span.look.clone(),
            };
            if marked {
                piece.look.background = Some(fill);
            }
            out.push(piece);
            at = to;
        }
        start = end;
    }
    out
}

fn header<T>(
    root: Entity,
    slot: RowSlot,
    style: &Arc<RowStyle>,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let spans = style.clone();
    let text = code(
        row_signal(root, slot, move |doc, index| {
            match doc.row(index) {
                Some(DiffRow::Header { file }) => header_spans(
                    &doc.file(file).diff,
                    doc.is_collapsed(file),
                    &spans,
                ),
                _ => Vec::new(),
            }
        }),
        style,
    )
    .wrap(false);
    let toggle = button(text)
        .width(percent(100.0))
        .justify(JustifyContent::Start)
        .rules(ghost)
        .on_activate(move |world| {
            let index = slot.index().get(world);
            let file = world
                .get::<DiffDoc>(root)
                .and_then(|doc| doc.row(index));
            if let Some(DiffRow::Header { file }) = file {
                doc_mut(world, root).toggle(file);
            }
        });
    row((toggle,)).fill(style.header).boxed()
}

fn header_spans(
    diff: &FileDiff,
    collapsed: bool,
    style: &RowStyle,
) -> Vec<Span> {
    let coloured = |text: String, color: Color| {
        let mut span = Span::new(text);
        span.look.color = Some(color);
        span
    };
    let chevron = if collapsed { "▸ " } else { "▾ " };
    let mut spans = vec![coloured(chevron.into(), style.faint)];
    let old = diff.old_name().unwrap_or_default();
    let new = diff.new_name().unwrap_or(old);
    let moved = matches!(
        diff.change(),
        FileChange::Renamed
            | FileChange::RenamedModified
            | FileChange::Copied
    ) && old != new;
    spans.push(Span::new(if moved {
        format!("{old} → {new}")
    } else {
        new.to_owned()
    }));
    let mut notes = Vec::new();
    if let (Some(old), Some(new)) = (diff.old_mode(), diff.new_mode())
        && old != new
    {
        notes.push(format!("mode {old:o} → {new:o}"));
    }
    notes.extend(
        match diff.change() {
            FileChange::Added => Some("new file"),
            FileChange::Deleted => Some("deleted"),
            FileChange::Copied => Some("copy"),
            FileChange::Binary => Some("binary"),
            _ => None,
        }
        .map(str::to_owned),
    );
    for note in notes {
        spans.push(coloured(format!("  {note}"), style.faint));
    }
    if diff.change() != FileChange::Binary {
        spans.push(coloured(
            format!("  +{}", diff.added()),
            style.added,
        ));
        spans.push(coloured(
            format!(" -{}", diff.removed()),
            style.removed,
        ));
    }
    spans
}

fn gap<T>(
    root: Entity,
    slot: RowSlot,
    style: &Arc<RowStyle>,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let text = code(
        row_signal(root, slot, |doc, index| match doc.body(index) {
            Some((_, Row::Gap { hidden, .. })) => {
                let lines =
                    if hidden == 1 { "line" } else { "lines" };
                vec![Span::new(format!("⋯ {hidden} hidden {lines}"))]
            }
            _ => Vec::new(),
        }),
        style,
    )
    .tone(Tone::Faint)
    .wrap(false);
    row((
        expand::<T>(root, slot, "↑ up", Side::Up),
        expand::<T>(root, slot, "↓ down", Side::Down),
        expand::<T>(root, slot, "↕ all", Side::All),
        text,
    ))
    .align(AlignItems::Center)
    .gap(style.gap)
    .fill(style.header)
    .boxed()
}

fn expand<T>(
    root: Entity,
    slot: RowSlot,
    text: &'static str,
    side: Side,
) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let display = row_signal(root, slot, move |doc, index| match doc
        .body(index)
    {
        Some((_, Row::Gap { up, down, .. }))
            if offers(side, up, down) =>
        {
            Display::Flex
        }
        _ => Display::None,
    });
    button(label(text))
        .display(display)
        .rules(ghost)
        .on_activate(move |world| {
            let index = slot.index().get(world);
            let gap = world
                .get::<DiffDoc>(root)
                .and_then(|doc| doc.row(index));
            if let Some(DiffRow::Body {
                file,
                row: Row::Gap { hunk, .. },
            }) = gap
            {
                doc_mut(world, root).reveal(file, hunk, side);
            }
        })
        .boxed()
}

fn offers(side: Side, up: bool, down: bool) -> bool {
    match side {
        Side::Up => up,
        Side::Down => down,
        Side::All => up || down,
    }
}

fn more<T>(root: Entity, slot: RowSlot) -> AnyView<Bevy, T>
where
    T: Tokens,
{
    let text =
        row_signal(root, slot, |doc, index| match doc.row(index) {
            Some(DiffRow::More { lines }) => {
                format!("... {lines} more lines")
            }
            _ => String::new(),
        });
    button(label(text))
        .rules(ghost)
        .on_activate(move |world| doc_mut(world, root).show_all())
        .boxed()
}

pub(crate) fn load_hatch(images: Option<ResMut<Assets<Image>>>) {
    let Some(mut images) = images else {
        return;
    };
    let mut image = Image::default_uninit();
    image.texture_descriptor.size.width = HATCH_SIZE;
    image.texture_descriptor.size.height = HATCH_SIZE;
    let stripe = |x: u32, y: u32| (x + y) % HATCH_SIZE < 2;
    let pixels = (0..HATCH_SIZE)
        .flat_map(|y| (0..HATCH_SIZE).map(move |x| (x, y)))
        .flat_map(
            |(x, y)| {
                if stripe(x, y) { [255; 4] } else { [0; 4] }
            },
        )
        .collect();
    image.data = Some(pixels);
    image.sampler = ImageSampler::nearest();
    images
        .insert(&HATCH, image)
        .expect("a uuid handle takes any image");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(spans: &[Span]) -> Vec<(&str, bool)> {
        spans
            .iter()
            .map(|span| {
                (span.text.as_str(), span.look.background.is_some())
            })
            .collect()
    }

    #[test]
    fn marks_split_spans_at_their_edges() {
        let spans = vec![Span::new("let total"), Span::new(" = 0;")];
        let marked = mark(spans, &[4..9, 12..13], Color::WHITE);
        assert_eq!(
            texts(&marked),
            [
                ("let ", false),
                ("total", true),
                (" = ", false),
                ("0", true),
                (";", false),
            ]
        );
    }

    #[test]
    fn a_mark_across_spans_marks_both_parts() {
        let spans = vec![Span::new("ab"), Span::new("cd")];
        let across = 1..3;
        let marked = mark(spans, &[across], Color::WHITE);
        assert_eq!(
            texts(&marked),
            [("a", false), ("b", true), ("c", true), ("d", false)]
        );
    }
}
