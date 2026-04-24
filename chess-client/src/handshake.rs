use bevy::prelude::*;
use http_for_bevy::HttpRequest;

use crate::net::{GetVersion, Handshake};

#[derive(Component)]
struct Status;

// #[derive(Component)]
// struct ExitButton;

fn spawn_ui(mut commands: Commands) {
    commands
        .spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            row_gap: px(64),
            ..Default::default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text("Connecting...".into()),
                TextFont::from_font_size(24.0),
                Status,
            ));

            // parent.spawn((
            //     Text("Exit".into()),
            //     TextFont::from_font_size(24.0),
            //     ExitButton,
            //     Button,
            //     observe(|_: On<Activate>, mut exit: MessageWriter<AppExit>| {
            //         exit.write(AppExit::Success);
            //     }),
            // ));
        });

    commands.trigger(HttpRequest(GetVersion));
}

#[derive(Message)]
pub struct Success;

fn on_handshake(
    mut ev: MessageReader<Handshake>,
    mut out: MessageWriter<Success>,
    mut status: Single<&mut Text, With<Status>>,
) {
    for ev in ev.read() {
        match ev {
            Handshake::Version(v) => {
                if v == chess_core::VERSION {
                    out.write(Success);
                } else {
                    status.0 = "Version mismatch. Please download the latest client.".into();
                }
            }
            Handshake::CantConnect => {
                status.0 = "Couldn't connect to the server. Check your internet.".into()
            }
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(crate::State::Init), spawn_ui)
        .add_systems(Update, on_handshake.run_if(in_state(crate::State::Init)))
        .add_message::<Success>();
}
