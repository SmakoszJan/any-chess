#![deny(clippy::unwrap_used)]

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use jsonwebtoken::{EncodingKey, Header};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Debug)]
pub enum Error {
    Sqlx(sqlx::Error),
    Io(std::io::Error),
    Jwt(jsonwebtoken::errors::Error),
    NotFound,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
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
    secret: EncodingKey,
}

pub async fn serve() -> Result<(), Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("DATABASE_URL").expect("missing environment variable"))
        .await?;

    let app = Router::new()
        .route("/rooms", get(get_rooms).post(create_room))
        .route("/rooms/{}/join", post(join_room))
        .with_state(AppState {
            pool,
            secret: EncodingKey::from_secret(
                std::env::var("JWT_SECRET")
                    .expect("missing environment variable")
                    .as_bytes(),
            ),
        });

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Serialize)]
pub struct Room {
    pub id: i32,
    pub name: Option<String>,
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
}

async fn create_room(
    State(state): State<AppState>,
    Json(name): Json<String>,
) -> Result<String, Error> {
    let is_white = rand::random_bool(0.5);
    let id = sqlx::query_scalar!(
        "INSERT INTO room (white_taken, name) VALUES ($1, $2) RETURNING room_id;",
        is_white,
        name
    )
    .fetch_one(&state.pool)
    .await?;

    Ok(jsonwebtoken::encode(
        &Header::default(),
        &PlayerClaims { room: id, is_white },
        &state.secret,
    )?)
}

async fn join_room(
    Query(room): Query<i32>,
    State(state): State<AppState>,
) -> Result<String, Error> {
    let white_taken = sqlx::query_scalar!(
        "UPDATE room SET open=false WHERE room_id=$1 AND open=true RETURNING white_taken;",
        room
    )
    .fetch_optional(&state.pool)
    .await?;

    if let Some(white_taken) = white_taken {
        Ok(jsonwebtoken::encode(
            &Header::default(),
            &PlayerClaims {
                room,
                is_white: !white_taken,
            },
            &state.secret,
        )?)
    } else {
        Err(Error::NotFound)
    }
}
