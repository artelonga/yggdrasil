//! Admin endpoints — operações privilegiadas que não cabem no fluxo
//! normal de usuário. Gated por `YGGDRASIL_ADMIN_TOKEN` (env var).
//!
//! Hoje só credita sementes. Pode crescer para incluir reset de saldo,
//! kick de jogadores, etc. Sempre auth-gated e auditável (logs).

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use yggdrasil_core::sementes::Sementes;

pub struct AdminState {
    pub admin_token: Option<String>,
    pub sementes: Arc<Sementes>,
}

fn extract_admin_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

#[allow(clippy::result_large_err)]
fn check_admin(
    headers: &HeaderMap,
    expected: &Option<String>,
) -> Result<(), axum::response::Response> {
    let expected = match expected {
        Some(t) if !t.is_empty() => t,
        _ => {
            tracing::warn!("admin endpoint chamado mas YGGDRASIL_ADMIN_TOKEN não está configurado");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"erro": "admin_disabled"})),
            )
                .into_response());
        }
    };
    let provided = match extract_admin_token(headers) {
        Some(t) => t,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "admin_token_ausente"})),
            )
                .into_response());
        }
    };
    if &provided != expected {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"erro": "admin_token_invalido"})),
        )
            .into_response());
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreditRequest {
    pub user_id: String,
    pub amount: u64,
}

#[derive(Serialize)]
struct CreditResponse {
    user_id: String,
    creditado: u64,
    saldo_apos: u64,
}

/// `POST /api/v1/admin/sementes/credit` body `{user_id, amount}` — credita
/// sementes na carteira do usuário. Auth via `Authorization: Bearer <admin_token>`.
pub async fn post_credit(
    State(state): State<Arc<AdminState>>,
    headers: HeaderMap,
    Json(req): Json<CreditRequest>,
) -> impl IntoResponse {
    if let Err(r) = check_admin(&headers, &state.admin_token) {
        return r;
    }
    if req.amount == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"erro": "amount_zero"})),
        )
            .into_response();
    }
    if let Err(e) = state.sementes.creditar(&req.user_id, req.amount) {
        tracing::error!(
            "admin credit falhou para user={} amount={}: {e}",
            req.user_id,
            req.amount
        );
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"erro": e.to_string()})),
        )
            .into_response();
    }
    let saldo_apos = state.sementes.saldo(&req.user_id).unwrap_or(0);
    tracing::info!(
        "admin credit: user={} amount={} saldo_apos={saldo_apos}",
        req.user_id,
        req.amount
    );
    (
        StatusCode::OK,
        Json(CreditResponse {
            user_id: req.user_id,
            creditado: req.amount,
            saldo_apos,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::post};
    use game_core::storage::Storage;
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn make_app(admin_token: Option<&str>) -> (Router, tempfile::TempDir, Arc<AdminState>) {
        let dir = tempdir().unwrap();
        let storage = Arc::new(Storage::open(&dir.path().join("test.db")).unwrap());
        let sementes = Arc::new(Sementes::new(storage));
        let state = Arc::new(AdminState {
            admin_token: admin_token.map(String::from),
            sementes,
        });
        let app = Router::new()
            .route("/api/v1/admin/sementes/credit", post(post_credit))
            .with_state(state.clone());
        (app, dir, state)
    }

    fn body(user_id: &str, amount: u64) -> Body {
        Body::from(serde_json::json!({"user_id": user_id, "amount": amount}).to_string())
    }

    #[tokio::test]
    async fn credit_sem_token_retorna_401() {
        let (app, _dir, _) = make_app(Some("secret"));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/sementes/credit")
                    .header("content-type", "application/json")
                    .body(body("yuri", 1000))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn credit_com_token_invalido_retorna_403() {
        let (app, _dir, _) = make_app(Some("secret"));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/sementes/credit")
                    .header("authorization", "Bearer wrong")
                    .header("content-type", "application/json")
                    .body(body("yuri", 1000))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn credit_com_token_correto_credita_e_retorna_saldo() {
        let (app, _dir, state) = make_app(Some("secret"));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/sementes/credit")
                    .header("authorization", "Bearer secret")
                    .header("content-type", "application/json")
                    .body(body("yuri", 1000))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["user_id"], "yuri");
        assert_eq!(v["creditado"], 1000);
        assert_eq!(v["saldo_apos"], 1000);
        // Real wallet check.
        assert_eq!(state.sementes.saldo("yuri").unwrap(), 1000);
    }

    #[tokio::test]
    async fn credit_sem_token_configurado_retorna_503() {
        // Operação fica desabilitada quando YGGDRASIL_ADMIN_TOKEN não está setada.
        let (app, _dir, _) = make_app(None);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/sementes/credit")
                    .header("authorization", "Bearer anything")
                    .header("content-type", "application/json")
                    .body(body("yuri", 1000))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn credit_amount_zero_retorna_400() {
        let (app, _dir, _) = make_app(Some("secret"));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/sementes/credit")
                    .header("authorization", "Bearer secret")
                    .header("content-type", "application/json")
                    .body(body("yuri", 0))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
