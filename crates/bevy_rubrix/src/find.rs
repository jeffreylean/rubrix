//! Find and replace in an editable view: [`find_bar`], its chords,
//! and its match highlights.
//!
//! A bar searches one doc with [`rubrix_edit::find::Search`], a slice
//! of at most [`SLICE`] a frame, so a search of a large doc spreads
//! over frames. Its matches tint the built rows of the view that
//! opened it, through the [`FIND`] layer of that view's [`Marks`].

use std::ops::Range;
use std::time::{Duration, Instant};

use bevy::color::{Alpha, Color};
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::observer::On;
use bevy::ecs::system::Local;
use bevy::ecs::world::World;
use bevy::input::keyboard::Key;
use bevy::input_focus::{AcquireFocus, FocusCause, InputFocus};
use bevy::picking::events::PointerPress;
use bevy::text::{EditableText, TextEdit};
use bevy::ui::{AlignItems, Display, Node, UiRect, px};
use bevy_fynix::tokens::{
    MotionTokens, SpacingTokens, SurfaceTokens, TextTokens, Tone,
};
use bevy_fynix::views::{
    BehaviorExt, FrameProps, VirtualList, button, column, label, row,
    text_field,
};
use bevy_fynix::{Bevy, Cx, Prop, View, component};
use rubrix_edit::find::{self, Query, Search};
use rubrix_edit::{Buffer, Selection, Selections};

use crate::editor::{
    EditDoc, EditRequest, EditView, EditorChanged, Request,
};
use crate::frame::Tx;
use crate::keymap::{Keymap, Keys, Mods};
use crate::marks::{Mark, MarkKind, MarkLayer, Marks};

type Run = fn(&mut Tx<'_>);

/// The [`Marks`] layer that holds a bar's match highlights.
pub const FIND: MarkLayer = MarkLayer("find");

/// The longest a search scans in one frame, a quarter of the 1 ms
/// slice Pierre gives its background tokenizer. A 100k-line frame
/// takes about 2 ms before the search runs, so a longer slice puts
/// the frames that scan a one-letter query over 3 ms.
pub const SLICE: Duration = Duration::from_micros(250);

/// A find bar for the editable views of `doc`, an entity holding an
/// [`EditDoc`]. It shows nothing until `mod+f` or `mod+alt+f` in one
/// of those views opens it, and the app places it, beside or over
/// the view.
pub fn find_bar(doc: impl Into<Prop<Entity>>) -> FindBarView {
    FindBarView { doc: doc.into() }
}

/// See [`find_bar`].
pub struct FindBarView {
    doc: Prop<Entity>,
}

/// A find bar's state, on its root.
#[derive(Component)]
pub struct FindBar {
    doc: Entity,
    view: Option<Entity>,
    replacing: bool,
    query: Query,
    template: String,
    search: Option<Search>,
    current: Option<usize>,
    current_from: usize,
    follow: bool,
    pending_since: Option<Instant>,
    settled: Option<Duration>,
    marked: usize,
    label: String,
    tint: Color,
    strong: Color,
    parts: Parts,
}

#[derive(Clone, Copy)]
struct Parts {
    find: Entity,
    replace: Entity,
    replace_row: Entity,
}

impl FindBar {
    pub fn doc(&self) -> Entity {
        self.doc
    }

    pub fn is_open(&self) -> bool {
        self.search.is_some()
    }

    pub fn query(&self) -> &Query {
        &self.query
    }

    /// The current match, from 0.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// The matches found so far.
    pub fn count(&self) -> usize {
        self.search
            .as_ref()
            .map_or(0, |search| search.matches().len())
    }

    /// Whether the count stopped at [`find::MAX_MATCHES`].
    pub fn is_capped(&self) -> bool {
        self.search.as_ref().is_some_and(Search::is_capped)
    }

    /// The built rows of the view that show a match.
    pub fn marked(&self) -> usize {
        self.marked
    }

    /// The time from the last change of the query until its count
    /// was final, or `None` while the search runs.
    pub fn settled(&self) -> Option<Duration> {
        self.settled.filter(|_| self.pending_since.is_none())
    }

    /// What the bar's count reads.
    pub fn label(&self) -> &str {
        &self.label
    }

    fn matches(&self) -> &[Range<usize>] {
        self.search.as_ref().map_or(&[], Search::matches)
    }

    fn restart(&mut self, from: usize, follow: bool) {
        match &mut self.search {
            Some(search) => search.set_query(&self.query),
            None => self.search = Some(Search::new(&self.query)),
        }
        self.current = None;
        self.current_from = from;
        self.follow = follow;
        self.pending_since = Some(Instant::now());
    }

    fn count_label(&self) -> String {
        let Some(search) = &self.search else {
            return String::new();
        };
        if let Some(error) = search.error() {
            return format!("Invalid regex: {error}");
        }
        let count = search.matches().len();
        let more = if search.is_capped() { "+" } else { "" };
        match self.current {
            _ if !search.is_done() => format!("searching {count}"),
            _ if count == 0 => "No results".into(),
            Some(current) => {
                format!("{} of {count}{more}", current + 1)
            }
            None => format!("{count}{more} results"),
        }
    }
}

impl<T> View<Bevy, T> for FindBarView
where
    T: TextTokens
        + SurfaceTokens
        + SpacingTokens
        + MotionTokens
        + Send
        + Sync
        + 'static,
{
    fn build(self, cx: &mut Cx<'_, Bevy, T>) -> Entity {
        let accent = cx.theme().tone(Tone::Accent);
        let root = cx.build(
            column(())
                .gap(4.0)
                .padding(UiRect::all(px(6.0)))
                .fill(cx.theme().panel())
                .border(1.0)
                .border_color(cx.theme().hairline())
                .radius(4.0),
        );
        let on = |pick: fn(&Query) -> bool| {
            component::<FindBar, Tone>(root, move |bar| {
                if bar.is_some_and(|bar| pick(&bar.query)) {
                    Tone::Accent
                } else {
                    Tone::Dim
                }
            })
        };
        let toggle =
            |text: &str, pick: fn(&mut Query) -> &mut bool, shown| {
                button(label(text.to_owned()).tone(shown))
                    .padding(UiRect::axes(px(6.0), px(2.0)))
                    .on_activate(move |world| {
                        let Some(mut bar) =
                            world.get_mut::<FindBar>(root)
                        else {
                            return;
                        };
                        let flag = pick(&mut bar.query);
                        *flag = !*flag;
                        let from = bar.current_from;
                        bar.restart(from, false);
                        let field = bar.parts.find;
                        focus(world, field);
                    })
            };
        let run = |text: &str, run: Run| {
            button(label(text.to_owned()))
                .padding(UiRect::axes(px(6.0), px(2.0)))
                .on_activate(move |world| {
                    let field =
                        world.get::<FindBar>(root).map(|bar| {
                            let parts = bar.parts;
                            if bar.replacing {
                                parts.replace
                            } else {
                                parts.find
                            }
                        });
                    if let Some(field) = field {
                        request(world, root, field, run);
                    }
                })
        };
        let query_text = component::<FindBar, String>(root, |bar| {
            bar.map(|bar| bar.query.text.clone()).unwrap_or_default()
        });
        let template = component::<FindBar, String>(root, |bar| {
            bar.map(|bar| bar.template.clone()).unwrap_or_default()
        });
        let count = component::<FindBar, String>(root, |bar| {
            bar.map(|bar| bar.label.clone()).unwrap_or_default()
        });
        let (find_row, replace_row) = cx.under(root, |cx| {
            let find_row = cx.build(
                row((
                    text_field(query_text, |_, _| {})
                        .width(px(220.0)),
                    toggle(
                        "Aa",
                        |q| &mut q.match_case,
                        on(|q| q.match_case),
                    ),
                    toggle(
                        "W",
                        |q| &mut q.whole_word,
                        on(|q| q.whole_word),
                    ),
                    toggle(".*", |q| &mut q.regex, on(|q| q.regex)),
                    row((label(count).tone(Tone::Dim).wrap(false),))
                        .width(px(120.0)),
                    run("Prev", previous),
                    run("Next", next),
                    run("Close", close),
                ))
                .gap(4.0)
                .align(AlignItems::Center),
            );
            let replace_row = cx.build(
                row((
                    text_field(template, |_, _| {}).width(px(220.0)),
                    run("Replace", replace),
                    run("Replace All", replace_all),
                ))
                .gap(4.0)
                .align(AlignItems::Center),
            );
            (find_row, replace_row)
        });
        let input = |world: &World, field_row: Entity| {
            let field =
                world.get::<Children>(field_row)?.first().copied()?;
            world.get::<Children>(field)?.first().copied()
        };
        let parts = Parts {
            find: input(cx.world, find_row).expect("the find field"),
            replace: input(cx.world, replace_row)
                .expect("the replace field"),
            replace_row,
        };
        cx.world
            .entity_mut(root)
            .insert(FindBar {
                doc: Entity::PLACEHOLDER,
                view: None,
                replacing: false,
                query: Query::default(),
                template: String::new(),
                search: None,
                current: None,
                current_from: 0,
                follow: false,
                pending_since: None,
                settled: None,
                marked: 0,
                label: String::new(),
                tint: accent.with_alpha(0.25),
                strong: accent.with_alpha(0.6),
                parts,
            })
            .observe(|mut press: On<PointerPress>| {
                press.propagate(false)
            })
            .observe(|mut acquire: On<AcquireFocus>| {
                acquire.propagate(false);
            });
        if let Some(mut node) = cx.world.get_mut::<Node>(root) {
            node.display = Display::None;
        }
        cx.effect(root, self.doc, |world, root, &doc| {
            let view =
                world.get::<FindBar>(root).and_then(|bar| bar.view);
            if let Some(mut bar) = world.get_mut::<FindBar>(root)
                && bar.doc != doc
            {
                bar.doc = doc;
                bar.search = None;
                bar.view = None;
            }
            if let Some(view) = view {
                clear_marks(world, view);
            }
        });
        root
    }
}

fn request(world: &mut World, bar: Entity, field: Entity, run: Run) {
    focus(world, field);
    let Some(bar) = world.get::<FindBar>(bar) else {
        return;
    };
    let (doc, view) = (bar.doc, bar.view);
    world.write_message(EditRequest {
        doc,
        view,
        request: Request::new(run),
    });
}

fn bar_of(tx: &mut Tx<'_>) -> Option<Entity> {
    let doc = tx.doc();
    let world = tx.world();
    world
        .query::<(Entity, &FindBar)>()
        .iter(world)
        .find(|(_, bar)| bar.doc == doc)
        .map(|(entity, _)| entity)
}

/// `mod+f`: opens the bar on the word at a bare caret.
pub fn open_find(tx: &mut Tx<'_>) {
    open(tx, false);
}

/// `mod+alt+f`: opens the bar with its replace field.
pub fn open_replace(tx: &mut Tx<'_>) {
    open(tx, true);
}

fn open(tx: &mut Tx<'_>, replacing: bool) {
    let Some(bar) = bar_of(tx) else {
        return;
    };
    let editor = tx.editor();
    let word = find::seed(editor.buffer(), editor.selections());
    let text = word
        .clone()
        .map(|word| editor.buffer().slice(word).into_owned());
    let from = word
        .clone()
        .map_or(editor.selections().primary().start(), |word| {
            word.start
        });
    if let Some(word) = word {
        tx.select(Selections::new(Selection {
            anchor: word.start,
            head: word.end,
        }));
    }
    let view = tx.view().or_else(|| {
        let doc = tx.doc();
        let world = tx.world();
        world
            .query::<(Entity, &EditView)>()
            .iter(world)
            .find(|(_, view)| view.doc() == doc)
            .map(|(entity, _)| entity)
    });
    let world = tx.world();
    let old = world.get::<FindBar>(bar).and_then(|state| state.view);
    if let Some(old) = old.filter(|&old| Some(old) != view) {
        clear_marks(world, old);
    }
    let Some(mut state) = world.get_mut::<FindBar>(bar) else {
        return;
    };
    if let Some(text) = text {
        state.query.text = text;
    }
    state.replacing = replacing;
    state.view = view;
    state.restart(from, false);
    let parts = state.parts;
    let query = state.query.text.clone();
    let field = if replacing { parts.replace } else { parts.find };
    if let Some(mut text) = world.get_mut::<EditableText>(parts.find)
    {
        text.editor_mut().set_text(&query);
        text.queue_edit(TextEdit::SelectAll);
    }
    focus(world, field);
}

/// Enter and `mod+g`: selects the next match.
pub fn next(tx: &mut Tx<'_>) {
    step(tx, true);
}

/// Shift+Enter and `mod+shift+g`: selects the previous match.
pub fn previous(tx: &mut Tx<'_>) {
    step(tx, false);
}

fn step(tx: &mut Tx<'_>, forward: bool) {
    let Some(bar) = bar_of(tx) else {
        return;
    };
    let editor = tx.editor();
    let caret = editor.selections().primary();
    let buffer = editor.buffer().snapshot();
    let Some(mut state) = tx.world().get_mut::<FindBar>(bar) else {
        return;
    };
    let current = state.current;
    let Some(search) = &mut state.search else {
        return;
    };
    search.run(&buffer);
    let matches = search.matches();
    let count = matches.len();
    if count == 0 {
        return;
    }
    let index = match current {
        Some(current) if forward => (current + 1) % count,
        Some(current) => (current + count - 1) % count,
        None if forward => {
            first_from(matches, caret.start()).unwrap_or(0)
        }
        None => first_from(matches, caret.start())
            .map_or(count - 1, |first| (first + count - 1) % count),
    };
    let range = matches[index].clone();
    state.current = Some(index);
    state.follow = false;
    tx.select(Selections::new(Selection {
        anchor: range.start,
        head: range.end,
    }));
}

/// Escape in the bar: closes it and gives the focus back to the view.
pub fn close(tx: &mut Tx<'_>) {
    let Some(bar) = bar_of(tx) else {
        return;
    };
    let world = tx.world();
    let Some(mut state) = world.get_mut::<FindBar>(bar) else {
        return;
    };
    state.search = None;
    state.current = None;
    state.pending_since = None;
    state.marked = 0;
    let view = state.view;
    if let Some(view) = view {
        clear_marks(world, view);
        focus(world, view);
    }
}

/// `Replace`: replaces the current match, or the first after the
/// caret, puts the caret after it and searches again.
pub fn replace(tx: &mut Tx<'_>) {
    edit(tx, false);
}

/// `Replace All`: replaces every match as one undo step.
pub fn replace_all(tx: &mut Tx<'_>) {
    edit(tx, true);
}

fn edit(tx: &mut Tx<'_>, all: bool) {
    let Some(bar) = bar_of(tx) else {
        return;
    };
    let caret = tx.editor().selections().primary().start();
    let Some(mut state) = tx.world().get_mut::<FindBar>(bar) else {
        return;
    };
    let Some(mut search) = state.search.take() else {
        return;
    };
    let template = state.template.clone();
    let current = state.current;
    tx.transact(&mut |editor, _| {
        if all {
            return find::replace_all(editor, &mut search, &template);
        }
        search.run(editor.buffer());
        let index = current
            .or_else(|| first_from(search.matches(), caret))?;
        find::replace(editor, &mut search, index, &template)
    });
    let after = tx.editor().selections().primary().start();
    let world = tx.world();
    if let Some(mut state) = world.get_mut::<FindBar>(bar) {
        state.search = Some(search);
        state.current = None;
        state.current_from = after;
        state.pending_since = Some(Instant::now());
    }
}

fn first_from(
    matches: &[Range<usize>],
    offset: usize,
) -> Option<usize> {
    let first = matches.partition_point(|m| m.start < offset);
    (first < matches.len()).then_some(first)
}

fn focus(world: &mut World, entity: Entity) {
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>()
        && focus.get() != Some(entity)
    {
        focus.set(entity, FocusCause::Navigated);
    }
}

fn clear_marks(world: &mut World, view: Entity) {
    if let Some(mut marks) = world.get_mut::<Marks>(view)
        && !marks.layer(FIND).is_empty()
    {
        marks.set(FIND, Vec::new());
    }
}

fn field_key(
    key: &Key,
    mods: Mods,
    platform_mod: Mods,
    in_replace: bool,
) -> Option<Run> {
    let plain = mods == Mods::NONE;
    let shift = mods
        == Mods {
            shift: true,
            ..Mods::NONE
        };
    let command = |extra: Mods| {
        mods == Mods {
            ctrl: platform_mod.ctrl || extra.ctrl,
            alt: extra.alt,
            shift: extra.shift,
            super_key: platform_mod.super_key || extra.super_key,
        }
    };
    let letter = |c: &str| matches!(key, Key::Character(k) if k.eq_ignore_ascii_case(c));
    Some(match key {
        Key::Enter if plain && in_replace => replace,
        Key::Enter if plain => next,
        Key::Enter if shift => previous,
        Key::Escape if plain => close,
        _ if letter("g") && command(Mods::NONE) => next,
        _ if letter("g")
            && command(Mods {
                shift: true,
                ..Mods::NONE
            }) =>
        {
            previous
        }
        _ if letter("f") && command(Mods::NONE) => open_find,
        _ if letter("f")
            && command(Mods {
                alt: true,
                ..Mods::NONE
            }) =>
        {
            open_replace
        }
        _ => return None,
    })
}

pub(crate) fn follow(
    world: &mut World,
    mut changes: Local<MessageCursor<EditorChanged>>,
) {
    let changed: Vec<EditorChanged> = world
        .get_resource::<Messages<EditorChanged>>()
        .map(|messages| changes.read(messages).cloned().collect())
        .unwrap_or_default();
    let bars: Vec<Entity> = world
        .query::<(Entity, &FindBar)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect();
    for bar in bars {
        keys(world, bar);
        fields(world, bar);
        let Some(state) = world.get::<FindBar>(bar) else {
            continue;
        };
        let (doc, view, open) =
            (state.doc, state.view, state.search.is_some());
        show(world, bar);
        if !open {
            continue;
        }
        let Some((buffer, caret)) =
            world.get::<EditDoc>(doc).map(|edit| {
                let editor = edit.editor();
                (
                    editor.buffer().snapshot(),
                    editor.selections().primary().start(),
                )
            })
        else {
            continue;
        };
        let mut state =
            world.get_mut::<FindBar>(bar).expect("the bar");
        let state = state.bypass_change_detection();
        if let Some(search) = &mut state.search {
            for change in
                changed.iter().filter(|change| change.doc == doc)
            {
                search.edited(&buffer, &change.change);
                state.current = None;
                state.current_from = caret;
            }
        }
        scan(state, &buffer);
        let pick = state
            .current
            .filter(|_| state.follow)
            .map(|i| state.matches()[i].clone());
        if pick.is_some() {
            state.follow = false;
        }
        let label = state.count_label();
        let relabel = label != state.label;
        state.label = label;
        if relabel {
            world
                .get_mut::<FindBar>(bar)
                .expect("the bar")
                .set_changed();
        }
        if let Some(range) = pick {
            world.write_message(EditRequest {
                doc,
                view,
                request: Request::select(Selections::new(
                    Selection {
                        anchor: range.start,
                        head: range.end,
                    },
                )),
            });
        }
        if let Some(view) = view {
            tint(world, bar, view, &buffer);
        }
    }
}

fn scan(state: &mut FindBar, buffer: &Buffer) {
    let Some(search) = &mut state.search else {
        return;
    };
    let done = search.step(buffer, SLICE);
    if state.current.is_none() {
        let matches = search.matches();
        state.current = first_from(matches, state.current_from)
            .or_else(|| (done && !matches.is_empty()).then_some(0));
    }
    if done && let Some(typed) = state.pending_since.take() {
        state.settled = Some(typed.elapsed());
    }
}

fn keys(world: &mut World, bar: Entity) {
    let Some(state) = world.get::<FindBar>(bar) else {
        return;
    };
    let parts = state.parts;
    let platform =
        Mods::primary(world.resource::<Keymap>().platform());
    let runs: Vec<(Entity, Run)> = world
        .resource::<Keys>()
        .presses()
        .iter()
        .filter(|press| press.input.state.is_pressed())
        .filter_map(|press| {
            let field = press
                .focus
                .filter(|&f| f == parts.find || f == parts.replace)?;
            let composing = world
                .get::<EditableText>(field)
                .is_some_and(EditableText::is_composing);
            if composing
                && matches!(
                    press.input.logical_key,
                    Key::Enter | Key::Escape
                )
            {
                return None;
            }
            let run = field_key(
                &press.input.logical_key,
                press.mods,
                platform,
                field == parts.replace,
            )?;
            Some((field, run))
        })
        .collect();
    for (field, run) in runs {
        request(world, bar, field, run);
    }
}

fn fields(world: &mut World, bar: Entity) {
    let Some(state) = world.get::<FindBar>(bar) else {
        return;
    };
    if state.search.is_none() {
        return;
    }
    let focused =
        world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let read = |entity| {
        world
            .get::<EditableText>(entity)
            .filter(|_| focused == Some(entity))
            .map(|text| text.value().to_string())
    };
    let (query, template) =
        (read(state.parts.find), read(state.parts.replace));
    let caret = world.get::<EditDoc>(state.doc).map_or(0, |edit| {
        edit.editor().selections().primary().start()
    });
    let mut state = world.get_mut::<FindBar>(bar).expect("the bar");
    if let Some(template) = template
        && template != state.template
    {
        state.template = template;
    }
    if let Some(query) = query
        && query != state.query.text
    {
        state.query.text = query;
        state.restart(caret, true);
    }
}

fn show(world: &mut World, bar: Entity) {
    let Some(state) = world.get::<FindBar>(bar) else {
        return;
    };
    let (open, replacing, row) = (
        state.search.is_some(),
        state.replacing,
        state.parts.replace_row,
    );
    let set = |world: &mut World, entity, on: bool| {
        let display = if on { Display::Flex } else { Display::None };
        if let Some(mut node) = world.get_mut::<Node>(entity)
            && node.display != display
        {
            node.display = display;
        }
    };
    set(world, bar, open);
    set(world, row, replacing);
}

fn tint(
    world: &mut World,
    bar: Entity,
    view: Entity,
    buffer: &Buffer,
) {
    let Some(window) = world
        .get::<EditView>(view)
        .and_then(EditView::list)
        .and_then(|list| world.get::<VirtualList>(list))
        .and_then(VirtualList::spawned)
    else {
        return;
    };
    let Some(state) = world.get::<FindBar>(bar) else {
        return;
    };
    let marks = visible_marks(
        buffer,
        state.matches(),
        state.current,
        *window.start() as u32..*window.end() as u32 + 1,
        (state.tint, state.strong),
    );
    let mut lines: Vec<u32> =
        marks.iter().map(|mark| mark.line).collect();
    lines.dedup();
    let marked = lines.len();
    world
        .get_mut::<FindBar>(bar)
        .expect("the bar")
        .bypass_change_detection()
        .marked = marked;
    if let Some(mut layers) = world.get_mut::<Marks>(view)
        && layers.layer(FIND) != marks.as_slice()
    {
        layers.set(FIND, marks);
    }
}

fn visible_marks(
    buffer: &Buffer,
    matches: &[Range<usize>],
    current: Option<usize>,
    lines: Range<u32>,
    (tint, strong): (Color, Color),
) -> Vec<Mark> {
    let last = lines.end.min(buffer.line_count());
    if lines.start >= last {
        return Vec::new();
    }
    let (start, end) =
        (buffer.line_start(lines.start), buffer.line_start(last));
    let first = matches.partition_point(|m| m.end <= start);
    let mut marks = Vec::new();
    for (i, range) in matches.iter().enumerate().skip(first) {
        if range.start >= end {
            break;
        }
        let color = if Some(i) == current { strong } else { tint };
        let from = buffer.line_of(range.start).max(lines.start);
        let to = buffer.line_of(range.end).min(last - 1);
        for line in from..=to {
            let at = buffer.line_start(line);
            let bytes = range.start.max(at) - at
                ..range.end.min(buffer.line_end(line)) - at;
            if !bytes.is_empty() {
                marks.push(Mark {
                    line,
                    kind: MarkKind::Tint { bytes, color },
                });
            }
        }
    }
    marks.sort_by_key(|mark| mark.line);
    marks
}
