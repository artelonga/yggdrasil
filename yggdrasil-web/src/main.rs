//! Yggdrasil web server — entrypoint.

mod lobby_routes;

use axum::{
    Router,
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = Router::new()
        .route("/", get(root))
        .route("/lobby", get(serve_lobby))
        .route("/health", get(health))
        .route("/api/v1/lobby", get(lobby_routes::get_lobby))
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

async fn health() -> impl IntoResponse {
    "ok"
}
