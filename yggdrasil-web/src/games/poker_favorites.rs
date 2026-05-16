//! Favorite hands — persiste o snapshot final de uma mão de pôquer para
//! revisão posterior.
//!
//! Não é replay step-by-step (ainda) — apenas o estado de showdown: community
//! cards, jogadores (com hole_cards revelados), pot, winner_message. Suficiente
//! para o usuário voltar a olhar uma mão "memorável" (e.g. dois pares jacks e
//! cincos) e mostrar para amigos.
//!
//! Cada mão é capturada **automaticamente** no fim de cada partida e fica
//! disponível por 1 hora ou até ser favoritada explicitamente. Após
//! favoritar (POST /api/v1/me/favorites/hands/{id}), persiste em SQLite e
//! sobrevive para sempre.
//!
//! Captura via [`capture_hand_snapshot`](super::poker_routes) no fim de cada ação
//! que termina mão. Vide [`docs/POKER-MULTIPLAYER.md`](../../../docs/POKER-MULTIPLAYER.md#mapa-de-métodos-server-side).

use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Estado capturado no fim de uma mão. JSON-serializável.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandSnapshot {
    pub hand_id: String,
    pub table_id: String,
    pub ended_at: i64, // unix epoch seconds
    pub winner_message: Option<String>,
    pub community_cards: Vec<CardJson>,
    pub players: Vec<PlayerSnapshot>,
    pub pot: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardJson {
    pub rank: String,
    pub suit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub user_id: String,
    pub chips: u32,
    pub folded: bool,
    /// Cartas hole reveladas no showdown (None se foldou antes).
    pub hole_cards: Option<[CardJson; 2]>,
}

pub fn init_favorites_db(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS poker_recent_hands (
            hand_id TEXT PRIMARY KEY,
            table_id TEXT NOT NULL,
            ended_at INTEGER NOT NULL,
            snapshot TEXT NOT NULL
        );
         CREATE TABLE IF NOT EXISTS poker_favorite_hands (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            hand_id TEXT NOT NULL,
            favorited_at INTEGER NOT NULL,
            snapshot TEXT NOT NULL,
            UNIQUE(user_id, hand_id)
         );",
    )?;
    Ok(conn)
}

/// Salva snapshot de uma mão recém-encerrada na tabela `poker_recent_hands`.
/// Garante TTL de 1h removendo entries mais velhas a cada save.
pub fn save_recent(conn: &Connection, snap: &HandSnapshot) -> rusqlite::Result<()> {
    let json = serde_json::to_string(snap).unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO poker_recent_hands (hand_id, table_id, ended_at, snapshot)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![snap.hand_id, snap.table_id, snap.ended_at, json],
    )?;
    // Garbage collect entries > 1 hora (3600 seg)
    let cutoff = chrono::Utc::now().timestamp() - 3600;
    let _ = conn.execute(
        "DELETE FROM poker_recent_hands WHERE ended_at < ?1",
        rusqlite::params![cutoff],
    );
    Ok(())
}

/// Recupera o snapshot recente mais recente para uma mesa. Usado por
/// "salvar última mão" sem o cliente precisar saber o hand_id.
pub fn latest_for_table(
    conn: &Connection,
    table_id: &str,
) -> rusqlite::Result<Option<HandSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT snapshot FROM poker_recent_hands WHERE table_id = ?1
         ORDER BY ended_at DESC LIMIT 1",
    )?;
    let mut rows = stmt.query([table_id])?;
    if let Some(row) = rows.next()? {
        let json: String = row.get(0)?;
        if let Ok(snap) = serde_json::from_str::<HandSnapshot>(&json) {
            return Ok(Some(snap));
        }
    }
    Ok(None)
}

/// Marca uma mão como favorita do usuário. Idempotente (UNIQUE).
pub fn favorite(conn: &Connection, user_id: &str, snap: &HandSnapshot) -> rusqlite::Result<()> {
    let json = serde_json::to_string(snap).unwrap_or_default();
    conn.execute(
        "INSERT OR IGNORE INTO poker_favorite_hands (user_id, hand_id, favorited_at, snapshot)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![user_id, snap.hand_id, chrono::Utc::now().timestamp(), json],
    )?;
    Ok(())
}

/// Lista as mãos favoritas do usuário em ordem cronológica decrescente.
pub fn list_favorites(conn: &Connection, user_id: &str) -> rusqlite::Result<Vec<HandSnapshot>> {
    let mut stmt = conn.prepare(
        "SELECT snapshot FROM poker_favorite_hands WHERE user_id = ?1
         ORDER BY favorited_at DESC LIMIT 50",
    )?;
    let rows = stmt.query_map([user_id], |row| {
        let json: String = row.get(0)?;
        Ok(serde_json::from_str::<HandSnapshot>(&json).ok())
    })?;
    Ok(rows.flatten().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn mk_snap(hand_id: &str, table_id: &str) -> HandSnapshot {
        HandSnapshot {
            hand_id: hand_id.to_string(),
            table_id: table_id.to_string(),
            ended_at: chrono::Utc::now().timestamp(),
            winner_message: Some("yuri venceu com dois pares".to_string()),
            community_cards: vec![
                CardJson {
                    rank: "J".into(),
                    suit: "hearts".into(),
                },
                CardJson {
                    rank: "5".into(),
                    suit: "spades".into(),
                },
            ],
            players: vec![PlayerSnapshot {
                user_id: "yuri".into(),
                chips: 2000,
                folded: false,
                hole_cards: Some([
                    CardJson {
                        rank: "J".into(),
                        suit: "clubs".into(),
                    },
                    CardJson {
                        rank: "5".into(),
                        suit: "diamonds".into(),
                    },
                ]),
            }],
            pot: 200,
        }
    }

    #[test]
    fn save_recent_and_latest_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = init_favorites_db(&path).unwrap();
        let snap = mk_snap("hand-1", "carvalho");
        save_recent(&conn, &snap).unwrap();
        let got = latest_for_table(&conn, "carvalho").unwrap().unwrap();
        assert_eq!(got.hand_id, "hand-1");
        assert_eq!(got.community_cards.len(), 2);
    }

    #[test]
    fn favorite_idempotente() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = init_favorites_db(&path).unwrap();
        let snap = mk_snap("hand-1", "carvalho");
        favorite(&conn, "yuri", &snap).unwrap();
        favorite(&conn, "yuri", &snap).unwrap(); // segunda vez = no-op
        let favs = list_favorites(&conn, "yuri").unwrap();
        assert_eq!(favs.len(), 1);
    }

    #[test]
    fn list_favorites_isola_por_usuario() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = init_favorites_db(&path).unwrap();
        favorite(&conn, "yuri", &mk_snap("h1", "carvalho")).unwrap();
        favorite(&conn, "bob", &mk_snap("h2", "olmo")).unwrap();
        assert_eq!(list_favorites(&conn, "yuri").unwrap().len(), 1);
        assert_eq!(list_favorites(&conn, "bob").unwrap().len(), 1);
        assert_eq!(list_favorites(&conn, "ninguem").unwrap().len(), 0);
    }

    #[test]
    fn save_recent_remove_entries_velhas() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = init_favorites_db(&path).unwrap();
        // Insert old entry directly.
        let old = HandSnapshot {
            ended_at: chrono::Utc::now().timestamp() - 7200, // 2h atrás
            ..mk_snap("old", "carvalho")
        };
        conn.execute(
            "INSERT INTO poker_recent_hands (hand_id, table_id, ended_at, snapshot)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![old.hand_id, old.table_id, old.ended_at, "{}"],
        )
        .unwrap();
        // Now a fresh save should garbage-collect.
        save_recent(&conn, &mk_snap("fresh", "carvalho")).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM poker_recent_hands", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
