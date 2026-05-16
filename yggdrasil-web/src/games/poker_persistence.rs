//! Persistência de mesas de pôquer em SQLite (YG-29).
//!
//! Antes desta task, `PokerState` era `Mutex<Vec<PokerTable>>` puramente em
//! memória — restart do servidor (deploy, crash) zerava todas as mesas e os
//! buy-ins (1k sementes por sentada) desapareciam. Isto é inaceitável porque
//! sementes são moeda real persistida via [`Sementes`].
//!
//! Esta camada serializa `PokerTable` para a mesma SQLite controlada por
//! `YGGDRASIL_DB` (default `yggdrasil.db`). Só persistimos **lobby + stacks**;
//! mãos em curso (`PokerGame`) são forfeit num restart — escolha pragmática
//! documentada em [`PokerTableSnapshot`].
//!
//! Não modificamos `co/game-core::Storage` — abrimos conexão própria com
//! rusqlite, espelhando o padrão de `common::init_db` usado por
//! snake/tetris/invaders.
//!
//! Vide [`docs/POKER-MULTIPLAYER.md`](../../../docs/POKER-MULTIPLAYER.md#persistência-snapshots-em-sqlite).

use std::path::Path;

use rusqlite::{Connection, params};
use yggdrasil_core::games::poker_game::{PokerTable, PokerTableSnapshot};

/// Inicializa o schema de persistência de pôquer. Idempotente: pode ser
/// chamado em todo boot sem efeito.
pub fn init_poker_db(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS poker_lobbies (
            id    TEXT PRIMARY KEY,
            name  TEXT NOT NULL,
            state TEXT NOT NULL
        );",
    )?;
    Ok(conn)
}

/// Salva (UPSERT) o snapshot de uma mesa. Chamado após cada mutação que
/// muda seating ou stacks.
pub fn save_lobby(conn: &Connection, snap: &PokerTableSnapshot) -> rusqlite::Result<()> {
    let state_json = serde_json::to_string(snap)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    conn.execute(
        "INSERT INTO poker_lobbies (id, name, state) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, state = excluded.state",
        params![snap.lobby.id, snap.lobby.name, state_json],
    )?;
    Ok(())
}

/// Lê todas as mesas persistidas, ordenadas por id (estável para tests).
pub fn load_all(conn: &Connection) -> rusqlite::Result<Vec<PokerTableSnapshot>> {
    let mut stmt = conn.prepare("SELECT state FROM poker_lobbies ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        let state_json: String = row.get(0)?;
        serde_json::from_str::<PokerTableSnapshot>(&state_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Materializa as mesas persistidas como `PokerTable`. Conveniência para
/// `PokerState::new`. Devolve `Vec` vazio se a tabela existe mas está vazia.
pub fn load_tables(conn: &Connection) -> rusqlite::Result<Vec<PokerTable>> {
    Ok(load_all(conn)?
        .into_iter()
        .map(PokerTable::from_snapshot)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;
    use yggdrasil_core::games::poker_game::PokerTable;
    use yggdrasil_core::games::poker_lobby::{PokerLobby, SeatOccupant};
    use yggdrasil_core::sementes::Sementes;

    fn make_sementes() -> (Sementes, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let storage =
            Arc::new(game_core::storage::Storage::open(&dir.path().join("s.db")).unwrap());
        (Sementes::new(storage), dir)
    }

    #[test]
    fn init_cria_tabela_poker_lobbies() {
        let dir = tempdir().unwrap();
        let conn = init_poker_db(&dir.path().join("p.db")).unwrap();
        // Tabela existe — count(*) deve retornar 0 sem erro.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM poker_lobbies", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn save_e_load_round_trip() {
        let dir = tempdir().unwrap();
        let conn = init_poker_db(&dir.path().join("p.db")).unwrap();

        let mut table = PokerTable::new(PokerLobby::new("carvalho", "Mesa Carvalho"));
        let (sementes, _sdir) = make_sementes();
        sementes.creditar("alice", 5_000).unwrap();
        table.sit_with_sementes(2, "alice", &sementes).unwrap();

        let snap = table.to_snapshot();
        save_lobby(&conn, &snap).unwrap();

        let loaded = load_all(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].lobby.id, "carvalho");
        assert_eq!(loaded[0].lobby.name, "Mesa Carvalho");
        let seat = &loaded[0].lobby.seats[2];
        assert!(matches!(
            seat,
            SeatOccupant::Human { user_id, .. } if user_id == "alice"
        ));
        // Stack persistido.
        assert_eq!(loaded[0].stacks.get("alice").copied(), Some(1_000));
    }

    #[test]
    fn save_e_load_persiste_atraves_de_reconexao() {
        // Simula "kill -9": grava com um Connection, fecha, reabre, lê.
        let dir = tempdir().unwrap();
        let path = dir.path().join("p.db");

        {
            let conn = init_poker_db(&path).unwrap();
            let mut table = PokerTable::new(PokerLobby::new("olmo", "Mesa Olmo"));
            let (sementes, _sdir) = make_sementes();
            sementes.creditar("bob", 5_000).unwrap();
            table.sit_with_sementes(0, "bob", &sementes).unwrap();
            save_lobby(&conn, &table.to_snapshot()).unwrap();
        }

        // Reabre
        let conn2 = init_poker_db(&path).unwrap();
        let tables = load_tables(&conn2).unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].lobby.id, "olmo");
        assert_eq!(tables[0].stack_of("bob"), 1_000);
        // game sempre None após restore (mãos em curso são forfeit).
        assert!(tables[0].game.is_none());
    }

    #[test]
    fn save_upsert_substitui_estado_existente() {
        let dir = tempdir().unwrap();
        let conn = init_poker_db(&dir.path().join("p.db")).unwrap();

        let mut table = PokerTable::new(PokerLobby::new("carvalho", "Mesa Carvalho"));
        let (sementes, _sdir) = make_sementes();
        sementes.creditar("alice", 5_000).unwrap();

        save_lobby(&conn, &table.to_snapshot()).unwrap();
        // Mutação + segundo save.
        table.sit_with_sementes(1, "alice", &sementes).unwrap();
        save_lobby(&conn, &table.to_snapshot()).unwrap();

        let loaded = load_all(&conn).unwrap();
        assert_eq!(loaded.len(), 1, "UPSERT — uma só linha");
        assert_eq!(loaded[0].lobby.human_count(), 1);
    }
}
