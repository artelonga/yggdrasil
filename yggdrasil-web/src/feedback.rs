//! Fale conosco — canal de feedback por universo (e na raiz/lobby).
//!
//! Cada mensagem é feedback, dúvida ou sugestão, vinda de um usuário logado
//! (`user_sub` preenchido) ou anônima. Nome e e-mail são opcionais — pedidos
//! só para quem quiser receber resposta. Schema criado idempotentemente no
//! boot via [`init_feedback_db`]; queries em SQL direto via rusqlite.

use std::sync::{Arc, Mutex};

use nanoid::nanoid;
use rusqlite::{Connection, params};

/// Tipos de mensagem aceitos (allowlist; validado na rota).
pub const KINDS: [&str; 3] = ["feedback", "duvida", "sugestao"];

/// Migração idempotente. Seguro chamar em todo boot.
pub fn init_feedback_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS feedback (
            id          TEXT    PRIMARY KEY,
            universe    TEXT    NOT NULL,
            kind        TEXT    NOT NULL,
            message     TEXT    NOT NULL,
            name        TEXT,
            email       TEXT,
            user_sub    TEXT,
            anonymous   INTEGER NOT NULL,
            created_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_universe
            ON feedback(universe, created_at);
        CREATE INDEX IF NOT EXISTS idx_feedback_created
            ON feedback(created_at);",
    )
}

/// Dados de uma nova mensagem (já validados pela rota).
pub struct NewFeedback<'a> {
    pub universe: &'a str,
    pub kind: &'a str,
    pub message: &'a str,
    pub name: Option<&'a str>,
    pub email: Option<&'a str>,
    /// `Some(sub)` se o caller estava logado (JWT válido); `None` = anônimo.
    pub user_sub: Option<&'a str>,
}

pub struct FeedbackDb {
    db: Arc<Mutex<Connection>>,
}

impl FeedbackDb {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        init_feedback_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        init_feedback_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Insere a mensagem e devolve o `id` gerado.
    pub fn submit(&self, f: &NewFeedback) -> String {
        let id = nanoid!();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let anonymous = i64::from(f.user_sub.is_none());
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO feedback
             (id, universe, kind, message, name, email, user_sub, anonymous, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id, f.universe, f.kind, f.message, f.name, f.email, f.user_sub, anonymous, now_ms
            ],
        );
        id
    }
}

// ── Test helpers ─────────────────────────────────────────────────────────────

#[cfg(test)]
impl FeedbackDb {
    pub fn count(&self) -> i64 {
        let conn = self.db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM feedback", [], |r| r.get(0))
            .unwrap_or(0)
    }

    /// (universe, kind, anonymous, name, email, user_sub) da linha `id`.
    pub fn row(
        &self,
        id: &str,
    ) -> Option<(
        String,
        String,
        bool,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        let conn = self.db.lock().unwrap();
        conn.query_row(
            "SELECT universe, kind, anonymous, name, email, user_sub
             FROM feedback WHERE id = ?1",
            params![id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? == 1,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .ok()
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_idempotente() {
        let db = FeedbackDb::in_memory().unwrap();
        let conn = db.db.lock().unwrap();
        init_feedback_db(&conn).unwrap(); // segunda chamada não falha
    }

    #[test]
    fn submit_anonimo_marca_anonymous_e_user_sub_null() {
        let db = FeedbackDb::in_memory().unwrap();
        let id = db.submit(&NewFeedback {
            universe: "neuro",
            kind: "sugestao",
            message: "Adicionem o cerebelo ao atlas",
            name: Some("Maria"),
            email: Some("maria@example.com"),
            user_sub: None,
        });
        let (universe, kind, anon, name, email, sub) = db.row(&id).unwrap();
        assert_eq!(universe, "neuro");
        assert_eq!(kind, "sugestao");
        assert!(anon);
        assert_eq!(name.as_deref(), Some("Maria"));
        assert_eq!(email.as_deref(), Some("maria@example.com"));
        assert_eq!(sub, None);
        assert_eq!(db.count(), 1);
    }

    #[test]
    fn submit_logado_grava_user_sub_e_nao_anonimo() {
        let db = FeedbackDb::in_memory().unwrap();
        let id = db.submit(&NewFeedback {
            universe: "root",
            kind: "feedback",
            message: "Plataforma muito boa!",
            name: None,
            email: None,
            user_sub: Some("user-123"),
        });
        let (_, _, anon, _, _, sub) = db.row(&id).unwrap();
        assert!(!anon);
        assert_eq!(sub.as_deref(), Some("user-123"));
    }

    #[test]
    fn kinds_allowlist_tem_os_tres() {
        assert_eq!(KINDS, ["feedback", "duvida", "sugestao"]);
    }
}
