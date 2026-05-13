//! Yggdrasil web server — entrypoint.

mod api;
mod auth;
mod games;
mod lobby_routes;
mod mail;

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
use games::poker_routes::{
    PokerState, get_hand as poker_get_hand, get_hole_cards as poker_hole_cards,
    get_lobby as poker_get_lobby, list_lobbies as poker_list_lobbies,
    post_action as poker_post_action, sit as poker_sit, stand as poker_stand,
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
        mail: mail::build_mail_provider(),
        jwt_secret,
    });

    let snake_state = make_snake_state(&db_path).expect("sqlite init (snake)");
    let tetris_state = make_tetris_state(&db_path).expect("sqlite init (tetris)");
    let invaders_state = make_invaders_state(&db_path).expect("sqlite init (invaders)");

    // Scores DB compartilhada (mesma tabela `scores` que snake/tetris/invaders gravam).
    let scores_conn = rusqlite::Connection::open(&db_path)?;
    let scores_state = Arc::new(api::scores::ScoresState {
        db: Arc::new(Mutex::new(scores_conn)),
    });

    let sementes_db = std::env::var("YGGDRASIL_SEMENTES_DB")
        .unwrap_or_else(|_| "yggdrasil-sementes.db".to_string());
    let sementes_storage = Arc::new(
        game_core::storage::Storage::open(std::path::Path::new(&sementes_db))
            .map_err(|e| anyhow::anyhow!("Erro ao abrir storage de sementes: {e}"))?,
    );
    let sementes = Arc::new(yggdrasil_core::sementes::Sementes::new(sementes_storage));
    let me_state = Arc::new(api::me::MeState {
        jwt_secret: auth_state.jwt_secret.clone(),
        sementes: sementes.clone(),
    });

    let me_router = Router::new()
        .route(
            "/api/v1/me/sementes",
            axum::routing::get(api::me::get_sementes),
        )
        .with_state(me_state);

    let scores_router = Router::new()
        .route("/api/v1/scores/top", get(api::scores::get_top))
        .route("/api/v1/scores/recent", get(api::scores::get_recent))
        .with_state(scores_state);

    let poker_state = Arc::new(PokerState::new(
        auth_state.jwt_secret.clone(),
        sementes.clone(),
    ));

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

    let poker_router = Router::new()
        .route("/api/v1/poker/lobbies", get(poker_list_lobbies))
        .route("/api/v1/poker/lobbies/{id}", get(poker_get_lobby))
        .route("/api/v1/poker/lobbies/{id}/sit", post(poker_sit))
        .route("/api/v1/poker/lobbies/{id}/stand", post(poker_stand))
        .route("/api/v1/poker/lobbies/{id}/hand", get(poker_get_hand))
        .route(
            "/api/v1/poker/lobbies/{id}/hole-cards",
            get(poker_hole_cards),
        )
        .route("/api/v1/poker/lobbies/{id}/action", post(poker_post_action))
        .with_state(poker_state);

    let app = Router::new()
        .route("/", get(root))
        .route("/lobby", get(serve_lobby))
        .route("/login", get(serve_login))
        .route("/universos/snake", get(serve_snake))
        .route("/universos/tetris", get(serve_tetris))
        .route("/universos/invaders", get(serve_invaders))
        .route("/universos/poker", get(serve_poker))
        // 301 redirects para preservar bookmarks/links externos com a URL
        // antiga `/games/<slug>`. Remover quando todos os universos ativos
        // estiverem na nova URL por ≥ 1 release.
        .route("/games/snake", get(redirect_to_universo_snake))
        .route("/games/tetris", get(redirect_to_universo_tetris))
        .route("/games/invaders", get(redirect_to_universo_invaders))
        .route("/games/poker", get(redirect_to_universo_poker))
        .route("/health", get(health))
        .route("/api/v1/lobby", get(lobby_routes::get_lobby))
        .route("/api/v1/lobby/enter", post(lobby_routes::post_enter))
        .merge(auth_router)
        .merge(me_router)
        .merge(scores_router)
        .merge(snake_router)
        .merge(tetris_router)
        .merge(invaders_router)
        .merge(poker_router)
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

async fn serve_login() -> impl IntoResponse {
    Html(include_str!("../static/login.html"))
}

async fn serve_snake() -> impl IntoResponse {
    Html(include_str!("../static/universos/snake.html"))
}

async fn serve_tetris() -> impl IntoResponse {
    Html(include_str!("../static/universos/tetris.html"))
}

async fn serve_invaders() -> impl IntoResponse {
    Html(include_str!("../static/universos/invaders.html"))
}

async fn serve_poker() -> impl IntoResponse {
    Html(include_str!("../static/universos/poker.html"))
}

// ── Legacy redirects (YG-N rename `/games/*` → `/universos/*`) ─────────────

async fn redirect_to_universo_snake() -> impl IntoResponse {
    Redirect::permanent("/universos/snake")
}
async fn redirect_to_universo_tetris() -> impl IntoResponse {
    Redirect::permanent("/universos/tetris")
}
async fn redirect_to_universo_invaders() -> impl IntoResponse {
    Redirect::permanent("/universos/invaders")
}
async fn redirect_to_universo_poker() -> impl IntoResponse {
    Redirect::permanent("/universos/poker")
}

async fn health() -> impl IntoResponse {
    "ok"
}
