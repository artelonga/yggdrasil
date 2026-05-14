//! User profile lookup — mapeia `user_id` (do CO) para `username` legível.
//!
//! O user_id é um identificador opaco (`usr_3ded7630`) que aparece em scores,
//! poker seats, etc. O usuário humano prefere ver "yuri" — slug do email.
//! Esta tabela é populada no `/auth/co-handover-receive` (quando temos o
//! email do CO) e lida pelas rotas que renderizam nomes (scores, poker UI).

use std::path::Path;

use rusqlite::Connection;

/// Inicializa schema. Idempotente.
pub fn init_db(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS user_profiles (
            user_id    TEXT PRIMARY KEY,
            email      TEXT,
            username   TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
         CREATE INDEX IF NOT EXISTS idx_user_profiles_username
             ON user_profiles(username);",
    )?;
    Ok(conn)
}

/// Slugifica o prefixo do email para virar username. Letras + dígitos + `_`/`-`/`.`.
/// `yuri@artelonga.com.br` → `yuri`. `Maria-João@x.com` → `maria-jo`.
pub fn slug_from_email(email: &str) -> String {
    let prefix = email.split('@').next().unwrap_or("anon");
    let s: String = prefix
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if s.is_empty() { "anon".to_string() } else { s }
}

/// Cria ou atualiza o perfil. Chamado no `/auth/co-handover-receive`.
pub fn upsert(conn: &Connection, user_id: &str, email: &str) -> rusqlite::Result<String> {
    let username = slug_from_email(email);
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO user_profiles (user_id, email, username, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id) DO UPDATE SET
             email = excluded.email,
             username = excluded.username,
             updated_at = excluded.updated_at",
        rusqlite::params![user_id, email, username, now],
    )?;
    Ok(username)
}

/// Lookup um único user_id → username. Retorna o próprio user_id se não há perfil.
pub fn username_or_id(conn: &Connection, user_id: &str) -> String {
    let result: rusqlite::Result<String> = conn.query_row(
        "SELECT username FROM user_profiles WHERE user_id = ?1",
        [user_id],
        |row| row.get(0),
    );
    result.unwrap_or_else(|_| user_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn slug_strips_email_domain_and_lowercases() {
        assert_eq!(slug_from_email("yuri@artelonga.com.br"), "yuri");
        // Accents (ã) filtered, demais letras preservadas → "maria-joo".
        assert_eq!(slug_from_email("Maria-João@x.com"), "maria-joo");
        assert_eq!(slug_from_email("a.b.c@x"), "a.b.c");
        assert_eq!(slug_from_email(""), "anon");
        assert_eq!(slug_from_email("!!!@x"), "anon");
    }

    #[test]
    fn upsert_creates_then_updates() {
        let dir = tempdir().unwrap();
        let conn = init_db(&dir.path().join("t.db")).unwrap();
        let u = upsert(&conn, "usr_1", "yuri@artelonga.com.br").unwrap();
        assert_eq!(u, "yuri");
        // Second call with different email overwrites.
        let u2 = upsert(&conn, "usr_1", "different@x.com").unwrap();
        assert_eq!(u2, "different");
        assert_eq!(username_or_id(&conn, "usr_1"), "different");
    }

    #[test]
    fn username_or_id_fallback() {
        let dir = tempdir().unwrap();
        let conn = init_db(&dir.path().join("t.db")).unwrap();
        // No row → returns the user_id verbatim.
        assert_eq!(username_or_id(&conn, "usr_unknown"), "usr_unknown");
    }
}
