//! Public profile endpoint — `GET /api/v1/users/{username}`.
//!
//! Permite que qualquer pessoa (autenticada ou não) veja o perfil público
//! de um usuário: high scores em cada universo e mãos favoritas no pôquer.
//! Email é PII, então NÃO retornamos.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use rusqlite::Connection;
use serde::Serialize;

pub struct ProfilesState {
    pub db_path: std::path::PathBuf,
}

#[derive(Serialize, Debug)]
pub struct ScoreEntry {
    pub game: String,
    pub score: u32,
    pub ts: String,
}

#[derive(Serialize)]
pub struct PublicProfile {
    pub username: String,
    pub user_id: String,
    pub scores: Vec<ScoreEntry>,
    pub favorite_hands: Vec<serde_json::Value>,
}

/// `GET /api/v1/users/{username}` — perfil público.
pub async fn get_profile(
    State(state): State<Arc<ProfilesState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let conn = match Connection::open(&state.db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("profiles open db: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "db_unavailable"})),
            )
                .into_response();
        }
    };
    // username → user_id
    let user_id: rusqlite::Result<String> = conn.query_row(
        "SELECT user_id FROM user_profiles WHERE username = ?1 LIMIT 1",
        [&username],
        |row| row.get(0),
    );
    let user_id = match user_id {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"erro": "usuario_nao_encontrado"})),
            )
                .into_response();
        }
    };

    // Top score por universo. Limit 4 (snake/tetris/invaders + futuro).
    let mut scores: Vec<ScoreEntry> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT game, score, ts FROM scores s1
         WHERE user_id = ?1 AND score = (
             SELECT MAX(score) FROM scores s2
             WHERE s2.user_id = s1.user_id AND s2.game = s1.game
         )
         ORDER BY game",
    ) && let Ok(rows) = stmt.query_map([&user_id], |row| {
        Ok(ScoreEntry {
            game: row.get(0)?,
            score: row.get(1)?,
            ts: row.get(2)?,
        })
    }) {
        scores.extend(rows.flatten());
    }

    // Mãos favoritas (até 20). Reusa o schema já criado por poker_favorites.
    let mut favs: Vec<serde_json::Value> = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT snapshot FROM poker_favorite_hands WHERE user_id = ?1
         ORDER BY favorited_at DESC LIMIT 20",
    ) && let Ok(rows) = stmt.query_map([&user_id], |row| {
        let json: String = row.get(0)?;
        Ok(serde_json::from_str::<serde_json::Value>(&json).ok())
    }) {
        favs.extend(rows.flatten().flatten());
    }

    (
        StatusCode::OK,
        Json(PublicProfile {
            username,
            user_id,
            scores,
            favorite_hands: favs,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn seed(path: &std::path::Path) {
        let c = Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS user_profiles (
                user_id TEXT PRIMARY KEY, email TEXT, username TEXT NOT NULL, updated_at INTEGER NOT NULL
            );
             CREATE TABLE IF NOT EXISTS scores (
                id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, game TEXT NOT NULL,
                score INTEGER NOT NULL, ts TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS poker_favorite_hands (
                id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, hand_id TEXT NOT NULL,
                favorited_at INTEGER NOT NULL, snapshot TEXT NOT NULL
             );
             INSERT INTO user_profiles VALUES ('usr_1', 'y@x.com', 'yuri', 0);
             INSERT INTO scores (user_id, game, score, ts) VALUES
                ('usr_1', 'snake', 100, '2026-05-15T00:00:00Z'),
                ('usr_1', 'snake', 80, '2026-05-15T00:00:00Z'),
                ('usr_1', 'tetris', 500, '2026-05-15T00:00:00Z');
             INSERT INTO poker_favorite_hands (user_id, hand_id, favorited_at, snapshot) VALUES
                ('usr_1', 'h1', 100, '{\"hand_id\":\"h1\"}');",
        )
        .unwrap();
    }

    fn make_app(path: std::path::PathBuf) -> Router {
        let state = Arc::new(ProfilesState { db_path: path });
        Router::new()
            .route("/api/v1/users/{username}", get(get_profile))
            .with_state(state)
    }

    #[tokio::test]
    async fn profile_existente_retorna_scores_e_favorites() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed(&path);
        let app = make_app(path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/users/yuri")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["username"], "yuri");
        assert_eq!(v["user_id"], "usr_1");
        // Top-per-game: snake=100, tetris=500 (snake 80 não aparece).
        let scores = v["scores"].as_array().unwrap();
        assert_eq!(scores.len(), 2);
        assert!(
            scores
                .iter()
                .any(|s| s["game"] == "snake" && s["score"] == 100)
        );
        assert!(
            scores
                .iter()
                .any(|s| s["game"] == "tetris" && s["score"] == 500)
        );
        assert_eq!(v["favorite_hands"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn profile_inexistente_retorna_404() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed(&path);
        let app = make_app(path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/users/ninguem")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
