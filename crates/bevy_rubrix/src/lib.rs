//! Code, diff and file-tree UI kit for Bevy, built on fynix.

mod code_view;
mod select;
mod tokens;

use bevy::app::{
    App, Plugin, PostUpdate, PreUpdate, Startup, Update,
};
use bevy::asset::{Assets, Handle, uuid_handle};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::ResMut;
use bevy::picking::PickingSystems;
use bevy::text::Font;
use bevy::ui::UiSystems;
pub use code_view::{
    CodeDoc, CodeLine, CodeSource, CodeStream, CodeView, Overflow,
    code_view,
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
        app.init_resource::<select::KeyTarget>()
            .add_systems(Startup, load_code_font)
            .add_systems(
                PreUpdate,
                (code_view::take_sent, select::follow_drags)
                    .chain()
                    .after(PickingSystems::Last),
            )
            .add_systems(Update, select::keys)
            .add_systems(
                PostUpdate,
                select::draw.after(UiSystems::PostLayout),
            );
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
