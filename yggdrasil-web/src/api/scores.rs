//! Leaderboard endpoints — leitura agregada da tabela `scores`.
//!
//! Anônimo (sem auth) — high scores são públicos no lobby. Cada universo
//! que persiste pontuação (snake, tetris, invaders) contribui rows via
//! `games::common::save_score`. Poker não entra aqui (economia via sementes).

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub struct ScoresState {
    pub db: Arc<Mutex<Connection>>,
}

#[derive(Serialize)]
pub struct ScoreRow {
    pub user_id: String,
    /// Username slugificado do email (preenchido por `api::user_profiles`).
    /// Quando ausente, igual a `user_id` — frontend pode renderizar diretamente.
    pub username: String,
    pub game: String,
    pub score: u32,
    pub ts: String,
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

/// `GET /api/v1/scores/top?limit=3` — top N por universo, todos os universos.
pub async fn get_top(
    State(state): State<Arc<ScoresState>>,
    Query(q): Query<TopQuery>,
) -> impl IntoResponse {
    let limit = q.limit.clamp(1, 50);
    let conn = state.db.lock().unwrap();
    // LEFT JOIN com user_profiles para resolver user_id → username.
    // COALESCE faz fallback para o próprio user_id quando o perfil não existe
    // (usuário anônimo, ou ainda não passou pelo handover do CO).
    let mut stmt = match conn.prepare(
        "SELECT s1.user_id, COALESCE(up.username, s1.user_id) AS username,
                s1.game, s1.score, s1.ts
         FROM scores s1
         LEFT JOIN user_profiles up ON up.user_id = s1.user_id
         WHERE s1.score >= (
           SELECT score FROM scores s2
           WHERE s2.game = s1.game
           ORDER BY score DESC LIMIT 1 OFFSET ?1
         )
         OR (
           SELECT COUNT(*) FROM scores s2 WHERE s2.game = s1.game
         ) <= ?1
         ORDER BY s1.game, s1.score DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("scores top prepare: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "Erro ao consultar pontuações"})),
            )
                .into_response();
        }
    };
    let rows = stmt
        .query_map([limit.saturating_sub(1) as i64], |row| {
            Ok(ScoreRow {
                user_id: row.get(0)?,
                username: row.get(1)?,
                game: row.get(2)?,
                score: row.get(3)?,
                ts: row.get(4)?,
            })
        })
        .and_then(|m| m.collect::<Result<Vec<_>, _>>());

    match rows {
        Ok(scores) => (
            StatusCode::OK,
            Json(serde_json::json!({ "scores": scores })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("scores top query: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "Erro ao consultar pontuações"})),
            )
                .into_response()
        }
    }
}

/// `GET /api/v1/scores/recent` — últimas 10 entradas (atividade recente).
pub async fn get_recent(State(state): State<Arc<ScoresState>>) -> impl IntoResponse {
    let conn = state.db.lock().unwrap();
    let mut stmt = match conn.prepare(
        "SELECT s.user_id, COALESCE(up.username, s.user_id) AS username,
                s.game, s.score, s.ts
         FROM scores s
         LEFT JOIN user_profiles up ON up.user_id = s.user_id
         ORDER BY s.ts DESC LIMIT 10",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("scores recent prepare: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "Erro ao consultar atividade"})),
            )
                .into_response();
        }
    };
    let rows = stmt
        .query_map([], |row| {
            Ok(ScoreRow {
                user_id: row.get(0)?,
                username: row.get(1)?,
                game: row.get(2)?,
                score: row.get(3)?,
                ts: row.get(4)?,
            })
        })
        .and_then(|m| m.collect::<Result<Vec<_>, _>>());

    match rows {
        Ok(scores) => (
            StatusCode::OK,
            Json(serde_json::json!({ "scores": scores })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("scores recent query: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "Erro ao consultar atividade"})),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use rusqlite::Connection;
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn make_state_with_scores(scores: Vec<(&str, &str, u32, &str)>) -> Arc<ScoresState> {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scores (
                id      INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id TEXT NOT NULL,
                game    TEXT NOT NULL,
                score   INTEGER NOT NULL,
                ts      TEXT NOT NULL
            );
             CREATE TABLE IF NOT EXISTS user_profiles (
                user_id    TEXT PRIMARY KEY,
                email      TEXT,
                username   TEXT NOT NULL,
                updated_at INTEGER NOT NULL
             );",
        )
        .unwrap();
        for (user_id, game, score, ts) in scores {
            conn.execute(
                "INSERT INTO scores (user_id, game, score, ts) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![user_id, game, score, ts],
            )
            .unwrap();
        }
        std::mem::forget(dir); // keep tempdir alive for the test
        Arc::new(ScoresState {
            db: Arc::new(Mutex::new(conn)),
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
