use std::sync::Arc;

use bevy::prelude::*;
use chess_core::net::RoomPlayer;
use http_for_bevy::{Headers, prelude::*};
use serde::{Deserialize, Serialize};

#[cfg(debug_assertions)]
const URL: &str = "http://0.0.0.0:3000";
#[cfg(not(debug_assertions))]
const URL: &str = "http://164.92.131.129";

#[derive(Serialize)]
pub struct CreateRoom(pub String);

impl RequestType for CreateRoom {
    type Extra = ();
    type Response = RoomJoined;
    const METHOD: Method = Method::POST;

    fn extra(&self) -> Self::Extra {}

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{URL}/rooms")
    }
}

#[derive(Serialize)]
pub struct JoinRoom(pub i32);

impl RequestType for JoinRoom {
    type Extra = ();
    type Response = RoomJoined;
    const METHOD: Method = Method::POST;

    fn extra(&self) -> Self::Extra {}

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{URL}/rooms/{}/join", self.0)
    }
}

#[derive(Message, Deserialize)]
pub struct RoomJoined(pub Arc<RoomPlayer>);

#[derive(Serialize)]
pub struct PlayRoom {
    pub room: i32,
    pub is_white: bool,
    pub token: String,
}

pub struct PlayRoomExtra {
    pub is_white: bool,
    pub room: i32,
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
        }
    }

    fn endpoint<'r>(&'r self) -> impl ToString {
        format!("{URL}/rooms/{}/play", self.room)
    }
}

#[derive(Message)]
pub struct RoomDeleted(pub i32);

#[derive(Message)]
pub struct PlayToken {
    pub token: String,
    pub is_white: bool,
    pub room: i32,
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
            tracing::error!("HTTP error from {}: {:?}", ev.url, ev.text());
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
                });
            }
            410 => {
                deleted.write(RoomDeleted(ev.extra().room));
            }
            _ => tracing::error!("HTTP error from {}: {:?}", ev.url, ev.text()),
        }
    }
}

fn on_errors<T: Send + Sync>(mut ev: MessageReader<http_for_bevy::Error>) {
    for err in ev.read() {
        tracing::error!("{:?}", err);
    }
}

pub fn plugin(app: &mut App) {
    app.add_plugins(HttpPlugin)
        .add_systems(
            Update,
            (
                on_errors::<Arc<RoomPlayer>>,
                on_room_created,
                on_room_joined,
                on_room_played,
            ),
        )
        .add_message::<RoomJoined>()
        .add_message::<PlayToken>()
        .add_message::<RoomDeleted>()
        .add_request_type::<CreateRoom>()
        .add_request_type::<JoinRoom>()
        .add_request_type::<PlayRoom>();
}
