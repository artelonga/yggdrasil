//! Yggdrasil web server — entrypoint.

mod auth;
mod games;
mod lobby_routes;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    Router,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use games::invaders_routes::{
    make_invaders_state, send_input as invaders_input, start_game as invaders_start,
};
use games::snake_routes::{make_snake_state, send_input as snake_input, start_game as snake_start};
use games::tetris_routes::{
    make_tetris_state, send_input as tetris_input, start_game as tetris_start,
};
use tower_http::services::ServeDir;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let jwt_secret = std::env::var("YGGDRASIL_JWT_SECRET").map_err(|_| {
        anyhow::anyhow!(
            "YGGDRASIL_JWT_SECRET não configurado — defina esta variável de ambiente para iniciar o servidor"
        )
    })?;

    let db_path = std::env::var("YGGDRASIL_DB").unwrap_or_else(|_| "yggdrasil.db".to_string());
    let auth_conn = rusqlite::Connection::open(&db_path)?;
    auth::init_auth_db(&auth_conn)?;
    let auth_state = Arc::new(auth::AuthState {
        db: Arc::new(Mutex::new(auth_conn)),
        mail: Arc::new(game_core::mail::LogMailProvider),
        jwt_secret,
    });

    let snake_state = make_snake_state(&db_path).expect("sqlite init (snake)");
    let tetris_state = make_tetris_state(&db_path).expect("sqlite init (tetris)");
    let invaders_state = make_invaders_state(&db_path).expect("sqlite init (invaders)");

    let auth_router = Router::new()
        .route("/api/v1/auth/code", post(auth::request_code))
        .route("/api/v1/auth/verify", post(auth::verify_code))
        .with_state(auth_state);

    let snake_router = Router::new()
        .route("/api/v1/games/snake/start", get(snake_start))
        .route("/api/v1/games/snake/{id}/input", post(snake_input))
        .with_state(snake_state);

    let tetris_router = Router::new()
        .route("/api/v1/games/tetris/start", get(tetris_start))
        .route("/api/v1/games/tetris/{id}/input", post(tetris_input))
        .with_state(tetris_state);

    let invaders_router = Router::new()
        .route("/api/v1/games/invaders/start", get(invaders_start))
        .route("/api/v1/games/invaders/{id}/input", post(invaders_input))
        .with_state(invaders_state);

    let app = Router::new()
        .route("/", get(root))
        .route("/lobby", get(serve_lobby))
        .route("/games/snake", get(serve_snake))
        .route("/games/tetris", get(serve_tetris))
        .route("/games/invaders", get(serve_invaders))
        .route("/health", get(health))
        .route("/api/v1/lobby", get(lobby_routes::get_lobby))
        .route("/api/v1/lobby/enter", post(lobby_routes::post_enter))
        .merge(auth_router)
        .merge(snake_router)
        .merge(tetris_router)
        .merge(invaders_router)
        .nest_service("/static", ServeDir::new("yggdrasil-web/static"));

    let addr: SocketAddr = "0.0.0.0:3030".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("yggdrasil-web ouvindo em http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> impl IntoResponse {
    Redirect::to("/lobby")
}

async fn serve_lobby() -> impl IntoResponse {
    Html(include_str!("../static/lobby.html"))
}

async fn serve_snake() -> impl IntoResponse {
    Html(include_str!("../static/games/snake.html"))
}

async fn serve_tetris() -> impl IntoResponse {
    Html(include_str!("../static/games/tetris.html"))
}

async fn serve_invaders() -> impl IntoResponse {
    Html(include_str!("../static/games/invaders.html"))
}

async fn health() -> impl IntoResponse {
    "ok"
}
