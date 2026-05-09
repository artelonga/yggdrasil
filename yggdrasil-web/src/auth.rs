use std::sync::{Arc, Mutex};

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use game_core::mail::MailProvider;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const RATE_LIMIT_MAX: usize = 3;
const RATE_LIMIT_WINDOW_SECS: i64 = 900;
const CODE_EXPIRY_SECS: i64 = 300;
const JWT_EXPIRY_SECS: i64 = 604_800;
const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct UserId(pub String);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for UserId {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<UserId>()
            .cloned()
            .ok_or_else(|| unauthorized("Não autenticado"));
        std::future::ready(result)
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
struct ErrorBody {
    error: String,
}

#[allow(dead_code)]
fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

pub struct AuthState {
    pub db: Arc<Mutex<Connection>>,
    pub mail: Arc<dyn MailProvider>,
    pub jwt_secret: String,
}

pub fn init_auth_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS usuarios (
            email   TEXT PRIMARY KEY,
            user_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS verify_codes (
            email      TEXT PRIMARY KEY,
            code       TEXT NOT NULL,
            user_id    TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            attempts   INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS rate_limits (
            email    TEXT PRIMARY KEY,
            requests TEXT NOT NULL
        );",
    )
}

pub fn sign_jwt(user_id: &str, email: &str, secret: &str) -> anyhow::Result<String> {
    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now.timestamp() + JWT_EXPIRY_SECS) as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp,
        iat,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn generate_code() -> String {
    let bytes = Uuid::new_v4();
    let n = u32::from_le_bytes(bytes.as_bytes()[..4].try_into().unwrap());
    format!("{n:06}", n = n % 1_000_000)
}

#[allow(dead_code)]
pub async fn require_auth(
    State(state): State<Arc<AuthState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| unauthorized("Cabeçalho Authorization ausente ou inválido"))?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => unauthorized("Token expirado"),
        _ => unauthorized("Token inválido"),
    })?;

    req.extensions_mut().insert(UserId(token_data.claims.sub));
    Ok(next.run(req).await)
}

#[derive(Deserialize)]
pub struct CodeRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub email: String,
    pub code: String,
}

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

fn do_request_code(conn: &Connection, email: &str) -> rusqlite::Result<Option<String>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, middleware, routing::get, routing::post};
    use game_core::mail::LogMailProvider;
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn make_test_state(secret: &str) -> Arc<AuthState> {
        let conn = Connection::open_in_memory().unwrap();
        init_auth_db(&conn).unwrap();
        Arc::new(AuthState {
            db: Arc::new(Mutex::new(conn)),
            mail: Arc::new(LogMailProvider),
            jwt_secret: secret.to_string(),
        })
    }

    fn make_app(state: Arc<AuthState>) -> Router {
        Router::new()
            .route("/api/v1/auth/code", post(request_code))
            .route("/api/v1/auth/verify", post(verify_code))
            .with_state(state)
    }

    fn make_protected_app(state: Arc<AuthState>) -> Router {
        Router::new()
            .route("/api/v1/me", get(|| async { "ok" }))
            .route_layer(middleware::from_fn_with_state(state.clone(), require_auth))
            .with_state(state)
    }

    fn insert_code(state: &AuthState, email: &str, code: &str) -> String {
        let conn = state.db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT OR REPLACE INTO usuarios (email, user_id) VALUES (?1, ?2)",
            rusqlite::params![email, user_id],
        )
        .unwrap();
        let expires_at = Utc::now().timestamp() + 300;
        conn.execute(
            "INSERT OR REPLACE INTO verify_codes (email, code, user_id, expires_at, attempts) VALUES (?1, ?2, ?3, ?4, 0)",
            rusqlite::params![email, code, user_id, expires_at],
        )
        .unwrap();
        user_id
    }

    #[tokio::test]
    async fn request_code_returns_200() {
        let state = make_test_state("secret");
        let app = make_app(state);
        let body = serde_json::json!({ "email": "user@test.com" }).to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/code")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn request_code_rate_limited_still_returns_200() {
        let state = make_test_state("secret");
        let email = "rl@test.com";
        // Exhaust rate limit directly
        {
            let conn = state.db.lock().unwrap();
            for _ in 0..3 {
                do_request_code(&conn, email).unwrap();
            }
        }
        let app = make_app(state);
        let body = serde_json::json!({ "email": email }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/code")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Always 200 to avoid email enumeration
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn verify_correct_code_returns_jwt() {
        let state = make_test_state("test-secret");
        let email = "user@test.com";
        insert_code(&state, email, "123456");

        let app = make_app(state);
        let body = serde_json::json!({ "email": email, "code": "123456" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/verify")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["token"].is_string(), "resposta deve conter token JWT");
    }

    #[tokio::test]
    async fn verify_wrong_code_returns_422() {
        let state = make_test_state("test-secret");
        let email = "user@test.com";
        insert_code(&state, email, "999999");

        let app = make_app(state);
        let body = serde_json::json!({ "email": email, "code": "000000" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/verify")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            v["error"].as_str().unwrap().contains("Código incorreto"),
            "mensagem de erro deve ser em PT-BR"
        );
    }

    #[tokio::test]
    async fn verify_no_code_returns_gone() {
        let state = make_test_state("test-secret");
        let app = make_app(state);
        let body = serde_json::json!({ "email": "nobody@test.com", "code": "000000" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/verify")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::GONE);
    }

    #[tokio::test]
    async fn rate_limit_blocks_after_3_requests() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("auth.db").to_string_lossy().to_string();
        let conn = Connection::open(&db).unwrap();
        init_auth_db(&conn).unwrap();

        let email = "rl@test.com";
        assert!(do_request_code(&conn, email).unwrap().is_some());
        assert!(do_request_code(&conn, email).unwrap().is_some());
        assert!(do_request_code(&conn, email).unwrap().is_some());
        assert!(
            do_request_code(&conn, email).unwrap().is_none(),
            "4ª solicitação deve ser bloqueada por rate limit"
        );
    }

    #[tokio::test]
    async fn require_auth_rejects_missing_token() {
        let state = make_test_state("test-secret");
        let app = make_protected_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn require_auth_rejects_invalid_token() {
        let state = make_test_state("test-secret");
        let app = make_protected_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me")
                    .header("authorization", "Bearer token-invalido")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn require_auth_accepts_valid_jwt() {
        let secret = "test-secret";
        let state = make_test_state(secret);
        let token = sign_jwt("user-1", "user@test.com", secret).unwrap();
        let app = make_protected_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn generate_code_is_6_digits() {
        for _ in 0..20 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }
}
