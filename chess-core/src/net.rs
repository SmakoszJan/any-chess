use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::ChessMove;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Room {
    pub id: i32,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RoomPlayer {
    pub id: i32,
    pub is_white: bool,
    pub name: String,
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateRoom(pub Option<String>);

#[derive(Serialize, Deserialize)]
pub enum ChessMessage<'r> {
    Sync(Cow<'r, [ChessEvent]>),
    Event(ChessEvent),
    MoveError,
}

#[derive(Serialize, Deserialize)]
pub enum ClientMessage {
    Move(ChessMove),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum ChessEvent {
    Move(ChessMove),
    GameEnded,
}
