mod support;

use std::time::Instant;

use bevy::app::{App, Last, PostUpdate};
use bevy::camera::visibility::Visibility;
use bevy::clipboard::{ClipboardError, ClipboardRead};
use bevy::color::Color;
use bevy::ecs::change_detection::DetectChanges;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::message::Messages;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, ResMut};
use bevy::ecs::world::{Ref, World};
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::pointer_focus::PointerFocusPlugin;
use bevy::input_focus::{FocusCause, InputFocus, InputFocusPlugin};
use bevy::math::Vec2;
use bevy::text::TextLayoutInfo;
use bevy::ui::widget::Text;
use bevy::ui::{
    ComputedNode, ScrollPosition, UiGlobalTransform, ZIndex, px,
};
use bevy::window::{PrimaryWindow, Window};
use bevy_fynix::views::parley::{Affinity, Cursor};
use bevy_fynix::views::{
    FrameProps, VirtualList, column, rich_layout,
};
use bevy_fynix::{ViewExt, keyed, mount, resource};
use bevy_rubrix::{
    Action, CaretBlink, ClipboardProvider, Command, EditClipboard,
    EditDoc, EditRequest, EditSystems, EditView, EditorChanged,
    Keymap, Lang, LineNumber, Mark, MarkKind, MarkLayer, Marks,
    Motion, Request, Selection, Selections, code_editor,
};
use rubrix_highlight::{Run, highlight, spans};
use support::{
    Distinct, Editing, chord, colour, edit_line, edit_lines, editor,
    hold_key, key_with, laid_out, laid_out_at, list, mount_editor,
    point_at, press_on, spans_of, text_of, type_text,
};

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn doc(app: &App, doc: Entity) -> &EditDoc {
    app.world().get::<EditDoc>(doc).expect("a doc")
}

fn text(app: &App, entity: Entity) -> String {
    doc(app, entity).editor().buffer().to_string()
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

fn request(
    app: &mut App,
    doc: Entity,
    view: Option<Entity>,
    request: Request,
) {
    app.world_mut()
        .write_message(EditRequest { doc, view, request });
    app.update();
}

fn caret_at(app: &mut App, doc: Entity, view: Entity, at: usize) {
    let caret = Selections::new(Selection::caret(at));
    request(app, doc, Some(view), Request::select(caret));
}

fn focus(app: &mut App, view: Entity) {
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
}

/// The spans a row draws for `line` coloured by `runs`, equal looks
/// merged, as `spans_of` reads them.
fn drawn(line: &str, runs: &[Run]) -> Vec<(String, Color)> {
    let mut out: Vec<(String, Color)> = Vec::new();
    for (range, kind) in spans(runs) {
        let piece = &line[range];
        match out.last_mut() {
            Some((text, last)) if *last == colour(kind) => {
                text.push_str(piece)
            }
            _ => out.push((piece.to_owned(), colour(kind))),
        }
    }
    out
}

#[test]
fn typing_colours_each_keystroke() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Rust,
        "fn main() {\n    \n}\n",
        600.0,
        400.0,
    );
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let at = point_at(&app, row, 4);
    press_on(&mut app, row, at, 1);
    type_text(&mut app, "let x = 1;");
    let typed = text(&app, entity);
    assert_eq!(typed, "fn main() {\n    let x = 1;\n}\n");
    let fresh = highlight(Lang::Rust, &typed);
    let lines = edit_lines(&mut app, view);
    assert_eq!(
        lines.len(),
        4,
        "the empty line after the last newline is a row"
    );
    for (line, row) in lines {
        let shown =
            doc(&app, entity).line(line as usize).expect("a line");
        let mut spans = spans_of(&app, row);
        spans.retain(|(text, _)| !text.is_empty());
        assert_eq!(
            spans,
            drawn(shown, fresh.stored_runs(line)),
            "line {line} in the keystroke frame"
        );
    }
}

/// Enter replaces every selection (E1's `Command::Enter`), so a range
/// held below the Enter becomes a caret before the text that followed
/// it. The view keeps both selections through the change and draws
/// the lower one, the primary, on the row its text moved to.
#[test]
fn splice_selection_below_an_edit_keeps_its_text() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        &numbered(20),
        600.0,
        400.0,
    );
    settle(&mut app);
    let buffer = doc(&app, entity).editor().buffer();
    let above = buffer.line_start(2);
    let below = buffer.line_start(10)..buffer.line_start(10) + 4;
    assert_eq!(buffer.slice(below.clone()), "line");
    let mut held = Selections::new(Selection::caret(above));
    held.add(Selection {
        anchor: below.start,
        head: below.end,
    });
    request(&mut app, entity, Some(view), Request::select(held));
    focus(&mut app, view);
    app.world_mut()
        .resource_mut::<Messages<EditorChanged>>()
        .clear();
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    let changes = app
        .world_mut()
        .resource_mut::<Messages<EditorChanged>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(changes.len(), 1, "one change, one message");
    assert_eq!(changes[0].seq, doc(&app, entity).seq());
    settle(&mut app);
    let editor = doc(&app, entity).editor();
    let buffer = editor.buffer();
    let kept = editor.selections().iter().collect::<Vec<_>>();
    assert_eq!(kept.len(), 2, "both selections outlive the Enter");
    assert_eq!(editor.selections().primary_index(), 1);
    let [upper, lower] = [kept[0], kept[1]];
    assert!(upper.is_caret() && lower.is_caret());
    assert_eq!(
        buffer.line_of(upper.head),
        3,
        "line 3 moved down one"
    );
    assert_eq!(upper.head, buffer.line_start(3));
    assert_eq!(
        buffer.line_of(lower.head),
        12,
        "the range's line moved down one and split"
    );
    assert_eq!(lower.head, buffer.line_start(12));
    let rest = buffer.line_start(12)..buffer.line_start(13) - 1;
    assert_eq!(buffer.slice(rest), " 11", "the text after the range");
    assert_eq!(text(&app, entity).lines().nth(11), Some(""));
    let row = edit_line(&mut app, view, 12);
    let drawn = app.world().get::<EditView>(view).expect("a view");
    let rect = drawn.caret_rect().expect("the lower caret is drawn");
    let origin = point_at(&app, row, 0);
    assert!(
        (rect.center() - origin).abs().max_element() < 1.0,
        "the caret sits at line 12's start: {rect:?} and {origin}"
    );
}

/// A count of texts Bevy laid out again, kept by `count_layouts`.
#[derive(Resource, Default)]
struct Relaid(usize);

fn count_layouts(
    texts: Query<Ref<TextLayoutInfo>>,
    mut relaid: ResMut<Relaid>,
) {
    relaid.0 += texts.iter().filter(|text| text.is_changed()).count();
}

/// Each row below an Enter moves to the next line. Its text is the
/// same, and so is the number at each line, so the Enter lays out
/// again only the two lines it touched and at most a number for a
/// line built new. Every row still shows its own number, at its top.
#[test]
fn splice_enter_lays_out_no_moved_row_again() {
    let mut app = laid_out();
    app.init_resource::<Relaid>()
        .add_systems(Last, count_layouts);
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        &numbered(10_000),
        600.0,
        400.0,
    );
    settle(&mut app);
    let area = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_to(4_995);
    settle(&mut app);
    let at = doc(&app, entity).editor().buffer().line_start(5_000);
    caret_at(&mut app, entity, view, at);
    focus(&mut app, view);
    settle(&mut app);
    let moved = edit_lines(&mut app, view)
        .iter()
        .filter(|&&(line, _)| line > 5_000)
        .count();
    assert!(moved > 20, "{moved} built rows below the Enter");
    app.world_mut().resource_mut::<Relaid>().0 = 0;
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    assert_eq!(doc(&app, entity).rows(), 10_002);
    let relaid = app.world().resource::<Relaid>().0;
    assert!(relaid <= 3, "{relaid} texts laid out again");
    settle(&mut app);
    let numbers = app
        .world_mut()
        .query::<(Entity, &LineNumber, &Visibility)>()
        .iter(app.world())
        .filter(|(_, number, shown)| {
            number.view == view && **shown != Visibility::Hidden
        })
        .map(|(node, number, _)| (number.line, node))
        .collect::<std::collections::HashMap<_, _>>();
    for (line, row) in edit_lines(&mut app, view) {
        let number = numbers[&line];
        let shown =
            &app.world().get::<Text>(number).expect("a text").0;
        assert_eq!(
            shown.trim(),
            (line + 1).to_string(),
            "line {line}'s number"
        );
        let top = |node| {
            let world = app.world();
            let size =
                world.get::<ComputedNode>(node).expect("a node");
            let at =
                world.get::<UiGlobalTransform>(node).expect("placed");
            (at.translation.y - size.size().y / 2.0)
                * size.inverse_scale_factor()
        };
        assert!(
            (top(number) - top(row)).abs() < 0.5,
            "line {line}'s number sits at its row's top"
        );
    }
}

#[test]
fn splice_enter_keeps_row_tops() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        &numbered(10_000),
        600.0,
        400.0,
    );
    settle(&mut app);
    let area = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_to(4_995);
    settle(&mut app);
    let at = doc(&app, entity).editor().buffer().line_start(5_000);
    caret_at(&mut app, entity, view, at);
    focus(&mut app, view);
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    settle(&mut app);
    assert_eq!(doc(&app, entity).rows(), 10_002);
    let rows = edit_lines(&mut app, view);
    assert!(rows.len() > 20);
    let world = app.world();
    let state = world.get::<VirtualList>(area).expect("a list");
    assert_eq!(state.count(), 10_002);
    let (node, at, scroll) = (
        world.get::<ComputedNode>(area).expect("a node"),
        world.get::<UiGlobalTransform>(area).expect("a transform"),
        world.get::<ScrollPosition>(area).expect("a scroll"),
    );
    let top = (at.translation.y + node.content_box().min.y)
        * node.inverse_scale_factor();
    for (line, row) in rows {
        let node = world.get::<ComputedNode>(row).expect("a row");
        let shown =
            world.get::<UiGlobalTransform>(row).expect("a row");
        let row_top = (shown.translation.y - node.size().y / 2.0)
            * node.inverse_scale_factor();
        let span = state.row_span(line as usize).expect("a span");
        assert!(
            (row_top - (top + span.top - scroll.y)).abs() < 1.0,
            "line {line} sits at the sum of the heights above it"
        );
        let expect =
            doc(&app, entity).line(line as usize).expect("a line");
        assert_eq!(
            text_of(&app, row),
            expect,
            "line {line} shows its text"
        );
    }
}

struct Stub(&'static str);

impl ClipboardProvider for Stub {
    fn write(
        &mut self,
        _: &mut World,
        _: &str,
    ) -> Result<(), ClipboardError> {
        Ok(())
    }

    fn read(&mut self, _: &mut World) -> ClipboardRead {
        ClipboardRead::Ready(Ok(self.0.to_owned()))
    }
}

#[test]
fn keymap_paste_from_a_stub_lands_at_every_caret() {
    let mut app = laid_out();
    app.insert_resource(Keymap::linux())
        .insert_resource(EditClipboard::new(Stub("ab")));
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Rust,
        "fn main() {}",
        600.0,
        400.0,
    );
    settle(&mut app);
    let mut two = Selections::new(Selection::caret(0));
    two.add(Selection::caret(4));
    request(&mut app, entity, Some(view), Request::select(two));
    focus(&mut app, view);
    chord(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyV, "v");
    assert_eq!(text(&app, entity), "abfn mabain() {}");
}

#[test]
fn keymap_macos_binds_cmd_z_and_option_arrows() {
    let macos = Keymap::macos();
    assert_eq!(
        macos.get("super+z"),
        Some(&Action::Command(Command::Undo))
    );
    let word = |motion| {
        Some(Action::Command(Command::Move {
            motion,
            extend: false,
        }))
    };
    assert_eq!(
        macos.get("alt+left").copied(),
        word(Motion::WordLeft)
    );
    assert_eq!(
        macos.get("alt+right").copied(),
        word(Motion::WordRight)
    );
    assert_eq!(macos.get("ctrl+z"), None);
    let linux = Keymap::linux();
    assert_eq!(
        linux.get("ctrl+z"),
        Some(&Action::Command(Command::Undo))
    );
    assert_eq!(
        linux.get("ctrl+left").copied(),
        word(Motion::WordLeft)
    );
}

#[test]
fn keymap_entries_bind_and_unbind() {
    let mut keymap = Keymap::linux();
    let undo = Some(Action::Command(Command::Undo));
    assert_eq!(
        keymap.bind("mod+u", Action::Command(Command::Undo)),
        Ok(None)
    );
    assert_eq!(keymap.get("ctrl+u").copied(), undo);
    assert_eq!(
        keymap.unbind("ctrl+u").map(|old| old.is_some()),
        Ok(true)
    );
    assert_eq!(keymap.get("ctrl+u"), None);
}

#[test]
fn a_key_handler_edits_through_the_transaction() {
    let mut app = laid_out();
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, "ab\ncd\n", 600.0, 400.0);
    app.world_mut()
        .resource_mut::<Keymap>()
        .bind(
            "mod+d",
            Action::Run(|tx| {
                tx.apply(Command::Enter);
                tx.apply(Command::Type("x"));
            }),
        )
        .expect("a chord");
    settle(&mut app);
    caret_at(&mut app, entity, view, 1);
    focus(&mut app, view);
    chord(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyD, "d");
    assert_eq!(text(&app, entity), "a\nxb\ncd\n");
    let rows = edit_lines(&mut app, view);
    assert_eq!(rows.len(), 4, "both changes reach the rows");
    assert_eq!(text_of(&app, rows[1].1), "xb");
}

#[test]
fn built_rows_cap_is_fynix_max_slots() {
    let mut app = laid_out();
    mount_editor(
        &mut app,
        Lang::Plain,
        &numbered(2_000),
        600.0,
        40_000.0,
    );
    settle(&mut app);
    assert_eq!(
        support::list_state(&mut app).rows(),
        199,
        "damage::BUILT_MAX copies fynix's MAX_SLOTS"
    );
}

#[derive(Resource)]
struct Generation(u32);

#[test]
fn rebuild_keeps_the_doc() {
    let mut app = laid_out();
    let entity = app
        .world_mut()
        .spawn(EditDoc::new(Lang::Rust, "fn a() {}\n"))
        .id();
    app.insert_resource(Editing(entity))
        .insert_resource(Generation(0));
    mount::<Distinct>(
        app.world_mut(),
        keyed(
            resource::<Generation, _>(|generation| generation.0),
            |_| {
                code_editor(resource::<Editing, _>(|editing| {
                    editing.0
                }))
                .boxed()
            },
        )
        .within(column(()).width(px(600.0)).height(px(400.0))),
    );
    settle(&mut app);
    let view = editor(&mut app);
    let row = edit_line(&mut app, view, 0);
    let at = point_at(&app, row, 9);
    press_on(&mut app, row, at, 1);
    type_text(&mut app, " b");
    key_with(
        &mut app,
        &[KeyCode::ShiftLeft],
        KeyCode::ArrowLeft,
        Key::ArrowLeft,
    );
    let before = (
        text(&app, entity),
        doc(&app, entity).editor().selections().clone(),
        doc(&app, entity).editor().history().undo_depth(),
    );
    assert_eq!(before.0, "fn a() {} b\n");
    app.world_mut().resource_mut::<Generation>().0 += 1;
    settle(&mut app);
    let rebuilt = editor(&mut app);
    assert_ne!(rebuilt, view, "the view was built again");
    let after = (
        text(&app, entity),
        doc(&app, entity).editor().selections().clone(),
        doc(&app, entity).editor().history().undo_depth(),
    );
    assert_eq!(after, before);
    let row = edit_line(&mut app, rebuilt, 0);
    assert_eq!(text_of(&app, row), "fn a() {} b");
}

/// The caret offset a press at byte 7 of line 1 lands on, the caret
/// rect relative to the row's text, and whether it shows.
fn press_at_scale(scale: f32) -> (usize, Vec2, bool) {
    let mut app = laid_out_at(scale);
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Rust,
        "fn main() {\n    let value = 1;\n}\n",
        400.0,
        300.0,
    );
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let at = point_at(&app, row, 7);
    press_on(&mut app, row, at, 1);
    settle(&mut app);
    let head = doc(&app, entity).editor().selections().primary().head;
    let shown = app.world().get::<EditView>(view).expect("a view");
    let rect = shown.caret_rect().expect("a drawn caret");
    let origin = point_at(&app, row, 0);
    (head, rect.min - origin, shown.caret_shown())
}

#[test]
fn scale_two_hits_the_same_byte() {
    let (one, rect_one, shown_one) = press_at_scale(1.0);
    let (two, rect_two, shown_two) = press_at_scale(2.0);
    assert_eq!(one, "fn main() {\n".len() + 7);
    assert_eq!(two, one);
    assert!(shown_one && shown_two, "the caret shows at both scales");
    assert!(
        (rect_one - rect_two).abs().max_element() < 1.0,
        "the caret rect is logical at both: {rect_one} and \
         {rect_two}"
    );
}

fn overlay(app: &App, view: Entity) -> Entity {
    let world = app.world();
    world
        .get::<Children>(view)
        .expect("children")
        .iter()
        .copied()
        .find(|&kid| world.get::<ZIndex>(kid) == Some(&ZIndex(1)))
        .expect("the caret overlay")
}

fn bar(app: &mut App, view: Entity) -> Entity {
    let overlay = overlay(app, view);
    app.world().get::<Children>(overlay).expect("a bar")[0]
}

fn caret_centre(app: &App, row: Entity, byte: usize) -> Vec2 {
    let world = app.world();
    let node = world.get::<ComputedNode>(row).expect("a row");
    let at = world.get::<UiGlobalTransform>(row).expect("a row");
    let layout = &rich_layout(world, row).expect("a layout");
    let scale = 1.0 / node.inverse_scale_factor();
    let bbox =
        Cursor::from_byte_index(layout, byte, Affinity::Downstream)
            .geometry(layout, 2.0 * scale);
    let corner = (at.translation
        + node.content_box().min
        + Vec2::new(bbox.x0 as f32, bbox.y0 as f32))
    .round();
    let size =
        Vec2::new(2.0 * scale, (bbox.y1 - bbox.y0) as f32).round();
    corner + size / 2.0
}

#[test]
fn the_caret_moves_in_the_keystroke_frame_at_scale_one_and_two() {
    for scale in [1.0, 2.0] {
        let mut app = laid_out_at(scale);
        let (view, _) = mount_editor(
            &mut app,
            Lang::Rust,
            "fn main() {\n    \n}\n",
            400.0,
            300.0,
        );
        settle(&mut app);
        let row = edit_line(&mut app, view, 1);
        let at = point_at(&app, row, 4);
        press_on(&mut app, row, at, 1);
        settle(&mut app);
        type_text(&mut app, "x");
        let row = edit_line(&mut app, view, 1);
        let bar = bar(&mut app, view);
        let drawn = app
            .world()
            .get::<UiGlobalTransform>(bar)
            .expect("a bar")
            .translation;
        assert_eq!(
            drawn,
            caret_centre(&app, row, 5),
            "scale {scale}"
        );
    }
}

#[test]
fn a_caret_shows_again_in_the_frame_its_row_comes_back() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        &numbered(2_000),
        400.0,
        300.0,
    );
    settle(&mut app);
    let row = edit_line(&mut app, view, 0);
    let at = point_at(&app, row, 6);
    press_on(&mut app, row, at, 1);
    settle(&mut app);
    let bar = bar(&mut app, view);
    let area = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_to(1_000);
    settle(&mut app);
    let parked = app
        .world()
        .get::<UiGlobalTransform>(bar)
        .expect("a bar")
        .translation;
    assert!(
        parked.x < -1_000.0,
        "its row is gone, so its bar is parked"
    );
    type_text(&mut app, "7");
    assert_eq!(text(&app, entity).lines().next(), Some("line 17"));
    let row = edit_line(&mut app, view, 0);
    let drawn = app
        .world()
        .get::<UiGlobalTransform>(bar)
        .expect("a bar")
        .translation;
    assert_eq!(
        drawn,
        caret_centre(&app, row, 7),
        "it draws at its byte"
    );
    assert!(
        app.world()
            .get::<EditView>(view)
            .expect("a view")
            .caret_shown()
    );
}

fn overlay_shown(app: &mut App, view: Entity) -> bool {
    let overlay = overlay(app, view);
    app.world().get::<Visibility>(overlay)
        == Some(&Visibility::Inherited)
}

#[test]
fn the_caret_blinks_at_its_period_while_focused() {
    let mut app = laid_out();
    let (view, _) =
        mount_editor(&mut app, Lang::Plain, "abc\n", 400.0, 300.0);
    settle(&mut app);
    assert!(!overlay_shown(&mut app, view), "no focus, no caret");
    let row = edit_line(&mut app, view, 0);
    let at = point_at(&app, row, 1);
    press_on(&mut app, row, at, 1);
    let mut shown = Vec::new();
    for _ in 0..24 {
        shown.push(overlay_shown(&mut app, view));
        app.update();
    }
    let flips =
        shown.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert!(
        shown[..8].iter().all(|&on| on),
        "solid after the press: {shown:?}"
    );
    assert!(
        shown[12..18].iter().all(|&on| !on),
        "off after 500 ms: {shown:?}"
    );
    assert_eq!(flips, 2, "50 ms frames flip every 10: {shown:?}");
    app.insert_resource(CaretBlink { period: None });
    for _ in 0..12 {
        app.update();
        assert!(
            overlay_shown(&mut app, view),
            "solid without a period"
        );
    }
    app.world_mut().resource_mut::<InputFocus>().clear();
    app.update();
    assert!(
        !overlay_shown(&mut app, view),
        "hidden once focus leaves"
    );
}

#[test]
fn shift_click_extends_and_clicks_select_words_and_lines() {
    let mut app = laid_out();
    let source = "fn main() {\n    self.foo_bar = 1;\n}\n";
    let (view, entity) =
        mount_editor(&mut app, Lang::Rust, source, 600.0, 300.0);
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let start = source.find("self").expect("self");
    let at = point_at(&app, row, 10);
    press_on(&mut app, row, at, 2);
    let word = doc(&app, entity).editor().selections().primary();
    let foo = source.find("foo_bar").expect("foo_bar");
    assert_eq!(word.range(), foo..foo + "foo_bar".len());
    press_on(&mut app, row, at, 3);
    let line = doc(&app, entity).editor().selections().primary();
    assert_eq!(
        line.range(),
        source.find("    self").expect("a line")
            ..source.find('}').expect("a brace")
    );
    let at = point_at(&app, row, 4);
    press_on(&mut app, row, at, 1);
    hold_key(&mut app, KeyCode::ShiftLeft, true);
    let to = point_at(&app, row, 8);
    press_on(&mut app, row, to, 1);
    hold_key(&mut app, KeyCode::ShiftLeft, false);
    let extended = doc(&app, entity).editor().selections().primary();
    assert_eq!((extended.anchor, extended.head), (start, start + 4));
}

#[test]
fn a_press_keeps_the_focus_under_bevy_pointer_focus() {
    let mut app = laid_out();
    app.add_plugins((InputFocusPlugin, PointerFocusPlugin));
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, "abc\n", 600.0, 300.0);
    settle(&mut app);
    let row = edit_line(&mut app, view, 0);
    let at = point_at(&app, row, 3);
    press_on(&mut app, row, at, 1);
    assert_eq!(
        app.world().resource::<InputFocus>().get(),
        Some(view),
        "the window's AcquireFocus does not clear the press's focus"
    );
    type_text(&mut app, "d");
    assert_eq!(
        text(&app, entity),
        "abcd\n",
        "so keys reach the view"
    );
}

#[test]
fn a_press_away_from_the_last_one_starts_a_new_click() {
    let mut app = laid_out();
    let source = "fn main() {\n    self.foo_bar = 1;\n}\n";
    let (view, entity) =
        mount_editor(&mut app, Lang::Rust, source, 600.0, 300.0);
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let (first, second) =
        (point_at(&app, row, 6), point_at(&app, row, 12));
    press_on(&mut app, row, first, 1);
    press_on(&mut app, row, second, 2);
    let line = source.find("    self").expect("a line");
    assert_eq!(
        doc(&app, entity).editor().selections().primary(),
        Selection::caret(line + 12),
        "Bevy counts the second press as a double click"
    );
    press_on(&mut app, row, second, 3);
    let word = doc(&app, entity).editor().selections().primary();
    let foo = source.find("foo_bar").expect("foo_bar");
    assert_eq!(
        word.range(),
        foo..foo + "foo_bar".len(),
        "a press on the same spot does"
    );
}

#[test]
fn a_press_on_the_gutter_puts_the_caret_at_the_line_start() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        "  abc\n  def\n",
        600.0,
        300.0,
    );
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let gutter = point_at(&app, row, 0) - Vec2::new(6.0, 0.0);
    press_on(&mut app, row, gutter, 1);
    let head = doc(&app, entity).editor().selections().primary().head;
    assert_eq!(head, "  abc\n".len());
}

#[test]
fn a_press_on_the_gutter_scrolls_its_caret_into_sight() {
    let mut app = laid_out();
    let long = "x".repeat(300);
    let source = format!("{long}\nabc\n");
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, &source, 600.0, 300.0);
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let gutter = point_at(&app, row, 0) - Vec2::new(6.0, 0.0);
    caret_at(&mut app, entity, view, long.len());
    settle(&mut app);
    let area = list(&mut app);
    let scrolled =
        app.world().get::<ScrollPosition>(area).expect("a scroll");
    assert!(scrolled.x > 0.0, "the long line's end scrolls the view");
    press_on(&mut app, row, gutter, 1);
    settle(&mut app);
    let head = doc(&app, entity).editor().selections().primary().head;
    assert_eq!(
        head,
        long.len() + 1,
        "the caret is at line 2's start"
    );
    let shown = app.world().get::<EditView>(view).expect("a view");
    assert!(shown.caret_shown(), "and shows");
}

#[test]
fn a_press_on_an_insert_lands_at_its_byte() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        "abcd\nefgh\n",
        600.0,
        300.0,
    );
    settle(&mut app);
    let insert = Mark {
        line: 1,
        kind: MarkKind::Insert {
            at: 2,
            text: "XYZ".to_owned(),
            underline: Some(Color::WHITE),
            caret: Some(3),
        },
    };
    app.world_mut()
        .get_mut::<Marks>(view)
        .expect("marks")
        .set(MarkLayer("ime"), vec![insert]);
    app.update();
    let row = edit_line(&mut app, view, 1);
    assert_eq!(
        text_of(&app, row),
        "efXYZgh",
        "the row draws the insert"
    );
    let start = "abcd\n".len();
    let on = point_at(&app, row, 3);
    press_on(&mut app, row, on, 1);
    assert_eq!(
        doc(&app, entity).editor().selections().primary().head,
        start + 2
    );
    let past = point_at(&app, row, 6);
    press_on(&mut app, row, past, 1);
    assert_eq!(
        doc(&app, entity).editor().selections().primary().head,
        start + 3
    );
}

#[test]
fn an_empty_selected_line_gets_a_narrow_rect() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        "ab\n\ncd\n",
        600.0,
        300.0,
    );
    settle(&mut app);
    let all = Selections::new(Selection { anchor: 0, head: 6 });
    request(&mut app, entity, Some(view), Request::select(all));
    settle(&mut app);
    let row = edit_line(&mut app, view, 1);
    let cell = app
        .world()
        .get::<bevy::ecs::hierarchy::ChildOf>(row)
        .expect("a cell")
        .parent();
    let world = app.world();
    let rects = world
        .get::<Children>(cell)
        .expect("a layer")
        .iter()
        .filter_map(|&kid| world.get::<Children>(kid))
        .flat_map(|rects| rects.iter())
        .filter_map(|&rect| world.get::<ComputedNode>(rect))
        .filter(|node| node.size().x > 0.0)
        .count();
    assert_eq!(rects, 1, "one rect for the empty line");
}

#[test]
fn down_moves_by_visual_row_in_wrap_mode() {
    let mut app = laid_out();
    let long = "word ".repeat(40);
    let source = format!("{long}\nnext\n");
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, &source, 240.0, 300.0);
    app.insert_resource(support::Wrapping(
        bevy_rubrix::Overflow::Wrap,
    ));
    settle(&mut app);
    let row = edit_line(&mut app, view, 0);
    let visual =
        rich_layout(app.world(), row).expect("a layout").len();
    assert!(visual > 2, "the long line wraps onto {visual} rows");
    caret_at(&mut app, entity, view, 0);
    focus(&mut app, view);
    key_with(&mut app, &[], KeyCode::ArrowDown, Key::ArrowDown);
    let once = doc(&app, entity).editor().selections().primary().head;
    assert!(
        0 < once && once < long.len(),
        "Down stays in the wrapped line: {once}"
    );
    key_with(&mut app, &[], KeyCode::ArrowDown, Key::ArrowDown);
    let twice =
        doc(&app, entity).editor().selections().primary().head;
    assert!(
        once < twice && twice < long.len(),
        "and moves one row more: {twice}"
    );
    key_with(&mut app, &[], KeyCode::ArrowUp, Key::ArrowUp);
    assert_eq!(
        doc(&app, entity).editor().selections().primary().head,
        once
    );
}

/// The frames in which a doc changed, and how many of those ended
/// with the primary caret out of sight.
#[derive(Resource, Default)]
struct Keystrokes {
    seq: u64,
    frames: u32,
    hidden: u32,
}

fn count_keystrokes(
    views: Query<&EditView>,
    docs: Query<&EditDoc>,
    mut strokes: ResMut<Keystrokes>,
) {
    for view in &views {
        let Ok(doc) = docs.get(view.doc()) else {
            continue;
        };
        if doc.seq() > strokes.seq {
            strokes.seq = doc.seq();
            strokes.frames += 1;
            strokes.hidden += u32::from(!view.caret_shown());
        }
    }
}

#[test]
fn a_join_onto_a_long_line_shows_its_caret_in_the_keystroke_frame() {
    let mut app = laid_out();
    let long = "x".repeat(300);
    let source = format!("{long}\n\nend\n");
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, &source, 600.0, 300.0);
    app.init_resource::<Keystrokes>().add_systems(
        PostUpdate,
        count_keystrokes.after(EditSystems::Draw),
    );
    settle(&mut app);
    caret_at(&mut app, entity, view, long.len() + 1);
    focus(&mut app, view);
    settle(&mut app);
    key_with(&mut app, &[], KeyCode::Backspace, Key::Backspace);
    assert_eq!(text(&app, entity), format!("{long}\nend\n"));
    let strokes = app.world().resource::<Keystrokes>();
    assert_eq!(
        (strokes.frames, strokes.hidden),
        (1, 0),
        "the view scrolls to the line's end in the frame it joins"
    );
}

#[test]
fn page_down_moves_by_the_rows_that_show() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        &numbered(200),
        600.0,
        300.0,
    );
    settle(&mut app);
    let area = list(&mut app);
    let state = app.world().get::<VirtualList>(area).expect("a list");
    let row = state.row_span(0).expect("a measured row").height;
    let node = app.world().get::<ComputedNode>(area).expect("a node");
    let view_height =
        node.content_box().height() * node.inverse_scale_factor();
    let page = (view_height / row).floor() as usize;
    caret_at(&mut app, entity, view, 0);
    focus(&mut app, view);
    key_with(&mut app, &[], KeyCode::PageDown, Key::PageDown);
    let editor = doc(&app, entity).editor();
    let head = editor.selections().primary().head;
    assert_eq!(
        editor.buffer().line_of(head) as usize,
        page,
        "{view_height} px of {row} px rows"
    );
}

/// A sanity number, not a perf claim: the update a key lands in, less
/// an idle update, at 100k lines. Run with `--release --ignored
/// --nocapture`.
#[test]
#[ignore = "a measurement, run by hand"]
fn apply_cost_at_100k_lines() {
    let mut app = laid_out();
    let large =
        include_str!("../../../bench/fixtures/rust/large.rs.txt");
    let text: String = large
        .lines()
        .cycle()
        .take(100_000)
        .flat_map(|line| [line, "\n"])
        .collect();
    let (view, entity) =
        mount_editor(&mut app, Lang::Rust, &text, 800.0, 600.0);
    settle(&mut app);
    let area = list(&mut app);
    app.world_mut()
        .get_mut::<VirtualList>(area)
        .expect("a list")
        .scroll_to(49_990);
    settle(&mut app);
    let at =
        doc(&app, entity).editor().buffer().line_start(50_000) + 4;
    caret_at(&mut app, entity, view, at);
    focus(&mut app, view);
    settle(&mut app);
    let time =
        |app: &mut App, runs: usize, step: &dyn Fn(&mut App)| {
            let start = Instant::now();
            for _ in 0..runs {
                step(app);
            }
            start.elapsed().as_secs_f64() * 1000.0 / runs as f64
        };
    let idle = time(&mut app, 50, &|app| app.update());
    let typing = time(&mut app, 50, &|app| type_text(app, "x"));
    let enter = time(&mut app, 20, &|app| {
        support::key_with(app, &[], KeyCode::Enter, Key::Enter);
    }) / 2.0;
    println!(
        "100k lines: idle update {idle:.3} ms, typed letter \
         {typing:.3} ms, Enter {enter:.3} ms per update"
    );
    assert_eq!(doc(&app, entity).rows(), 100_021);
}
