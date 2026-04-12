use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: if std::env::var("TEST").is_ok() {
                        "anychess-test"
                    } else {
                        "anychess"
                    }
                    .to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            chess_client::plugin,
        ))
        .run();
}
