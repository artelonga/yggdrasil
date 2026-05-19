use std::sync::{Arc, Mutex};

use axum::{Router, body::Body, http::Request, middleware, routing::get, routing::post};
use game_core::mail::LogMailProvider;
use rusqlite::Connection;
use tempfile::tempdir;
use tower::ServiceExt;

use super::jwt::{require_auth, sign_jwt};
use super::magic_link::{do_request_code, request_code, verify_code};
use super::state::{AuthState, init_auth_db};
use axum::http::StatusCode;
use chrono::Utc;
use uuid::Uuid;

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
    use super::magic_link::generate_code;
    for _ in 0..20 {
        let code = generate_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
