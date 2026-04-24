use bevy::{
    prelude::*,
    ui_widgets::{Activate, Button, observe},
};

use crate::{GoBack, net::RoomInfo};

#[derive(Component)]
struct Lobby;

fn spawn_ui(mut commands: Commands, room: Res<RoomInfo>) {
    commands
        .spawn((
            Lobby,
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(64),
                ..Default::default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Waiting for an opponent..."),
                TextFont::from_font_size(24.0),
            ));

            parent.spawn((
                Text::new(format!(
                    "Code: {}",
                    room.code.as_ref().map(String::as_str).unwrap_or("")
                )),
                TextFont::from_font_size(24.0),
                if room.code.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));

            parent.spawn((
                Text::new("Go back"),
                Button,
                observe(|_: On<Activate>, mut ev: MessageWriter<GoBack>| {
                    ev.write(GoBack);
                }),
            ));
        });
}

fn despawn_ui(mut commands: Commands, menu: Single<Entity, With<Lobby>>) {
    commands.get_entity(menu.entity()).unwrap().despawn();
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(crate::State::Waiting), spawn_ui)
        .add_systems(OnExit(crate::State::Waiting), despawn_ui);
}
