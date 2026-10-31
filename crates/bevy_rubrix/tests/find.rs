mod support;

use core::time::Duration;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::resource::Resource;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::{
    FocusCause, InputDispatchPlugin, InputFocus,
};
use bevy::math::Rect;
use bevy::text::{EditableText, TextBackgroundColor, TextLayoutInfo};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::widget::Text;
use bevy::ui::{ComputedNode, UiGlobalTransform, px};
use bevy::ui_widgets::Activate;
use bevy::window::{PrimaryWindow, Window};
use bevy_fynix::views::{FrameProps, VirtualList, column};
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::find::{FIND, FindBar, find_bar};
use bevy_rubrix::{
    Command, EditDoc, EditRequest, Lang, Marks, Request,
    RubrixPlugin, code_editor,
};
use rubrix_edit::Buffer;
use rubrix_edit::find::Search;
use support::diff::Plain;
use support::{chord, edit_line, editor, key_with, lay_out, list};

#[derive(Resource)]
struct Editing(Entity);

fn editing(text: &str) -> (App, Entity, Entity) {
    editing_at(text, 1.0)
}

fn editing_at(
    text: &str,
    scale_factor: f32,
) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        FynixPlugin::<Plain>::default(),
        RubrixPlugin,
    ))
    .insert_resource(Theme(Plain))
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        Duration::from_millis(50),
    ));
    lay_out(&mut app, scale_factor);
    app.add_plugins(InputDispatchPlugin);
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let doc =
        app.world_mut().spawn(EditDoc::new(Lang::Rust, text)).id();
    app.insert_resource(Editing(doc));
    let shown = || resource::<Editing, _>(|editing| editing.0);
    mount::<Plain>(
        app.world_mut(),
        column((code_editor(shown()), find_bar(shown())))
            .width(px(800.0))
            .height(px(400.0)),
    );
    app.update();
    let view = editor(&mut app);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
    settle(&mut app);
    (app, view, doc)
}

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn bar(app: &mut App) -> &FindBar {
    app.world_mut()
        .query::<&FindBar>()
        .single(app.world())
        .expect("one bar")
}

fn text_of(app: &App, doc: Entity) -> String {
    let doc = app.world().get::<EditDoc>(doc).expect("a doc");
    doc.editor().buffer().to_string()
}

fn counts_until_final(app: &mut App) -> Vec<usize> {
    let mut counts = Vec::new();
    while bar(app).settled().is_none() {
        counts.push(bar(app).count());
        app.update();
        assert!(counts.len() < 10_000, "the search ends");
    }
    settle(app);
    counts
}

fn open(app: &mut App) -> Vec<usize> {
    chord(app, &[KeyCode::ControlLeft], KeyCode::KeyF, "f");
    counts_until_final(app)
}

fn tinted(app: &mut App, view: Entity, line: u32) -> bool {
    let row = edit_line(app, view, line);
    let world = app.world();
    world.get::<Children>(row).is_some_and(|spans| {
        spans.iter().any(|span| {
            world.get::<TextBackgroundColor>(*span).is_some()
        })
    })
}

#[test]
fn only_built_rows_get_match_marks() {
    let text = "theme x\n".repeat(2_000);
    let (mut app, view, _) = editing(&text);
    open(&mut app);
    assert_eq!(bar(&mut app).count(), 2_000);
    let list_entity = list(&mut app);
    let spawned = app
        .world()
        .get::<VirtualList>(list_entity)
        .and_then(VirtualList::spawned)
        .expect("built rows");
    let marks = app.world().get::<Marks>(view).expect("marks");
    let lines: Vec<u32> =
        marks.layer(FIND).iter().map(|mark| mark.line).collect();
    let built: Vec<u32> =
        (*spawned.start() as u32..=*spawned.end() as u32).collect();
    assert_eq!(lines, built, "one mark per built row, and no other");
    assert!(built.len() < 2_000);
    assert_eq!(bar(&mut app).marked(), built.len());
    assert!(tinted(&mut app, view, 0), "a built row draws its tint");
}

#[test]
fn replace_all_is_one_undo_step() {
    let text = "theme = theme();\nfn theme() {}\n";
    let (mut app, view, doc) = editing(text);
    chord(
        &mut app,
        &[KeyCode::ControlLeft, KeyCode::AltLeft],
        KeyCode::KeyF,
        "f",
    );
    counts_until_final(&mut app);
    assert_eq!(bar(&mut app).count(), 3);
    let world = app.world_mut();
    let field = world.resource::<InputFocus>().get();
    let field = field.filter(|&field| field != view);
    let field = field.expect("the replace field has the focus");
    world
        .get_mut::<EditableText>(field)
        .expect("a field")
        .editor_mut()
        .set_text("palette");
    app.update();
    app.world_mut().write_message(EditRequest {
        doc,
        view: Some(view),
        request: Request::new(bevy_rubrix::find::replace_all),
    });
    counts_until_final(&mut app);
    assert_eq!(
        text_of(&app, doc),
        "palette = palette();\nfn palette() {}\n"
    );
    assert_eq!(bar(&mut app).label(), "No results");
    let undo = Request::command(Command::Undo);
    app.world_mut().write_message(EditRequest::new(doc, undo));
    counts_until_final(&mut app);
    assert_eq!(text_of(&app, doc), text);
    assert_eq!(bar(&mut app).count(), 3);
}

#[test]
fn a_sliced_100k_line_search_finds_what_one_pass_finds() {
    let text =
        "let theme = self.theme();\nfn other() {}\n".repeat(50_000);
    let (mut app, _, _) = editing(&text);
    let counts = open(&mut app);
    let query = bar(&mut app).query().clone();
    assert_eq!(
        query.text, "let",
        "the bar seeds the word at the caret"
    );
    let mut one = Search::new(&query);
    one.run(&Buffer::new(&text));
    let all = one.matches();
    assert_eq!(all.len(), 50_000);
    assert!(!bar(&mut app).is_capped());
    assert!(
        counts.iter().any(|&count| 0 < count && count < all.len()),
        "a slice ended between two matches: {counts:?}"
    );
    let found = bar(&mut app).matches();
    let first_difference =
        found.iter().zip(all).position(|(found, one)| found != one);
    assert_eq!(
        (found.len(), first_difference),
        (all.len(), None),
        "the sliced search finds the matches one pass finds"
    );
}

fn focused(app: &App) -> Option<Entity> {
    app.world().resource::<InputFocus>().get()
}

#[test]
fn no_key_in_a_field_leaves_the_focus_on_nothing() {
    let (mut app, view, _) = editing("theme\ntheme\n");
    let (ctrl, alt, shift) =
        (KeyCode::ControlLeft, KeyCode::AltLeft, KeyCode::ShiftLeft);
    let presses: [(&[KeyCode], KeyCode, Key); 6] = [
        (&[ctrl], KeyCode::Enter, Key::Enter),
        (&[alt], KeyCode::Enter, Key::Enter),
        (&[ctrl, shift], KeyCode::Enter, Key::Enter),
        (&[shift, alt], KeyCode::Enter, Key::Enter),
        (&[shift], KeyCode::Escape, Key::Escape),
        (&[ctrl], KeyCode::Escape, Key::Escape),
    ];
    for replacing in [false, true] {
        for (mods, code, key) in presses.clone() {
            let open =
                if replacing { &[ctrl, alt][..] } else { &[ctrl] };
            chord(&mut app, open, KeyCode::KeyF, "f");
            settle(&mut app);
            let field = focused(&app).filter(|&field| field != view);
            let field = field.expect("a field has the focus");
            key_with(&mut app, mods, code, key.clone());
            settle(&mut app);
            assert_eq!(
                focused(&app),
                Some(field),
                "{mods:?} {key:?} in the {} field",
                if replacing { "replace" } else { "find" }
            );
            key_with(&mut app, &[], KeyCode::Escape, Key::Escape);
            settle(&mut app);
            assert!(
                !bar(&mut app).is_open(),
                "Escape closes the bar"
            );
            assert_eq!(
                focused(&app),
                Some(view),
                "the view has the focus"
            );
        }
    }
}

struct Drawn {
    node: Rect,
    width: f32,
}

fn drawn(app: &mut App, text: &str) -> Drawn {
    let world = app.world_mut();
    let mut labels = world.query::<(
        &Text,
        &ComputedNode,
        &UiGlobalTransform,
        &TextLayoutInfo,
    )>();
    let (_, node, at, info) = labels
        .iter(world)
        .find(|(label, ..)| label.0 == text)
        .unwrap_or_else(|| panic!("a label reading {text:?}"));
    Drawn {
        node: Rect::from_center_size(at.translation, node.size()),
        width: info.size.x,
    }
}

fn press(app: &mut App, button_text: &str) {
    let world = app.world_mut();
    let label = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, label)| label.0 == button_text)
        .map(|(label, _)| label)
        .expect("a button label");
    let button =
        world.get::<ChildOf>(label).expect("a button").parent();
    world.trigger(Activate { entity: button });
    settle(app);
}

#[test]
fn a_regex_error_reads_whole_in_the_bar() {
    for scale in [1.0, 2.0] {
        let (mut app, _, _) = editing_at("theme\n", scale);
        open(&mut app);
        press(&mut app, ".*");
        let field = focused(&app).expect("the find field");
        app.world_mut()
            .get_mut::<EditableText>(field)
            .expect("a field")
            .editor_mut()
            .set_text("e(");
        counts_until_final(&mut app);
        let error = "Invalid regex: unclosed group";
        assert_eq!(bar(&mut app).label(), error);
        let Drawn { node, width } = drawn(&mut app, error);
        assert!(
            width <= node.width() + 0.5,
            "at scale {scale}, the error draws {width} px in a {} \
             px label",
            node.width()
        );
        for button in ["Aa", "W", ".*", "Prev", "Next", "Close"] {
            let other = drawn(&mut app, button).node;
            assert!(
                node.intersect(other).is_empty(),
                "at scale {scale}, the error {node:?} runs under \
                 {button} {other:?}"
            );
        }
    }
}
