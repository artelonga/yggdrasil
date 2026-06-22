//! Passkeys (WebAuthn) — login biométrico/security-key real, self-contained no
//! Yggdrasil (YG-174). O usuário registra um passkey amarrado ao seu `sub`
//! (estando logado via CO/email); depois autentica com Face ID / digital /
//! chave de segurança e o Yggdrasil **emite o próprio JWT** (mesmo `sign_jwt`
//! do handover do CO). Sem round-trip ao CO no login por passkey.
//!
//! Guardamos a credencial (`Passkey` serializada) amarrada a `user_sub`+`email`.
//! O estado efêmero das cerimônias (registration/authentication) vive em memória
//! na [`crate::api::passkey::PasskeyState`], não aqui.

use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};
use webauthn_rs::prelude::{CredentialID, Passkey};

/// Migração idempotente. Seguro chamar em todo boot.
pub fn init_passkey_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS passkeys (
            cred_id     TEXT PRIMARY KEY,
            user_sub    TEXT NOT NULL,
            email       TEXT NOT NULL,
            passkey     TEXT NOT NULL,
            label       TEXT,
            created_at  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_passkeys_email ON passkeys(email);
        CREATE INDEX IF NOT EXISTS idx_passkeys_sub ON passkeys(user_sub);",
    )
}

/// Identidade do dono de uma credencial (para emitir o JWT no login).
pub struct Owner {
    pub sub: String,
    pub email: String,
}

pub struct PasskeyDb {
    db: Arc<Mutex<Connection>>,
}

/// Base64-url (sem padding) do CredentialID — usado como chave estável no DB.
fn cred_key(id: &CredentialID) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id.as_ref())
}

impl PasskeyDb {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        init_passkey_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        init_passkey_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Grava uma credencial recém-registrada, amarrada a `sub`+`email`.
    pub fn save(&self, sub: &str, email: &str, pk: &Passkey, label: Option<&str>) {
        let key = cred_key(pk.cred_id());
        let json = match serde_json::to_string(pk) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("serialize passkey: {e}");
                return;
            }
        };
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR REPLACE INTO passkeys (cred_id, user_sub, email, passkey, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![key, sub, email, json, label, now],
        );
    }

    /// Passkeys de um e-mail (para iniciar a autenticação). Vazio se nenhum.
    pub fn for_email(&self, email: &str) -> Vec<Passkey> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT passkey FROM passkeys WHERE email = ?1") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![email], |r| r.get::<_, String>(0));
        match rows {
            Ok(it) => it
                .filter_map(Result::ok)
                .filter_map(|j| serde_json::from_str::<Passkey>(&j).ok())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Dono (sub/email) + a credencial guardada, pelo id (após autenticar) —
    /// a `Passkey` é necessária para atualizar o contador de assinatura.
    pub fn owner_of(&self, cred_id: &CredentialID) -> Option<(Owner, Passkey)> {
        let key = cred_key(cred_id);
        let conn = self.db.lock().unwrap();
        let (sub, email, json): (String, String, String) = conn
            .query_row(
                "SELECT user_sub, email, passkey FROM passkeys WHERE cred_id = ?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok()?;
        let pk = serde_json::from_str::<Passkey>(&json).ok()?;
        Some((Owner { sub, email }, pk))
    }

    /// Atualiza a credencial (ex.: contador de assinatura após login). Idempotente.
    pub fn update(&self, pk: &Passkey) {
        let key = cred_key(pk.cred_id());
        if let Ok(json) = serde_json::to_string(pk) {
            let conn = self.db.lock().unwrap();
            let _ = conn.execute(
                "UPDATE passkeys SET passkey = ?1 WHERE cred_id = ?2",
                params![json, key],
            );
        }
    }

    #[cfg(test)]
    pub fn count(&self) -> i64 {
        let conn = self.db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM passkeys", [], |r| r.get(0))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_idempotente() {
        let db = PasskeyDb::in_memory().unwrap();
        let conn = db.db.lock().unwrap();
        init_passkey_db(&conn).unwrap();
    }

    #[test]
    fn for_email_vazio_quando_sem_passkey() {
        let db = PasskeyDb::in_memory().unwrap();
        assert!(db.for_email("ninguem@x.com").is_empty());
        assert_eq!(db.count(), 0);
    }
}
