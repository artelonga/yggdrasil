//! Perfil universal de usuário (YG-137). UM perfil por usuário (JWT), sobre o
//! qual cada universo compõe via `join_universe`. Espelha o gating de
//! `instances.rs` (Bearer JWT → `sub`).

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::auth::verify_jwt;
use yggdrasil_core::profile::{ProfileError, ProfileStore};

pub struct ProfileApiState {
    pub jwt_secret: String,
    pub store: Arc<ProfileStore>,
}

type ApiState = State<Arc<ProfileApiState>>;

fn unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "erro": "nao_autenticado" })),
    )
        .into_response()
}

#[allow(clippy::result_large_err)] // o `Err` é uma `Response` axum por design
fn require_user(
    state: &ProfileApiState,
    headers: &HeaderMap,
) -> Result<String, axum::response::Response> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    verify_jwt(token, &state.jwt_secret).map_err(|_| unauthorized())
}

fn map_err(e: ProfileError) -> axum::response::Response {
    match e {
        ProfileError::InvalidUser => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "erro": "usuario_invalido" })),
        )
            .into_response(),
        ProfileError::Io(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "erro": "erro_io" })),
        )
            .into_response(),
    }
}

/// `GET /api/v1/profile` — perfil do usuário logado (cria vazio se não existe).
pub async fn get_profile(State(state): ApiState, headers: HeaderMap) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match state.store.load_or_new(&user) {
        Ok(p) => Json(p).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<String>,
}

/// `PUT /api/v1/profile` — atualiza a base (campos ausentes preservados).
pub async fn put_profile(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match state.store.update_base(
        &user,
        body.display_name.as_deref(),
        body.bio.as_deref(),
        body.avatar.as_deref(),
    ) {
        Ok(p) => Json(p).into_response(),
        Err(e) => map_err(e),
    }
}

#[derive(Deserialize, Default)]
pub struct JoinBody {
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
}

/// `PUT /api/v1/profile/universos/{slug}` — "criar perfil neste universo" (a
/// CTA do catálogo). Idempotente; mescla `props`. Vale para qualquer slug,
/// inclusive universos ainda inexistentes (planned/external) — é o perfil que
/// passa a existir, não o universo.
pub async fn join_universe(
    State(state): ApiState,
    headers: HeaderMap,
    Path(slug): Path<String>,
    body: Option<Json<JoinBody>>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let props = body.map(|b| b.0.props).unwrap_or_default();
    match state.store.join_universe(&user, &slug, props) {
        Ok(p) => Json(p).into_response(),
        Err(e) => map_err(e),
    }
}

/// `DELETE /api/v1/profile/universos/{slug}` — sai do universo.
pub async fn leave_universe(
    State(state): ApiState,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match state.store.leave_universe(&user, &slug) {
        Ok(removido) => {
            Json(serde_json::json!({ "slug": slug, "removido": removido })).into_response()
        }
        Err(e) => map_err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    const SECRET: &str = "test-secret";

    fn app() -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(ProfileApiState {
            jwt_secret: SECRET.into(),
            store: Arc::new(ProfileStore::new(dir.path())),
        });
        let router = Router::new()
            .route("/api/v1/profile", get(get_profile).put(put_profile))
            .route(
                "/api/v1/profile/universos/{slug}",
                axum::routing::put(join_universe).delete(leave_universe),
            )
            .with_state(state);
        (router, dir)
    }

    fn token(user: &str) -> String {
        crate::auth::sign_jwt(user, user, SECRET).unwrap()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn get_exige_jwt_e_cria_perfil_default() {
        let (app, _d) = app();
        // sem JWT → 401
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/profile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        // com JWT → perfil default
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/profile")
                    .header("authorization", format!("Bearer {}", token("ana@x.com")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["user"], "ana@x.com");
        assert_eq!(v["display_name"], "ana");
    }

    #[tokio::test]
    async fn join_universe_cria_perfil_no_universo() {
        let (app, _d) = app();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/profile/universos/shandara")
                    .header("authorization", format!("Bearer {}", token("ana@x.com")))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"props":{"papel":"explorador"}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["universes"]["shandara"]["props"]["papel"], "explorador");
        // não-dono não vê: GET de outro user não traz shandara
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/profile")
                    .header("authorization", format!("Bearer {}", token("bob@x.com")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        assert!(v["universes"].as_object().unwrap().is_empty());
    }
}
