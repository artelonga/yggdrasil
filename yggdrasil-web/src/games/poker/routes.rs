use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use game_core::games::poker::PokerAction;
use serde::Deserialize;
use yggdrasil_core::games::poker_bot::auto_step_bots;
use yggdrasil_core::games::poker_lobby::PokerLobby;

use crate::auth::verify_jwt;

use super::chip_flow::{
    Response, lobby_not_found, sit_error, stand_error, table_error, unauthorized,
};
use super::serialization::waiting_state;
use super::state::PokerState;

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

#[derive(Deserialize)]
pub struct SitRequest {
    pub seat: usize,
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub action: String,
    pub amount: Option<u32>,
}

pub async fn list_lobbies(
    State(state): State<Arc<PokerState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(r) = require_user(&headers, &state.jwt_secret) {
        return r;
    }
    let tables = state.tables.lock().unwrap();
    let lobbies: Vec<&PokerLobby> = tables.iter().map(|t| &t.lobby).collect();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "lobbies": lobbies })),
    )
        .into_response()
}

pub async fn get_lobby(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(r) = require_user(&headers, &state.jwt_secret) {
        return r;
    }
    let tables = state.tables.lock().unwrap();
    match tables.iter().find(|t| t.lobby.id == id) {
        Some(t) => (StatusCode::OK, Json(t.lobby.clone())).into_response(),
        None => lobby_not_found(),
    }
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
    let mut tables = state.tables.lock().unwrap();
    let table = match tables.iter_mut().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };
    match table.sit_with_sementes(req.seat, &user_id, &state.sementes) {
        Ok(()) => {
            state.persist_table(table);
            (StatusCode::OK, Json(table.lobby.clone())).into_response()
        }
        Err(e) => sit_error(e),
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
    let mut tables = state.tables.lock().unwrap();
    let table = match tables.iter_mut().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };
    match table.stand_with_sementes(&user_id, &state.sementes) {
        Ok(()) => {
            state.persist_table(table);
            (StatusCode::OK, Json(table.lobby.clone())).into_response()
        }
        Err(e) => stand_error(e),
    }
}

/// Estado público da mão: community cards, pot, current actor, próximos a falar.
/// Auto-inicia a mão se houver ≥ 2 ocupantes e nenhuma partida em curso.
pub async fn get_hand(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(r) = require_user(&headers, &state.jwt_secret) {
        return r;
    }
    let mut tables = state.tables.lock().unwrap();
    let table = match tables.iter_mut().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };
    if table.game.is_none() {
        let _ = table.start_hand();
    }
    auto_step_bots(table);
    match table.hand_state() {
        Some(s) => (StatusCode::OK, Json(s)).into_response(),
        None => (StatusCode::OK, Json(waiting_state())).into_response(),
    }
}

/// Hole cards do usuário autenticado (apenas as suas).
pub async fn get_hole_cards(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match require_user(&headers, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(r) => return r,
    };
    let tables = state.tables.lock().unwrap();
    let table = match tables.iter().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };
    match table.hole_cards_for(&user_id) {
        Some(cards) => (StatusCode::OK, Json(serde_json::json!({"cards": cards}))).into_response(),
        None => (StatusCode::OK, Json(serde_json::json!({"cards": null}))).into_response(),
    }
}

/// Aplica uma ação do jogador. Body: `{action: "call"|"raise"|"fold"|"check", amount?: u32}`.
pub async fn post_action(
    State(state): State<Arc<PokerState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ActionRequest>,
) -> impl IntoResponse {
    let user_id = match require_user(&headers, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(r) => return r,
    };
    let mut tables = state.tables.lock().unwrap();
    let table = match tables.iter_mut().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };

    let default_raise = table
        .game
        .as_ref()
        .map(|g| g.config.big_blind * 2)
        .unwrap_or(40);

    let poker_action = match req.action.as_str() {
        "fold" => PokerAction::Fold,
        "check" => PokerAction::Check,
        "call" => PokerAction::Call,
        "raise" => PokerAction::Raise(req.amount.unwrap_or(default_raise)),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"erro": "ação desconhecida"})),
            )
                .into_response();
        }
    };

    match table.act(&user_id, poker_action) {
        Ok(()) => {
            auto_step_bots(table);
            // Persiste após cada ação: stacks mudam constantemente durante a mão.
            state.persist_table(table);
            match table.hand_state() {
                Some(s) => (StatusCode::OK, Json(s)).into_response(),
                None => (StatusCode::OK, Json(waiting_state())).into_response(),
            }
        }
        Err(e) => table_error(e),
    }
}
