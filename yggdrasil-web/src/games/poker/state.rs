use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};

use yggdrasil_core::games::poker_game::PokerTable;
use yggdrasil_core::games::poker_lobby::PokerLobby;
use yggdrasil_core::sementes::Sementes;

use crate::games::poker_persistence;

pub struct PokerState {
    pub jwt_secret: String,
    pub tables: Mutex<Vec<PokerTable>>,
    pub sementes: Arc<Sementes>,
    /// Caminho do SQLite onde mesas são persistidas (YG-29).
    /// `None` desativa persistência — usado em testes in-memory.
    pub db_path: Option<PathBuf>,
}

impl PokerState {
    fn seed_tables() -> Vec<PokerTable> {
        vec![
            PokerTable::new(PokerLobby::new("carvalho", "Mesa Carvalho")),
            PokerTable::new(PokerLobby::new("olmo", "Mesa Olmo")),
            // YG-37 variante `poker/heads-up`: mesa 2-seats para duelos.
            PokerTable::new(PokerLobby::with_max_seats(
                "heads-up",
                "Heads-Up Carvalho",
                2,
            )),
        ]
    }

    /// Boot in-memory (sem persistência) — usado em testes que não precisam
    /// validar restart, e como fallback se o DB falhar.
    #[allow(dead_code)]
    pub fn new(jwt_secret: String, sementes: Arc<Sementes>) -> Self {
        Self {
            jwt_secret,
            tables: Mutex::new(Self::seed_tables()),
            sementes,
            db_path: None,
        }
    }

    /// Boot com persistência em SQLite (YG-29). No primeiro run, semeia
    /// 3 mesas defaults e grava no DB. Em boots subsequentes, restaura as
    /// mesas persistidas — seating e chip-stacks sobrevivem ao restart;
    /// mãos em curso são forfeit.
    pub fn with_persistence(jwt_secret: String, sementes: Arc<Sementes>, db_path: &FsPath) -> Self {
        let tables = match poker_persistence::init_poker_db(db_path) {
            Ok(conn) => match poker_persistence::load_tables(&conn) {
                Ok(loaded) if !loaded.is_empty() => loaded,
                Ok(_) => {
                    let mut seeds = Self::seed_tables();
                    for t in seeds.iter_mut() {
                        if let Err(e) = poker_persistence::save_lobby(&conn, &t.to_snapshot()) {
                            tracing::warn!("poker_persistence seed save falhou: {e}");
                        }
                    }
                    seeds
                }
                Err(e) => {
                    tracing::warn!("poker_persistence load falhou: {e} — usando seeds em memória");
                    Self::seed_tables()
                }
            },
            Err(e) => {
                tracing::warn!("poker_persistence init falhou: {e} — usando seeds em memória");
                Self::seed_tables()
            }
        };
        Self {
            jwt_secret,
            tables: Mutex::new(tables),
            sementes,
            db_path: Some(db_path.to_path_buf()),
        }
    }

    /// Persiste o snapshot de uma mesa. Idempotente. Falhas viram warning.
    pub fn persist_table(&self, table: &mut PokerTable) {
        let Some(ref path) = self.db_path else {
            return;
        };
        let snap = table.to_snapshot();
        match poker_persistence::init_poker_db(path) {
            Ok(conn) => {
                if let Err(e) = poker_persistence::save_lobby(&conn, &snap) {
                    tracing::warn!("poker_persistence save falhou: {e}");
                }
            }
            Err(e) => tracing::warn!("poker_persistence open falhou: {e}"),
        }
    }
}
