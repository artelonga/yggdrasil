//! User metadata CRUD.
//!
//! Read-only por enquanto: `GET /api/v1/me` retorna o que está no JWT. Não
//! há tabela `user_profiles` ainda — display_name e bio virão num task
//! seguinte (YG-N). Por ora isso é o suficiente para o cliente saber seu
//! próprio user_id (útil para debug + para o admin endpoint mapear).

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Serialize;

use crate::api::user_profiles;
use crate::auth;

pub struct UsersState {
    pub jwt_secret: String,
    /// Path do SQLite onde `user_profiles` mora. Lido para resolver username.
    pub db_path: std::path::PathBuf,
}

#[derive(Serialize)]
struct MeResponse {
    user_id: String,
    email: String,
    username: String,
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"erro": "nao_autenticado"})),
    )
        .into_response()
}

/// `GET /api/v1/me` — retorna `{user_id, email, username}` do JWT + DB.
/// 401 sem auth.
pub async fn get_me(State(state): State<Arc<UsersState>>, headers: HeaderMap) -> impl IntoResponse {
    let token = match extract_bearer(&headers) {
        Some(t) => t,
        None => return unauthorized(),
    };
    // Use full Claims (não só sub) para extrair email.
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    let data = match jsonwebtoken::decode::<auth::Claims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &validation,
    ) {
        Ok(d) => d,
        Err(_) => return unauthorized(),
    };
    // Resolve username via DB. Fallback para user_id se sem perfil.
    let username = match user_profiles::init_db(&state.db_path) {
        Ok(conn) => user_profiles::username_or_id(&conn, &data.claims.sub),
        Err(_) => data.claims.sub.clone(),
    };
    (
        StatusCode::OK,
        Json(MeResponse {
            user_id: data.claims.sub,
            email: data.claims.email,
            username,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    use crate::auth::sign_jwt;
    use tempfile::tempdir;

    fn make_app(secret: &str) -> Router {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let _ = user_profiles::init_db(&db_path).unwrap();
        std::mem::forget(dir);
        let state = Arc::new(UsersState {
            jwt_secret: secret.to_string(),
            db_path,
        });
        Router::new()
            .route("/api/v1/me", get(get_me))
            .with_state(state)
    }

    async fn parse_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn get_me_sem_auth_retorna_401() {
        let app = make_app("s");
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
    async fn get_me_com_auth_retorna_user_id_e_email() {
        let app = make_app("s");
        let token = sign_jwt("yuri-123", "yuri@artelonga.com.br", "s").unwrap();
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
        let v = parse_body(resp).await;
        assert_eq!(v["user_id"], "yuri-123");
        assert_eq!(v["email"], "yuri@artelonga.com.br");
        // Sem perfil registrado → username = user_id (fallback).
        assert_eq!(v["username"], "yuri-123");
    }

    #[tokio::test]
    async fn get_me_com_token_invalido_retorna_401() {
        let app = make_app("s");
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me")
                    .header("authorization", "Bearer not-a-jwt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
