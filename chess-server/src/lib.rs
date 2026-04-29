use std::{
    borrow::Cow,
    hash::{DefaultHasher, Hash as _, Hasher},
    net::IpAddr,
    ops::ControlFlow,
    sync::{Arc, atomic::AtomicUsize},
    time::{Duration, SystemTime, UNIX_EPOCH},
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
    Board, Color, Move, VERSION,
    net::{ChessEvent, ChessMessage, ClientMessage, RoomPlayer},
};
use futures::lock::Mutex;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::{
    sync::broadcast,
    time::{Instant, interval, sleep},
};
use tower_http::trace::TraceLayer;

use crate::state_set::StateSet;

pub mod state_set;

#[derive(Debug)]
pub enum Error {
    Sqlx(sqlx::Error),
    Io(std::io::Error),
    Jwt(jsonwebtoken::errors::Error),
    Serde(serde_json::Error),
    NotFound,
    Unauthorized,
    Rejected,
    Gone,
}

impl From<serde_json::Error> for Error {
    fn from(v: serde_json::Error) -> Self {
        Self::Serde(v)
    }
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
    async fn load(&self, room: i32) -> Result<Arc<Mutex<Table>>, Error> {
        let table = self.games.lock().await.get(room);

        if let Some(table) = table {
            Ok(table)
        } else {
            struct Res {
                payload: serde_json::Value,
            }
            let events: Vec<ChessEvent> = sqlx::query_as!(
                Res,
                "SELECT payload FROM event WHERE room_id=$1 ORDER BY time ASC;",
                room
            )
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|v| serde_json::from_value(v.payload))
            .collect::<Result<_, _>>()?;

            let mut table = Table::new(room);
            table.load(events);

            Ok(self
                .games
                .lock()
                .await
                .maybe_insert(room, Arc::new(Mutex::new(table))))
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

    let password = std::env::var("PASSWORD").expect("missing password");
    let secret = std::env::var("SECRET_JWT").expect("missing environment variable");
    let secret: Arc<str> = Arc::from(secret);
    let secret_bytes = secret.as_bytes();
    let app = Router::new()
        .route("/", get(async || StatusCode::OK))
        .route("/version", get(async || Json(VERSION)))
        .route("/rooms", post(create_room))
        .route("/rooms/match", post(match_room))
        .route("/rooms/{code}", post(join_room))
        .route("/rooms/{id}/play", get(play))
        .route("/connect", get(connect))
        .route("/prune", post(prune))
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

#[derive(Serialize, Deserialize)]
struct PlayerClaims {
    room: i32,
    is_white: bool,
    exp: u64,
    code: Option<String>,
}

#[derive(Hash)]
struct Obscured<'r> {
    ip: IpAddr,
    secret: &'r str,
}

const CHARSET: &[u8] = b"bcdfghjklmnpqrstvwxyz23456789";

fn extract_ip(headers: &HeaderMap, state: &AppState) -> Option<i64> {
    client_ip::x_real_ip(&headers).ok().map(|ip| {
        let data = Obscured {
            ip,
            secret: &state.secret,
        };
        let mut s = DefaultHasher::default();
        data.hash(&mut s);
        s.finish() as i64
    })
}

fn connection_token(room: i32, is_white: bool, state: &AppState) -> Result<String, Error> {
    jsonwebtoken::encode(
        &Header::default(),
        &ConnectClaims {
            exp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60,
            room,
            is_white,
        },
        &state.encode_key,
    )
    .map_err(Error::from)
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RoomPlayer>, Error> {
    let ip = extract_ip(&headers, &state);

    let is_white = rand::random_bool(0.5);
    let mut code = String::new();
    let id = loop {
        let chars = rand::random_iter::<u8>()
            .take(5)
            .map(|i| CHARSET[i as usize % CHARSET.len()] as char);
        code.clear();
        code.extend(chars);
        let id = sqlx::query_scalar!(
            "INSERT INTO room (white_taken, open, created_by, entry_code)
                SELECT $1, true, $2, $3
                WHERE (
                    SELECT COUNT(*) FROM room WHERE created_by=$2
                ) < 100
            RETURNING room_id;",
            is_white,
            ip,
            code.as_str()
        )
        .fetch_optional(&state.pool)
        .await;

        if id.as_ref().is_err_and(|err| {
            err.as_database_error()
                .is_some_and(|v| v.is_unique_violation())
        }) {
            continue;
        }

        break id?;
    };

    let Some(id) = id else {
        return Err(Error::Rejected);
    };

    let token = jsonwebtoken::encode(
        &Header::default(),
        &PlayerClaims {
            room: id,
            is_white,
            exp: u64::MAX,
            code: Some(code.clone()),
        },
        &state.encode_key,
    )?;

    Ok(Json(RoomPlayer {
        id,
        is_white,
        token,
        connection_token: connection_token(id, is_white, &state)?,
        code: Some(code),
    }))
}

struct JoinResult {
    room_id: i32,
    white_taken: bool,
}

async fn join(
    room: i32,
    white_taken: bool,
    code: Option<String>,
    state: &AppState,
) -> Result<Json<RoomPlayer>, Error> {
    let token = jsonwebtoken::encode(
        &Header::default(),
        &PlayerClaims {
            room,
            is_white: !white_taken,
            exp: u64::MAX,
            code: code.clone(),
        },
        &state.encode_key,
    )?;

    // Notify the table that the game can start.
    state
        .load(room)
        .await?
        .lock()
        .await
        .process(ChessEvent::Start, state)
        .await?;

    Ok(Json(RoomPlayer {
        id: room,
        is_white: !white_taken,
        token,
        connection_token: connection_token(room, !white_taken, &state)?,
        code,
    }))
}

async fn join_room(
    Path(code): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<RoomPlayer>, Error> {
    let result = sqlx::query_as!(
        JoinResult,
        "UPDATE room SET open=false WHERE entry_code=$1 AND open=true RETURNING room_id, white_taken;",
        code
    )
    .fetch_optional(&state.pool)
    .await?;

    if let Some(result) = result {
        join(result.room_id, result.white_taken, Some(code), &state).await
    } else {
        Err(Error::NotFound)
    }
}

async fn match_room(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<RoomPlayer>, Error> {
    let result = sqlx::query_as!(
        JoinResult,
        "UPDATE room SET open=false WHERE room_id = (
            SELECT room_id FROM room WHERE entry_code is NULL AND open=true LIMIT 1 FOR UPDATE
        ) RETURNING room_id, white_taken;"
    )
    .fetch_optional(&state.pool)
    .await?;

    if let Some(result) = result {
        // If exists, join
        join(result.room_id, result.white_taken, None, &state).await
    } else {
        // Otherwise, create
        let ip = extract_ip(&headers, &state);

        let is_white = rand::random_bool(0.5);
        let id = sqlx::query_scalar!(
            "INSERT INTO room (white_taken, open, created_by)
                SELECT $1, true, $2
                WHERE (
                    SELECT COUNT(*) FROM room WHERE created_by=$2
                ) < 100
            RETURNING room_id;",
            is_white,
            ip,
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
                code: None,
            },
            &state.encode_key,
        )?;
        Ok(Json(RoomPlayer {
            id,
            is_white,
            token,
            connection_token: connection_token(id, is_white, &state)?,
            code: None,
        }))
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
        return Err(Error::Gone);
    }

    connection_token(room, claims.is_white, &state)
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

    async fn ping(&mut self) -> Result<(), axum::Error> {
        self.0.send(ws::Message::Ping(vec![].into())).await
    }

    async fn recv(&mut self) -> Option<ClientMessage> {
        loop {
            let msg = self.0.recv().await?.ok()?.into_text().ok()?;
            if msg.is_empty() {
                continue;
            }

            return serde_json::from_slice(msg.as_bytes()).ok();
        }
    }

    async fn close(&mut self) -> Result<(), axum::Error> {
        self.0.send(ws::Message::Close(None)).await
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        WEBSOCKET_COUNT.fetch_sub(1, std::sync::atomic::Ordering::Release);
    }
}

async fn handle_msg(
    msg: Option<ClientMessage>,
    socket: &mut Connection,
    table: &Mutex<Table>,
    color: Color,
    app_state: &AppState,
) -> ControlFlow<()> {
    let Some(msg) = msg else {
        return ControlFlow::Break(());
    };

    match msg {
        ClientMessage::Move(mv) => {
            let mut table = table.lock().await;

            if table.board[mv.from].is_none_or(|v| v.color != color)
                || mv.check(&table.board).is_err()
            {
                tracing::debug!("erroneous move {mv:?}");
                socket.send(&ChessMessage::MoveError).await.ok();
                return ControlFlow::Continue(());
            }

            if let Err(err) = table.process(ChessEvent::Move(mv), &app_state).await {
                tracing::error!("Sqlx call failed: {err:?}, aborting");
                return ControlFlow::Break(());
            }
        }
    }

    ControlFlow::Continue(())
}

async fn handle_websocket(socket: WebSocket, app_state: AppState, room: i32, is_white: bool) {
    let mut socket = Connection(socket);
    let color = if is_white { Color::White } else { Color::Black };

    // First, load the state. We'll send it over the websocket
    let table = match app_state.load(room).await {
        Ok(v) => v,
        Err(err) => {
            tracing::error!("Room {room} failed to load: {err:?}.");
            return;
        }
    };

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

    tracing::debug!("room {room} sync sent");

    let mut ping = interval(Duration::from_secs(30));
    let timeout = sleep(Duration::from_secs(30 * 60));
    tokio::pin!(timeout);

    // Handle moves/events
    loop {
        let flow = tokio::select! {
            _ = ping.tick() => {
                if socket.ping().await.is_err() {
                    break;
                }

                ControlFlow::Continue(())
            }
            msg = socket.recv() => {
                timeout.as_mut().reset(Instant::now() + Duration::from_secs(30 * 60));
                handle_msg(msg, &mut socket, &table, color, &app_state).await
            }
            ev = receiver.recv() => {
                let Ok(()) = socket.send(&ChessMessage::Event(ev.unwrap())).await else {return;};
                ControlFlow::Continue(())
            }
            _ = &mut timeout => {
                socket.close().await.ok();
                return;
            }
        };

        if flow.is_break() {
            break;
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
    pub id: i32,
    pub board: Board,
    pub events: Vec<ChessEvent>,
    pub sender: broadcast::Sender<ChessEvent>,
}

impl Table {
    pub fn new(id: i32) -> Self {
        Self {
            id,
            board: Board::new(),
            events: Vec::new(),
            sender: broadcast::Sender::new(64),
        }
    }

    async fn process(&mut self, ev: ChessEvent, state: &AppState) -> Result<(), sqlx::Error> {
        // This order is necessary for clients to properly connect (so that no move is skipped)
        sqlx::query!(
            "INSERT INTO event (room_id, payload) VALUES ($1, $2);",
            self.id,
            serde_json::to_value(ev).expect("event serialization failed")
        )
        .execute(&state.pool)
        .await?;

        self.process_inner(ev);

        self.sender.send(ev).ok();

        Ok(())
    }

    fn process_inner(&mut self, ev: ChessEvent) {
        match ev {
            ChessEvent::Move(mv) => mv.exec(&mut self.board),
            ChessEvent::Start | ChessEvent::GameEnded => (),
        }

        self.events.push(ev);
    }

    pub fn load(&mut self, ev: impl IntoIterator<Item = ChessEvent>) {
        for ev in ev {
            self.process_inner(ev);
        }
    }
}
