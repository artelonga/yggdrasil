//! Telemetria — funnel_events + session_records + analytics.
//!
//! Schema criado de forma idempotente no boot via [`init_telemetry_db`].
//! Todas as queries usam SQL direto via rusqlite — sem ORM.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use nanoid::nanoid;
use rusqlite::{Connection, params};
use serde::Serialize;

/// Idempotent schema migration for telemetry tables. Safe to call on every boot.
pub fn init_telemetry_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS funnel_events (
            event_id    TEXT    PRIMARY KEY,
            user_id     TEXT,
            session_id  TEXT,
            universe_id TEXT,
            event_type  TEXT    NOT NULL,
            properties  TEXT,
            created_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_funnel_universe
            ON funnel_events(universe_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_funnel_user
            ON funnel_events(user_id, created_at);

        CREATE TABLE IF NOT EXISTS session_records (
            session_id  TEXT    PRIMARY KEY,
            user_id     TEXT,
            universe_id TEXT    NOT NULL,
            started_at  INTEGER NOT NULL,
            ended_at    INTEGER,
            duration_ms INTEGER,
            final_score INTEGER,
            abandoned   INTEGER NOT NULL DEFAULT 0
        );",
    )
}

pub struct TelemetriaDb {
    db: Arc<Mutex<Connection>>,
}

impl TelemetriaDb {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        init_telemetry_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        init_telemetry_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    fn emit_event(
        &self,
        event_type: &str,
        user_id: Option<&str>,
        session_id: Option<&str>,
        universe_id: Option<&str>,
        properties: Option<&str>,
    ) {
        let event_id = nanoid!();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO funnel_events
             (event_id, user_id, session_id, universe_id, event_type, properties, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event_id,
                user_id,
                session_id,
                universe_id,
                event_type,
                properties,
                now_ms
            ],
        );
    }

    /// Records SESSION_CREATE event and inserts into session_records.
    pub fn session_create(&self, session_id: &str, universe_id: &str, started_at: i64) {
        self.emit_event(
            "SESSION_CREATE",
            None,
            Some(session_id),
            Some(universe_id),
            None,
        );
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR IGNORE INTO session_records
             (session_id, universe_id, started_at, abandoned)
             VALUES (?1, ?2, ?3, 0)",
            params![session_id, universe_id, started_at],
        );
    }

    /// Records SESSION_COMPLETE event and updates session_records with score and duration.
    pub fn session_complete(
        &self,
        session_id: &str,
        universe_id: &str,
        ended_at: i64,
        duration_ms: i64,
        final_score: u32,
    ) {
        let props = serde_json::json!({ "final_score": final_score }).to_string();
        self.emit_event(
            "SESSION_COMPLETE",
            None,
            Some(session_id),
            Some(universe_id),
            Some(&props),
        );
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "UPDATE session_records
             SET ended_at = ?1, duration_ms = ?2, final_score = ?3
             WHERE session_id = ?4",
            params![ended_at, duration_ms, final_score as i64, session_id],
        );
    }

    /// Records SESSION_ABANDON event and marks session_records.abandoned = 1.
    pub fn session_abandon(&self, session_id: &str, universe_id: &str, ended_at: i64) {
        self.emit_event(
            "SESSION_ABANDON",
            None,
            Some(session_id),
            Some(universe_id),
            None,
        );
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "UPDATE session_records
             SET ended_at = ?1, abandoned = 1
             WHERE session_id = ?2 AND ended_at IS NULL",
            params![ended_at, session_id],
        );
    }

    /// Records UNIVERSE_VIEW event.
    pub fn universe_view(&self, universe_id: &str) {
        self.emit_event("UNIVERSE_VIEW", None, None, Some(universe_id), None);
    }

    /// Contagem pública de sessões iniciadas desde `since_ms` (para stats
    /// anônimas da landing). Só um número agregado — sem PII.
    pub fn sessions_since(&self, since_ms: i64) -> i64 {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM session_records WHERE started_at >= ?1",
            params![since_ms],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Retenção (YG-128, espelha os 90 dias do CO): apaga eventos/sessões mais
    /// antigos que `cutoff_ms`. Devolve (eventos, sessões) removidos. Os
    /// agregados históricos sobrevivem nos rollups diários enviados ao CO.
    pub fn cleanup_older_than(&self, cutoff_ms: i64) -> (usize, usize) {
        let conn = self.db.lock().unwrap();
        let ev = conn
            .execute(
                "DELETE FROM funnel_events WHERE created_at < ?1",
                params![cutoff_ms],
            )
            .unwrap_or(0);
        let ses = conn
            .execute(
                "DELETE FROM session_records WHERE started_at < ?1",
                params![cutoff_ms],
            )
            .unwrap_or(0);
        (ev, ses)
    }

    /// Returns analytics report covering the window [since_ms, now).
    pub fn get_analytics(
        &self,
        since_ms: i64,
        active_counts: &HashMap<String, i64>,
    ) -> AnalyticsReport {
        let generated_at = chrono::Utc::now().to_rfc3339();
        let conn = self.db.lock().unwrap();

        let universo_ids = ["snake", "tetris", "invaders", "vim"];
        let universos: Vec<UniversoStats> = universo_ids
            .iter()
            .map(|&uid| {
                let sessions_today: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM session_records
                         WHERE universe_id = ?1 AND started_at >= ?2",
                        params![uid, since_ms],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);

                let completions_today: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM session_records
                         WHERE universe_id = ?1 AND started_at >= ?2
                           AND ended_at IS NOT NULL AND abandoned = 0",
                        params![uid, since_ms],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);

                let completion_rate_pct = if sessions_today > 0 {
                    completions_today * 100 / sessions_today
                } else {
                    0
                };

                let median_duration_ms: i64 = conn
                    .prepare(
                        "SELECT duration_ms FROM session_records
                         WHERE universe_id = ?1 AND started_at >= ?2
                           AND duration_ms IS NOT NULL
                         ORDER BY duration_ms",
                    )
                    .and_then(|mut stmt| {
                        stmt.query_map(params![uid, since_ms], |r| r.get::<_, i64>(0))
                            .and_then(|m| m.collect::<rusqlite::Result<Vec<_>>>())
                    })
                    .map(|durations| {
                        if durations.is_empty() {
                            0
                        } else {
                            durations[durations.len() / 2]
                        }
                    })
                    .unwrap_or(0);

                let active_now = *active_counts.get(uid).unwrap_or(&0);

                UniversoStats {
                    id: uid.to_string(),
                    sessions_today,
                    completions_today,
                    completion_rate_pct,
                    median_duration_ms,
                    active_now,
                }
            })
            .collect();

        let universe_views: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM funnel_events
                 WHERE event_type = 'UNIVERSE_VIEW' AND created_at >= ?1",
                params![since_ms],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let session_creates: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM funnel_events
                 WHERE event_type = 'SESSION_CREATE' AND created_at >= ?1",
                params![since_ms],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let session_completions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM funnel_events
                 WHERE event_type = 'SESSION_COMPLETE' AND created_at >= ?1",
                params![since_ms],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let conversion_view_to_create_pct = if universe_views > 0 {
            session_creates * 100 / universe_views
        } else {
            0
        };

        let conversion_create_to_complete_pct = if session_creates > 0 {
            session_completions * 100 / session_creates
        } else {
            0
        };

        AnalyticsReport {
            generated_at,
            period: "24h".to_string(),
            universos,
            funnel_24h: FunnelStats {
                universe_views,
                session_creates,
                session_completions,
                conversion_view_to_create_pct,
                conversion_create_to_complete_pct,
            },
        }
    }
}

#[derive(Serialize)]
pub struct UniversoStats {
    pub id: String,
    pub sessions_today: i64,
    pub completions_today: i64,
    pub completion_rate_pct: i64,
    pub median_duration_ms: i64,
    pub active_now: i64,
}

#[derive(Serialize)]
pub struct FunnelStats {
    pub universe_views: i64,
    pub session_creates: i64,
    pub session_completions: i64,
    pub conversion_view_to_create_pct: i64,
    pub conversion_create_to_complete_pct: i64,
}

#[derive(Serialize)]
pub struct AnalyticsReport {
    pub generated_at: String,
    pub period: String,
    pub universos: Vec<UniversoStats>,
    pub funnel_24h: FunnelStats,
}

// ── Test helpers ─────────────────────────────────────────────────────────────

#[cfg(test)]
impl TelemetriaDb {
    pub fn count_events(&self, event_type: &str) -> i64 {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM funnel_events WHERE event_type = ?1",
            params![event_type],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    pub fn count_session_records(&self) -> i64 {
        let conn = self.db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM session_records", [], |r| r.get(0))
            .unwrap_or(0)
    }

    pub fn session_abandoned(&self, session_id: &str) -> bool {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT abandoned FROM session_records WHERE session_id = ?1",
            params![session_id],
            |r| r.get::<_, i64>(0),
        )
        .map(|v| v == 1)
        .unwrap_or(false)
    }

    pub fn session_duration(&self, session_id: &str) -> Option<i64> {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT duration_ms FROM session_records WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .ok()
    }

    pub fn session_score(&self, session_id: &str) -> Option<i64> {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT final_score FROM session_records WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        )
        .ok()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    // ── Schema idempotency ───────────────────────────────────────────────────

    #[test]
    fn init_telemetry_db_idempotente() {
        let db = TelemetriaDb::in_memory().unwrap();
        let conn = db.db.lock().unwrap();
        // Second call must not fail
        init_telemetry_db(&conn).unwrap();
    }

    #[test]
    fn init_telemetry_db_com_arquivo_e_idempotente() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        init_telemetry_db(&conn).unwrap();
        init_telemetry_db(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM funnel_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    // ── SESSION_CREATE ───────────────────────────────────────────────────────

    #[test]
    fn session_create_insere_evento_e_record() {
        let db = TelemetriaDb::in_memory().unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        db.session_create("sid-1", "snake", now_ms);

        assert_eq!(db.count_events("SESSION_CREATE"), 1);
        assert_eq!(db.count_session_records(), 1);
    }

    #[test]
    fn session_create_idempotente_nao_duplica_record() {
        let db = TelemetriaDb::in_memory().unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        db.session_create("sid-dup", "snake", now_ms);
        db.session_create("sid-dup", "snake", now_ms);

        // Two CREATE events but only one session_record (INSERT OR IGNORE)
        assert_eq!(db.count_events("SESSION_CREATE"), 2);
        assert_eq!(db.count_session_records(), 1);
    }

    // ── SESSION_COMPLETE ─────────────────────────────────────────────────────

    #[test]
    fn session_complete_atualiza_record_com_score_e_duration() {
        let db = TelemetriaDb::in_memory().unwrap();
        let started_at = chrono::Utc::now().timestamp_millis();
        db.session_create("sid-2", "tetris", started_at);

        let ended_at = started_at + 5_000;
        db.session_complete("sid-2", "tetris", ended_at, 5_000, 42);

        assert_eq!(db.count_events("SESSION_COMPLETE"), 1);
        assert_eq!(db.session_duration("sid-2"), Some(5_000));
        assert_eq!(db.session_score("sid-2"), Some(42));
        assert!(!db.session_abandoned("sid-2"));
    }

    // ── SESSION_ABANDON ──────────────────────────────────────────────────────

    #[test]
    fn session_abandon_marca_abandoned_1() {
        let db = TelemetriaDb::in_memory().unwrap();
        let started_at = chrono::Utc::now().timestamp_millis();
        db.session_create("sid-3", "snake", started_at);
        db.session_abandon("sid-3", "snake", started_at + 1_000);

        assert_eq!(db.count_events("SESSION_ABANDON"), 1);
        assert!(db.session_abandoned("sid-3"));
    }

    #[test]
    fn session_abandon_nao_sobreescreve_sessao_completa() {
        let db = TelemetriaDb::in_memory().unwrap();
        let started_at = chrono::Utc::now().timestamp_millis();
        db.session_create("sid-4", "snake", started_at);
        db.session_complete("sid-4", "snake", started_at + 2_000, 2_000, 10);
        // Abandon called after complete should have no effect on ended_at
        db.session_abandon("sid-4", "snake", started_at + 99_999);

        // Still not abandoned (WHERE ended_at IS NULL filtered it out)
        assert!(!db.session_abandoned("sid-4"));
    }

    // ── UNIVERSE_VIEW ────────────────────────────────────────────────────────

    #[test]
    fn sessions_since_conta_apenas_dentro_da_janela() {
        let db = TelemetriaDb::in_memory().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        db.session_create("s-old", "snake", now - 48 * 60 * 60 * 1000); // fora de 24h
        db.session_create("s-new", "tetris", now - 60 * 1000); // dentro
        let since = now - 24 * 60 * 60 * 1000;
        assert_eq!(db.sessions_since(since), 1);
    }

    #[test]
    fn universe_view_registra_evento() {
        let db = TelemetriaDb::in_memory().unwrap();
        db.universe_view("invaders");
        db.universe_view("invaders");

        assert_eq!(db.count_events("UNIVERSE_VIEW"), 2);
    }

    // ── get_analytics ────────────────────────────────────────────────────────

    #[test]
    fn get_analytics_retorna_estrutura_correta() {
        let db = TelemetriaDb::in_memory().unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let since_ms = now_ms - 24 * 60 * 60 * 1000;

        db.universe_view("snake");
        db.universe_view("snake");
        db.session_create("s1", "snake", now_ms - 1_000);
        db.session_complete("s1", "snake", now_ms, 1_000, 10);
        db.session_create("s2", "snake", now_ms - 2_000);
        db.session_abandon("s2", "snake", now_ms - 500);

        let report = db.get_analytics(since_ms, &HashMap::new());

        assert_eq!(report.period, "24h");
        assert!(!report.generated_at.is_empty());
        assert_eq!(report.universos.len(), 4);

        let snake = report.universos.iter().find(|u| u.id == "snake").unwrap();
        assert_eq!(snake.sessions_today, 2);
        assert_eq!(snake.completions_today, 1);
        assert_eq!(snake.completion_rate_pct, 50);
        assert_eq!(snake.median_duration_ms, 1_000);

        assert_eq!(report.funnel_24h.universe_views, 2);
        assert_eq!(report.funnel_24h.session_creates, 2);
        assert_eq!(report.funnel_24h.session_completions, 1);
        assert_eq!(report.funnel_24h.conversion_view_to_create_pct, 100);
        assert_eq!(report.funnel_24h.conversion_create_to_complete_pct, 50);
    }

    #[test]
    fn get_analytics_active_now_vem_dos_counts_passados() {
        let db = TelemetriaDb::in_memory().unwrap();
        let since_ms = chrono::Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000;
        let mut counts = HashMap::new();
        counts.insert("snake".to_string(), 3i64);

        let report = db.get_analytics(since_ms, &counts);
        let snake = report.universos.iter().find(|u| u.id == "snake").unwrap();
        assert_eq!(snake.active_now, 3);
    }

    // ── Performance: < 50ms with 10k rows ───────────────────────────────────

    #[test]
    fn analytics_query_rapida_com_10k_registros() {
        let db = TelemetriaDb::in_memory().unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let since_ms = now_ms - 24 * 60 * 60 * 1000;

        {
            let conn = db.db.lock().unwrap();
            conn.execute_batch("BEGIN").unwrap();
            for i in 0..10_000i64 {
                let event_id = format!("ev-{i}");
                let event_type = match i % 3 {
                    0 => "UNIVERSE_VIEW",
                    1 => "SESSION_CREATE",
                    _ => "SESSION_COMPLETE",
                };
                conn.execute(
                    "INSERT INTO funnel_events
                     (event_id, universe_id, event_type, created_at)
                     VALUES (?1, 'snake', ?2, ?3)",
                    rusqlite::params![event_id, event_type, now_ms - i * 1_000],
                )
                .unwrap();
            }
            conn.execute_batch("COMMIT").unwrap();
        }

        let start = std::time::Instant::now();
        let report = db.get_analytics(since_ms, &HashMap::new());
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "analytics took {elapsed:?} > 50ms"
        );
        assert!(report.funnel_24h.session_creates > 0);
    }

    #[test]
    fn cleanup_remove_so_o_que_passou_do_corte() {
        let t = TelemetriaDb::in_memory().unwrap();
        t.session_create("velha", "snake", 1_000);
        t.session_create("nova", "snake", 100_000);
        let (_ev, ses) = t.cleanup_older_than(50_000);
        assert_eq!(ses, 1, "só a sessão antiga sai");
        assert_eq!(t.sessions_since(0), 1, "a nova fica");
    }
}
