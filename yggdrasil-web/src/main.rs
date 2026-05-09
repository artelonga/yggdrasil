//! Yggdrasil web server — entrypoint.
//!
//! `v0.0.1`: apenas health check e serve estáticos de `static/`. As rotas
//! reais (lobby, jogos, API pública) entram nos YG-3+ tasks.

use axum::{Router, response::IntoResponse, routing::get};
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
        .route("/health", get(health))
        .nest_service("/static", ServeDir::new("yggdrasil-web/static"));

    let addr: SocketAddr = "0.0.0.0:3030".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("yggdrasil-web ouvindo em http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    "ok"
}
