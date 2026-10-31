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
use support::{laid_out, mount_editor, type_text};

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
