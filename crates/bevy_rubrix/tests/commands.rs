mod support;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input_focus::{
    FocusCause, FocusedInput, InputDispatchPlugin, InputFocus,
    InputFocusPlugin,
};
use bevy::math::Vec2;
use bevy::window::{PrimaryWindow, Window};
use bevy_rubrix::{
    Action, Affinity, BRACKET, BracketMatch, Brackets, Command,
    EditDoc, EditRequest, Keymap, Lang, Mark, MarkKind, Marks,
    Motion, Overflow, Request, Selection, Selections,
    toggle_block_comment, toggle_line_comment,
};
use support::{
    Wrapping, chord, drag_end_on, drag_on, edit_line, hold_key,
    hold_on, key_with, laid_out, mod_key, mount_editor, point_at,
    press_on, type_text,
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

fn carets(app: &App, entity: Entity) -> usize {
    doc(app, entity).editor().selections().len()
}

fn caret_at(app: &mut App, doc: Entity, view: Entity, at: usize) {
    let caret = Selections::new(Selection::caret(at));
    app.world_mut().write_message(EditRequest {
        doc,
        view: Some(view),
        request: Request::select(caret),
    });
    settle(app);
}

fn focus(app: &mut App, view: Entity) {
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
}

fn command(command: Command<'static>) -> Action {
    Action::Command(command)
}

fn go(motion: Motion) -> Action {
    command(Command::Move {
        motion,
        extend: false,
    })
}

/// The chords bound on both platforms, written with `mod`.
fn shared() -> Vec<(&'static str, Action)> {
    vec![
        ("tab", command(Command::Indent)),
        ("shift+tab", command(Command::Outdent)),
        ("mod+]", command(Command::IndentLines)),
        ("mod+[", command(Command::Outdent)),
        ("alt+up", command(Command::MoveLines { down: false })),
        ("alt+down", command(Command::MoveLines { down: true })),
        ("shift+alt+up", command(Command::CopyLines { down: false })),
        (
            "shift+alt+down",
            command(Command::CopyLines { down: true }),
        ),
        ("mod+enter", command(Command::BlankLineBelow)),
        ("ctrl+t", command(Command::Transpose)),
        ("mod+d", command(Command::AddNextOccurrence)),
        ("mod+/", Action::Run(toggle_line_comment)),
        ("shift+alt+a", Action::Run(toggle_block_comment)),
    ]
}

fn emacs() -> Vec<(&'static str, Action)> {
    vec![
        ("ctrl+k", command(Command::DeleteToLineEnd)),
        ("ctrl+a", go(Motion::LineStart)),
        ("ctrl+e", go(Motion::End)),
        ("ctrl+p", go(Motion::Up)),
        ("ctrl+n", go(Motion::Down)),
        ("ctrl+f", go(Motion::Right)),
        ("ctrl+b", go(Motion::Left)),
    ]
}

#[test]
fn both_keymaps_bind_every_new_chord() {
    for keymap in [Keymap::linux(), Keymap::macos()] {
        let platform = keymap.platform();
        for (chord, action) in shared() {
            assert_eq!(
                keymap.get(chord),
                Some(&action),
                "{chord} on {platform:?}"
            );
        }
    }
}

#[test]
fn only_macos_binds_the_emacs_keys() {
    let macos = Keymap::macos();
    for (chord, action) in emacs() {
        assert_eq!(macos.get(chord), Some(&action), "{chord}");
    }
    let linux = Keymap::linux();
    assert_ne!(
        linux.get("ctrl+k"),
        Some(&command(Command::DeleteToLineEnd))
    );
    assert_eq!(
        linux.get("ctrl+a"),
        Some(&command(Command::SelectAll)),
        "Ctrl+A still selects all on Linux"
    );
}

#[test]
fn home_goes_to_the_row_start_and_only_cmd_left_to_the_indent() {
    let reach = |motion| {
        command(Command::Move {
            motion,
            extend: true,
        })
    };
    for keymap in [Keymap::linux(), Keymap::macos()] {
        let platform = keymap.platform();
        assert_eq!(
            keymap.get("home"),
            Some(&go(Motion::LineStart)),
            "{platform:?}"
        );
        assert_eq!(
            keymap.get("shift+home"),
            Some(&reach(Motion::LineStart)),
            "{platform:?}"
        );
        assert_eq!(keymap.get("end"), Some(&go(Motion::End)));
        assert_eq!(
            keymap.get("shift+end"),
            Some(&reach(Motion::End))
        );
    }
    let macos = Keymap::macos();
    assert_eq!(macos.get("super+left"), Some(&go(Motion::Home)));
    assert_eq!(
        macos.get("super+shift+left"),
        Some(&reach(Motion::Home))
    );
    assert_eq!(macos.get("super+right"), Some(&go(Motion::End)));
    let linux = Keymap::linux();
    assert_eq!(linux.get("ctrl+left"), Some(&go(Motion::WordLeft)));
    assert_eq!(linux.get("ctrl+right"), Some(&go(Motion::WordRight)));
}

#[test]
fn mod_click_adds_carets_and_removes_the_one_it_lands_on() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        "aaa\nbbb\nccc\nddd\n",
        600.0,
        300.0,
    );
    settle(&mut app);
    let at = |app: &mut App, line: u32| {
        let row = edit_line(app, view, line);
        (row, point_at(app, row, 1))
    };
    let (row, first) = at(&mut app, 0);
    press_on(&mut app, row, first, 1);
    assert_eq!(carets(&app, entity), 1);
    hold_key(&mut app, mod_key(), true);
    let mut counts = Vec::new();
    for line in [1, 2, 3, 1] {
        let (row, point) = at(&mut app, line);
        press_on(&mut app, row, point, 1);
        counts.push(carets(&app, entity));
    }
    hold_key(&mut app, mod_key(), false);
    assert_eq!(counts, [2, 3, 4, 3]);
    let lines: Vec<u32> = doc(&app, entity)
        .editor()
        .selections()
        .iter()
        .map(|s| doc(&app, entity).editor().buffer().line_of(s.head))
        .collect();
    assert_eq!(lines, [0, 2, 3], "the caret on line 2 went");
}

#[test]
fn alt_drag_selects_a_column_over_five_lines() {
    let mut app = laid_out();
    let source = "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\n";
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, source, 600.0, 300.0);
    settle(&mut app);
    let top = edit_line(&mut app, view, 0);
    let from = point_at(&app, top, 2);
    let bottom = edit_line(&mut app, view, 4);
    let to = point_at(&app, bottom, 4);
    hold_key(&mut app, KeyCode::AltLeft, true);
    hold_on(&mut app, top, from, 1);
    drag_on(&mut app, top, from, to);
    drag_end_on(&mut app, top, from, to);
    hold_key(&mut app, KeyCode::AltLeft, false);
    assert_eq!(carets(&app, entity), 5);
    focus(&mut app, view);
    type_text(&mut app, "x");
    assert_eq!(
        text(&app, entity),
        "lix 1\nlix 2\nlix 3\nlix 4\nlix 5\nline 6\n"
    );
}

#[test]
fn alt_drag_past_a_short_line_keeps_the_pointer_column_on_long_lines()
{
    let mut app = laid_out();
    let source = "abcdefgh\nabcdefgh\nab\n";
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, source, 600.0, 300.0);
    settle(&mut app);
    let top = edit_line(&mut app, view, 0);
    let from = point_at(&app, top, 2);
    let short = edit_line(&mut app, view, 2);
    let to = Vec2::new(
        point_at(&app, top, 6).x,
        point_at(&app, short, 1).y,
    );
    hold_key(&mut app, KeyCode::AltLeft, true);
    hold_on(&mut app, top, from, 1);
    drag_on(&mut app, top, from, to);
    drag_end_on(&mut app, top, from, to);
    hold_key(&mut app, KeyCode::AltLeft, false);
    focus(&mut app, view);
    type_text(&mut app, "x");
    assert_eq!(text(&app, entity), "abxgh\nabxgh\nabx\n");
}

fn ranges(app: &App, entity: Entity) -> Vec<(usize, usize)> {
    doc(app, entity)
        .editor()
        .selections()
        .iter()
        .map(|s| (s.start(), s.end()))
        .collect()
}

#[test]
fn mod_double_and_triple_clicks_add_the_word_and_the_line() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Plain,
        "aaa bbb\nccc ddd\neee fff\n",
        600.0,
        300.0,
    );
    settle(&mut app);
    let top = edit_line(&mut app, view, 0);
    let first = point_at(&app, top, 0);
    press_on(&mut app, top, first, 1);
    hold_key(&mut app, mod_key(), true);
    let middle = edit_line(&mut app, view, 1);
    let ddd = point_at(&app, middle, 5);
    for count in 1..=2 {
        press_on(&mut app, middle, ddd, count);
    }
    assert_eq!(
        ranges(&app, entity),
        [(0, 0), (12, 15)],
        "a quick second mod+click keeps the carets and adds the word"
    );
    let bottom = edit_line(&mut app, view, 2);
    let eee = point_at(&app, bottom, 1);
    for count in 1..=3 {
        press_on(&mut app, bottom, eee, count);
    }
    hold_key(&mut app, mod_key(), false);
    assert_eq!(
        ranges(&app, entity),
        [(0, 0), (12, 15), (16, 24)],
        "a mod+triple-click adds the line with its break"
    );
}

fn alt_drag(
    app: &mut App,
    on: Entity,
    from: Vec2,
    to: Vec2,
    count: u8,
) {
    hold_on(app, on, from, count);
    drag_on(app, on, from, to);
    drag_end_on(app, on, from, to);
}

#[test]
fn a_quick_second_alt_drag_still_selects_a_column() {
    let mut app = laid_out();
    let source = "line 1\nline 2\nline 3\n";
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, source, 600.0, 300.0);
    settle(&mut app);
    let top = edit_line(&mut app, view, 0);
    let from = point_at(&app, top, 2);
    let bottom = edit_line(&mut app, view, 2);
    let to = point_at(&app, bottom, 4);
    hold_key(&mut app, KeyCode::AltLeft, true);
    alt_drag(&mut app, top, from, to, 1);
    alt_drag(&mut app, top, from, to, 2);
    hold_key(&mut app, KeyCode::AltLeft, false);
    assert_eq!(ranges(&app, entity), [(2, 4), (9, 11), (16, 18)]);
}

#[test]
fn alt_drag_from_a_wrapped_row_takes_the_column_on_that_row() {
    let mut app = laid_out();
    let long = "word ".repeat(40);
    let source = format!("{long}\nabcdefgh\nabcdefgh\n");
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, &source, 300.0, 300.0);
    app.world_mut().resource_mut::<Wrapping>().0 = Overflow::Wrap;
    settle(&mut app);
    let top = edit_line(&mut app, view, 0);
    let row_one = point_at(&app, top, 0).y;
    let second = (0..long.len())
        .find(|&byte| point_at(&app, top, byte).y > row_one + 1.0)
        .expect("the long line wraps");
    let from = point_at(&app, top, second + 2);
    let bottom = edit_line(&mut app, view, 2);
    let to = Vec2::new(
        point_at(&app, top, second + 5).x,
        point_at(&app, bottom, 0).y,
    );
    hold_key(&mut app, KeyCode::AltLeft, true);
    alt_drag(&mut app, top, from, to, 1);
    hold_key(&mut app, KeyCode::AltLeft, false);
    let next = long.len() + 1;
    assert_eq!(
        ranges(&app, entity),
        [
            (second + 2, second + 5),
            (next + 2, next + 5),
            (next + 11, next + 14),
        ],
        "the press line takes its column on the row under the press"
    );
}

fn head(app: &App, entity: Entity) -> (usize, Affinity) {
    let selections = doc(app, entity).editor().selections();
    let primary = selections.primary_index();
    (selections.primary().head, selections.affinity(primary))
}

#[test]
fn a_press_right_of_a_wrapped_row_lands_at_its_end() {
    let mut app = laid_out();
    let long = "word ".repeat(40);
    let source = format!("{long}\nnext\n");
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, &source, 300.0, 300.0);
    app.world_mut().resource_mut::<Wrapping>().0 = Overflow::Wrap;
    settle(&mut app);
    let top = edit_line(&mut app, view, 0);
    let row_one = point_at(&app, top, 0).y;
    let second = (0..long.len())
        .find(|&byte| point_at(&app, top, byte).y > row_one + 1.0)
        .expect("the long line wraps");
    let advance = point_at(&app, top, 1).x - point_at(&app, top, 0).x;
    let right = Vec2::new(
        point_at(&app, top, second - 1).x + 0.75 * advance,
        row_one,
    );
    let wrapped = (second, Affinity::Upstream);
    let row_two = point_at(&app, top, second);
    press_on(&mut app, top, row_two, 1);
    assert_eq!(
        head(&app, entity),
        (second, Affinity::Downstream),
        "a press at the start of row 1"
    );
    press_on(&mut app, top, right, 1);
    assert_eq!(head(&app, entity), wrapped, "a press");
    let from = point_at(&app, top, 2);
    hold_on(&mut app, top, from, 1);
    drag_on(&mut app, top, from, right);
    drag_end_on(&mut app, top, from, right);
    assert_eq!(head(&app, entity), wrapped, "a drag");
    for key in [KeyCode::ShiftLeft, mod_key(), KeyCode::AltLeft] {
        press_on(&mut app, top, from, 1);
        hold_key(&mut app, key, true);
        press_on(&mut app, top, right, 1);
        hold_key(&mut app, key, false);
        assert_eq!(
            head(&app, entity),
            wrapped,
            "a press with {key:?}"
        );
    }
}

#[derive(Resource, Default)]
struct ReachedWindow(Vec<KeyCode>);

/// An editor with the focus under Bevy's focused input, whose
/// window records each key press that bubbles up to it, where Bevy's
/// and fynix's tab navigation would move the focus.
fn focused_editor(text: &str) -> (App, Entity, Entity) {
    let mut app = laid_out();
    app.add_plugins((InputFocusPlugin, InputDispatchPlugin))
        .init_resource::<ReachedWindow>();
    app.world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .observe(
            |key: On<FocusedInput<KeyboardInput>>,
             mut reached: ResMut<ReachedWindow>| {
                if key.input.state == ButtonState::Pressed {
                    reached.0.push(key.input.key_code);
                }
            },
        );
    let (view, entity) =
        mount_editor(&mut app, Lang::Plain, text, 600.0, 300.0);
    settle(&mut app);
    caret_at(&mut app, entity, view, 0);
    focus(&mut app, view);
    (app, view, entity)
}

fn reached_tab(app: &App) -> bool {
    app.world()
        .resource::<ReachedWindow>()
        .0
        .contains(&KeyCode::Tab)
}

#[test]
fn tab_stays_in_the_focused_editor() {
    let (mut app, view, _) = focused_editor("a\n");
    key_with(&mut app, &[], KeyCode::Tab, Key::Tab);
    key_with(&mut app, &[KeyCode::ShiftLeft], KeyCode::Tab, Key::Tab);
    assert_eq!(
        app.world().resource::<InputFocus>().get(),
        Some(view)
    );
    assert!(!reached_tab(&app), "no tab navigation saw it");
    app.world_mut()
        .resource_mut::<Keymap>()
        .unbind("tab")
        .expect("a chord");
    key_with(&mut app, &[], KeyCode::Tab, Key::Tab);
    assert!(reached_tab(&app), "an unbound Tab bubbles up");
}

#[test]
fn tab_indents_at_the_focused_caret() {
    let (mut app, view, entity) = focused_editor("a\n");
    key_with(&mut app, &[], KeyCode::Tab, Key::Tab);
    assert_eq!(text(&app, entity), "    a\n");
    assert_eq!(
        app.world().resource::<InputFocus>().get(),
        Some(view)
    );
}

#[test]
fn mod_slash_comments_a_rust_line() {
    let mut app = laid_out();
    let (view, entity) = mount_editor(
        &mut app,
        Lang::Rust,
        "let a = 1;\n",
        600.0,
        300.0,
    );
    settle(&mut app);
    caret_at(&mut app, entity, view, 0);
    focus(&mut app, view);
    chord(&mut app, &[mod_key()], KeyCode::Slash, "/");
    assert_eq!(text(&app, entity), "// let a = 1;\n");
}

fn bracket_marks(app: &App, view: Entity) -> Vec<(u32, usize)> {
    app.world()
        .get::<Marks>(view)
        .expect("marks")
        .layer(BRACKET)
        .iter()
        .map(|mark: &Mark| match &mark.kind {
            MarkKind::Tint { bytes, .. } => (mark.line, bytes.start),
            other => panic!("a tint, not {other:?}"),
        })
        .collect()
}

#[test]
fn the_bracket_match_and_its_marks_follow_the_caret() {
    let mut app = laid_out();
    let source = "fn a() {\n    b(\"(\");\n}\n";
    let (view, entity) =
        mount_editor(&mut app, Lang::Rust, source, 600.0, 300.0);
    settle(&mut app);
    let brackets = |app: &App| {
        app.world()
            .get::<BracketMatch>(entity)
            .and_then(BracketMatch::brackets)
    };
    let open = source.find('{').expect("a brace");
    let close = source.rfind('}').expect("a brace");
    caret_at(&mut app, entity, view, open + 1);
    assert_eq!(
        brackets(&app),
        Some(Brackets {
            near: open,
            far: close
        })
    );
    assert_eq!(bracket_marks(&app, view), [(0, open), (2, 0)]);
    let quoted = source.find("\"(").expect("a string") + 2;
    caret_at(&mut app, entity, view, quoted);
    assert_eq!(brackets(&app), None, "a bracket in a string");
    assert_eq!(bracket_marks(&app, view), []);
}
