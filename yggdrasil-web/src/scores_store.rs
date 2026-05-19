//! ScoresStore trait — abstracts score persistence for game states and the leaderboard API.

use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Serialize)]
pub struct ScoreRow {
    pub user_id: String,
    pub game: String,
    pub score: u32,
    pub ts: String,
}

pub trait ScoresStore: Send + Sync {
    fn save_score(&self, user_id: &str, game: &str, score: u32);
    fn top_scores(&self, limit: u32) -> Vec<ScoreRow>;
    fn recent_scores(&self, n: u32) -> Vec<ScoreRow>;
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scores (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            game    TEXT NOT NULL,
            score   INTEGER NOT NULL,
            ts      TEXT NOT NULL
        );",
    )
}

fn do_save_score(db: &Mutex<Connection>, user_id: &str, game: &str, score: u32) {
    let ts = chrono::Utc::now().to_rfc3339();
    let conn = db.lock().unwrap();
    let _ = conn.execute(
        "INSERT INTO scores (user_id, game, score, ts) VALUES (?1, ?2, ?3, ?4)",
        params![user_id, game, score, ts],
    );
}

fn do_top_scores(db: &Mutex<Connection>, limit: u32) -> Vec<ScoreRow> {
    let conn = db.lock().unwrap();
    let mut stmt = match conn.prepare(
        "SELECT user_id, game, score, ts FROM scores s1
         WHERE score >= (
           SELECT score FROM scores s2
           WHERE s2.game = s1.game
           ORDER BY score DESC LIMIT 1 OFFSET ?1
         )
         OR (
           SELECT COUNT(*) FROM scores s2 WHERE s2.game = s1.game
         ) <= ?1
         ORDER BY game, score DESC",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("scores top prepare: {e}");
            return vec![];
        }
    };
    stmt.query_map([limit.saturating_sub(1) as i64], |row| {
        Ok(ScoreRow {
            user_id: row.get(0)?,
            game: row.get(1)?,
            score: row.get(2)?,
            ts: row.get(3)?,
        })
    })
    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
    .unwrap_or_else(|e| {
        tracing::error!("scores top query: {e}");
        vec![]
    })
}

fn do_recent_scores(db: &Mutex<Connection>, n: u32) -> Vec<ScoreRow> {
    let conn = db.lock().unwrap();
    let mut stmt = match conn
        .prepare("SELECT user_id, game, score, ts FROM scores ORDER BY ts DESC LIMIT ?1")
    {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("scores recent prepare: {e}");
            return vec![];
        }
    };
    stmt.query_map([n as i64], |row| {
        Ok(ScoreRow {
            user_id: row.get(0)?,
            game: row.get(1)?,
            score: row.get(2)?,
            ts: row.get(3)?,
        })
    })
    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
    .unwrap_or_else(|e| {
        tracing::error!("scores recent query: {e}");
        vec![]
    })
}

pub struct SqliteScoresStore {
    db: Arc<Mutex<Connection>>,
}

impl SqliteScoresStore {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }
}

impl ScoresStore for SqliteScoresStore {
    fn save_score(&self, user_id: &str, game: &str, score: u32) {
        do_save_score(&self.db, user_id, game, score);
    }

    fn top_scores(&self, limit: u32) -> Vec<ScoreRow> {
        do_top_scores(&self.db, limit)
    }

    fn recent_scores(&self, n: u32) -> Vec<ScoreRow> {
        do_recent_scores(&self.db, n)
    }
}

/// In-memory SQLite backend for use in tests.
#[cfg(test)]
pub struct InMemoryScoresStore {
    db: Arc<Mutex<Connection>>,
}

#[cfg(test)]
impl InMemoryScoresStore {
    pub fn new() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        Self {
            db: Arc::new(Mutex::new(conn)),
        }
    }

    /// Seed rows with explicit timestamps for deterministic ordering in tests.
    pub fn seed(&self, rows: &[(&str, &str, u32, &str)]) {
        let conn = self.db.lock().unwrap();
        for (user_id, game, score, ts) in rows {
            conn.execute(
                "INSERT INTO scores (user_id, game, score, ts) VALUES (?1, ?2, ?3, ?4)",
                params![user_id, game, score, ts],
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
impl ScoresStore for InMemoryScoresStore {
    fn save_score(&self, user_id: &str, game: &str, score: u32) {
        do_save_score(&self.db, user_id, game, score);
    }

    fn top_scores(&self, limit: u32) -> Vec<ScoreRow> {
        do_top_scores(&self.db, limit)
    }

    fn recent_scores(&self, n: u32) -> Vec<ScoreRow> {
        do_recent_scores(&self.db, n)
    }
}
