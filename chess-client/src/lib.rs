use bevy::{
    input_focus::{InputDispatchPlugin, tab_navigation::TabNavigationPlugin},
    prelude::*,
    ui_widgets::UiWidgetsPlugins,
};

use crate::{
    board::GameEnded,
    net::{GameStarted, Play, PlayToken},
    rooms::RemoveRoom,
};

mod board;
/// Initial stage of the client. Checks if it can connect to the server and if it's up to date.
mod handshake;
/// In-between menu and board
mod lobby;
mod net;
mod rooms;

fn on_play_token(
    mut ev: ResMut<Messages<PlayToken>>,
    mut commands: Commands,
    mut next: ResMut<NextState<State>>,
) {
    for ev in ev.drain() {
        next.set(State::Waiting);
        commands.trigger(Play {
            token: ev.token,
            is_white: ev.is_white,
            room: ev.room,
            code: ev.code,
        });
    }
}

fn on_go_back(mut ev: MessageReader<GoBack>, mut next: ResMut<NextState<State>>) {
    if !ev.is_empty() {
        next.set(State::Menu);
        ev.clear();
    }
}

fn on_game_start(mut ev: MessageReader<GameStarted>, mut next: ResMut<NextState<State>>) {
    if !ev.is_empty() {
        next.set(State::Game);
        ev.clear();
    }
}

fn on_game_ended(mut ev: MessageReader<GameEnded>, mut commands: Commands) {
    for ev in ev.read() {
        commands.trigger(RemoveRoom(ev.0));
    }
}

fn on_success(mut ev: MessageReader<handshake::Success>, mut next: ResMut<NextState<State>>) {
    if !ev.is_empty() {
        next.set(State::Menu);
        ev.clear();
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

#[derive(Message)]
pub struct GoBack;

#[derive(Debug, Clone, Copy, States, PartialEq, Eq, Hash)]
pub enum State {
    Init,
    Menu,
    Waiting,
    Game,
}

pub fn plugin(app: &mut App) {
    app.add_plugins((
        UiWidgetsPlugins,
        InputDispatchPlugin,
        TabNavigationPlugin,
        net::plugin,
        handshake::plugin,
        rooms::plugin,
        lobby::plugin,
        board::plugin,
    ))
    .insert_state(State::Init)
    .add_message::<GoBack>()
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            on_play_token,
            on_go_back,
            on_game_ended,
            on_success,
            on_game_start,
        ),
    );
}
