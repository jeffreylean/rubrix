//! Typing through an input method into the focused editable view.
//!
//! While a view holds `InputFocus`, the window takes IME input and
//! puts the candidate box under the primary caret. A composition
//! shows as an underlined [`Preedit`] at the primary caret, outside
//! the buffer and the history, and its commit goes in at every caret
//! as one undo step.

use bevy::app::{App, PostUpdate, PreUpdate};
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::query::{With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::input::InputSystems;
use bevy::input_focus::InputFocus;
use bevy::math::{Rect, Vec2};
use bevy::picking::Pickable;
use bevy::text::{ComputedTextBlock, TextLayout, TextLayoutInfo};
use bevy::ui::{
    BackgroundColor, CalculatedClip, ComputedNode, Node, Overflow,
    PositionType, UiGlobalTransform, UiScale, ZIndex, px,
};
use bevy::window::{Ime, PrimaryWindow, Window};
use bevy_fynix::views::drawn_layout;
use rubrix_edit::Command;

use crate::caret::CaretLayer;
use crate::editor::{
    Composing, EditDoc, EditLine, EditRequest, EditSystems, EditView,
    Request,
};
use crate::keymap::content;
use crate::marks::{Mark, MarkKind, MarkLayer, Marks};
use crate::rows::{Shown, inserts};
use crate::select;

/// The [`Marks`] layer a preedit draws in.
const IME: MarkLayer = MarkLayer("ime");

/// The text an input method is composing in an editable view, on the
/// view's root while it is not empty. The view's keys go to nobody
/// meanwhile, so nothing types beside it.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    value: String,
    cursor: Option<(usize, usize)>,
}

impl Preedit {
    /// `None` for an empty `value`, which ends the composition. A
    /// cursor off `value`'s char boundaries is hidden.
    fn new(
        value: &str,
        cursor: Option<(usize, usize)>,
    ) -> Option<Preedit> {
        let on = |at: usize| value.is_char_boundary(at);
        (!value.is_empty()).then(|| Preedit {
            value: value.to_owned(),
            cursor: cursor.filter(|&(from, to)| {
                from <= to && on(from) && on(to)
            }),
        })
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// The input method's cursor, as bytes of [`Preedit::value`], or
    /// `None` while it hides the cursor.
    pub fn cursor(&self) -> Option<(usize, usize)> {
        self.cursor
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<Ime>()
        .add_systems(
            PreUpdate,
            (
                read.after(InputSystems).before(EditSystems::Apply),
                place.after(EditSystems::Apply),
            ),
        )
        .add_systems(
            PostUpdate,
            (enable, underline).after(EditSystems::Draw),
        );
}

/// Takes this frame's IME messages into the focused view, in order:
/// a preedit replaces the composition, a commit goes to the doc as a
/// request, and the IME turning on or off ends the composition. A
/// view that lost the focus loses its composition.
fn read(
    mut ime: MessageReader<Ime>,
    focus: Option<Res<InputFocus>>,
    views: Query<(Entity, &EditView, Option<&Preedit>)>,
    mut requests: MessageWriter<EditRequest>,
    mut commands: Commands,
) {
    let focused = focus
        .and_then(|focus| focus.get())
        .filter(|&view| views.contains(view));
    for (view, _, preedit) in &views {
        if preedit.is_some() && focused != Some(view) {
            commands.entity(view).remove::<(Preedit, Composing)>();
        }
    }
    let Some((view, edit, was)) =
        focused.and_then(|view| views.get(view).ok())
    else {
        ime.clear();
        return;
    };
    let mut now = was.cloned();
    for message in ime.read() {
        now = match message {
            Ime::Preedit { value, cursor, .. } => {
                Preedit::new(value, *cursor)
            }
            Ime::Commit { value, .. } => {
                if !value.is_empty() {
                    // A paste is its own undo step, which a typing
                    // run does not join.
                    let text = value.clone();
                    requests.write(EditRequest {
                        doc: edit.doc(),
                        view: Some(view),
                        request: Request::new(move |tx| {
                            tx.apply(Command::Paste(&text));
                        }),
                    });
                }
                None
            }
            Ime::Enabled { .. } | Ime::Disabled { .. } => None,
        };
    }
    if now.as_ref() == was {
        return;
    }
    match now {
        Some(preedit) => {
            commands.entity(view).insert((preedit, Composing));
        }
        None => {
            commands.entity(view).remove::<(Preedit, Composing)>();
        }
    }
}

/// Draws each view's preedit at its primary caret, after this frame's
/// edits moved the caret, and clears the layer of a view whose
/// composition ended. The layer changes only when the mark does, so
/// the caret's row reads again only then.
fn place(
    mut views: Query<(&EditView, &Preedit, &mut Marks)>,
    mut ended: RemovedComponents<Preedit>,
    mut cleared: Query<&mut Marks, Without<Preedit>>,
    docs: Query<&EditDoc>,
    layers: Query<&CaretLayer>,
) {
    for view in ended.read() {
        if let Ok(mut marks) = cleared.get_mut(view) {
            marks.bypass_change_detection().set(IME, Vec::new());
        }
    }
    for (edit, preedit, mut marks) in &mut views {
        let Ok(doc) = docs.get(edit.doc()) else {
            continue;
        };
        let Ok(layer) = layers.get(edit.carets()) else {
            continue;
        };
        let buffer = doc.editor().buffer();
        let head = doc.editor().selections().primary().head;
        let line = buffer.line_of(head);
        let mark = Mark {
            line,
            kind: MarkKind::Insert {
                at: head - buffer.line_start(line),
                text: preedit.value.clone(),
                underline: Some(layer.color),
                caret: preedit.cursor.map(|(_, to)| to),
            },
        };
        if marks.layer(IME) != core::slice::from_ref(&mark) {
            marks.bypass_change_detection().set(IME, vec![mark]);
        }
    }
}

/// Keeps the window's IME on while an editable view holds the focus,
/// with the candidate box at the bottom-left of the primary caret.
///
/// Bevy's `TextInputPlugin` turns the IME off in `PreUpdate` whenever
/// no `EditableText` has the focus, and winit reads the window in
/// `Last`, so this runs between them. Once the focus leaves, it
/// writes nothing, and `TextInputPlugin` turns the IME off in the
/// next frame.
fn enable(
    focus: Option<Res<InputFocus>>,
    views: Query<&EditView>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    scale: Option<Res<UiScale>>,
) {
    let Some(edit) = focus
        .and_then(|focus| focus.get())
        .and_then(|view| views.get(view).ok())
    else {
        return;
    };
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    if !window.ime_enabled {
        window.ime_enabled = true;
    }
    // The caret is in UI pixels, which `UiScale` scales.
    let scale = scale.map_or(1.0, |scale| scale.0);
    if let Some(caret) = edit.caret_rect() {
        let at = Vec2::new(caret.min.x, caret.max.y) * scale;
        if window.ime_position != at {
            window.ime_position = at;
        }
    }
}

/// The overlay on a view's root that holds its preedit's underline
/// bars, above the rows and clipped to the view like its carets.
#[derive(Component)]
struct Underlines(Entity);

#[derive(Component)]
struct UnderlineLayer;

#[derive(Component)]
struct UnderlineBar;

/// Where an unused bar waits, outside the clipping overlay.
const PARKED: f32 = -1.0e5;

/// Draws a bar under each visual line of the preedit, at least one
/// device pixel thick.
///
/// Bevy draws a span's underline as a rect as thin as the font says,
/// 0.7 px at 14 px, and fills only the pixels whose centres it
/// covers. On some runs, such as a CJK fallback font's at scale 1, it
/// covers none, and the preedit shows no underline. The bar sits on
/// the pixel row nearest the font's underline, so where Bevy's line
/// shows, the bar covers it.
fn underline(
    views: Query<(Entity, &EditView, &Marks, Option<&Underlines>)>,
    docs: Query<&EditDoc>,
    rows: Query<RowText, Without<UnderlineBar>>,
    overlays: Query<
        Overlay,
        (With<UnderlineLayer>, Without<UnderlineBar>),
    >,
    mut bars: Query<
        (&mut Node, &mut UiGlobalTransform),
        With<UnderlineBar>,
    >,
    mut commands: Commands,
) {
    for (view, edit, marks, layer) in &views {
        let insert = marks.layer(IME).first().and_then(|mark| {
            match &mark.kind {
                MarkKind::Insert {
                    at,
                    text,
                    underline: Some(color),
                    ..
                } => Some((mark.line, *at, text.len(), *color)),
                _ => None,
            }
        });
        let Some(layer) = layer.map(|layer| layer.0) else {
            if insert.is_some() {
                spawn_layer(&mut commands, view);
            }
            continue;
        };
        let Ok((overlay, overlay_at, kids)) = overlays.get(layer)
        else {
            continue;
        };
        let (rects, color) = insert
            .and_then(|(line, at, len, color)| {
                let buffer =
                    docs.get(edit.doc()).ok()?.editor().buffer();
                let row = rows
                    .iter()
                    .find(|row| *row.0 == EditLine { view, line })?;
                let shown = Shown::with(
                    content(buffer, line),
                    &inserts(marks.on(line)),
                );
                let from = shown.shown(at);
                Some((bar_rects(row, from..from + len), color))
            })
            .unwrap_or_default();
        let logical = overlay.inverse_scale_factor();
        let origin =
            overlay_at.translation + overlay.border_box().min;
        let kids = kids.map_or(&[][..], |kids| &kids[..]);
        for (at, rect) in rects.iter().enumerate() {
            let placed = Node {
                position_type: PositionType::Absolute,
                left: px((rect.min.x - origin.x) * logical),
                top: px((rect.min.y - origin.y) * logical),
                width: px(rect.width() * logical),
                height: px(rect.height() * logical),
                ..Node::default()
            };
            let centre =
                UiGlobalTransform::from_translation(rect.center());
            match kids.get(at).and_then(|&kid| bars.get_mut(kid).ok())
            {
                Some((mut node, mut transform)) => {
                    node.set_if_neq(placed);
                    *transform = centre;
                }
                None => {
                    commands.spawn((
                        placed,
                        centre,
                        BackgroundColor(color),
                        Pickable::IGNORE,
                        UnderlineBar,
                        ChildOf(layer),
                    ));
                }
            }
        }
        for &kid in kids.iter().skip(rects.len()) {
            if let Ok((mut node, mut transform)) = bars.get_mut(kid)
                && node.left != px(PARKED)
            {
                node.left = px(PARKED);
                node.top = px(PARKED);
                *transform = UiGlobalTransform::from_translation(
                    Vec2::splat(PARKED),
                );
            }
        }
    }
}

type Overlay = (
    &'static ComputedNode,
    &'static UiGlobalTransform,
    Option<&'static Children>,
);

type Row<'a> = (
    &'a EditLine,
    &'a ComputedNode,
    &'a UiGlobalTransform,
    &'a ComputedTextBlock,
    &'a TextLayout,
    &'a TextLayoutInfo,
    Option<&'a CalculatedClip>,
);

type RowText = Row<'static>;

/// The bars under the drawn bytes `drawn` of a row, in device pixels
/// from the window's corner, each on the pixel row nearest the font's
/// underline. A bar the row's clip hides is left out.
fn bar_rects(
    (_, node, at, block, text, info, clip): Row,
    drawn: core::ops::Range<usize>,
) -> Vec<Rect> {
    let layout = drawn_layout(block, text, node);
    let corner = at.translation + node.content_box().min;
    select::rects(&layout, drawn, 1.0, Color::NONE)
        .filter_map(|([x0, y0, x1, y1], _)| {
            let run = info.run_geometry.iter().find(|run| {
                run.bounds.min.x <= x0
                    && x0 < run.bounds.max.x
                    && run.bounds.min.y < y1
                    && y0 < run.bounds.max.y
            });
            let (middle, thickness) =
                run.map_or((y1 - 2.0, 1.0), |run| {
                    (
                        run.underline_y
                            + run.underline_thickness / 2.0,
                        run.underline_thickness,
                    )
                });
            let top = (corner.y + middle).floor();
            let rect = Rect::new(
                (corner.x + x0).round(),
                top,
                (corner.x + x1).round(),
                top + thickness.round().max(1.0),
            );
            clip.is_none_or(|clip| clip.contains_point(rect.center()))
                .then_some(rect)
        })
        .collect()
}

fn spawn_layer(commands: &mut Commands, view: Entity) {
    let layer = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                right: px(0.0),
                top: px(0.0),
                bottom: px(0.0),
                overflow: Overflow::clip(),
                ..Node::default()
            },
            ZIndex(1),
            Pickable::IGNORE,
            UnderlineLayer,
            ChildOf(view),
        ))
        .id();
    commands.entity(view).insert(Underlines(layer));
}
