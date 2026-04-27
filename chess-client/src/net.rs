use std::ops::{Deref, Index};
use std::sync::Arc;

use aeronet_io::Session;
use aeronet_io::connection::Disconnect;
use aeronet_websocket::client::{ClientConfig, WebSocketClient};
use bevy::prelude::*;
use chess_core::net::{ChessEvent, ChessMessage};
use chess_core::{Board, net::RoomPlayer};
use chess_core::{Color, Move, Piece, Pos};
use http_for_bevy::{Headers, prelude::*};
use serde::{Deserialize, Serialize};

use crate::GoBack;

#[cfg(debug_assertions)]
const HTTP_URL: &str = "http://0.0.0.0:3000";
#[cfg(not(debug_assertions))]
const HTTP_URL: &str = "http://164.92.131.129";

#[cfg(debug_assertions)]
const WS_URL: &str = "ws://0.0.0.0:3000";
#[cfg(not(debug_assertions))]
const WS_URL: &str = "ws://164.92.131.129";

#[derive(Serialize)]
pub struct CreateRoom;

impl RequestType for CreateRoom {
    type Extra = ();
    type Response = RoomJoined;
    const METHOD: Method = Method::POST;

    fn extra(&self) -> Self::Extra {}

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{HTTP_URL}/rooms")
    }
}

#[derive(Serialize)]
pub struct JoinRoom(pub String);

impl RequestType for JoinRoom {
    type Extra = ();
    type Response = RoomJoined;
    const METHOD: Method = Method::POST;

    fn extra(&self) -> Self::Extra {}

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{HTTP_URL}/rooms/{}", self.0)
    }
}

#[derive(Message, Deserialize)]
pub struct RoomJoined(pub Arc<RoomPlayer>);

#[derive(Serialize)]
pub struct PlayRoom {
    pub room: i32,
    pub is_white: bool,
    pub token: String,
    pub code: Option<String>,
}

pub struct PlayRoomExtra {
    pub is_white: bool,
    pub room: i32,
    pub code: Option<String>,
}

impl RequestType for PlayRoom {
    type Extra = PlayRoomExtra;
    type Response = String;
    const METHOD: Method = Method::GET;

    fn headers(&self, headers: &mut Headers) {
        headers.insert("Authorization", format!("Bearer {}", self.token));
    }

    fn extra(&self) -> Self::Extra {
        PlayRoomExtra {
            is_white: self.is_white,
            room: self.room,
            code: self.code.clone(),
        }
    }

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{HTTP_URL}/rooms/{}/play", self.room)
    }
}

#[derive(Serialize)]
pub struct Match;

impl RequestType for Match {
    type Extra = ();
    type Response = RoomJoined;
    const METHOD: Method = Method::POST;

    fn extra(&self) -> Self::Extra {
        ()
    }

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{HTTP_URL}/rooms/match")
    }
}

fn on_room_matched(mut ev: MessageReader<HttpResponse<Match>>, mut out: MessageWriter<RoomJoined>) {
    for ev in ev.read() {
        if ev.status / 100 != 2 {
            tracing::error!("HTTP error from {}: {:?}", ev.url, ev.text());
            continue;
        }

        out.write(ev.json().unwrap());
    }
}

#[derive(Message)]
pub struct RoomDeleted(pub i32);

#[derive(Message)]
pub struct PlayToken {
    pub token: String,
    pub is_white: bool,
    pub room: i32,
    pub code: Option<String>,
}

fn on_room_created(
    mut ev: MessageReader<HttpResponse<CreateRoom>>,
    mut out: MessageWriter<RoomJoined>,
) {
    for ev in ev.read() {
        if ev.status != 200 {
            tracing::error!("HTTP error from {}: {:?}", ev.url, ev.text());
            continue;
        }

        out.write(ev.json().unwrap());
    }
}

fn on_room_joined(
    mut ev: MessageReader<HttpResponse<JoinRoom>>,
    mut out: MessageWriter<RoomJoined>,
) {
    for ev in ev.read() {
        if ev.status != 200 {
            tracing::error!("HTTP error {} from {}: {:?}", ev.status, ev.url, ev.text());
            continue;
        }

        out.write(ev.json().unwrap());
    }
}

fn on_room_played(
    mut ev: MessageReader<HttpResponse<PlayRoom>>,
    mut out: MessageWriter<PlayToken>,
    mut deleted: MessageWriter<RoomDeleted>,
) {
    for ev in ev.read() {
        // 410 means the room no longer exists
        match ev.status {
            200 => {
                out.write(PlayToken {
                    token: ev.text().unwrap().to_string(),
                    is_white: ev.extra().is_white,
                    room: ev.extra().room,
                    code: ev.extra().code.clone(),
                });
            }
            410 => {
                deleted.write(RoomDeleted(ev.extra().room));
            }
            status => tracing::error!("HTTP error {status} from {}: {:?}", ev.url, ev.text()),
        }
    }
}

fn on_errors(mut ev: MessageReader<http_for_bevy::Error>) {
    for err in ev.read() {
        tracing::error!("{:?}", err);
    }
}

#[derive(Component, Clone, Copy)]
pub struct BoardPosition(pub Pos);

impl Deref for BoardPosition {
    type Target = Pos;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Resource)]
pub struct ServerBoard {
    board: Board,
}

impl Deref for ServerBoard {
    type Target = Board;

    fn deref(&self) -> &Self::Target {
        &self.board
    }
}

impl Index<BoardPosition> for ServerBoard {
    type Output = Option<Piece>;

    fn index(&self, index: BoardPosition) -> &Self::Output {
        &self.board[index.0]
    }
}

#[derive(Message)]
pub struct GameStarted;

fn process_event(ev: &ChessEvent, board: &mut Board, out: &mut MessageWriter<GameStarted>) {
    match ev {
        ChessEvent::Move(mv) => {
            mv.exec(board);
        }
        ChessEvent::Start => {
            out.write(GameStarted);
        }
        _ => (),
    }
}

fn process_msgs(
    mut session: Single<&mut Session>,
    mut board: ResMut<ServerBoard>,
    mut out: MessageWriter<GameStarted>,
) {
    for msg in session.recv.drain(..) {
        println!("Received {}", String::from_utf8_lossy(msg.payload.as_ref()));
        if msg.payload.is_empty() {
            continue;
        }
        let msg: ChessMessage = serde_json::from_slice(msg.payload.as_ref()).unwrap();
        tracing::info!("Received {msg:?}");

        match msg {
            ChessMessage::Sync(events) => {
                for ev in events.as_ref() {
                    process_event(ev, &mut board.board, &mut out);
                }
            }
            ChessMessage::Event(ev) => {
                process_event(&ev, &mut board.board, &mut out);
            }
            ChessMessage::MoveError => panic!("something must have gone terribly wrong"),
        }
    }
}

#[derive(Event)]
pub struct Play {
    pub token: String,
    pub is_white: bool,
    pub room: i32,
    pub code: Option<String>,
}

fn on_play(
    play: On<Play>,
    mut board: ResMut<ServerBoard>,
    mut room: ResMut<RoomInfo>,
    mut commands: Commands,
) {
    board.board = Board::new();
    *room = RoomInfo {
        room: play.room,
        color: if play.is_white {
            Color::White
        } else {
            Color::Black
        },
        code: play.code.clone(),
    };

    // Connect to client
    commands.spawn_empty().queue(WebSocketClient::connect(
        ClientConfig::default(),
        format!("{WS_URL}/connect?token={}", play.token),
    ));
}

#[derive(Serialize)]
pub struct GetVersion;

#[derive(Message, Deserialize)]
pub enum Handshake {
    Version(String),
    CantConnect,
}

impl RequestType for GetVersion {
    type Extra = ();
    type Response = String;
    const METHOD: Method = Method::GET;

    fn extra(&self) -> Self::Extra {
        ()
    }

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{HTTP_URL}/version")
    }
}

fn on_version_returned(
    mut ev: MessageReader<HttpResponse<GetVersion>>,
    mut out: MessageWriter<Handshake>,
) {
    for ev in ev.read() {
        if ev.status != 200 {
            tracing::error!("HTTP error from {}: {:?}", ev.url, ev.text());
            continue;
        }

        out.write(Handshake::Version(ev.json().unwrap()));
    }
}

fn on_handshake_errors(ev: MessageReader<http_for_bevy::Error>, mut out: MessageWriter<Handshake>) {
    if !ev.is_empty() {
        out.write(Handshake::CantConnect);
    }
}

fn disconnect_on_go_back(
    ev: MessageReader<GoBack>,
    session: Single<Entity, With<Session>>,
    mut commands: Commands,
) {
    if !ev.is_empty() {
        println!("????????????");
        commands.trigger(Disconnect::new(session.entity(), "client disconnected"));
    }
}

#[derive(Resource, Default)]
pub struct RoomInfo {
    pub room: i32,
    pub color: Color,
    pub code: Option<String>,
}

pub fn plugin(app: &mut App) {
    app.add_plugins(HttpPlugin)
        .add_systems(
            Update,
            (
                on_errors,
                on_handshake_errors,
                on_room_created,
                on_room_joined,
                on_room_played,
                on_room_matched,
                on_version_returned,
                process_msgs,
                disconnect_on_go_back,
            ),
        )
        .insert_resource(ServerBoard {
            board: Board::new(),
        })
        .init_resource::<RoomInfo>()
        .add_message::<RoomJoined>()
        .add_message::<PlayToken>()
        .add_message::<RoomDeleted>()
        .add_message::<Handshake>()
        .add_message::<GameStarted>()
        .add_request_type::<CreateRoom>()
        .add_request_type::<JoinRoom>()
        .add_request_type::<PlayRoom>()
        .add_request_type::<Match>()
        .add_request_type::<GetVersion>()
        .add_observer(on_play);
}
