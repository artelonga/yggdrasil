mod html;
mod routes;

use axum::{
    Router,
    routing::{get, post},
};

pub fn router() -> Router {
    Router::new()
        .route("/lobby", get(html::serve_lobby))
        .route("/api/v1/lobby", get(routes::get_lobby))
        .route("/api/v1/lobby/enter", post(routes::post_enter))
}
