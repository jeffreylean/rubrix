//! Code, diff and file-tree UI kit for Bevy, built on fynix.

mod caret;
mod clipboard;
mod code_view;
mod commands;
mod damage;
mod diff_edit;
mod diff_view;
mod editor;
pub mod find;
mod frame;
mod hooks;
mod ime;
mod keymap;
mod marks;
mod numbers;
mod remote;
mod rows;
mod select;
mod tokens;

use bevy::app::{
    App, Plugin, PostUpdate, PreUpdate, Startup, Update,
};
use bevy::asset::{Assets, Handle, uuid_handle};
use bevy::ecs::schedule::common_conditions::any_with_component;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemCondition};
use bevy::ecs::system::ResMut;
use bevy::input::InputSystems;
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input_focus::InputFocusSystems;
use bevy::picking::PickingSystems;
use bevy::text::Font;
use bevy::ui::UiSystems;
pub use caret::{CaretBlink, Hit};
pub use clipboard::{
    ClipboardProvider, EditClipboard, SystemClipboard,
};
pub use code_view::{
    CodeDoc, CodeLine, CodeSource, CodeStream, CodeView, Overflow,
    code_view,
};
pub use commands::{
    BRACKET, BracketMatch, toggle_block_comment, toggle_line_comment,
};
pub use diff_edit::DiffEditStatus;
pub use diff_view::{
    DiffDoc, DiffRow, DiffRowAt, DiffSource, DiffView, diff_view,
};
pub use editor::{
    CodeEditor, Composing, EditDoc, EditLine, EditRequest,
    EditSystems, EditView, EditorChanged, Request, code_editor,
};
pub use frame::Tx;
pub use hooks::{
    EditKey, EditStates, FollowPlugin, Follows, LineAnnotations,
    Markers, Severity,
};
pub use ime::Preedit;
pub use keymap::{
    Action, ChordError, Gesture, Keymap, Mods, Platform, Stroke,
};
pub use marks::{Mark, MarkKind, MarkLayer, Marks};
pub use numbers::LineNumber;
pub use remote::{
    Edge, Placed, RemoteCaret, RemoteCarets, RemoteView,
};
pub use rubrix_diff::{Layout, Row, Side};
pub use rubrix_edit::{
    Brackets, Buffer, Command, EditState, Editor, History,
    LineAnchors, Motion, Ranges, Selection, Selections,
};
pub use rubrix_highlight::{Kind, Lang, Run};
pub use select::{CodeCopied, CodeSelection};
pub use tokens::{
    Face, SyntaxStyle, SyntaxTokens, default_style, resolve,
};

/// The bundled JetBrains Mono NL Regular, which [`RubrixPlugin`]
/// loads at startup. Code shows in it unless a theme's
/// [`SyntaxTokens::code_font`] names another.
pub const CODE_FONT: Handle<Font> =
    uuid_handle!("06a7136d-edc5-415b-ad38-f32f1b1f10a6");

const CODE_FONT_BYTES: &[u8] =
    include_bytes!("../assets/fonts/JetBrainsMonoNL-Regular.ttf");

/// Adds rubrix to an app.
pub struct RubrixPlugin;

impl Plugin for RubrixPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Keymap>()
            .init_resource::<keymap::Keys>()
            .init_resource::<EditClipboard>()
            .init_resource::<CaretBlink>()
            .add_message::<KeyboardInput>()
            .add_message::<KeyboardFocusLost>()
            .add_message::<EditRequest>()
            .add_message::<EditorChanged>()
            .init_resource::<EditStates>()
            .add_observer(hooks::save_state)
            .add_observer(hooks::restore_state)
            .add_plugins((
                FollowPlugin::<RemoteCarets>::default(),
                FollowPlugin::<Markers>::default(),
            ))
            .add_systems(
                Startup,
                (load_code_font, diff_view::load_hatch),
            )
            .configure_sets(
                PreUpdate,
                (
                    EditSystems::Keys
                        .after(InputSystems)
                        .before(InputFocusSystems::Dispatch),
                    EditSystems::Apply
                        .after(EditSystems::Keys)
                        .after(PickingSystems::Last),
                ),
            )
            .add_systems(
                PreUpdate,
                keymap::read_keys.in_set(EditSystems::Keys),
            )
            .add_systems(
                PreUpdate,
                (
                    code_view::take_sent,
                    frame::apply
                        .in_set(EditSystems::Apply)
                        .run_if(any_with_component::<EditDoc>),
                    caret::blink
                        .run_if(any_with_component::<EditView>),
                    select::follow_drags::<CodeDoc>,
                    select::follow_drags::<DiffDoc>,
                )
                    .chain()
                    .after(PickingSystems::Last),
            )
            .add_systems(
                PreUpdate,
                commands::match_brackets
                    .after(EditSystems::Apply)
                    .run_if(any_with_component::<EditDoc>),
            )
            .add_systems(
                PreUpdate,
                (
                    hooks::draw_markers
                        .after(EditSystems::Apply)
                        .run_if(any_with_component::<EditView>),
                    find::follow
                        .after(EditSystems::Apply)
                        .after(InputFocusSystems::Dispatch)
                        .run_if(any_with_component::<find::FindBar>),
                ),
            )
            .add_systems(
                Update,
                (select::keys::<CodeDoc>, select::keys::<DiffDoc>),
            )
            .add_systems(
                PostUpdate,
                numbers::place
                    .before(UiSystems::Prepare)
                    .run_if(any_with_component::<EditView>),
            )
            .add_systems(
                PostUpdate,
                (
                    select::draw::<CodeDoc>,
                    select::draw::<DiffDoc>,
                    caret::draw
                        .in_set(EditSystems::Draw)
                        .run_if(any_with_component::<EditView>),
                    remote::draw.in_set(EditSystems::Draw).run_if(
                        any_with_component::<RemoteCarets>.or_else(
                            any_with_component::<RemoteView>,
                        ),
                    ),
                )
                    .after(UiSystems::PostLayout),
            );
        ime::plugin(app);
        diff_edit::plugin(app);
    }
}

fn load_code_font(fonts: Option<ResMut<Assets<Font>>>) {
    if let Some(mut fonts) = fonts {
        fonts
            .insert(
                &CODE_FONT,
                Font::from_bytes(CODE_FONT_BYTES.to_vec()),
            )
            .expect("a uuid handle takes any font");
    }
}
