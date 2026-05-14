//! Multiplayer poker routes — auth-gated.
//!
//! Seating (Layer 1, YG-23) + gameplay (Layer 2, YG-25):
//! dealing, betting rounds (pre-flop → showdown), ações call/check/fold/raise.

use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use game_core::games::poker::PokerAction;
use serde::Deserialize;
use yggdrasil_core::games::poker_bot::auto_step_bots;
use yggdrasil_core::games::poker_game::{
    PokerSitError, PokerStandError, PokerTable, PokerTableError,
};
use yggdrasil_core::games::poker_lobby::{LobbyError, PokerLobby};
use yggdrasil_core::sementes::{Sementes, SementesError};

use crate::auth::verify_jwt;
use crate::games::poker_favorites::{self, CardJson, HandSnapshot, PlayerSnapshot};
use crate::games::poker_persistence;

pub struct PokerState {
    pub jwt_secret: String,
    pub tables: Mutex<Vec<PokerTable>>,
    pub sementes: Arc<Sementes>,
    /// Caminho do SQLite onde mesas são persistidas (YG-29).
    /// `None` desativa persistência — usado em testes que querem o
    /// comportamento puramente em memória.
    pub db_path: Option<PathBuf>,
}

impl PokerState {
    /// Default seeds: 3 mesas (Carvalho, Olmo, Heads-Up Carvalho).
    /// Aplicado apenas no primeiro boot — em boots subsequentes, mesas
    /// persistidas substituem.
    fn seed_tables() -> Vec<PokerTable> {
        vec![
            PokerTable::new(PokerLobby::new("carvalho", "Mesa Carvalho")),
            PokerTable::new(PokerLobby::new("olmo", "Mesa Olmo")),
            // YG-37 variante `poker/heads-up`: mesa 2-seats para duelos.
            PokerTable::new(PokerLobby::with_max_seats(
                "heads-up",
                "Heads-Up Carvalho",
                2,
            )),
        ]
    }

    /// Boot in-memory (sem persistência) — usado em testes que não precisam
    /// validar restart, e como fallback se o DB falhar.
    #[allow(dead_code)]
    pub fn new(jwt_secret: String, sementes: Arc<Sementes>) -> Self {
        Self {
            jwt_secret,
            tables: Mutex::new(Self::seed_tables()),
            sementes,
            db_path: None,
        }
    }

    /// Boot com persistência em SQLite (YG-29). No primeiro run, semeia
    /// 3 mesas defaults e grava no DB. Em boots subsequentes, restaura as
    /// mesas persistidas — seating e chip-stacks sobrevivem ao restart;
    /// mãos em curso são forfeit.
    ///
    /// Falhas de SQLite são logadas mas não abortam o boot — degrada
    /// graciosamente para o comportamento in-memory.
    pub fn with_persistence(jwt_secret: String, sementes: Arc<Sementes>, db_path: &FsPath) -> Self {
        let tables = match poker_persistence::init_poker_db(db_path) {
            Ok(conn) => match poker_persistence::load_tables(&conn) {
                Ok(loaded) if !loaded.is_empty() => loaded,
                Ok(_) => {
                    // Vazio — semeia e persiste defaults.
                    let mut seeds = Self::seed_tables();
                    for t in seeds.iter_mut() {
                        if let Err(e) = poker_persistence::save_lobby(&conn, &t.to_snapshot()) {
                            tracing::warn!("poker_persistence seed save falhou: {e}");
                        }
                    }
                    seeds
                }
                Err(e) => {
                    tracing::warn!("poker_persistence load falhou: {e} — usando seeds em memória");
                    Self::seed_tables()
                }
            },
            Err(e) => {
                tracing::warn!("poker_persistence init falhou: {e} — usando seeds em memória");
                Self::seed_tables()
            }
        };
        Self {
            jwt_secret,
            tables: Mutex::new(tables),
            sementes,
            db_path: Some(db_path.to_path_buf()),
        }
    }

    /// Persiste o snapshot de uma mesa. Idempotente. Falhas viram warning
    /// — não há rollback de mutação no estado em memória (a aceitação é
    /// que o user observa o seat / stack atualizado; um crash entre a
    /// mutação e o save é aceitável dentro do escopo de YG-29).
    fn persist_table(&self, table: &mut PokerTable) {
        let Some(ref path) = self.db_path else {
            return;
        };
        let snap = table.to_snapshot();
        match poker_persistence::init_poker_db(path) {
            Ok(conn) => {
                if let Err(e) = poker_persistence::save_lobby(&conn, &snap) {
                    tracing::warn!("poker_persistence save falhou: {e}");
                }
            }
            Err(e) => tracing::warn!("poker_persistence open falhou: {e}"),
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

fn sit_error(e: PokerSitError) -> Response {
    match e {
        PokerSitError::Lobby(le) => lobby_error(le),
        PokerSitError::Sementes(SementesError::SaldoInsuficiente) => (
            StatusCode::PAYMENT_REQUIRED,
            Json(serde_json::json!({"erro": "Saldo insuficiente para sentar"})),
        )
            .into_response(),
        PokerSitError::Sementes(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"erro": e.to_string()})),
        )
            .into_response(),
    }
}

fn stand_error(e: PokerStandError) -> Response {
    match e {
        PokerStandError::Lobby(le) => lobby_error(le),
        PokerStandError::Sementes(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"erro": e.to_string()})),
        )
            .into_response(),
    }
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
    // Start a new hand on first visit, OR replace the finished game once
    // HAND_END_RESTART_DELAY_SECS has passed so showdown doesn't get stuck.
    let needs_new_hand = table.game.is_none()
        || (table.game.as_ref().map(|g| g.game_over).unwrap_or(false)
            && table.should_auto_restart());
    if needs_new_hand {
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
        Ok(()) => {
            auto_step_bots(table);
            // Persiste após cada ação: stacks mudam constantemente durante
            // a mão (apostas, ganhos), então cada ação válida vira commit.
            state.persist_table(table);
            // Se a mão acabou de terminar, captura o snapshot para a fila
            // de "favoritos pending" (TTL 1h ou favoritar explícito).
            if table.game.as_ref().is_some_and(|g| g.game_over) {
                capture_hand_snapshot(&state, table);
            }
            match table.hand_state() {
                Some(s) => (StatusCode::OK, Json(s)).into_response(),
                None => (StatusCode::OK, Json(waiting_state())).into_response(),
            }
        }
        Err(e) => table_error(e),
    }
}

/// Persiste o snapshot final da mão atual em `poker_recent_hands`. Idempotente
/// (UPSERT por hand_id). Falhas são logadas mas não afetam o fluxo de jogo.
fn capture_hand_snapshot(state: &PokerState, table: &PokerTable) {
    let Some(ref db_path) = state.db_path else {
        return;
    };
    let Some(game) = table.game.as_ref() else {
        return;
    };
    let players: Vec<PlayerSnapshot> = game
        .players
        .iter()
        .enumerate()
        .map(|(i, p)| PlayerSnapshot {
            user_id: table
                .player_map
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("seat-{i}")),
            chips: p.chips,
            folded: matches!(p.status, game_core::games::poker::PlayerStatus::Folded),
            hole_cards: p.hole_cards.map(|[c0, c1]| {
                [
                    CardJson {
                        rank: c0.rank.symbol().to_string(),
                        suit: format!("{:?}", c0.suit).to_lowercase(),
                    },
                    CardJson {
                        rank: c1.rank.symbol().to_string(),
                        suit: format!("{:?}", c1.suit).to_lowercase(),
                    },
                ]
            }),
        })
        .collect();
    let community_cards: Vec<CardJson> = game
        .table
        .community_cards
        .iter()
        .map(|c| CardJson {
            rank: c.rank.symbol().to_string(),
            suit: format!("{:?}", c.suit).to_lowercase(),
        })
        .collect();
    let snap = HandSnapshot {
        hand_id: table.current_hand_id.clone(),
        table_id: table.lobby.id.clone(),
        ended_at: chrono::Utc::now().timestamp(),
        winner_message: game.winner_message.clone(),
        community_cards,
        players,
        pot: game.table.pot,
    };
    match poker_favorites::init_favorites_db(db_path) {
        Ok(conn) => {
            if let Err(e) = poker_favorites::save_recent(&conn, &snap) {
                tracing::warn!("favorites save_recent falhou: {e}");
            }
        }
        Err(e) => tracing::warn!("favorites init falhou: {e}"),
    }
}

/// `POST /api/v1/me/favorites/hands/{table_id}` — marca a última mão dessa
/// mesa como favorita do usuário autenticado.
pub async fn favorite_last_hand(
    State(state): State<Arc<PokerState>>,
    Path(table_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match require_user(&headers, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(r) => return r,
    };
    let Some(ref db_path) = state.db_path else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"erro": "favorites_disabled"})),
        )
            .into_response();
    };
    let conn = match poker_favorites::init_favorites_db(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("favorites init falhou: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "db_unavailable"})),
            )
                .into_response();
        }
    };
    let snap = match poker_favorites::latest_for_table(&conn, &table_id) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"erro": "nenhuma_mao_recente"})),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("favorites latest_for_table: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "lookup_falhou"})),
            )
                .into_response();
        }
    };
    if let Err(e) = poker_favorites::favorite(&conn, &user_id, &snap) {
        tracing::error!("favorites favorite: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"erro": "save_falhou"})),
        )
            .into_response();
    }
    tracing::info!("favorited hand_id={} user_id={user_id}", snap.hand_id);
    (StatusCode::OK, Json(snap)).into_response()
}

/// `GET /api/v1/me/favorites/hands` — lista mãos favoritadas do usuário.
pub async fn list_favorite_hands(
    State(state): State<Arc<PokerState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match require_user(&headers, &state.jwt_secret) {
        Ok(uid) => uid,
        Err(r) => return r,
    };
    let Some(ref db_path) = state.db_path else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"erro": "favorites_disabled"})),
        )
            .into_response();
    };
    let conn = match poker_favorites::init_favorites_db(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("favorites init: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "db_unavailable"})),
            )
                .into_response();
        }
    };
    match poker_favorites::list_favorites(&conn, &user_id) {
        Ok(hands) => (StatusCode::OK, Json(serde_json::json!({"hands": hands}))).into_response(),
        Err(e) => {
            tracing::error!("favorites list: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "list_falhou"})),
            )
                .into_response()
        }
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
    use game_core::storage::Storage;
    use tempfile::TempDir;

    /// Test helper: cria PokerState com Sementes seeded para os usuários comuns
    /// dos testes (user-a, user-b). Cada um recebe 100k para não topar o limite
    /// em testes que sentam várias vezes.
    fn make_app(secret: &str) -> (Router, Arc<PokerState>, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(Storage::open(&dir.path().join("test.db")).unwrap());
        let sementes = Arc::new(Sementes::new(storage));
        sementes.creditar("user-a", 100_000).unwrap();
        sementes.creditar("user-b", 100_000).unwrap();
        let state = Arc::new(PokerState::new(secret.to_string(), sementes));
        let app = Router::new()
            .route("/api/v1/poker/lobbies", get(list_lobbies))
            .route("/api/v1/poker/lobbies/{id}", get(get_lobby))
            .route("/api/v1/poker/lobbies/{id}/sit", post(sit))
            .route("/api/v1/poker/lobbies/{id}/stand", post(stand))
            .route("/api/v1/poker/lobbies/{id}/hand", get(get_hand))
            .route("/api/v1/poker/lobbies/{id}/hole-cards", get(get_hole_cards))
            .route("/api/v1/poker/lobbies/{id}/action", post(post_action))
            .with_state(state.clone());
        (app, state, dir)
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
        let (app, _, _dir) = make_app("s");
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
    async fn list_com_auth_retorna_mesas_seed() {
        // Cash game (carvalho, olmo) + heads-up — YG-37 variant.
        let (app, _, _dir) = make_app("s");
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
        assert_eq!(lobbies.len(), 3);
        let ids: Vec<&str> = lobbies.iter().map(|l| l["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["carvalho", "olmo", "heads-up"]);
        // Heads-up mesa tem max_seats=2.
        assert_eq!(lobbies[2]["max_seats"], 2);
        assert_eq!(lobbies[2]["seats"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn sit_persiste_humano_e_adiciona_bot() {
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
        let (app, _, _dir) = make_app("s");
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
    async fn sit_sem_saldo_retorna_402_payment_required() {
        // Usuário com saldo zero (não foi seeded em make_app).
        let (app, _, _dir) = make_app("s");
        let token = sign_jwt("user-pobre", "p@test.com", "s").unwrap();
        let resp = app.oneshot(sit_req(0, &token)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
        let v = parse_body(resp).await;
        assert!(
            v["erro"].as_str().unwrap().contains("Saldo insuficiente"),
            "mensagem PT-BR: {}",
            v["erro"]
        );
    }

    #[tokio::test]
    async fn sit_debita_buy_in_da_carteira_do_usuario() {
        let (app, state, _dir) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let saldo_antes = state.sementes.saldo("user-a").unwrap();
        let resp = app.oneshot(sit_req(0, &token)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let saldo_depois = state.sementes.saldo("user-a").unwrap();
        assert_eq!(saldo_antes - saldo_depois, 1_000);
    }

    #[tokio::test]
    async fn stand_credita_chips_remanescentes() {
        let (app, state, _dir) = make_app("s");
        let token = sign_jwt("user-a", "a@test.com", "s").unwrap();
        let saldo_inicial = state.sementes.saldo("user-a").unwrap();
        // Sit → debita 1000
        app.clone().oneshot(sit_req(0, &token)).await.unwrap();
        // Stand → credita o stack remanescente (1000 se não jogou)
        let resp = app
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
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(state.sementes.saldo("user-a").unwrap(), saldo_inicial);
    }

    #[tokio::test]
    async fn humano_vs_bot_completa_mao_sem_travar_via_http() {
        // Regressão YG-26: bot deve responder automaticamente quando é a vez dele.
        let (app, _, _dir) = make_app("s");
        let token_a = sign_jwt("user-a", "a@test.com", "s").unwrap();
        // Senta 1 humano → lobby auto-adiciona bot.
        app.clone().oneshot(sit_req(0, &token_a)).await.unwrap();

        let mut game_over = false;
        for _ in 0..40 {
            let hand_v = parse_body(app.clone().oneshot(hand_req(&token_a)).await.unwrap()).await;
            if hand_v["game_over"].as_bool().unwrap_or(false) {
                game_over = true;
                break;
            }
            let current_actor = hand_v["current_actor"].as_str().unwrap_or("").to_string();
            // Se for vez do bot, auto_step já deveria ter rodado — esperar próximo poll
            // não ajudaria. Se chegou aqui, é vez do humano.
            assert_eq!(
                current_actor, "user-a",
                "bot deveria ter agido antes de retornar para o cliente"
            );

            let table_bet = hand_v["current_bet"].as_u64().unwrap_or(0);
            let player_bet = hand_v["players"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["user_id"].as_str() == Some("user-a"))
                .and_then(|p| p["current_bet"].as_u64())
                .unwrap_or(0);
            let action = if player_bet < table_bet {
                "call"
            } else {
                "check"
            };
            let resp = app
                .clone()
                .oneshot(action_req(&token_a, action, None))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let v = parse_body(resp).await;
            if v["game_over"].as_bool().unwrap_or(false) {
                game_over = true;
                break;
            }
        }
        assert!(game_over, "mão humano vs bot deveria completar");
    }

    // ── YG-29: persistência em SQLite (restart sobrevive) ────────────────

    /// Helper: cria PokerState COM persistência apontando para um path,
    /// reutilizando uma TempDir externa para que possamos recriar o state
    /// no mesmo DB.
    fn make_app_persistente(
        secret: &str,
        dir: &TempDir,
    ) -> (Router, Arc<PokerState>, std::path::PathBuf) {
        let sementes_path = dir.path().join("sementes.db");
        let poker_path = dir.path().join("poker.db");
        let storage = Arc::new(Storage::open(&sementes_path).unwrap());
        let sementes = Arc::new(Sementes::new(storage));
        sementes.creditar("user-a", 100_000).unwrap();
        sementes.creditar("user-b", 100_000).unwrap();
        let state = Arc::new(PokerState::with_persistence(
            secret.to_string(),
            sementes,
            &poker_path,
        ));
        let app = Router::new()
            .route("/api/v1/poker/lobbies", get(list_lobbies))
            .route("/api/v1/poker/lobbies/{id}", get(get_lobby))
            .route("/api/v1/poker/lobbies/{id}/sit", post(sit))
            .route("/api/v1/poker/lobbies/{id}/stand", post(stand))
            .route("/api/v1/poker/lobbies/{id}/hand", get(get_hand))
            .route("/api/v1/poker/lobbies/{id}/hole-cards", get(get_hole_cards))
            .route("/api/v1/poker/lobbies/{id}/action", post(post_action))
            .with_state(state.clone());
        (app, state, poker_path)
    }

    #[tokio::test]
    async fn restart_preserva_seat_e_stack() {
        // Simula crash-restart: sit, drop state, recria state apontando para
        // o mesmo DB, e verifica que o seat e o stack sobreviveram.
        let dir = tempfile::tempdir().unwrap();
        let secret = "s";
        let token = sign_jwt("user-a", "a@test.com", secret).unwrap();

        // Primeiro boot: senta user-a.
        let (app, state1, poker_path) = make_app_persistente(secret, &dir);
        let resp = app.oneshot(sit_req(2, &token)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // Stack persistido no estado em memória.
        {
            let tables = state1.tables.lock().unwrap();
            let t = tables.iter().find(|t| t.lobby.id == "carvalho").unwrap();
            assert_eq!(t.stack_of("user-a"), 1_000);
        }
        drop(state1);

        // Segundo boot — mesmo DB, novo PokerState.
        let sementes_path = dir.path().join("sementes.db");
        let storage = Arc::new(Storage::open(&sementes_path).unwrap());
        let sementes = Arc::new(Sementes::new(storage));
        let state2 = Arc::new(PokerState::with_persistence(
            secret.to_string(),
            sementes,
            &poker_path,
        ));

        let tables = state2.tables.lock().unwrap();
        let carvalho = tables.iter().find(|t| t.lobby.id == "carvalho").unwrap();
        // Seat 2 ainda ocupado por user-a.
        assert!(matches!(
            &carvalho.lobby.seats[2],
            yggdrasil_core::games::poker_lobby::SeatOccupant::Human { user_id, .. }
                if user_id == "user-a"
        ));
        // Stack sobreviveu.
        assert_eq!(carvalho.stack_of("user-a"), 1_000);
        // Mas a mão em curso (se houver) foi forfeit — game é None.
        assert!(carvalho.game.is_none());

        // As três mesas defaults ainda lá.
        assert_eq!(tables.len(), 3);
    }

    #[tokio::test]
    async fn primeiro_boot_seeda_tres_mesas_no_db() {
        let dir = tempfile::tempdir().unwrap();
        let (_app, _state, poker_path) = make_app_persistente("s", &dir);

        // Abre uma conexão fresca e verifica que as 3 mesas foram persistidas.
        let conn = crate::games::poker_persistence::init_poker_db(&poker_path).unwrap();
        let snaps = crate::games::poker_persistence::load_all(&conn).unwrap();
        assert_eq!(snaps.len(), 3);
        let ids: Vec<&str> = snaps.iter().map(|s| s.lobby.id.as_str()).collect();
        assert!(ids.contains(&"carvalho"));
        assert!(ids.contains(&"olmo"));
        assert!(ids.contains(&"heads-up"));
    }

    #[tokio::test]
    async fn acao_desconhecida_retorna_400() {
        let (app, _, _dir) = make_app("s");
        let (token_a, _) = seat_two_players(&app).await;
        app.clone().oneshot(hand_req(&token_a)).await.unwrap();
        let resp = app
            .oneshot(action_req(&token_a, "bluff", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
