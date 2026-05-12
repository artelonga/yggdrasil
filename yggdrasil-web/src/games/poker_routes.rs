//! Multiplayer poker lobby routes — auth-gated.
//!
//! Layer 1 of the poker rollout (seating only). Card play / betting will be a
//! follow-up that composes [`yggdrasil_core::games::poker_lobby::PokerLobby`]
//! with `game_core::PokerGame`.

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use yggdrasil_core::games::poker_lobby::{LobbyError, PokerLobby};

use crate::auth::verify_jwt;

pub struct PokerState {
    pub jwt_secret: String,
    pub lobbies: Mutex<Vec<PokerLobby>>,
}

impl PokerState {
    pub fn new(jwt_secret: String) -> Self {
        Self {
            jwt_secret,
            lobbies: Mutex::new(vec![
                PokerLobby::new("carvalho", "Mesa Carvalho"),
                PokerLobby::new("olmo", "Mesa Olmo"),
            ]),
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

#[allow(clippy::result_large_err)]
fn require_user(headers: &HeaderMap, secret: &str) -> Result<String, Response> {
    let token = extract_bearer(headers).ok_or_else(unauthorized)?;
    verify_jwt(&token, secret).map_err(|_| unauthorized())
}

type Response = axum::response::Response;

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"erro": "nao_autenticado"})),
    )
        .into_response()
}

fn lobby_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"erro": "mesa_nao_encontrada"})),
    )
        .into_response()
}

fn lobby_error(e: LobbyError) -> Response {
    let status = match e {
        LobbyError::InvalidSeat(_) => StatusCode::BAD_REQUEST,
        LobbyError::SeatTaken | LobbyError::AlreadySeated => StatusCode::CONFLICT,
        LobbyError::NotSeated => StatusCode::BAD_REQUEST,
    };
    (status, Json(serde_json::json!({"erro": e.to_string()}))).into_response()
}

pub async fn list_lobbies(
    State(state): State<Arc<PokerState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(r) = require_user(&headers, &state.jwt_secret) {
        return r;
    }
    let lobbies = state.lobbies.lock().unwrap();
    let view: Vec<PokerLobby> = lobbies.clone();
    (StatusCode::OK, Json(serde_json::json!({ "lobbies": view }))).into_response()
}

pub async fn get_lobby(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(r) = require_user(&headers, &state.jwt_secret) {
        return r;
    }
    let lobbies = state.lobbies.lock().unwrap();
    match lobbies.iter().find(|l| l.id == id) {
        Some(l) => (StatusCode::OK, Json(l.clone())).into_response(),
        None => lobby_not_found(),
    }
}

#[derive(Deserialize)]
pub struct SitRequest {
    pub seat: usize,
}

pub async fn sit(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SitRequest>,
) -> impl IntoResponse {
    let user_id = match require_user(&headers, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(r) => return r,
    };
    let mut lobbies = state.lobbies.lock().unwrap();
    let lobby = match lobbies.iter_mut().find(|l| l.id == id) {
        Some(l) => l,
        None => return lobby_not_found(),
    };
    match lobby.sit(req.seat, &user_id) {
        Ok(()) => (StatusCode::OK, Json(lobby.clone())).into_response(),
        Err(e) => lobby_error(e),
    }
}

pub async fn stand(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match require_user(&headers, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(r) => return r,
    };
    let mut lobbies = state.lobbies.lock().unwrap();
    let lobby = match lobbies.iter_mut().find(|l| l.id == id) {
        Some(l) => l,
        None => return lobby_not_found(),
    };
    match lobby.stand(&user_id) {
        Ok(()) => (StatusCode::OK, Json(lobby.clone())).into_response(),
        Err(e) => lobby_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::Request,
        routing::{get, post},
    };
    use tower::ServiceExt;

    use crate::auth::sign_jwt;

    fn make_app(secret: &str) -> (Router, Arc<PokerState>) {
        let state = Arc::new(PokerState::new(secret.to_string()));
        let app = Router::new()
            .route("/api/v1/poker/lobbies", get(list_lobbies))
            .route("/api/v1/poker/lobbies/{id}", get(get_lobby))
            .route("/api/v1/poker/lobbies/{id}/sit", post(sit))
            .route("/api/v1/poker/lobbies/{id}/stand", post(stand))
            .with_state(state.clone());
        (app, state)
    }

    #[tokio::test]
    async fn list_sem_auth_retorna_401() {
        let (app, _) = make_app("s");
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/poker/lobbies")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_com_auth_retorna_duas_mesas() {
        let (app, _) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/poker/lobbies")
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
        let lobbies = v["lobbies"].as_array().unwrap();
        assert_eq!(lobbies.len(), 2);
        assert_eq!(lobbies[0]["id"], "carvalho");
        assert_eq!(lobbies[1]["id"], "olmo");
    }

    #[tokio::test]
    async fn sit_persiste_humano_e_adiciona_bot() {
        let (app, _) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let body = serde_json::json!({ "seat": 2 }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/poker/lobbies/carvalho/sit")
                    .header("authorization", format!("Bearer {token}"))
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
        let seats = v["seats"].as_array().unwrap();
        let humans = seats.iter().filter(|s| s["kind"] == "human").count();
        let bots = seats.iter().filter(|s| s["kind"] == "bot").count();
        assert_eq!(humans, 1);
        assert_eq!(bots, 1);
    }

    #[tokio::test]
    async fn sit_em_mesa_inexistente_retorna_404() {
        let (app, _) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let body = serde_json::json!({ "seat": 0 }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/poker/lobbies/inexistente/sit")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stand_remove_jogador() {
        let (app, _) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        // Sit first
        let sit_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/poker/lobbies/carvalho/sit")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"seat": 0}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(sit_resp.status(), StatusCode::OK);
        // Stand
        let stand_resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/poker/lobbies/carvalho/stand")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stand_resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(stand_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let humans = v["seats"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "human")
            .count();
        let bots = v["seats"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "bot")
            .count();
        assert_eq!(humans, 0);
        assert_eq!(bots, 0);
    }
}
