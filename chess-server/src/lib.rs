#![deny(clippy::unwrap_used)]

use std::{
    borrow::Cow,
    hash::{DefaultHasher, Hash as _, Hasher},
    net::IpAddr,
    sync::{Arc, atomic::AtomicUsize},
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
    Board, Color, Move,
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
    Rejected,
    Gone,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self:?}");

        match &self {
            Self::NotFound => return StatusCode::NOT_FOUND.into_response(),
            Self::Gone => return StatusCode::GONE.into_response(),
            Self::Unauthorized => {
                return StatusCode::UNAUTHORIZED.into_response();
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
    secret: Arc<str>,
    encode_key: EncodingKey,
    decode_secret: DecodingKey,
    games: Arc<Mutex<StateSet>>,
    password: Arc<str>,
}

impl AppState {
    async fn load(&self, room: i32) -> Result<Table, Error> {
        struct Res {
            payload: serde_json::Value,
        }

        let pool = self.pool.clone();

        let events = sqlx::query_as!(
            Res,
            "SELECT payload FROM event WHERE room_id=$1 ORDER BY time ASC;",
            room
        )
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|v| serde_json::from_value(v.payload).expect("event deserialization failed"));

        let mut table = Table::new();
        table.process_all(events);

        Ok(table)
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

    let password = std::env::var("PASSWORD").expect("missing password");
    let secret = std::env::var("SECRET_JWT").expect("missing environment variable");
    let secret: Arc<str> = Arc::from(secret);
    let secret_bytes = secret.as_bytes();
    let app = Router::new()
        .route("/rooms", get(get_rooms).post(create_room))
        .route("/rooms/{id}/join", post(join_room))
        .route("/rooms/{id}/play", get(play))
        .route("/connect", get(connect))
        .route("/prune", post(prune))
        .route("/kaithhealth", get(health))
        .route("/kaithheathcheck", get(health))
        .with_state(AppState {
            pool,
            encode_key: EncodingKey::from_secret(secret_bytes),
            decode_secret: DecodingKey::from_secret(secret_bytes),
            secret: secret,
            games: Arc::default(),
            password: password.into(),
        })
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> StatusCode {
    StatusCode::OK
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

#[derive(Hash)]
struct Obscured<'r> {
    ip: IpAddr,
    secret: &'r str,
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(room): Json<CreateRoom>,
) -> Result<Json<RoomPlayer>, Error> {
    let ip = client_ip::rightmost_x_forwarded_for(&headers)
        .ok()
        .map(|ip| {
            let data = Obscured {
                ip,
                secret: &state.secret,
            };
            let mut s = DefaultHasher::default();
            data.hash(&mut s);
            s.finish() as i64
        });

    let is_white = rand::random_bool(0.5);
    let id = sqlx::query_scalar!(
        "INSERT INTO room (white_taken, name, open, created_by)
            SELECT $1, $2, true, $3
            WHERE (
                SELECT COUNT(*) FROM room WHERE created_by=$3
            ) < 100
        RETURNING room_id;",
        is_white,
        room.0,
        ip
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(id) = id else {
        return Err(Error::Rejected);
    };

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
    is_white: bool,
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

    // Confirm the room exists.
    let exists = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM room WHERE room_id = $1);",
        room
    )
    .fetch_one(&state.pool)
    .await?;
    if exists != Some(true) {
        return Err(Error::NotFound);
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
            is_white: claims.is_white,
        },
        &state.encode_key,
    )?)
}

#[derive(Deserialize)]
struct ConnectQuery {
    token: String,
}

const WEBSOCKET_CAP: usize = 10_000;
static WEBSOCKET_COUNT: AtomicUsize = AtomicUsize::new(0);

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

    // We acquire here, so that by the time we create the websocket, we remain safely within cap.
    if WEBSOCKET_COUNT.fetch_add(1, std::sync::atomic::Ordering::Acquire) >= WEBSOCKET_CAP {
        // If we fail, we put the websocket back down
        // I'm not actually sure if the Release is necessary here. Let's pretend it is, though.
        WEBSOCKET_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Release);
        return Err(Error::Rejected);
    }

    // Everything fine by now.
    Ok(ws
        .on_failed_upgrade(|_| {
            WEBSOCKET_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        })
        .on_upgrade(move |ws| handle_websocket(ws, state, claims.room, claims.is_white)))
}

pub struct Connection(WebSocket);

impl Connection {
    async fn send(&mut self, msg: &ChessMessage<'_>) -> Result<(), axum::Error> {
        self.0
            .send(ws::Message::text(
                serde_json::to_string(msg).expect("message serialization failed"),
            ))
            .await
    }

    async fn recv(&mut self) -> Option<ClientMessage> {
        self.0
            .recv()
            .await?
            .ok()
            .and_then(|v| v.into_text().ok())
            .and_then(|v| serde_json::from_slice(v.as_bytes()).ok())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        WEBSOCKET_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Release);
    }
}

async fn handle_websocket(socket: WebSocket, app_state: AppState, room: i32, is_white: bool) {
    let mut socket = Connection(socket);
    let color = if is_white { Color::White } else { Color::Black };
    // First, load the state. We'll send it over the websocket
    let table = app_state.games.lock().await.get(room);

    let table = if let Some(table) = table {
        table
    } else {
        let table = match app_state.load(room).await {
            Ok(table) => table,
            Err(err) => {
                tracing::error!("room {room} failed to load: {err:?}");
                return;
            }
        };
        app_state
            .games
            .lock()
            .await
            .maybe_insert(room, Arc::new(Mutex::new(table)))
    };

    tracing::info!("room {room} loaded state");

    // Sync the new guy
    let mut receiver = {
        let table = table.lock().await;
        let Ok(()) = socket
            .send(&ChessMessage::Sync(Cow::Borrowed(&table.events)))
            .await
        else {
            return;
        };

        table.sender.subscribe()
    };

    tracing::info!("room {room} sync sent");

    // Handle moves/events
    loop {
        tokio::select! {
            msg = socket.recv() => {
                let Some(msg) = msg else {
                    break;
                };

                match msg {
                    ClientMessage::Move(mv) => {
                        let mut table = table.lock().await;

                        if table.board[mv.from].is_none_or(|v| v.color != color) || mv.check(&table.board).is_err() {
                            tracing::debug!("erroneous move {mv:?}");
                            socket
                                .send(&ChessMessage::MoveError)
                                .await
                                .ok();
                            continue;
                        }

                        // This order is necessary for clients to properly connect (so that no move is skipped)
                        let res = sqlx::query!(
                            "INSERT INTO event (room_id, payload) VALUES ($1, $2);",
                            room,
                            serde_json::to_value(ChessEvent::Move(mv)).expect("move serialization failed")
                        )
                            .execute(&app_state.pool)
                            .await;

                        if let Err(err) = res {
                            tracing::error!("Sqlx call failed: {err:?}, aborting");
                            break;
                        }

                        // Move is ok now, we can make it
                        table.process(ChessEvent::Move(mv), true);
                    }
                }
            }
            ev = receiver.recv() => {
                let Ok(()) = socket.send(&ChessMessage::Event(ev.unwrap())).await else {return;};
            }
        }
    }

    // All resource management SHOULD be done via RAII and regular pruning.
    // SHOULD
}

async fn prune(headers: HeaderMap, State(state): State<AppState>) -> Result<(), Error> {
    // Auth
    let (_, auth) = headers
        .get(AUTHORIZATION)
        .ok_or(Error::Unauthorized)?
        .to_str()
        .map_err(|_| Error::Unauthorized)?
        .split_once(' ')
        .ok_or(Error::Unauthorized)?;

    if auth != state.password.as_ref() {
        return Err(Error::Unauthorized);
    }

    // Prune dead tables
    state.games.lock().await.collect();

    // Prune idle games older than 2 hours
    sqlx::query!(
        "DELETE FROM room WHERE room_id IN (
            SELECT room_id FROM event RIGHT JOIN room USING(room_id)
            WHERE  created_at < NOW() - INTERVAL '2 hours'
            GROUP BY room_id HAVING COUNT(time) < 4
        );"
    )
    .execute(&state.pool)
    .await?;

    // Prune games that have been inactive for 24 hrs
    sqlx::query!(
        "
            DELETE FROM room WHERE NOT EXISTS (
                SELECT 1 FROM event
                WHERE event.room_id = room.room_id
                AND time < NOW() - INTERVAL '1 day'
            );
        "
    )
    .execute(&state.pool)
    .await?;

    Ok(())
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
