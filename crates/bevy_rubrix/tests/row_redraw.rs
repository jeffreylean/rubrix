mod support;

use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::input::keyboard::{Key, KeyCode};
use bevy::input_focus::{FocusCause, InputFocus};
use bevy_rubrix::{
    EditRequest, Lang, Request, Selection, Selections,
};
use support::{
    edit_lines, key_with, laid_out, mount_editor, text_of,
};

fn shown(app: &mut App, view: Entity) -> Vec<(u32, String)> {
    edit_lines(app, view)
        .into_iter()
        .map(|(line, row)| (line, text_of(app, row)))
        .collect()
}

#[test]
fn a_join_redraws_the_row_below_it() {
    let mut app = laid_out();
    let (view, doc) = mount_editor(
        &mut app,
        Lang::Plain,
        "aaa\nbbb\nccc",
        600.0,
        400.0,
    );
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        shown(&mut app, view),
        [(0, "aaa".into()), (1, "bbb".into()), (2, "ccc".into())]
    );
    app.world_mut().write_message(EditRequest {
        doc,
        view: Some(view),
        request: Request::select(Selections::new(Selection::caret(
            4,
        ))),
    });
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(view, FocusCause::Navigated);
    app.update();
    key_with(&mut app, &[], KeyCode::Backspace, Key::Backspace);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        shown(&mut app, view),
        [(0, "aaabbb".into()), (1, "ccc".into())],
        "the row of line 1 shows the line the join moved up"
    );
}
