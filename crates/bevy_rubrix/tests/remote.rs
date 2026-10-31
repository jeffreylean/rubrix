mod support;

use bevy::app::App;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::ui::widget::Text;
use bevy::ui::{Display, Node};
use bevy_rubrix::{
    Edge, EditDoc, EditRequest, Lang, Placed, RemoteCaret,
    RemoteCarets, RemoteView, Request, Selection, Selections,
};
use support::{Editing, laid_out, mount_editor, type_text};

fn settle(app: &mut App) {
    for _ in 0..4 {
        app.update();
    }
}

fn numbered(count: usize) -> String {
    (1..=count).map(|n| format!("line {n}\n")).collect()
}

fn ada(at: usize) -> RemoteCaret {
    RemoteCaret {
        anchor: at,
        head: at,
        color: Color::srgb(0.9, 0.3, 0.4),
        label: "Ada".into(),
    }
}

fn placed(app: &App, view: Entity) -> Vec<Placed> {
    app.world()
        .get::<RemoteView>(view)
        .expect("a remote view")
        .placed()
        .to_vec()
}

/// The texts on screen that read `text`.
fn shown(app: &mut App, text: &str) -> usize {
    app.world_mut()
        .query::<(&Text, &Node)>()
        .iter(app.world())
        .filter(|(shown, node)| {
            shown.0 == text && node.display != Display::None
        })
        .count()
}

#[test]
fn a_remote_caret_below_the_window_draws_no_bar_and_one_edge_marker()
{
    let mut app = laid_out();
    let text = numbered(2_000);
    let (view, doc) =
        mount_editor(&mut app, Lang::Rust, &text, 600.0, 400.0);
    let far = app
        .world()
        .get::<EditDoc>(doc)
        .expect("a doc")
        .editor()
        .buffer()
        .line_start(1_500);
    app.world_mut()
        .entity_mut(doc)
        .insert(RemoteCarets(vec![ada(far)]));
    settle(&mut app);
    assert_eq!(placed(&app, view), [Placed::Edge(Edge::Bottom)]);
    assert_eq!(shown(&mut app, "Ada"), 0, "no label, so no bar");
    assert_eq!(shown(&mut app, "↓ Ada"), 1);
}

#[test]
fn text_typed_at_a_remote_caret_pushes_it_right() {
    let mut app = laid_out();
    let (view, doc) = mount_editor(
        &mut app,
        Lang::Rust,
        "fn main() {}\n",
        600.0,
        400.0,
    );
    app.world_mut()
        .entity_mut(doc)
        .insert(RemoteCarets(vec![ada(3)]));
    settle(&mut app);
    let [Placed::Bar(before)] = placed(&app, view)[..] else {
        panic!("a bar");
    };
    assert_eq!(shown(&mut app, "Ada"), 1);
    app.world_mut().write_message(EditRequest {
        doc,
        view: Some(view),
        request: Request::select(Selections::new(Selection::caret(
            3,
        ))),
    });
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
    app.update();
    type_text(&mut app, "ab");
    settle(&mut app);
    let carets =
        app.world().get::<RemoteCarets>(doc).expect("carets");
    assert_eq!((carets.0[0].anchor, carets.0[0].head), (5, 5));
    let [Placed::Bar(after)] = placed(&app, view)[..] else {
        panic!("a bar");
    };
    let column = (after.min.x - before.min.x) / 2.0;
    assert!(
        column > 4.0,
        "the bar moved two columns right: {column}"
    );
    assert_eq!(after.min.y, before.min.y);
}

#[test]
fn removing_the_last_remote_carets_hides_their_bars() {
    let mut app = laid_out();
    let (_, doc) = mount_editor(
        &mut app,
        Lang::Rust,
        "fn main() {}\n",
        600.0,
        400.0,
    );
    app.world_mut()
        .entity_mut(doc)
        .insert(RemoteCarets(vec![ada(3)]));
    settle(&mut app);
    assert_eq!(shown(&mut app, "Ada"), 1);
    app.world_mut().entity_mut(doc).remove::<RemoteCarets>();
    settle(&mut app);
    assert_eq!(shown(&mut app, "Ada"), 0);
}

fn far_caret(app: &App, doc: Entity) -> RemoteCaret {
    let far = app
        .world()
        .get::<EditDoc>(doc)
        .expect("a doc")
        .editor()
        .buffer()
        .line_start(1_500);
    ada(far)
}

#[test]
fn a_view_shown_another_doc_drops_the_old_docs_edge_markers() {
    let mut app = laid_out();
    let text = numbered(2_000);
    let (_, doc) =
        mount_editor(&mut app, Lang::Rust, &text, 600.0, 400.0);
    let caret = far_caret(&app, doc);
    app.world_mut()
        .entity_mut(doc)
        .insert(RemoteCarets(vec![caret]));
    settle(&mut app);
    assert_eq!(shown(&mut app, "↓ Ada"), 1);
    let other =
        app.world_mut().spawn(EditDoc::new(Lang::Rust, &text)).id();
    app.world_mut().resource_mut::<Editing>().0 = other;
    settle(&mut app);
    assert_eq!(
        shown(&mut app, "↓ Ada"),
        0,
        "the new doc has no carets"
    );
}

#[test]
fn a_respawned_doc_with_its_carets_shows_one_edge_marker() {
    let mut app = laid_out();
    let text = numbered(2_000);
    let (_, doc) =
        mount_editor(&mut app, Lang::Rust, &text, 600.0, 400.0);
    let caret = far_caret(&app, doc);
    app.world_mut()
        .entity_mut(doc)
        .insert(RemoteCarets(vec![caret.clone()]));
    settle(&mut app);
    let respawned = app
        .world_mut()
        .spawn((
            EditDoc::new(Lang::Rust, &text),
            RemoteCarets(vec![caret]),
        ))
        .id();
    app.world_mut().despawn(doc);
    app.world_mut().resource_mut::<Editing>().0 = respawned;
    settle(&mut app);
    assert_eq!(shown(&mut app, "↓ Ada"), 1);
}
