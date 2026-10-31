mod support;

use bevy::app::{App, Update};
use bevy::clipboard::{ClipboardError, ClipboardRead};
use bevy::ecs::entity::Entity;
use bevy::ecs::message::MessageReader;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::ecs::world::World;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::ui::px;
use bevy_fynix::views::{FrameProps, column};
use bevy_fynix::{ViewExt, keyed, mount, resource};
use bevy_rubrix::{
    ClipboardProvider, Command, EditClipboard, EditDoc, EditKey,
    EditRequest, EditStates, EditorChanged, FollowPlugin, Keymap,
    Lang, LineAnchors, LineAnnotations, MarkKind, MarkLayer, Markers,
    Marks, Ranges, Request, Selection, Selections, Severity,
    code_editor,
};
use support::{
    Distinct, Editing, chord, editor, key_with, laid_out,
    mount_editor, type_text,
};

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn text(app: &App, doc: Entity) -> String {
    app.world()
        .get::<EditDoc>(doc)
        .expect("a doc")
        .editor()
        .buffer()
        .to_string()
}

fn request(
    app: &mut App,
    doc: Entity,
    view: Entity,
    request: Request,
) {
    app.world_mut().write_message(EditRequest {
        doc,
        view: Some(view),
        request,
    });
    app.update();
}

fn caret_at(app: &mut App, doc: Entity, view: Entity, at: usize) {
    let caret = Selections::new(Selection::caret(at));
    request(app, doc, view, Request::select(caret));
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
}

#[derive(Resource, Default)]
struct Seen(Vec<(Entity, u64)>);

fn record(
    mut changed: MessageReader<EditorChanged>,
    mut seen: ResMut<Seen>,
) {
    seen.0.extend(
        changed.read().map(|changed| (changed.doc, changed.seq)),
    );
}

#[test]
fn editor_changed_arrives_once_per_change_with_a_rising_revision() {
    let mut app = laid_out();
    app.init_resource::<Seen>().add_systems(Update, record);
    let (view, doc) = mount_editor(
        &mut app,
        Lang::Rust,
        "fn a() {}\n",
        600.0,
        400.0,
    );
    settle(&mut app);
    caret_at(&mut app, doc, view, 9);
    for c in ["x", "y", "z"] {
        type_text(&mut app, c);
    }
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    request(&mut app, doc, view, Request::command(Command::Undo));
    settle(&mut app);
    let seen = &app.world().resource::<Seen>().0;
    assert_eq!(seen.len(), 5, "three keys, an Enter and an undo");
    assert!(seen.iter().all(|&(at, _)| at == doc));
    let first = seen[0].1;
    let revisions: Vec<u64> =
        seen.iter().map(|&(_, seq)| seq).collect();
    assert_eq!(revisions, (first..first + 5).collect::<Vec<_>>());
    assert_eq!(text(&app, doc), "fn a() {}xyz\n");
}

#[derive(Resource)]
struct Generation(u32);

#[test]
fn undo_after_a_respawn_restores_the_text_from_before_it() {
    const TEXT: &str = "fn a() {}\n";
    let mut app = laid_out();
    let key = EditKey("src/a.rs".into());
    let doc = app
        .world_mut()
        .spawn((EditDoc::new(Lang::Rust, TEXT), key.clone()))
        .id();
    app.insert_resource(Editing(doc))
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
    caret_at(&mut app, doc, view, 9);
    type_text(&mut app, " b");
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    let typed = text(&app, doc);
    assert_eq!(typed, "fn a() {} b\n\n");

    app.world_mut().despawn(doc);
    assert_eq!(app.world().resource::<EditStates>().len(), 1);
    let doc = app
        .world_mut()
        .spawn((EditDoc::new(Lang::Rust, TEXT), key))
        .id();
    assert!(app.world().resource::<EditStates>().is_empty());
    app.world_mut().resource_mut::<Editing>().0 = doc;
    app.world_mut().resource_mut::<Generation>().0 += 1;
    settle(&mut app);
    assert_eq!(text(&app, doc), typed, "the key's state wins");

    let view = editor(&mut app);
    request(&mut app, doc, view, Request::command(Command::Undo));
    request(&mut app, doc, view, Request::command(Command::Undo));
    assert_eq!(text(&app, doc), TEXT);
}

#[test]
fn edit_states_keep_the_newest_hundred() {
    let mut app = laid_out();
    for n in 0..=EditStates::MAX {
        let doc = app
            .world_mut()
            .spawn((
                EditDoc::new(Lang::Rust, &format!("{n}\n")),
                EditKey(n.to_string()),
            ))
            .id();
        app.world_mut().despawn(doc);
    }
    let states = app.world().resource::<EditStates>();
    assert_eq!(states.len(), EditStates::MAX);
    assert!(states.get(&EditKey("0".into())).is_none());
    let newest = states.get(&EditKey(EditStates::MAX.to_string()));
    assert_eq!(
        newest.map(|state| state.buffer().to_string()),
        Some(format!("{}\n", EditStates::MAX))
    );
}

#[derive(Resource, Default)]
struct Board(String);

/// Keeps copies in a resource and tags each paste with where it came
/// from, as the gallery's `collab` page does.
struct Tagged;

impl ClipboardProvider for Tagged {
    fn write(
        &mut self,
        world: &mut World,
        text: &str,
    ) -> Result<(), ClipboardError> {
        world.resource_mut::<Board>().0 = text.to_owned();
        Ok(())
    }

    fn read(&mut self, world: &mut World) -> ClipboardRead {
        let copied = world.resource::<Board>().0.clone();
        ClipboardRead::Ready(Ok(format!("[{copied}]")))
    }
}

#[test]
fn an_app_clipboard_serves_copy_and_paste() {
    let mut app = laid_out();
    app.init_resource::<Board>()
        .insert_resource(Keymap::linux())
        .insert_resource(EditClipboard::new(Tagged));
    let (view, doc) =
        mount_editor(&mut app, Lang::Rust, "abc def\n", 600.0, 400.0);
    settle(&mut app);
    let word = Selections::new(Selection { anchor: 4, head: 7 });
    request(&mut app, doc, view, Request::select(word));
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
    chord(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyC, "c");
    assert_eq!(app.world().resource::<Board>().0, "def");
    caret_at(&mut app, doc, view, 0);
    chord(&mut app, &[KeyCode::ControlLeft], KeyCode::KeyV, "v");
    settle(&mut app);
    assert_eq!(text(&app, doc), "[def]abc def\n");
}

#[test]
fn markers_follow_edits_and_underline_the_rows_near_the_viewport() {
    let mut app = laid_out();
    let (view, doc) = mount_editor(
        &mut app,
        Lang::Rust,
        "let a = 1;\nlet b = 2;\nlet c = 3;\n",
        600.0,
        400.0,
    );
    app.world_mut().entity_mut(doc).insert(Markers(Ranges::new([
        (4..5, Severity::Error),
        (15..16, Severity::Warning),
        (26..27, Severity::Hint),
    ])));
    settle(&mut app);
    caret_at(&mut app, doc, view, 0);
    type_text(&mut app, "ab");
    let second = Selections::new(Selection {
        anchor: 17,
        head: 18,
    });
    request(&mut app, doc, view, Request::select(second));
    key_with(&mut app, &[], KeyCode::Backspace, Key::Backspace);
    settle(&mut app);
    let markers = app.world().get::<Markers>(doc).expect("markers");
    let left: Vec<_> = markers
        .0
        .iter()
        .map(|(range, severity)| (range, *severity))
        .collect();
    assert_eq!(
        left,
        [(6..7, Severity::Error), (27..28, Severity::Hint)]
    );
    let marks = app.world().get::<Marks>(view).expect("marks");
    let underlines: Vec<_> = marks
        .layer(MarkLayer("markers"))
        .iter()
        .map(|mark| match &mark.kind {
            MarkKind::Underline { bytes, color } => {
                (mark.line, bytes.clone(), *color)
            }
            other => panic!("an underline, not {other:?}"),
        })
        .collect();
    assert_eq!(
        underlines,
        [
            (0, 6..7, Severity::Error.color()),
            (2, 4..5, Severity::Hint.color()),
        ]
    );
}

#[derive(Clone, Debug, PartialEq)]
struct Badge;

#[test]
fn a_line_annotation_follows_an_enter_and_its_undo() {
    let mut app = laid_out();
    app.add_plugins(FollowPlugin::<LineAnnotations<Badge>>::default());
    let text = "zero\none\ntwo\nthree\n";
    let (view, doc) =
        mount_editor(&mut app, Lang::Rust, text, 600.0, 400.0);
    let anchors = LineAnchors::new(
        app.world()
            .get::<EditDoc>(doc)
            .expect("a doc")
            .editor()
            .buffer(),
        [(2, Badge)],
    );
    app.world_mut()
        .entity_mut(doc)
        .insert(LineAnnotations(anchors));
    settle(&mut app);
    let lines = |app: &App| -> Vec<u32> {
        let buffer = app.world().get::<EditDoc>(doc).expect("a doc");
        app.world()
            .get::<LineAnnotations<Badge>>(doc)
            .expect("annotations")
            .0
            .lines(buffer.editor().buffer())
            .map(|(line, _)| line)
            .collect()
    };
    caret_at(&mut app, doc, view, 2);
    key_with(&mut app, &[], KeyCode::Enter, Key::Enter);
    assert_eq!(lines(&app), [3]);
    request(&mut app, doc, view, Request::command(Command::Undo));
    assert_eq!(lines(&app), [2]);
}

#[test]
fn removing_the_last_markers_clears_their_underlines() {
    let mut app = laid_out();
    let (view, doc) = mount_editor(
        &mut app,
        Lang::Rust,
        "let a = 1;\n",
        600.0,
        400.0,
    );
    app.world_mut()
        .entity_mut(doc)
        .insert(Markers(Ranges::new([(4..5, Severity::Error)])));
    settle(&mut app);
    let underlines = |app: &App| {
        app.world()
            .get::<Marks>(view)
            .expect("marks")
            .layer(MarkLayer("markers"))
            .len()
    };
    assert_eq!(underlines(&app), 1);
    app.world_mut().entity_mut(doc).remove::<Markers>();
    settle(&mut app);
    assert_eq!(underlines(&app), 0);
}
