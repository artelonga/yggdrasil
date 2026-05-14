use std::sync::Mutex;

use axum::http::HeaderMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Query string `?variant=<slug>` para rotas de início de jogo (YG-37).
/// Slug `None` ou desconhecido → root default.
#[derive(Deserialize, Default)]
pub struct VariantQuery {
    pub variant: Option<String>,
}

/// Extrai `user_id` da Authorization Bearer JWT, ou `"anonymous"` se ausente/inválido.
/// Permite que jogadores anônimos joguem (sem persistir score em conta), mas registra
/// scores em conta quando autenticado.
pub fn user_id_from_jwt(headers: &HeaderMap, jwt_secret: &str) -> String {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| crate::auth::verify_jwt(t, jwt_secret).ok())
        .unwrap_or_else(|| "anonymous".to_string())
}

pub fn init_db(db_path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scores (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            game    TEXT NOT NULL,
            score   INTEGER NOT NULL,
            ts      TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

pub fn save_score(conn: &Connection, user_id: &str, game: &str, score: u32) {
    let ts = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        "INSERT INTO scores (user_id, game, score, ts) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![user_id, game, score, ts],
    );
}

pub fn save_score_locked(db: &Mutex<Connection>, user_id: &str, game: &str, score: u32) {
    let conn = db.lock().unwrap();
    save_score(&conn, user_id, game, score);
}

#[derive(Deserialize)]
pub struct InputRequest {
    pub direction: String,
    // YG-N: user_id removed from body — now read from JWT Authorization header
    // via `user_id_from_jwt`. Old clients sending `user_id` are silently ignored
    // (serde default behavior for unknown fields).
}

pub fn map_to_value(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap_or(serde_json::Value::Null)
}

#[derive(Serialize)]
pub struct StartResponse {
    pub id: String,
    pub state: serde_json::Value,
    pub score: u32,
}

#[derive(Serialize)]
pub struct TickResponse {
    pub action: String,
    pub state: serde_json::Value,
    pub score: u32,
}
