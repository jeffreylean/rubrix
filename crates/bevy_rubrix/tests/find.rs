mod support;

use core::time::Duration;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::resource::Resource;
use bevy::input::keyboard::KeyCode;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::text::{EditableText, TextBackgroundColor};
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::ui::px;
use bevy_fynix::views::{FrameProps, VirtualList, column};
use bevy_fynix::{FynixPlugin, Theme, mount, resource};
use bevy_rubrix::find::{FIND, FindBar, find_bar};
use bevy_rubrix::{
    Command, EditDoc, EditRequest, Lang, Marks, Request,
    RubrixPlugin, code_editor,
};
use rubrix_edit::Buffer;
use rubrix_edit::find::{Query, Search};
use support::diff::Plain;
use support::{chord, edit_line, editor, lay_out, list};

#[derive(Resource)]
struct Editing(Entity);

fn editing(text: &str) -> (App, Entity, Entity) {
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
    lay_out(&mut app, 1.0);
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

fn searched(app: &mut App) -> u32 {
    let mut frames = 0;
    while bar(app).settled().is_none() {
        app.update();
        frames += 1;
        assert!(frames < 10_000, "the search ends");
    }
    settle(app);
    frames
}

fn open(app: &mut App) -> u32 {
    chord(app, &[KeyCode::ControlLeft], KeyCode::KeyF, "f");
    searched(app)
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
    searched(&mut app);
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
    searched(&mut app);
    assert_eq!(
        text_of(&app, doc),
        "palette = palette();\nfn palette() {}\n"
    );
    assert_eq!(bar(&mut app).label(), "No results");
    let undo = Request::command(Command::Undo);
    app.world_mut().write_message(EditRequest::new(doc, undo));
    searched(&mut app);
    assert_eq!(text_of(&app, doc), text);
    assert_eq!(bar(&mut app).count(), 3);
}

#[test]
fn a_search_of_100k_lines_spans_frames_and_counts_as_one_pass() {
    let text = "let theme = self.theme();\n".repeat(100_000);
    let (mut app, _, _) = editing(&text);
    let frames = open(&mut app);
    let mut one = Search::new(&Query {
        text: "theme".into(),
        ..Query::default()
    });
    one.run(&Buffer::new(&text));
    assert!(frames > 2, "the search took {frames} frames");
    assert_eq!(bar(&mut app).count(), one.matches().len());
}
