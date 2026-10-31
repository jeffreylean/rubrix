use std::collections::HashMap;
use std::mem;
use std::path::PathBuf;

use bevy::app::{App, PostUpdate};
use bevy::camera::visibility::InheritedVisibility;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::ecs::world::{FromWorld, World};
use bevy::text::{TextLayoutInfo, TextSpan};
use bevy::ui::widget::Text;
use bevy::ui::{
    AlignItems, ComputedNode, JustifyContent, UiGlobalTransform,
    UiRect, percent, px,
};
use bevy_fynix::tokens::Tone;
use bevy_fynix::views::{
    FrameProps, VirtualList, column, label, overlay, row,
};
use bevy_fynix::{AnyView, Signal, ViewExt, derived, resource};
use bevy_rubrix::find::{FindBar, find_bar};
use bevy_rubrix::{
    Buffer, EditDoc, EditLine, EditSystems, EditView, Lang,
    LineNumber, Overflow, Preedit, code_editor,
};

use super::code::{
    LARGE_RS, LARGE_TS, flip, read, stress, wrap_name,
};
use super::{
    Ask, control, descendants_in_order, find_under, wrapping,
};
use crate::{Build, PageView};

/// The text the editor page shows, on an entity of its own so a theme
/// toggle that builds the page again keeps it. The page makes it the
/// first time it is built.
#[derive(Resource)]
pub struct EditorFile {
    name: String,
    doc: Entity,
    overflow: Overflow,
}

impl FromWorld for EditorFile {
    fn from_world(world: &mut World) -> Self {
        let doc = world.spawn(large_rs()).id();
        Self {
            name: "large.rs.txt".into(),
            doc,
            overflow: Overflow::Scroll,
        }
    }
}

fn large_rs() -> EditDoc {
    EditDoc::new(Lang::Rust, LARGE_RS)
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Typing>().add_systems(
        PostUpdate,
        count_typing
            .after(EditSystems::Draw)
            .run_if(resource_exists::<EditorFile>),
    );
}

pub fn build() -> PageView {
    AnyView::new(|cx: &mut Build| {
        cx.world.init_resource::<EditorFile>();
        cx.build(
            column((
                label("Editor").size(20.0),
                controls(),
                label(size_label()).tone(Tone::Dim),
                code_editor(resource::<EditorFile, _>(|file| {
                    file.doc
                }))
                .overflow(
                    resource::<EditorFile, _>(|file| file.overflow),
                ),
                find_overlay(),
            ))
            .height(percent(100.0))
            .min_height(px(0.0))
            .gap(8.0),
        )
    })
    .boxed()
}

fn size_label() -> Signal<String> {
    let rows = |world: &World| {
        let file = world.resource::<EditorFile>();
        let rows =
            world.get::<EditDoc>(file.doc).map_or(0, EditDoc::rows);
        (file.doc, rows)
    };
    let mut shown = None;
    derived(move |world| {
        let rows = rows(world).1;
        let lines = if rows == 1 { "line" } else { "lines" };
        format!(
            "{}, {rows} {lines}",
            world.resource::<EditorFile>().name
        )
    })
    .when(move |world| {
        let now = Some(rows(world));
        mem::replace(&mut shown, now) != now
    })
}

fn controls() -> PageView {
    let buttons = row((
        control("large.rs", |world| {
            show(world, "large.rs.txt", large_rs());
        }),
        control("large.ts", |world| {
            let doc = EditDoc::new(Lang::TypeScript, LARGE_TS);
            show(world, "large.ts.txt", doc);
        }),
        control("100k lines", |world| {
            let doc = EditDoc::new(Lang::Rust, &stress());
            show(world, "large.rs.txt repeated", doc);
        }),
        control(
            resource::<EditorFile, _>(|file| {
                wrap_name(file.overflow)
            }),
            |world| {
                flip(&mut world.resource_mut::<EditorFile>().overflow)
            },
        ),
    ))
    .gap(8.0);
    wrapping(buttons.boxed())
}

/// Spawns `doc` in place of the doc on show and despawns that one.
/// The view's doc prop changes, so it builds its rows again at line
/// 1.
fn show(world: &mut World, name: &str, doc: EditDoc) {
    let doc = world.spawn(doc).id();
    let mut file = world.resource_mut::<EditorFile>();
    file.name = name.into();
    let old = mem::replace(&mut file.doc, doc);
    world.despawn(old);
}

pub fn open(
    world: &mut World,
    paths: &[PathBuf],
) -> Result<(), String> {
    let (name, text) = read("editor", paths)?;
    let doc = EditDoc::detect(&name, &text);
    world.init_resource::<EditorFile>();
    show(world, &name, doc);
    Ok(())
}

/// The page's answers.
pub fn answer(world: &mut World, ask: Ask) -> Option<String> {
    let doc = world.get_resource::<EditorFile>()?.doc;
    match ask {
        Ask::Ime => ime(world, doc),
        ask => answer_doc(world, doc, ask),
    }
}

/// The caret, text and undo answers of an editable `doc`, for each
/// page that shows one. Lines and columns count from 1, and a column
/// counts chars from the line's start.
pub fn answer_doc(
    world: &mut World,
    doc: Entity,
    ask: Ask,
) -> Option<String> {
    let shown = world
        .query::<&EditView>()
        .iter(world)
        .find(|view| view.doc() == doc)
        .is_some_and(EditView::caret_shown);
    let edit = world.get::<EditDoc>(doc)?;
    let editor = edit.editor();
    let buffer = editor.buffer();
    Some(match ask {
        Ask::Caret => {
            let selections = editor.selections();
            let each = selections
                .iter()
                .map(|s| {
                    format!(
                        "{}..{}",
                        place(buffer, s.start()),
                        place(buffer, s.end())
                    )
                })
                .collect::<Vec<_>>();
            format!(
                "{} primary {} {} visible {shown}",
                selections.len(),
                selections.primary_index() + 1,
                each.join(" "),
            )
        }
        Ask::Text(line) => match edit.line(line.get() - 1) {
            Some(text) => format!("{text:?}"),
            None => "none".into(),
        },
        Ask::Undo => format!(
            "{} redo {}",
            editor.history().undo_depth(),
            editor.history().redo_depth()
        ),
        Ask::Ime => return None,
    })
}

/// Byte `at` of `buffer` as `line:column`, both from 1, the column in
/// chars.
pub fn place(buffer: &Buffer, at: usize) -> String {
    let line = buffer.line_of(at);
    let column =
        buffer.slice(buffer.line_start(line)..at).chars().count();
    format!("{}:{}", line + 1, column + 1)
}

/// The frames in which the doc on show took a change, and how many
/// of those ended with the primary caret outside the viewport.
#[derive(Resource, Default)]
struct Typing {
    doc: Option<Entity>,
    seq: u64,
    edits: u32,
    hidden: u32,
}

fn count_typing(world: &mut World) {
    let doc = world.resource::<EditorFile>().doc;
    let Some(seq) = world.get::<EditDoc>(doc).map(EditDoc::seq)
    else {
        return;
    };
    let shown = world
        .query::<&EditView>()
        .iter(world)
        .find(|view| view.doc() == doc)
        .is_some_and(EditView::caret_shown);
    let mut typing = world.resource_mut::<Typing>();
    if typing.doc != Some(doc) {
        *typing = Typing {
            doc: Some(doc),
            seq,
            ..Typing::default()
        };
    } else if seq > typing.seq {
        typing.seq = seq;
        typing.edits += 1;
        typing.hidden += u32::from(!shown);
    }
}

pub fn typing(world: &World) -> String {
    let typing = world.resource::<Typing>();
    format!("typing edits {} hidden {}", typing.edits, typing.hidden)
}

/// A built row of the first editable view, in logical pixels.
struct Shown {
    line: u32,
    top: f32,
    bottom: f32,
    text: String,
    number: Option<(String, f32)>,
}

fn first_view(world: &mut World) -> Option<(Entity, Entity)> {
    world
        .query::<(Entity, &EditView)>()
        .iter(world)
        .next()
        .map(|(view, edit)| (view, edit.doc()))
}

fn shown_rows(world: &mut World, view: Entity) -> Option<Vec<Shown>> {
    let list = find_under::<VirtualList>(world, view)?;
    let node = world.get::<ComputedNode>(list)?;
    let at = world.get::<UiGlobalTransform>(list)?;
    let scale = node.inverse_scale_factor();
    let content = node.content_box();
    let view_top = (at.translation.y + content.min.y) * scale;
    let view_bottom = (at.translation.y + content.max.y) * scale;
    let rows = world
        .query::<(Entity, &EditLine, &ComputedNode, &UiGlobalTransform)>()
        .iter(world)
        .filter(|(_, line, _, _)| line.view == view)
        .map(|(node, line, size, at)| {
            let top = (at.translation.y - size.size().y / 2.0) * scale;
            (node, line.line, top, top + size.size().y * scale)
        })
        .filter(|&(_, _, top, bottom)| {
            bottom > view_top + 0.5 && top < view_bottom - 0.5
        })
        .collect::<Vec<_>>();
    let numbers = world
        .query::<(
            Entity,
            &LineNumber,
            &InheritedVisibility,
            &ComputedNode,
            &UiGlobalTransform,
        )>()
        .iter(world)
        .filter(|(_, number, shown, _, _)| {
            number.view == view && shown.get()
        })
        .map(|(node, number, _, size, at)| {
            let top =
                (at.translation.y - size.size().y / 2.0) * scale;
            (number.line, (node, top))
        })
        .collect::<HashMap<_, _>>();
    let mut shown = rows
        .into_iter()
        .map(|(node, line, top, bottom)| Shown {
            line,
            top,
            bottom,
            text: text_of(world, node),
            number: numbers
                .get(&line)
                .map(|&(number, at)| (text_of(world, number), at)),
        })
        .collect::<Vec<_>>();
    shown.sort_by_key(|row| row.line);
    Some(shown)
}

fn text_of(world: &World, node: Entity) -> String {
    let mut text = world
        .get::<Text>(node)
        .map_or_else(String::new, |text| text.0.clone());
    for &span in world.get::<Children>(node).into_iter().flatten() {
        if let Some(span) = world.get::<TextSpan>(span) {
            text.push_str(&span.0);
        }
    }
    text
}

fn gutter_of(world: &World, node: Entity) -> Option<Entity> {
    let mut inner = node;
    for _ in 0..4 {
        let outer = world.get::<ChildOf>(inner)?.parent();
        let number = world
            .get::<Children>(outer)?
            .iter()
            .copied()
            .filter(|&kid| kid != inner)
            .flat_map(|kid| descendants_in_order(world, kid))
            .find(|&node| world.get::<Text>(node).is_some());
        if number.is_some() {
            return number;
        }
        inner = outer;
    }
    None
}

fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut column = 0;
    for c in line.chars() {
        if c == '\t' {
            let stop = 4 - column % 4;
            out.extend(std::iter::repeat_n(' ', stop));
            column += stop;
        } else {
            out.push(c);
            column += 1;
        }
    }
    out
}

/// Checks the rows of the first editable view that show: no gap or
/// overlap between neighbours, no line skipped, each row's text its
/// line's, and each line number its line's, at its row's top.
pub fn seams(world: &mut World) -> String {
    let Some((view, doc)) = first_view(world) else {
        return "seams none".into();
    };
    let Some(rows) = shown_rows(world, view) else {
        return "seams none".into();
    };
    let (Some(first), Some(last)) = (rows.first(), rows.last())
    else {
        return "seams none".into();
    };
    let (mut gaps, mut overlaps, mut missing) = (0, 0, 0);
    for pair in rows.windows(2) {
        let (above, below) = (&pair[0], &pair[1]);
        missing += u32::from(below.line != above.line + 1);
        let seam = below.top - above.bottom;
        gaps += u32::from(seam > 0.5);
        overlaps += u32::from(seam < -0.5);
    }
    let edit = world.get::<EditDoc>(doc);
    let mismatched = rows
        .iter()
        .filter(|row| {
            edit.and_then(|edit| edit.line(row.line as usize))
                .map(expand_tabs)
                .as_deref()
                != Some(row.text.as_str())
        })
        .count();
    let numbers = rows
        .iter()
        .find(|row| {
            row.number.as_ref().is_none_or(|(number, top)| {
                number.trim() != (row.line + 1).to_string()
                    || (top - row.top).abs() > 0.5
            })
        })
        .map_or_else(
            || "ok".into(),
            |row| format!("jump {}", row.line + 1),
        );
    format!(
        "seams shown {}..={} gaps {gaps} overlaps {overlaps} \
         missing {missing} mismatched {mismatched} numbers {numbers}",
        first.line + 1,
        last.line + 1,
    )
}

/// Line `line`'s row of the first editable view, from 1: where the
/// list puts it and how many visual lines its text takes.
pub fn row_line(world: &mut World, line: usize) -> String {
    let span = first_view(world).zip(line.checked_sub(1)).and_then(
        |((view, _), index)| {
            let list = find_under::<VirtualList>(world, view)?;
            let span =
                world.get::<VirtualList>(list)?.row_span(index)?;
            Some((view, index, span))
        },
    );
    let Some((view, index, span)) = span else {
        return format!("row {line} none");
    };
    let want = EditLine {
        view,
        line: index as u32,
    };
    let text = world
        .query::<(Entity, &EditLine)>()
        .iter(world)
        .find(|(_, at)| **at == want)
        .map(|(node, _)| node);
    let lines = text
        .and_then(|text| visual_lines(world, text))
        .map_or_else(|| "unbuilt".into(), |lines| lines.to_string());
    format!(
        "row {line} top {} height {} lines {lines}",
        span.top, span.height
    )
}

/// The row text's laid-out height over its gutter number's, which is
/// always one line.
fn visual_lines(world: &World, text: Entity) -> Option<f32> {
    let height = |node| {
        world.get::<TextLayoutInfo>(node).map(|info| info.size.y)
    };
    let one = height(gutter_of(world, text)?)?;
    (one > 0.0).then_some((height(text)? / one).round())
}

/// The `log ime` answer: the editor's preedit and its cursor, and the
/// bottom-left of its primary caret in logical pixels, which is where
/// the window should put the input method's candidate box.
fn ime(world: &World, doc: Entity) -> Option<String> {
    let mut views =
        world.try_query::<(&EditView, Option<&Preedit>)>()?;
    let (view, preedit) =
        views.iter(world).find(|(view, _)| view.doc() == doc)?;
    let preedit = preedit.map_or_else(
        || "none".into(),
        |preedit| {
            let cursor = preedit.cursor().map_or_else(
                || "none".into(),
                |(from, to)| format!("{from}..{to}"),
            );
            format!("{:?} cursor {cursor}", preedit.value())
        },
    );
    let caret = view.caret_rect().map_or_else(
        || "none".into(),
        |caret| format!("{} {}", caret.min.x, caret.max.y),
    );
    Some(format!("preedit {preedit} caret {caret}"))
}

fn find_overlay() -> PageView {
    overlay((find_bar(resource::<EditorFile, _>(|file| file.doc)),))
        .justify(JustifyContent::FlexEnd)
        .align(AlignItems::FlexStart)
        .padding(UiRect::new(px(0.0), px(24.0), px(112.0), px(0.0)))
        .boxed()
}

pub fn find(world: &mut World) -> String {
    let Some(doc) =
        world.get_resource::<EditorFile>().map(|file| file.doc)
    else {
        return "find none".into();
    };
    let Some(bar) = world
        .query::<&FindBar>()
        .iter(world)
        .find(|bar| bar.doc() == doc)
    else {
        return "find none".into();
    };
    let settled = bar.settled().map_or_else(
        || "pending".into(),
        |time| format!("{:.1}", time.as_secs_f64() * 1000.0),
    );
    format!(
        "find {} query {:?} {} of {}{} marked {} final {settled} \
         label {:?}",
        if bar.is_open() { "open" } else { "closed" },
        bar.query().text,
        bar.current().map_or(0, |current| current + 1),
        bar.count(),
        if bar.is_capped() { " capped" } else { "" },
        bar.marked(),
        bar.label(),
    )
}
