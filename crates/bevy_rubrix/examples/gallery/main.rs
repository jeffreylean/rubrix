use bevy::DefaultPlugins;
use bevy::app::App;
use bevy_rubrix::RubrixPlugin;

fn main() {
    App::new().add_plugins((DefaultPlugins, RubrixPlugin)).run();
}
