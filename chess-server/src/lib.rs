#![deny(clippy::unwrap_used)]

use std::{
    borrow::Cow,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{
        Path, Query, State, WebSocketUpgrade,
        ws::{self, WebSocket},
    },
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chess_core::{
    Board, Move,
    net::{ChessEvent, ChessMessage, ClientMessage, CreateRoom, Room, RoomPlayer},
};
use futures::lock::Mutex;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;

use crate::state_set::StateSet;

pub mod state_set;

#[derive(Debug)]
pub enum Error {
    Sqlx(sqlx::Error),
    Io(std::io::Error),
    Jwt(jsonwebtoken::errors::Error),
    NotFound,
    Unauthorized,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self:?}");

        match &self {
            Self::NotFound => return (StatusCode::NOT_FOUND, "Not found.").into_response(),
            Self::Unauthorized => {
                return (StatusCode::UNAUTHORIZED, "Unauthorized.").into_response();
            }
            _ => (),
        }

        (StatusCode::INTERNAL_SERVER_ERROR, format!("{self:?}")).into_response()
    }
}

impl From<jsonwebtoken::errors::Error> for Error {
    fn from(v: jsonwebtoken::errors::Error) -> Self {
        Self::Jwt(v)
    }
}

impl From<std::io::Error> for Error {
    fn from(v: std::io::Error) -> Self {
        Self::Io(v)
    }
}

impl From<sqlx::Error> for Error {
    fn from(v: sqlx::Error) -> Self {
        Self::Sqlx(v)
    }
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    encode_key: EncodingKey,
    decode_secret: DecodingKey,
    games: Arc<Mutex<StateSet>>,
}

impl AppState {
    fn state_loader(
        &self,
        room: i32,
    ) -> impl FnOnce() -> Pin<Box<dyn Future<Output = Table> + Send>> {
        struct Res {
            payload: serde_json::Value,
        }

        let pool = self.pool.clone();
        move || {
            Box::pin(async move {
                let events = sqlx::query_as!(
                    Res,
                    "SELECT payload FROM event WHERE room_id=$1 ORDER BY time ASC;",
                    room
                )
                .fetch_all(&pool)
                .await
                .unwrap()
                .into_iter()
                .map(|v| serde_json::from_value(v.payload).unwrap());

                let mut table = Table::new();
                table.process_all(events);

                table
            })
        }
    }
}

pub async fn serve() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("DATABASE_URL").expect("missing environment variable"))
        .await?;

    let secret = std::env::var("SECRET_JWT").expect("missing environment variable");
    let secret = secret.as_bytes();
    let app = Router::new()
        .route("/rooms", get(get_rooms).post(create_room))
        .route("/rooms/{id}/join", post(join_room))
        .route("/rooms/{id}/play", get(play))
        .route("/connect", get(connect))
        .with_state(AppState {
            pool,
            encode_key: EncodingKey::from_secret(secret),
            decode_secret: DecodingKey::from_secret(secret),
            games: Arc::default(),
        })
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_rooms(State(state): State<AppState>) -> Result<Json<Vec<Room>>, Error> {
    Ok(Json(
        sqlx::query_as!(
            Room,
            "SELECT room_id AS id, name FROM room WHERE open=true;"
        )
        .fetch_all(&state.pool)
        .await?,
    ))
}

#[derive(Serialize, Deserialize)]
struct PlayerClaims {
    room: i32,
    is_white: bool,
    exp: u64,
}

async fn create_room(
    State(state): State<AppState>,
    Json(room): Json<CreateRoom>,
) -> Result<Json<RoomPlayer>, Error> {
    let is_white = rand::random_bool(0.5);
    let id = sqlx::query_scalar!(
        "INSERT INTO room (white_taken, name, open) VALUES ($1, $2, true) RETURNING room_id;",
        is_white,
        room.0
    )
    .fetch_one(&state.pool)
    .await?;

    let token = jsonwebtoken::encode(
        &Header::default(),
        &PlayerClaims {
            room: id,
            is_white,
            exp: u64::MAX,
        },
        &state.encode_key,
    )?;
    Ok(Json(RoomPlayer {
        id,
        is_white,
        name: room.0.unwrap_or_default(),
        token,
    }))
}

struct JoinResult {
    white_taken: bool,
    name: Option<String>,
}

async fn join_room(
    Path(room): Path<i32>,
    State(state): State<AppState>,
) -> Result<Json<RoomPlayer>, Error> {
    tracing::info!("Does this work?");
    let result = sqlx::query_as!(
        JoinResult,
        "UPDATE room SET open=false WHERE room_id=$1 AND open=true RETURNING white_taken, name;",
        room
    )
    .fetch_optional(&state.pool)
    .await?;

    if let Some(result) = result {
        let token = jsonwebtoken::encode(
            &Header::default(),
            &PlayerClaims {
                room,
                is_white: !result.white_taken,
                exp: u64::MAX,
            },
            &state.encode_key,
        )?;
        Ok(Json(RoomPlayer {
            id: room,
            is_white: !result.white_taken,
            name: result.name.unwrap_or_default(),
            token,
        }))
    } else {
        Err(Error::NotFound)
    }
}

#[derive(Serialize, Deserialize)]
struct ConnectClaims {
    exp: u64,
    room: i32,
}

async fn play(
    Path(room): Path<i32>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<String, Error> {
    let (kind, token) = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .ok_or(Error::Unauthorized)?;

    if kind != "Bearer" {
        return Err(Error::Unauthorized);
    }

    let claims: PlayerClaims = jsonwebtoken::decode(
        token,
        &state.decode_secret,
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    )?
    .claims;

    if claims.room != room {
        return Err(Error::Unauthorized);
    }

    Ok(jsonwebtoken::encode(
        &Header::default(),
        &ConnectClaims {
            exp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60,
            room,
        },
        &state.encode_key,
    )?)
}

#[derive(Deserialize)]
struct ConnectQuery {
    token: String,
}

async fn connect(
    Query(token): Query<ConnectQuery>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<Response, Error> {
    let claims: ConnectClaims = jsonwebtoken::decode(
        token.token,
        &state.decode_secret,
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    )?
    .claims;

    // Everything fine by now.
    Ok(ws.on_upgrade(move |ws| handle_websocket(ws, state, claims.room)))
}

async fn handle_websocket(mut socket: WebSocket, app_state: AppState, room: i32) {
    // First, load the state. We'll send it over the websocket
    let table = app_state
        .games
        .lock()
        .await
        .insert_or_bump(room, app_state.state_loader(room))
        .await;

    // Sync the new guy
    let mut receiver = {
        let table = table.lock().await;
        let Ok(()) = socket
            .send(ws::Message::text(
                serde_json::to_string(&ChessMessage::Sync(Cow::Borrowed(&table.events))).unwrap(),
            ))
            .await
        else {
            return;
        };

        table.sender.subscribe()
    };

    // Handle moves/events
    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else {
                    break;
                };

                let Ok(msg) = msg.to_text() else {
                    continue;
                };
                let Ok(msg) = serde_json::from_str::<ClientMessage>(msg) else {
                    continue;
                };

                match msg {
                    ClientMessage::Move(mv) => {
                        let mut table = table.lock().await;
                        if mv.check(&table.board).is_err() {
                            tracing::debug!("erroneous move {mv:?}");
                            socket
                                .send(ws::Message::text(
                                    serde_json::to_string(&ChessMessage::MoveError).unwrap(),
                                ))
                                .await
                                .ok();
                            continue;
                        }

                        // This order is necessary for clients to properly connect (so that no move is skipped)
                        let res = sqlx::query!(
                            "INSERT INTO event (room_id, payload) VALUES ($1, $2);",
                            room,
                            serde_json::to_value(ChessEvent::Move(mv)).unwrap()
                        )
                            .execute(&app_state.pool)
                            .await;

                        if let Err(err) = res {
                            tracing::error!("Sqlx call failed: {err:?}, aborting");
                            return;
                        }

                        // Move is ok now, we can make it
                        table.process(ChessEvent::Move(mv), true);
                    }
                }
            }
            ev = receiver.recv() => {
                socket.send(ws::Message::text(serde_json::to_string(&ChessMessage::Event(ev.unwrap())).unwrap())).await.unwrap();
            }
        }
    }
}

pub struct Table {
    pub board: Board,
    pub events: Vec<ChessEvent>,
    pub sender: broadcast::Sender<ChessEvent>,
}

impl Table {
    pub fn new() -> Self {
        Self {
            board: Board::new(),
            events: Vec::new(),
            sender: broadcast::Sender::new(64),
        }
    }

    pub fn process(&mut self, ev: ChessEvent, send: bool) {
        match ev {
            ChessEvent::Move(mv) => mv.exec(&mut self.board),
            ChessEvent::GameEnded => (),
        }

        self.events.push(ev);

        if send {
            self.sender.send(ev).unwrap();
        }
    }

    pub fn process_all(&mut self, ev: impl IntoIterator<Item = ChessEvent>) {
        for ev in ev {
            self.process(ev, false);
        }
    }
}
