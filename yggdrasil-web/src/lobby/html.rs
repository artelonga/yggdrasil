use axum::response::{Html, IntoResponse};

pub async fn serve_lobby() -> impl IntoResponse {
    Html(include_str!("../../static/lobby.html"))
}
