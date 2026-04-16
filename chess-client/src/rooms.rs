use std::{
    fs::{self, File},
    ops::{Deref, DerefMut},
    sync::Arc,
};

use bevy::{
    color::palettes::css,
    input_focus::{
        AcquireFocus, InputFocus,
        tab_navigation::{TabGroup, TabIndex},
    },
    prelude::*,
    ui_widgets::{Activate, Button, observe},
};

use bevy_simple_text_input::{
    TextInput, TextInputInactive, TextInputPlugin, TextInputTextColor, TextInputValue,
};
use chess_core::net::RoomPlayer;
use http_for_bevy::HttpRequest;
use net::{ReceivedRooms, ReloadRooms};
use serde::{Deserialize, Serialize};

use crate::rooms::net::{CreateRoom, JoinRoom, PlayRoom, RoomDeleted, RoomJoined};

mod net;

pub use net::PlayToken;

#[derive(EntityEvent)]
struct FocusLost(Entity);

#[derive(Resource)]
struct OldFocus(Option<Entity>);

fn track_focus(focus: Res<InputFocus>, mut old_focus: ResMut<OldFocus>, mut commands: Commands) {
    if focus.is_changed() {
        if let Some(old) = old_focus.0
            && let Ok(mut old) = commands.get_entity(old)
        {
            old.trigger(FocusLost);
        }

        old_focus.0 = focus.get();
    }
}

#[derive(Component)]
struct MainMenu;

#[derive(Component)]
struct MyRoomsList;

#[derive(Resource, Default, Serialize, Deserialize)]
pub struct MyRooms(pub Vec<Arc<RoomPlayer>>);

impl Deref for MyRooms {
    type Target = Vec<Arc<RoomPlayer>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MyRooms {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn render_my_rooms(
    container: Single<Entity, With<MyRoomsList>>,
    mut commands: Commands,
    rooms: Res<MyRooms>,
) {
    if rooms.is_changed() {
        let mut parent = commands.get_entity(container.entity()).unwrap();
        parent.despawn_children();

        if rooms.is_empty() {
            parent.with_child((
                Text::new("Nothing here"),
                TextColor::from(css::LIGHT_GRAY),
                Node {
                    margin: UiRect::top(px(64.0)),
                    ..Default::default()
                },
            ));
        } else {
            parent.with_children(|parent| {
                for room in rooms.iter() {
                    let room = room.clone();
                    parent
                        .spawn((
                            Node {
                                width: percent(100),
                                padding: UiRect::all(px(4.0)),
                                align_items: AlignItems::Center,
                                ..Default::default()
                            },
                            BackgroundColor(Color::BLACK),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Node {
                                    flex_grow: 1.0,
                                    ..Default::default()
                                },
                                Text::new(room.name.to_string()),
                                TextFont {
                                    font_size: 16.0,
                                    ..Default::default()
                                },
                            ));
                            parent.spawn((
                                Text::new("Play"),
                                BackgroundColor(Color::from(css::DARK_SLATE_GRAY)),
                                Button,
                                observe(move |_: On<Activate>, mut commands: Commands| {
                                    commands.trigger(HttpRequest(PlayRoom {
                                        room: room.id,
                                        is_white: room.is_white,
                                        token: room.token.clone(),
                                    }))
                                }),
                            ));
                        });
                }
            });
        }
    }
}

fn on_room_joined(
    mut ev: ResMut<Messages<RoomJoined>>,
    mut rooms: ResMut<MyRooms>,
    mut commands: Commands,
) {
    let is_empty = ev.is_empty();
    for ev in ev.drain() {
        rooms.push(ev.0);
    }

    if !is_empty {
        commands.trigger(HttpRequest(ReloadRooms));
    }
}

#[derive(Component)]
struct AvailableRooms;

fn on_received_rooms(
    mut ev: ResMut<Messages<ReceivedRooms>>,
    container: Single<Entity, With<AvailableRooms>>,
    mut commands: Commands,
    my_rooms: Res<MyRooms>,
) {
    if let Some(ReceivedRooms(rooms)) = ev.drain().last() {
        let rooms: Vec<_> = rooms
            .into_iter()
            .filter(|v| my_rooms.iter().all(|my| my.id != v.id))
            .collect();
        let mut parent = commands.get_entity(container.entity()).unwrap();
        parent.despawn_children();

        if rooms.is_empty() {
            parent.with_child((
                Text::new("Nothing here"),
                TextColor::from(css::LIGHT_GRAY),
                Node {
                    margin: UiRect::top(px(64.0)),
                    ..Default::default()
                },
            ));
        } else {
            parent.with_children(|parent| {
                for room in rooms {
                    let room_id = room.id;
                    parent
                        .spawn((
                            Node {
                                width: percent(100),
                                padding: UiRect::all(px(4.0)),
                                align_items: AlignItems::Center,
                                ..Default::default()
                            },
                            BackgroundColor(Color::BLACK),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Node {
                                    flex_grow: 1.0,
                                    ..Default::default()
                                },
                                Text::new(room.name.as_ref().map_or("", String::as_str)),
                                TextFont {
                                    font_size: 16.0,
                                    ..Default::default()
                                },
                            ));
                            parent.spawn((
                                Text::new("Join"),
                                BackgroundColor(Color::from(css::DARK_SLATE_GRAY)),
                                Button,
                                observe(move |_: On<Activate>, mut commands: Commands| {
                                    commands.trigger(HttpRequest(JoinRoom(room_id)));
                                }),
                            ));
                        });
                }
            });
        }
    }
}

fn text_input() -> impl Bundle {
    (
        TextInput,
        Node {
            width: px(256.0),
            ..Default::default()
        },
        TextInputInactive(true),
        BackgroundColor(Color::WHITE),
        TextInputTextColor(TextColor(Color::BLACK)),
        TabIndex(0),
        observe(
            |ev: On<AcquireFocus>, mut input: Query<&mut TextInputInactive>| {
                input.get_mut(ev.focused_entity).unwrap().0 = false;
            },
        ),
        observe(
            |ev: On<FocusLost>, mut input: Query<&mut TextInputInactive, With<TextInput>>| {
                input.get_mut(ev.0).unwrap().0 = true;
            },
        ),
    )
}

fn create_button() -> impl Bundle {
    (
        Text::new("Create"),
        BackgroundColor(Color::from(css::DARK_SLATE_GRAY)),
        Button,
        observe(
            |_: On<Activate>, name: Single<&TextInputValue>, mut commands: Commands| {
                commands.trigger(HttpRequest(CreateRoom(name.0.clone())))
            },
        ),
    )
}

fn spawn_ui(mut commands: Commands, mut rooms: ResMut<MyRooms>) {
    rooms.set_changed();
    commands
        .spawn((
            Node {
                display: Display::Grid,
                grid_template_rows: vec![GridTrack::min_content()],
                grid_template_columns: vec![GridTrack::flex(1.0), GridTrack::flex(1.0)],
                width: percent(100),
                height: percent(100),
                column_gap: px(4.0),
                ..Default::default()
            },
            MainMenu,
        ))
        .with_children(|parent| {
            // Header
            parent
                .spawn(Node {
                    width: percent(100),
                    ..Default::default()
                })
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("Available Rooms"),
                        Node {
                            flex_grow: 1.0,
                            ..Default::default()
                        },
                    ));
                    parent.spawn((
                        Text::new("Reload"),
                        BackgroundColor(Color::from(css::DARK_SLATE_GRAY)),
                        Button,
                        observe(|_: On<Activate>, mut commands: Commands| {
                            commands.trigger(HttpRequest(ReloadRooms))
                        }),
                    ));
                });
            parent
                .spawn((
                    Node {
                        width: percent(100),
                        ..Default::default()
                    },
                    TabGroup::default(),
                ))
                .with_children(|parent| {
                    parent.spawn((
                        Text::new("My Rooms"),
                        Node {
                            flex_grow: 1.0,
                            ..Default::default()
                        },
                    ));
                    parent.spawn(text_input());
                    parent.spawn(create_button());
                });

            // Main content
            parent.spawn((
                Node {
                    width: percent(100.0),
                    padding: UiRect::all(px(4.0)),
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                AvailableRooms,
            ));
            parent.spawn((
                Node {
                    width: percent(100.0),
                    padding: UiRect::all(px(4.0)),
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                MyRoomsList,
            ));
        });

    commands.trigger(HttpRequest(ReloadRooms));
}

fn despawn_ui(menu: Single<Entity, With<MainMenu>>, mut commands: Commands) {
    commands.get_entity(menu.entity()).unwrap().despawn();
}

fn save_rooms(rooms: Res<MyRooms>) {
    if rooms.is_changed() {
        let path = dirs::data_dir().unwrap();
        let path = path.join(if std::env::var("TEST").is_ok() {
            "anychess-test"
        } else {
            "anychess"
        });

        fs::create_dir_all(&path).unwrap();
        serde_json::to_writer(File::create(path.join("rooms.json")).unwrap(), &*rooms).unwrap();
    }
}

fn load_rooms(mut rooms: ResMut<MyRooms>, mut commands: Commands) {
    let path = dirs::data_dir().unwrap();
    let path = path.join(if std::env::var("TEST").is_ok() {
        "anychess-test"
    } else {
        "anychess"
    });

    if let Ok(file) = File::open(path.join("rooms.json")) {
        *rooms = serde_json::from_reader(file).unwrap();
    }

    commands.trigger(HttpRequest(ReloadRooms));
}

#[derive(Event)]
pub struct RemoveRoom(pub i32);

fn on_remove_room(room: On<RemoveRoom>, mut rooms: ResMut<MyRooms>) {
    let index = rooms.iter().position(|v| v.id == room.0);

    if let Some(index) = index {
        rooms.remove(index);
    }
}

fn on_room_deleted(mut ev: MessageReader<RoomDeleted>, mut commands: Commands) {
    for ev in ev.read() {
        commands.trigger(RemoveRoom(ev.0));
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins((net::plugin, TextInputPlugin))
        .add_systems(Startup, load_rooms)
        .add_systems(OnEnter(super::State::Menu), spawn_ui)
        .add_systems(OnExit(super::State::Menu), despawn_ui)
        .add_systems(
            Update,
            (
                on_received_rooms,
                track_focus,
                on_room_joined,
                render_my_rooms,
                save_rooms,
                on_room_deleted,
            )
                .run_if(in_state(super::State::Menu)),
        )
        .add_observer(on_remove_room)
        .init_resource::<MyRooms>()
        .insert_resource(OldFocus(None));
}
