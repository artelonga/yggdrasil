use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Serialize;
use utoipa::ToSchema;
use yggdrasil_core::sementes::Sementes;

use crate::auth::verify_jwt;

pub struct MeState {
    pub jwt_secret: String,
    pub sementes: Arc<Sementes>,
}

#[derive(Serialize, ToSchema)]
struct SaldoResponse {
    saldo: u64,
    #[schema(value_type = String)]
    moeda: &'static str,
    atualizado_em: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/me/sementes",
    responses(
        (status = 200, description = "Saldo de sementes do usuário autenticado", body = SaldoResponse),
        (status = 401, description = "Não autenticado", body = crate::openapi::ErrorDoc),
    ),
    security(("bearerAuth" = [])),
    tag = "me"
)]
pub async fn get_sementes(
    State(state): State<Arc<MeState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match extract_bearer(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "nao_autenticado"})),
            )
                .into_response();
        }
    };

    let user_id = match verify_jwt(&token, &state.jwt_secret) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "nao_autenticado"})),
            )
                .into_response();
        }
    };

    match state.sementes.saldo_info(&user_id) {
        Ok(info) => (
            StatusCode::OK,
            Json(SaldoResponse {
                saldo: info.saldo,
                moeda: "sementes",
                atualizado_em: info.atualizado_em.to_rfc3339(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Erro ao buscar saldo de sementes: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "Erro interno"})),
            )
                .into_response()
        }
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use game_core::storage::{Storage, schema};
    use tempfile::tempdir;
    use tower::ServiceExt;

    use crate::auth::sign_jwt;

    fn make_state_with_storage(secret: &str, storage: Arc<Storage>) -> Arc<MeState> {
        Arc::new(MeState {
            jwt_secret: secret.to_string(),
            sementes: Arc::new(Sementes::new(storage)),
        })
    }

    fn make_state(secret: &str) -> (Arc<MeState>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());
        (make_state_with_storage(secret, storage), dir)
    }

    fn make_app(state: Arc<MeState>) -> Router {
        Router::new()
            .route("/api/v1/me/sementes", get(get_sementes))
            .with_state(state)
    }

    #[tokio::test]
    async fn sem_jwt_retorna_401_nao_autenticado() {
        let (state, _dir) = make_state("test-secret");
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["erro"], "nao_autenticado");
    }

    #[tokio::test]
    async fn jwt_invalido_retorna_401() {
        let (state, _dir) = make_state("test-secret");
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", "Bearer token-invalido")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["erro"], "nao_autenticado");
    }

    #[tokio::test]
    async fn jwt_valido_retorna_200_com_saldo_e_timestamp() {
        let secret = "test-secret";
        let (state, _dir) = make_state(secret);
        let token = sign_jwt("user-1", "user@test.com", secret).unwrap();
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["saldo"].is_number(), "saldo deve ser número");
        assert_eq!(v["moeda"], "sementes");
        assert!(
            v["atualizado_em"].is_string(),
            "atualizado_em deve ser string ISO 8601"
        );
    }

    #[tokio::test]
    async fn saldo_u64_correto_para_usuario_com_carteira() {
        let secret = "test-secret";
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());

        let wallet = schema::Wallet {
            user_id: "user-42".to_string(),
            balance: 9_750,
            last_updated: 1_700_000_000,
        };
        storage.save_wallet_for_user("user-42", &wallet).unwrap();

        let state = make_state_with_storage(secret, storage);
        let token = sign_jwt("user-42", "u@test.com", secret).unwrap();
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["saldo"].as_u64().unwrap(), 9_750u64);
        assert!(v["atualizado_em"].as_str().unwrap().starts_with("2023-"));
    }

    #[tokio::test]
    async fn usuario_sem_carteira_retorna_saldo_zero() {
        let secret = "test-secret";
        let (state, _dir) = make_state(secret);
        let token = sign_jwt("novo-usuario", "novo@test.com", secret).unwrap();
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["saldo"].as_u64().unwrap(), 0u64);
        assert_eq!(v["moeda"], "sementes");
    }
}
