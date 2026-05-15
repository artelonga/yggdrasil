use std::sync::Mutex;

use axum::http::HeaderMap;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Query string `?variant=<slug>` para rotas de início de jogo (YG-37).
/// Slug `None` ou desconhecido → root default.
#[derive(Deserialize, Default)]
pub struct VariantQuery {
    pub variant: Option<String>,
}

/// Extrai `(user_id, email)` do JWT. Email vazio quando JWT ausente/inválido ou
/// claim sem email. Útil para lazy-upsert de `user_profiles` quando o usuário
/// ainda não passou por um novo `/auth/co-handover-receive` depois do deploy
/// que introduziu a tabela.
pub fn user_info_from_jwt(headers: &HeaderMap, jwt_secret: &str) -> (String, String) {
    let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return ("anonymous".to_string(), String::new());
    };
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    match jsonwebtoken::decode::<crate::auth::Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    ) {
        Ok(data) => (data.claims.sub, data.claims.email),
        Err(_) => ("anonymous".to_string(), String::new()),
    }
}

/// Cria/atualiza o perfil do usuário no DB se temos email. No-op se anonymous
/// ou sem email. Falhas são logadas mas não afetam o fluxo.
pub fn lazy_upsert_profile(conn: &Mutex<Connection>, user_id: &str, email: &str) {
    if user_id == "anonymous" || email.is_empty() {
        return;
    }
    let c = conn.lock().unwrap();
    if let Err(e) = crate::api::user_profiles::upsert(&c, user_id, email) {
        tracing::warn!("lazy_upsert_profile: {e}");
    }
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

#[derive(Deserialize)]
pub struct InputRequest {
    pub direction: String,
    // YG-N: user_id removed from body — now read from JWT Authorization header
    // via `user_id_from_jwt`. Old clients sending `user_id` are silently ignored
    // (serde default behavior for unknown fields).
}

pub fn map_to_value(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap_or(serde_json::Value::Null)
}

#[derive(Serialize)]
pub struct StartResponse {
    pub id: String,
    pub state: serde_json::Value,
    pub score: u32,
}

#[derive(Serialize)]
pub struct TickResponse {
    pub action: String,
    pub state: serde_json::Value,
    pub score: u32,
}
