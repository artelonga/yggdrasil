use axum::{Json, response::IntoResponse};
use yggdrasil_core::lobby::lobby;

pub async fn get_lobby() -> impl IntoResponse {
    Json(lobby())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
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
}
