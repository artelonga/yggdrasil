use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Query string `?variant=<slug>` para rotas de início de jogo (YG-37).
/// Slug `None` ou desconhecido → root default.
#[derive(Deserialize, Default)]
pub struct VariantQuery {
    pub variant: Option<String>,
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

fn default_user_id() -> String {
    "anonymous".to_string()
}

#[derive(Deserialize)]
pub struct InputRequest {
    pub direction: String,
    #[serde(default = "default_user_id")]
    pub user_id: String,
}

#[derive(Serialize)]
pub struct StartResponse<S> {
    pub id: String,
    pub state: S,
    pub score: u32,
}

#[derive(Serialize)]
pub struct TickResponse<S> {
    pub action: String,
    pub state: S,
    pub score: u32,
}
