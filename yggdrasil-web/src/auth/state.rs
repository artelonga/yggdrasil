use std::sync::{Arc, Mutex};

use game_core::mail::MailProvider;
use rusqlite::Connection;

pub struct AuthState {
    pub db: Arc<Mutex<Connection>>,
    pub mail: Arc<dyn MailProvider>,
    pub jwt_secret: String,
}

pub fn init_auth_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS usuarios (
            email   TEXT PRIMARY KEY,
            user_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS verify_codes (
            email      TEXT PRIMARY KEY,
            code       TEXT NOT NULL,
            user_id    TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            attempts   INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS rate_limits (
            email    TEXT PRIMARY KEY,
            requests TEXT NOT NULL
        );",
    )
}
