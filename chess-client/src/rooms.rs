use std::{
    fs::{self, File},
    ops::{Deref, DerefMut},
    sync::Arc,
};

use bevy::{
    color::palettes::css::{self, DARK_SLATE_GRAY, GRAY},
    ecs::system::IntoObserverSystem,
    input_focus::{AcquireFocus, InputFocus, tab_navigation::TabIndex},
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::{Activate, Button, observe},
};

use bevy_simple_text_input::{
    TextInput, TextInputInactive, TextInputPlaceholder, TextInputPlugin, TextInputTextColor,
    TextInputValue,
};
use chess_core::net::RoomPlayer;
use http_for_bevy::HttpRequest;
use serde::{Deserialize, Serialize};

use crate::net::{CreateRoom, Match, PlayRoom, RoomDeleted, RoomJoined};

mod net;

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

#[derive(Component)]
struct EnterHint;

#[derive(Component)]
struct RoomEntry(Color);

fn room_hover(
    rooms: Query<(&Children, &Hovered, &mut BackgroundColor, &RoomEntry), Changed<Hovered>>,
    mut vis: Query<&mut Visibility, With<EnterHint>>,
) {
    for mut room in rooms {
        let mut hint = vis.get_mut(room.0.last().copied().unwrap()).unwrap();
        *hint = if room.1.get() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        room.2.0 = if room.1.get() {
            Color::from(DARK_SLATE_GRAY)
        } else {
            room.3.0
        };
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
                for (i, room) in rooms.iter().enumerate() {
                    let room = room.clone();
                    let room2 = room.clone();
                    let color = if i % 2 == 1 {
                        Color::srgb_u8(0x22, 0x22, 0x22)
                    } else {
                        Color::NONE
                    };
                    parent
                        .spawn((
                            Node {
                                width: percent(100),
                                padding: UiRect::axes(px(12.0), px(8)),
                                align_items: AlignItems::Center,
                                ..Default::default()
                            },
                            BackgroundColor(color),
                            Button,
                            Hovered::default(),
                            RoomEntry(color),
                            observe(move |_: On<Activate>, mut commands: Commands| {
                                commands.trigger(HttpRequest(PlayRoom {
                                    room: room2.id,
                                    is_white: room2.is_white,
                                    token: room2.token.clone(),
                                }));
                            }),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                Node {
                                    flex_grow: 1.0,
                                    ..Default::default()
                                },
                                Text::new(format!(
                                    "Game #{} : {}",
                                    room.id,
                                    room.code.as_ref().map(String::as_str).unwrap_or("public")
                                )),
                                TextFont {
                                    font_size: 20.0,
                                    ..Default::default()
                                },
                            ));

                            parent.spawn((
                                Text::new("Click to enter"),
                                TextColor(GRAY.into()),
                                Visibility::Hidden,
                                EnterHint,
                            ));
                        });
                }
            });
        }
    }
}

fn on_room_joined(mut ev: ResMut<Messages<RoomJoined>>, mut rooms: ResMut<MyRooms>) {
    for ev in ev.drain() {
        rooms.push(ev.0);
    }
}

fn base(width: Val) -> impl Bundle {
    (
        Node {
            width,
            border_radius: BorderRadius::all(px(24)),
            padding: UiRect::all(px(12)),
            ..Default::default()
        },
        TextFont::from_font_size(24.0),
        // TextLayout::new_with_justify(Justify::Center),
    )
}

fn text_input() -> impl Bundle {
    (
        base(percent(50)),
        TextInput,
        TextInputInactive(true),
        BackgroundColor(Color::srgb_u8(0x33, 0x33, 0x33)),
        TextInputTextColor(TextColor(Color::WHITE)),
        TabIndex(0),
        TextInputPlaceholder {
            value: "Enter code... (optional)".into(),
            text_color: Some(TextColor(Color::from(GRAY))),
            ..Default::default()
        },
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

const NORMAL_COLOR: Color = Color::srgb_u8(0x33, 0x33, 0x77);
const HOVER_COLOR: Color = Color::srgb_u8(0x55, 0x55, 0xaa);

fn button<M: Send + Sync + 'static>(
    width: Val,
    text: &'static str,
    activate: impl IntoObserverSystem<Activate, (), M> + Send + Sync,
) -> impl Bundle {
    (
        base(width),
        Button,
        BackgroundColor(Color::srgb_u8(0x33, 0x33, 0x77)),
        observe(activate),
        observe(
            |ev: On<Pointer<Over>>, mut bg: Query<&mut BackgroundColor>| {
                bg.get_mut(ev.entity).unwrap().0 = HOVER_COLOR;
            },
        ),
        observe(
            |ev: On<Pointer<Out>>, mut bg: Query<&mut BackgroundColor>| {
                bg.get_mut(ev.entity).unwrap().0 = NORMAL_COLOR;
            },
        ),
        children![(
            Node {
                width: percent(100),
                ..Default::default()
            },
            Text::new(text),
            TextLayout::new_with_justify(Justify::Center)
        ),],
    )
}

fn play_action(_: On<Activate>, code: Single<&TextInputValue>, mut commands: Commands) {
    if code.0.is_empty() {
        tracing::info!("Matching randomly");
        commands.trigger(HttpRequest(Match));
    }
}

fn create_action(_: On<Activate>, mut commands: Commands) {
    commands.trigger(HttpRequest(CreateRoom));
}

fn spawn_ui(mut commands: Commands, mut rooms: ResMut<MyRooms>) {
    rooms.set_changed();
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..Default::default()
            },
            MainMenu,
            BackgroundColor(Color::srgb_u8(0x33, 0x33, 0x33)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        width: percent(80),
                        height: percent(80),
                        border_radius: BorderRadius::all(px(50)),
                        padding: UiRect::all(px(48)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        column_gap: px(12),
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb_u8(0x11, 0x11, 0x11)),
                ))
                .with_children(|parent| {
                    // My games
                    parent.spawn((
                        Node {
                            overflow: Overflow::scroll_y(),
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Column,
                            width: px(584),
                            align_items: AlignItems::Center,
                            ..Default::default()
                        },
                        MyRoomsList,
                    ));

                    // Buttons
                    parent
                        .spawn(Node {
                            width: percent(100),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(12),
                            align_items: AlignItems::Stretch,
                            ..Default::default()
                        })
                        .with_children(|buttons| {
                            // Play
                            buttons
                                .spawn(Node {
                                    column_gap: px(8),
                                    ..Default::default()
                                })
                                .with_children(|play| {
                                    play.spawn(text_input());
                                    play.spawn(button(percent(50), "Play", play_action));
                                });

                            // Create private
                            buttons.spawn(button(
                                percent(100),
                                "Create a private room",
                                create_action,
                            ));
                        });
                });
        });
}

fn despawn_ui(menu: Single<Entity, With<MainMenu>>, mut commands: Commands) {
    commands.get_entity(menu.entity()).unwrap().despawn();
}

fn save_rooms(rooms: Res<MyRooms>) {
    if rooms.is_changed() {
        let path = dirs::data_dir().unwrap();
        let path = path.join(if cfg!(debug_assertions) {
            if std::env::var("TEST").is_ok() {
                "anychess-dbg-test"
            } else {
                "anychess-dbg"
            }
        } else {
            if std::env::var("TEST").is_ok() {
                "anychess-test"
            } else {
                "anychess"
            }
        });

        fs::create_dir_all(&path).unwrap();
        serde_json::to_writer(File::create(path.join("rooms.json")).unwrap(), &*rooms).unwrap();
    }
}

fn load_rooms(mut rooms: ResMut<MyRooms>) {
    let path = dirs::data_dir().unwrap();
    let path = path.join(if cfg!(debug_assertions) {
        if std::env::var("TEST").is_ok() {
            "anychess-dbg-test"
        } else {
            "anychess-dbg"
        }
    } else {
        if std::env::var("TEST").is_ok() {
            "anychess-test"
        } else {
            "anychess"
        }
    });

    if let Ok(file) = File::open(path.join("rooms.json")) {
        *rooms = serde_json::from_reader(file).unwrap();
    }
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
    app.add_plugins(TextInputPlugin)
        .add_systems(Startup, load_rooms)
        .add_systems(OnEnter(super::State::Menu), spawn_ui)
        .add_systems(OnExit(super::State::Menu), despawn_ui)
        .add_systems(
            Update,
            (
                (track_focus, render_my_rooms, room_hover).run_if(in_state(super::State::Menu)),
                on_room_joined,
                on_room_deleted,
                save_rooms,
            ),
        )
        .add_observer(on_remove_room)
        .init_resource::<MyRooms>()
        .insert_resource(OldFocus(None));
}
