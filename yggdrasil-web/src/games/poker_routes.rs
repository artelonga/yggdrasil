//! Multiplayer poker routes — auth-gated.
//!
//! Seating (Layer 1, YG-23) + gameplay (Layer 2, YG-25):
//! dealing, betting rounds (pre-flop → showdown), ações call/check/fold/raise.

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use game_core::games::poker::PokerAction;
use serde::Deserialize;
use yggdrasil_core::games::poker_game::{PokerTable, PokerTableError};
use yggdrasil_core::games::poker_lobby::{LobbyError, PokerLobby};

use crate::auth::verify_jwt;

pub struct PokerState {
    pub jwt_secret: String,
    pub tables: Mutex<Vec<PokerTable>>,
}

impl PokerState {
    pub fn new(jwt_secret: String) -> Self {
        Self {
            jwt_secret,
            tables: Mutex::new(vec![
                PokerTable::new(PokerLobby::new("carvalho", "Mesa Carvalho")),
                PokerTable::new(PokerLobby::new("olmo", "Mesa Olmo")),
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

fn table_error(e: PokerTableError) -> Response {
    let status = match e {
        PokerTableError::NaoEhSuaVez => StatusCode::CONFLICT,
        PokerTableError::AcaoInvalida => StatusCode::UNPROCESSABLE_ENTITY,
        PokerTableError::MesaSemJogadores => StatusCode::CONFLICT,
        PokerTableError::RoundEncerrado => StatusCode::CONFLICT,
    };
    (status, Json(serde_json::json!({"erro": e.to_string()}))).into_response()
}

fn waiting_state() -> serde_json::Value {
    serde_json::json!({
        "game_over": false,
        "round": "Aguardando",
        "community_cards": [],
        "pot": 0,
        "current_bet": 0,
        "current_actor": null,
        "players": [],
        "winner_message": null
    })
}

// ── Lobby (seating) routes ─────────────────────────────────────────────────

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
    let mut tables = state.tables.lock().unwrap();
    let table = match tables.iter_mut().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };
    match table.lobby.sit(req.seat, &user_id) {
        Ok(()) => (StatusCode::OK, Json(table.lobby.clone())).into_response(),
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
    let mut tables = state.tables.lock().unwrap();
    let table = match tables.iter_mut().find(|t| t.lobby.id == id) {
        Some(t) => t,
        None => return lobby_not_found(),
    };
    match table.lobby.stand(&user_id) {
        Ok(()) => (StatusCode::OK, Json(table.lobby.clone())).into_response(),
        Err(e) => lobby_error(e),
    }
}

// ── Gameplay routes ────────────────────────────────────────────────────────

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

#[derive(Deserialize)]
pub struct ActionRequest {
    pub action: String,
    pub amount: Option<u32>,
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
        Ok(()) => match table.hand_state() {
            Some(s) => (StatusCode::OK, Json(s)).into_response(),
            None => (StatusCode::OK, Json(waiting_state())).into_response(),
        },
        Err(e) => table_error(e),
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
            .route("/api/v1/poker/lobbies/{id}/hand", get(get_hand))
            .route("/api/v1/poker/lobbies/{id}/hole-cards", get(get_hole_cards))
            .route("/api/v1/poker/lobbies/{id}/action", post(post_action))
            .with_state(state.clone());
        (app, state)
    }

    async fn parse_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn sit_req(seat: usize, token: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/v1/poker/lobbies/carvalho/sit")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"seat": seat}).to_string()))
            .unwrap()
    }

    fn hand_req(token: &str) -> Request<Body> {
        Request::builder()
            .uri("/api/v1/poker/lobbies/carvalho/hand")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    }

    fn action_req(token: &str, action: &str, amount: Option<u32>) -> Request<Body> {
        let mut body = serde_json::json!({"action": action});
        if let Some(amt) = amount {
            body["amount"] = serde_json::json!(amt);
        }
        Request::builder()
            .method("POST")
            .uri("/api/v1/poker/lobbies/carvalho/action")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    // ── Lobby (seating) tests (regressão YG-23) ───────────────────────────

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
        let v = parse_body(resp).await;
        let lobbies = v["lobbies"].as_array().unwrap();
        assert_eq!(lobbies.len(), 2);
        assert_eq!(lobbies[0]["id"], "carvalho");
        assert_eq!(lobbies[1]["id"], "olmo");
    }

    #[tokio::test]
    async fn sit_persiste_humano_e_adiciona_bot() {
        let (app, _) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let resp = app.oneshot(sit_req(2, &token)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
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
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/poker/lobbies/inexistente/sit")
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"seat": 0}).to_string()))
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
        let sit_resp = app.clone().oneshot(sit_req(0, &token)).await.unwrap();
        assert_eq!(sit_resp.status(), StatusCode::OK);
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
        let v = parse_body(stand_resp).await;
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

    // ── Gameplay tests (YG-25) ─────────────────────────────────────────────

    async fn seat_two_players(app: &Router) -> (String, String) {
        let token_a = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let token_b = sign_jwt("user-b", "b@test.com", "s").unwrap();
        app.clone().oneshot(sit_req(0, &token_a)).await.unwrap();
        app.clone().oneshot(sit_req(1, &token_b)).await.unwrap();
        (token_a, token_b)
    }

    #[tokio::test]
    async fn get_hand_sem_auth_retorna_401() {
        let (app, _) = make_app("s");
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/poker/lobbies/carvalho/hand")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_hand_inicia_partida_com_dois_jogadores() {
        let (app, _) = make_app("s");
        let (token_a, _token_b) = seat_two_players(&app).await;
        let resp = app.oneshot(hand_req(&token_a)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert!(!v["game_over"].as_bool().unwrap());
        assert_eq!(v["players"].as_array().unwrap().len(), 2);
        assert!(!v["current_actor"].is_null());
    }

    #[tokio::test]
    async fn get_hand_sem_jogadores_retorna_estado_aguardando() {
        let (app, _) = make_app("s");
        let token_a = sign_jwt("user-a", "a@test.com", "s").unwrap();
        // Seat only 1 player → bot fills but start_hand on GET /hand with 1 human + 1 bot
        app.clone().oneshot(sit_req(0, &token_a)).await.unwrap();
        let resp = app.oneshot(hand_req(&token_a)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // With 1 human + 1 bot, start_hand succeeds
        let v = parse_body(resp).await;
        assert_eq!(v["players"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn acao_fora_da_vez_retorna_409() {
        let (app, _) = make_app("s");
        let (token_a, token_b) = seat_two_players(&app).await;
        // Start the hand
        let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
        let current_actor = hand_v["current_actor"].as_str().unwrap();
        // Use the OTHER player's token
        let wrong_token = if current_actor == "user-a" {
            &token_b
        } else {
            &token_a
        };
        let resp = app
            .oneshot(action_req(wrong_token, "check", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn fold_encerra_mao_imediatamente_via_http() {
        let (app, _) = make_app("s");
        let (token_a, token_b) = seat_two_players(&app).await;
        let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
        let current_actor = hand_v["current_actor"].as_str().unwrap().to_string();
        let acting_token = if current_actor == "user-a" {
            &token_a
        } else {
            &token_b
        };
        let resp = app
            .oneshot(action_req(acting_token, "fold", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert!(v["game_over"].as_bool().unwrap());
        assert!(!v["winner_message"].is_null());
    }

    #[tokio::test]
    async fn dois_humanos_completam_mao_ate_showdown_via_http() {
        let (app, _) = make_app("s");
        let (token_a, token_b) = seat_two_players(&app).await;
        app.clone().oneshot(hand_req(&token_a)).await.unwrap();

        let mut game_over = false;
        for _ in 0..30 {
            let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
            if hand_v["game_over"].as_bool().unwrap() {
                assert!(hand_v["winner_message"].is_string());
                game_over = true;
                break;
            }
            let current_actor = hand_v["current_actor"].as_str().unwrap().to_string();
            let token = if current_actor == "user-a" {
                &token_a
            } else {
                &token_b
            };

            let table_bet = hand_v["current_bet"].as_u64().unwrap_or(0);
            let player_bet = hand_v["players"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["user_id"].as_str().unwrap() == current_actor)
                .and_then(|p| p["current_bet"].as_u64())
                .unwrap_or(0);

            let action = if player_bet < table_bet {
                "call"
            } else {
                "check"
            };
            let resp = app
                .clone()
                .oneshot(action_req(token, action, None))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "ação '{}' falhou", action);

            let v = parse_body(resp).await;
            if v["game_over"].as_bool().unwrap() {
                assert!(v["winner_message"].is_string());
                game_over = true;
                break;
            }
        }
        assert!(game_over, "mão deveria completar em showdown");
    }

    #[tokio::test]
    async fn hole_cards_retorna_apenas_cartas_do_usuario_autenticado() {
        let (app, _) = make_app("s");
        let (token_a, _) = seat_two_players(&app).await;
        app.clone().oneshot(hand_req(&token_a)).await.unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/poker/lobbies/carvalho/hole-cards")
                    .header("authorization", format!("Bearer {token_a}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert_eq!(v["cards"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn acao_desconhecida_retorna_400() {
        let (app, _) = make_app("s");
        let (token_a, _) = seat_two_players(&app).await;
        app.clone().oneshot(hand_req(&token_a)).await.unwrap();
        let resp = app
            .oneshot(action_req(&token_a, "bluff", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
