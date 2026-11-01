//! Typing through an input method into the focused editable view.
//!
//! While a view holds `InputFocus`, the window takes IME input and
//! puts the candidate box under the primary caret. A composition
//! shows as an underlined [`Preedit`] at the primary caret, outside
//! the buffer and the history, and its commit goes in at every caret
//! as one undo step. A press on the view's text ends a composition
//! as a native text view does: the preedit goes in at every caret,
//! and then the press takes effect.

use bevy::app::{App, PostUpdate, PreUpdate};
use bevy::color::Color;
use bevy::ecs::change_detection::{DetectChangesMut, Mut};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::lifecycle::RemovedComponents;
use bevy::ecs::message::{MessageReader, MessageWriter};
use bevy::ecs::query::{Has, With, Without};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res};
use bevy::ecs::world::World;
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

/// A composition a press ended, until the input method lets go of it
/// too. Without that it keeps composing the old letters.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// The frame of the press. [`enable`] turns the window's IME off,
    /// so winit clears its marked text on macOS and disables the text
    /// input on Wayland and X11.
    Off,
    /// The IME is on again, and the input method has not said so yet
    /// with `Ime::Enabled`. On every backend winit sends that before
    /// any event of the next composition, so a preedit or commit that
    /// arrives first is the ended composition's, which the doc
    /// already holds, and [`read`] drops it.
    Stale,
}

pub(crate) fn end(world: &mut World, view: Entity) -> Option<String> {
    let mut entity = world.entity_mut(view);
    let preedit = entity.take::<Preedit>()?;
    entity.remove::<Composing>().insert(Ended::Off);
    if let Some(marks) = entity.get_mut::<Marks>() {
        clear(marks);
    }
    Some(preedit.value)
}

fn clear(mut marks: Mut<Marks>) {
    marks.bypass_change_detection().set(IME, Vec::new());
}

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<Ime>()
        .add_systems(
            PreUpdate,
            (
                (age, read)
                    .chain()
                    .after(InputSystems)
                    .before(EditSystems::Apply),
                place.after(EditSystems::Apply),
            ),
        )
        .add_systems(
            PostUpdate,
            (enable, underline).after(EditSystems::Draw),
        );
    #[cfg(target_os = "macos")]
    app.add_systems(
        PreUpdate,
        discard.after(InputSystems).before(age),
    );
}

/// Takes this frame's IME messages into the focused view, in order:
/// a preedit replaces the composition, a commit goes to the doc as a
/// request, and the IME turning on or off ends the composition. A
/// view that lost the focus loses its composition, and stops waiting
/// for an `Ime::Enabled` that may reach another view. A composition
/// that starts or changes is an edit the user sees, so it brings the
/// caret into view as an edit does.
fn read(
    mut ime: MessageReader<Ime>,
    focus: Option<Res<InputFocus>>,
    views: Query<(Entity, &EditView, Option<&Preedit>, Has<Ended>)>,
    mut requests: MessageWriter<EditRequest>,
    mut commands: Commands,
) {
    let focused = focus
        .and_then(|focus| focus.get())
        .filter(|&view| views.contains(view));
    for (view, _, preedit, ended) in &views {
        if (preedit.is_some() || ended) && focused != Some(view) {
            commands
                .entity(view)
                .remove::<(Preedit, Composing, Ended)>();
        }
    }
    let Some((view, edit, was, mut ended)) =
        focused.and_then(|view| views.get(view).ok())
    else {
        ime.clear();
        return;
    };
    let mut now = was.cloned();
    for message in ime.read() {
        now = match message {
            Ime::Preedit { .. } | Ime::Commit { .. } if ended => {
                continue;
            }
            Ime::Preedit { value, cursor, .. } => {
                Preedit::new(value, *cursor)
            }
            Ime::Commit { value, .. } => {
                if !value.is_empty() {
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
            Ime::Enabled { .. } => {
                if ended {
                    commands.entity(view).remove::<Ended>();
                    ended = false;
                }
                None
            }
            Ime::Disabled { .. } => None,
        };
    }
    if now.as_ref() == was {
        return;
    }
    match now {
        Some(preedit) => {
            commands.entity(view).insert((preedit, Composing));
            requests.write(EditRequest {
                doc: edit.doc(),
                view: Some(view),
                request: Request::new(|tx| tx.keep_in_sight()),
            });
        }
        None => {
            commands.entity(view).remove::<(Preedit, Composing)>();
        }
    }
}

/// Draws each view's preedit at its primary caret, after this frame's
/// edits moved the caret, and clears the layer of a view whose
/// composition ended.
fn place(
    mut views: Query<(&EditView, &Preedit, &mut Marks)>,
    mut ended: RemovedComponents<Preedit>,
    mut cleared: Query<&mut Marks, Without<Preedit>>,
    docs: Query<&EditDoc>,
    layers: Query<&CaretLayer>,
) {
    for view in ended.read() {
        if let Ok(marks) = cleared.get_mut(view) {
            clear(marks);
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

fn age(mut views: Query<&mut Ended>) {
    for mut ended in &mut views {
        if *ended == Ended::Off {
            *ended = Ended::Stale;
        }
    }
}

/// winit's macOS view never tells its input context when the IME
/// goes off (its `set_ime_allowed(false)` only clears its own marked
/// text), so the input method would keep composing the old text. On
/// Linux the IME going off and on is the reset, through winit.
#[cfg(target_os = "macos")]
fn discard(
    views: Query<&Ended>,
    windows: Query<
        &bevy::window::RawHandleWrapper,
        With<PrimaryWindow>,
    >,
    _main_thread: bevy::ecs::system::NonSendMarker,
) {
    use objc2_app_kit::NSView;
    use raw_window_handle::RawWindowHandle;

    if !views.iter().any(|ended| *ended == Ended::Off) {
        return;
    }
    let Ok(handle) = windows.single() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.get_window_handle()
    else {
        return;
    };
    // SAFETY: the window's live NSView, on the main thread, which
    // `NonSendMarker` guarantees.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    if let Some(context) = view.inputContext() {
        context.discardMarkedText();
    }
}

/// Keeps the window's IME on while an editable view holds the focus,
/// except for the frame of a press that ended a composition, when it
/// goes off so that the input method drops the composition too, and
/// is on again the frame after. The candidate box sits at the
/// bottom-left of the primary caret, or at the top-left of the view's
/// rows while the caret has no rect.
///
/// Bevy's `TextInputPlugin` turns the IME off in `PreUpdate` whenever
/// no `EditableText` has the focus, and winit reads the window in
/// `Last`, so this runs between them. Once the focus leaves, it
/// writes nothing, and `TextInputPlugin` turns the IME off in the
/// next frame.
fn enable(
    focus: Option<Res<InputFocus>>,
    views: Query<(&EditView, Option<&Ended>)>,
    areas: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    ui_scale: Option<Res<UiScale>>,
) {
    let Some((edit, ended)) = focus
        .and_then(|focus| focus.get())
        .and_then(|view| views.get(view).ok())
    else {
        return;
    };
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let on = ended != Some(&Ended::Off);
    if window.ime_enabled != on {
        window.ime_enabled = on;
    }
    let at = match edit.caret_rect() {
        Some(caret) => Vec2::new(caret.min.x, caret.max.y),
        None => {
            let Some((node, area)) =
                edit.list().and_then(|list| areas.get(list).ok())
            else {
                return;
            };
            (area.translation + node.content_box().min)
                * node.inverse_scale_factor()
        }
    } * ui_scale.map_or(1.0, |scale| scale.0);
    if window.ime_position != at {
        window.ime_position = at;
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
///
/// The run blocks of a row's visual lines overlap when the line is
/// shorter than the font, so a bar takes the run that holds its
/// line's centre. A fallback font, such as a colour emoji font, can
/// put its underline below the line, where the row clips it, so such
/// a bar sits 2 px above the line's bottom instead.
fn bar_rects(
    (_, node, at, block, text, info, clip): Row,
    drawn: core::ops::Range<usize>,
) -> Vec<Rect> {
    let layout = drawn_layout(block, text, node);
    let corner = at.translation + node.content_box().min;
    select::rects(&layout, drawn, 1.0, Color::NONE)
        .filter_map(|([x0, y0, x1, y1], _)| {
            let centre = (y0 + y1) / 2.0;
            let run = info.run_geometry.iter().find(|run| {
                run.bounds.min.x <= x0
                    && x0 < run.bounds.max.x
                    && run.bounds.min.y <= centre
                    && centre < run.bounds.max.y
            });
            let (middle, thickness) = run
                .map(|run| {
                    (
                        run.underline_y
                            + run.underline_thickness / 2.0,
                        run.underline_thickness,
                    )
                })
                .filter(|&(middle, _)| y0 <= middle && middle < y1)
                .unwrap_or((y1 - 2.0, 1.0));
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
