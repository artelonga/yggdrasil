//! Leaderboard endpoints — leitura agregada da tabela `scores`.
//!
//! Anônimo (sem auth) — high scores são públicos no lobby. Cada universo
//! que persiste pontuação (snake, tetris, invaders) contribui rows via
//! `ScoresStore::save_score`. Poker não entra aqui (economia via sementes).

use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::scores_store::ScoresStore;

pub struct ScoresState {
    pub scores: Arc<dyn ScoresStore>,
}

#[derive(Deserialize)]
pub struct TopQuery {
    /// Quantos por universo (default 3).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    3
}

#[utoipa::path(
    get,
    path = "/api/v1/scores/top",
    params(
        ("limit" = Option<u32>, Query, description = "Máx por universo (1–50, default 3)")
    ),
    responses(
        (status = 200, description = "Top scores por universo", body = crate::openapi::ScoresListDoc),
    ),
    tag = "scores"
)]
pub async fn get_top(
    State(state): State<Arc<ScoresState>>,
    Query(q): Query<TopQuery>,
) -> impl IntoResponse {
    let limit = q.limit.clamp(1, 50);
    let scores = state.scores.top_scores(limit);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "scores": scores })),
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/scores/recent",
    responses(
        (status = 200, description = "Últimas 10 pontuações", body = crate::openapi::ScoresListDoc),
    ),
    tag = "scores"
)]
pub async fn get_recent(State(state): State<Arc<ScoresState>>) -> impl IntoResponse {
    let scores = state.scores.recent_scores(10);
    (
        StatusCode::OK,
        Json(serde_json::json!({ "scores": scores })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    use crate::scores_store::InMemoryScoresStore;

    fn make_state_with_scores(scores: Vec<(&str, &str, u32, &str)>) -> Arc<ScoresState> {
        let store = InMemoryScoresStore::new();
        store.seed(&scores);
        Arc::new(ScoresState {
            scores: Arc::new(store) as Arc<dyn ScoresStore>,
        })
    }

    fn make_app(state: Arc<ScoresState>) -> Router {
        Router::new()
            .route("/api/v1/scores/top", get(get_top))
            .route("/api/v1/scores/recent", get(get_recent))
            .with_state(state)
    }

    async fn parse_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn top_retorna_3_por_universo_por_padrao() {
        let state = make_state_with_scores(vec![
            ("alice", "snake", 100, "2026-05-01T00:00:00Z"),
            ("bob", "snake", 80, "2026-05-02T00:00:00Z"),
            ("clara", "snake", 60, "2026-05-03T00:00:00Z"),
            ("dani", "snake", 40, "2026-05-04T00:00:00Z"),
            ("alice", "tetris", 200, "2026-05-01T00:00:00Z"),
            ("bob", "tetris", 150, "2026-05-02T00:00:00Z"),
        ]);
        let app = make_app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/scores/top")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        let snake: Vec<_> = v["scores"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["game"] == "snake")
            .collect();
        // Top 3 de snake (limit padrão = 3) — dani (40) fica de fora
        assert_eq!(snake.len(), 3);
        assert_eq!(snake[0]["score"].as_u64().unwrap(), 100);
    }

    #[tokio::test]
    async fn recent_retorna_em_ordem_decrescente_de_ts() {
        let state = make_state_with_scores(vec![
            ("alice", "snake", 10, "2026-05-01T00:00:00Z"),
            ("bob", "tetris", 20, "2026-05-05T00:00:00Z"),
            ("clara", "invaders", 30, "2026-05-03T00:00:00Z"),
        ]);
        let app = make_app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/scores/recent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        let scores = v["scores"].as_array().unwrap();
        assert_eq!(scores[0]["game"], "tetris"); // ts mais recente
        assert_eq!(scores[1]["game"], "invaders");
        assert_eq!(scores[2]["game"], "snake");
    }

    #[tokio::test]
    async fn recent_sem_dados_retorna_array_vazio() {
        let state = make_state_with_scores(vec![]);
        let app = make_app(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/scores/recent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert_eq!(v["scores"].as_array().unwrap().len(), 0);
    }
}
