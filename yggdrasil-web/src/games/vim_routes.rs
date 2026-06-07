//! Rotas HTTP para o universo Vim (YG-58).
//!
//! POST /api/v1/universos/vim/sessoes         — cria sessão
//! POST /api/v1/universos/vim/sessoes/{id}/tecla — envia tecla

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use universe_vim::{GameState, VimSession};
use utoipa::ToSchema;

// ── Estado compartilhado ─────────────────────────────────────────────────────

pub struct VimState {
    sessions: Mutex<HashMap<String, VimSession>>,
}

pub fn make_vim_state() -> Arc<VimState> {
    Arc::new(VimState {
        sessions: Mutex::new(HashMap::new()),
    })
}

// ── Request / Response types ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub state: GameState,
}

#[derive(Deserialize, ToSchema)]
pub struct SendKeyRequest {
    pub key: String,
}

#[derive(Serialize)]
pub struct SendKeyResponse {
    pub state: GameState,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/api/v1/universos/vim/sessoes",
    responses(
        (status = 200, description = "Sessão Vim criada", body = crate::openapi::VimCreateSessionDoc),
        (status = 500, description = "Erro interno"),
    ),
    tag = "games"
)]
pub async fn create_session(State(state): State<Arc<VimState>>) -> impl IntoResponse {
    let session = VimSession::new();
    let game_state = session.state();
    let id = nanoid!();

    match state.sessions.lock() {
        Ok(mut sessions) => {
            sessions.insert(id.clone(), session);
        }
        Err(_) => {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    Json(CreateSessionResponse {
        session_id: id,
        state: game_state,
    })
    .into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/universos/vim/sessoes/{id}/tecla",
    params(
        ("id" = String, Path, description = "ID da sessão Vim")
    ),
    request_body = SendKeyRequest,
    responses(
        (status = 200, description = "Estado após a tecla", body = crate::openapi::VimSendKeyResponseDoc),
        (status = 404, description = "Sessão não encontrada"),
    ),
    tag = "games"
)]
pub async fn send_key(
    Path(id): Path<String>,
    State(state): State<Arc<VimState>>,
    Json(body): Json<SendKeyRequest>,
) -> impl IntoResponse {
    let game_state = {
        let mut sessions = match state.sessions.lock() {
            Ok(s) => s,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let session = match sessions.get_mut(&id) {
            Some(s) => s,
            None => return StatusCode::NOT_FOUND.into_response(),
        };
        session.send_key(&body.key)
    };

    Json(SendKeyResponse { state: game_state }).into_response()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use tower::ServiceExt;

    fn make_app() -> Router {
        let state = make_vim_state();
        Router::new()
            .route("/api/v1/universos/vim/sessoes", post(create_session))
            .route("/api/v1/universos/vim/sessoes/{id}/tecla", post(send_key))
            .with_state(state)
    }

    #[tokio::test]
    async fn create_session_returns_id_and_state() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/vim/sessoes")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["session_id"].is_string());
        assert!(v["state"]["lines"].is_array());
        assert_eq!(v["state"]["level"], 1);
        assert_eq!(v["state"]["score"], 0);
        assert_eq!(v["state"]["mode"], "NORMAL");
    }

    #[tokio::test]
    async fn send_key_h_moves_cursor() {
        let state = make_vim_state();
        let app = Router::new()
            .route("/api/v1/universos/vim/sessoes", post(create_session))
            .route("/api/v1/universos/vim/sessoes/{id}/tecla", post(send_key))
            .with_state(state);

        // Cria sessão
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/vim/sessoes")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = v["session_id"].as_str().unwrap().to_string();

        // Envia tecla 'j'
        let body = serde_json::json!({ "key": "j" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/universos/vim/sessoes/{id}/tecla"))
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
        // Cursor deve ter se movido para a linha 1
        assert_eq!(v["state"]["cursor"][0], 1);
    }

    #[tokio::test]
    async fn send_key_unknown_session_returns_404() {
        let app = make_app();
        let body = serde_json::json!({ "key": "h" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/vim/sessoes/nao-existe/tecla")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn send_key_enters_insert_mode() {
        let state = make_vim_state();
        let app = Router::new()
            .route("/api/v1/universos/vim/sessoes", post(create_session))
            .route("/api/v1/universos/vim/sessoes/{id}/tecla", post(send_key))
            .with_state(state);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/vim/sessoes")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let create: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = create["session_id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "key": "i" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/universos/vim/sessoes/{id}/tecla"))
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
        assert_eq!(v["state"]["mode"], "INSERT");
    }

    #[tokio::test]
    async fn state_has_hint() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/vim/sessoes")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["state"]["hint"].is_string());
    }
}
