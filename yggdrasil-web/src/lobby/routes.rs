use axum::{Json, http::StatusCode, response::IntoResponse};
use game_core::engine::Tile;
use serde::{Deserialize, Serialize};
use yggdrasil_core::lobby::lobby;

pub async fn get_lobby() -> impl IntoResponse {
    Json(lobby())
}

#[derive(Deserialize)]
pub struct EnterRequest {
    x: usize,
    y: usize,
}

#[derive(Serialize, Deserialize)]
pub struct EnterResponse {
    slug: String,
}

pub async fn post_enter(Json(req): Json<EnterRequest>) -> impl IntoResponse {
    let u = lobby();
    match u.map.get_tile(req.x, req.y) {
        Some(Tile::Portal(slug)) => Json(EnterResponse { slug: slug.clone() }).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::{get, post},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn get_lobby_returns_40x20_universe() {
        let app = Router::new().route("/api/v1/lobby", get(get_lobby));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/lobby")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let universe: game_core::engine::Universe = serde_json::from_slice(&body).unwrap();
        assert_eq!(universe.map.width, 40);
        assert_eq!(universe.map.height, 20);
    }

    #[tokio::test]
    async fn get_lobby_content_type_is_json() {
        let app = Router::new().route("/api/v1/lobby", get(get_lobby));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/lobby")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("application/json"), "content-type: {ct}");
    }

    #[tokio::test]
    async fn post_enter_snake_portal_returns_slug() {
        let app = Router::new().route("/api/v1/lobby/enter", post(post_enter));
        let body = serde_json::json!({ "x": 10, "y": 8 }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/lobby/enter")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let enter: EnterResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(enter.slug, "snake");
    }

    #[tokio::test]
    async fn post_enter_non_portal_returns_404() {
        let app = Router::new().route("/api/v1/lobby/enter", post(post_enter));
        let body = serde_json::json!({ "x": 5, "y": 5 }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/lobby/enter")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
