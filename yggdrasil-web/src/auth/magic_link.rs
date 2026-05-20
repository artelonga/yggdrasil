use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::jwt::sign_jwt;
use super::state::AuthState;

const RATE_LIMIT_MAX: usize = 3;
const RATE_LIMIT_WINDOW_SECS: i64 = 900;
const CODE_EXPIRY_SECS: i64 = 300;
const MAX_ATTEMPTS: u32 = 3;

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CodeRequest {
    pub email: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct VerifyRequest {
    pub email: String,
    pub code: String,
}

pub fn generate_code() -> String {
    let bytes = Uuid::new_v4();
    let n = u32::from_le_bytes(bytes.as_bytes()[..4].try_into().unwrap());
    format!("{n:06}", n = n % 1_000_000)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/code",
    request_body = CodeRequest,
    responses(
        (status = 200, description = "Código enviado por e-mail (ou ignorado se rate-limited)"),
    ),
    tag = "auth"
)]
pub async fn request_code(
    State(state): State<Arc<AuthState>>,
    Json(req): Json<CodeRequest>,
) -> impl IntoResponse {
    let email = req.email.to_lowercase();

    let result = {
        let db = state.db.lock().unwrap();
        do_request_code(&db, &email)
    };

    match result {
        Ok(Some(code)) => {
            let body =
                format!("Seu código de acesso ao Yggdrasil é: {code}\nEle expira em 5 minutos.");
            let _ = state
                .mail
                .send(&email, "Seu código de acesso — Yggdrasil", &body);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Erro ao processar código de acesso: {e}");
        }
    }

    StatusCode::OK
}

pub(super) fn do_request_code(conn: &Connection, email: &str) -> rusqlite::Result<Option<String>> {
    let now = Utc::now().timestamp();
    let cutoff = now - RATE_LIMIT_WINDOW_SECS;

    let requests_json: Option<String> = conn
        .query_row(
            "SELECT requests FROM rate_limits WHERE email = ?1",
            [email],
            |row| row.get(0),
        )
        .ok();

    let mut requests: Vec<i64> = requests_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    requests.retain(|&ts| ts >= cutoff);

    if requests.len() >= RATE_LIMIT_MAX {
        return Ok(None);
    }

    let user_id: Option<String> = conn
        .query_row(
            "SELECT user_id FROM usuarios WHERE email = ?1",
            [email],
            |row| row.get(0),
        )
        .ok();

    let user_id = match user_id {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO usuarios (email, user_id) VALUES (?1, ?2)",
                rusqlite::params![email, id],
            )?;
            id
        }
    };

    requests.push(now);
    let requests_json = serde_json::to_string(&requests).unwrap_or_default();
    conn.execute(
        "INSERT OR REPLACE INTO rate_limits (email, requests) VALUES (?1, ?2)",
        rusqlite::params![email, requests_json],
    )?;

    let code = generate_code();
    let expires_at = now + CODE_EXPIRY_SECS;
    conn.execute(
        "INSERT OR REPLACE INTO verify_codes (email, code, user_id, expires_at, attempts) VALUES (?1, ?2, ?3, ?4, 0)",
        rusqlite::params![email, code, user_id, expires_at],
    )?;

    Ok(Some(code))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/verify",
    request_body = VerifyRequest,
    responses(
        (status = 200, description = "JWT emitido", body = crate::openapi::AuthTokenDoc),
        (status = 410, description = "Código expirado ou não encontrado", body = crate::openapi::ErrorDoc),
        (status = 422, description = "Código incorreto", body = crate::openapi::ErrorDoc),
    ),
    tag = "auth"
)]
pub async fn verify_code(
    State(state): State<Arc<AuthState>>,
    Json(req): Json<VerifyRequest>,
) -> impl IntoResponse {
    let email = req.email.to_lowercase();
    let code = req.code.clone();
    let jwt_secret = state.jwt_secret.clone();

    let result = {
        let db = state.db.lock().unwrap();
        do_verify_code(&db, &email, &code)
    };

    match result {
        Ok(user_id) => match sign_jwt(&user_id, &email, &jwt_secret) {
            Ok(token) => {
                (StatusCode::OK, Json(serde_json::json!({ "token": token }))).into_response()
            }
            Err(e) => {
                tracing::error!("Erro ao assinar JWT: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Erro interno ao gerar token" })),
                )
                    .into_response()
            }
        },
        Err(AuthError::CodeNotFound | AuthError::CodeExpired) => (
            StatusCode::GONE,
            Json(serde_json::json!({ "error": "Código expirado, solicite um novo" })),
        )
            .into_response(),
        Err(AuthError::WrongCode { remaining }) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": format!("Código incorreto. Tentativas restantes: {remaining}")
            })),
        )
            .into_response(),
        Err(AuthError::Exhausted) => (
            StatusCode::GONE,
            Json(serde_json::json!({ "error": "Tentativas esgotadas, solicite um novo código" })),
        )
            .into_response(),
        Err(AuthError::Storage(e)) => {
            tracing::error!("Erro de armazenamento na verificação: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Erro interno" })),
            )
                .into_response()
        }
    }
}

enum AuthError {
    CodeNotFound,
    CodeExpired,
    WrongCode { remaining: u32 },
    Exhausted,
    Storage(rusqlite::Error),
}

impl From<rusqlite::Error> for AuthError {
    fn from(e: rusqlite::Error) -> Self {
        AuthError::Storage(e)
    }
}

fn do_verify_code(conn: &Connection, email: &str, code: &str) -> Result<String, AuthError> {
    let now = Utc::now().timestamp();

    let row: rusqlite::Result<(String, String, i64, u32)> = conn.query_row(
        "SELECT code, user_id, expires_at, attempts FROM verify_codes WHERE email = ?1",
        [email],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    );

    let (stored_code, user_id, expires_at, attempts) = match row {
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(AuthError::CodeNotFound),
        Err(e) => return Err(AuthError::Storage(e)),
        Ok(v) => v,
    };

    if now > expires_at {
        let _ = conn.execute("DELETE FROM verify_codes WHERE email = ?1", [email]);
        return Err(AuthError::CodeExpired);
    }

    if stored_code != code {
        let new_attempts = attempts + 1;
        if new_attempts >= MAX_ATTEMPTS {
            let _ = conn.execute("DELETE FROM verify_codes WHERE email = ?1", [email]);
            return Err(AuthError::Exhausted);
        }
        conn.execute(
            "UPDATE verify_codes SET attempts = ?1 WHERE email = ?2",
            rusqlite::params![new_attempts, email],
        )?;
        return Err(AuthError::WrongCode {
            remaining: MAX_ATTEMPTS - new_attempts,
        });
    }

    let _ = conn.execute("DELETE FROM verify_codes WHERE email = ?1", [email]);
    Ok(user_id)
}
