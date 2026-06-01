//! Unified API — `/api/v1/universos` — session routes + WebSocket.
//!
//! Single entry point for all single-player universos. Poker multiplayer
//! remains at `/api/v1/poker/...` for v1.0; unified WS integration comes in v1.1.
//!
//! # Routes
//!
//! | Method   | Path                                          | Purpose                      |
//! |----------|-----------------------------------------------|------------------------------|
//! | GET      | /api/v1/universos                             | list all 6 universos         |
//! | GET      | /api/v1/universos/{id}                        | metadata + schema            |
//! | POST     | /api/v1/universos/{id}/sessoes                | create session               |
//! | POST     | /api/v1/universos/{id}/sessoes/{sid}/tick     | tick one or more inputs      |
//! | DELETE   | /api/v1/universos/{id}/sessoes/{sid}          | end session + persist score  |
//! | GET (WS) | /api/v1/universos/{id}/sessoes/{sid}/ws       | real-time stream             |
//! | GET      | /api/v1/admin/analytics                       | funnel + engagement metrics  |

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
};
use game_core::engine::{Direction, Input, Universe};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use yggdrasil_core::games::{YggGame, YggInvaders, YggSnake, YggTetris};

use crate::scores_store::ScoresStore;
use crate::telemetria::TelemetriaDb;

// ─── Universo list ───────────────────────────────────────────────────────────

#[derive(Serialize, Clone, ToSchema)]
pub struct UniversoMeta {
    #[schema(value_type = String)]
    pub id: &'static str,
    #[schema(value_type = String)]
    pub name: &'static str,
    #[schema(value_type = String)]
    pub version: &'static str,
    pub max_players: u32,
    pub api_version: u32,
}

pub fn universo_list() -> Vec<UniversoMeta> {
    vec![
        UniversoMeta {
            id: "snake",
            name: "Snake",
            version: "1.0",
            max_players: 1,
            api_version: 1,
        },
        UniversoMeta {
            id: "tetris",
            name: "Tetris",
            version: "1.0",
            max_players: 1,
            api_version: 1,
        },
        UniversoMeta {
            id: "invaders",
            name: "Space Invaders",
            version: "1.0",
            max_players: 1,
            api_version: 1,
        },
        UniversoMeta {
            id: "pointset",
            name: "Pointset",
            version: "1.0",
            max_players: 1,
            api_version: 1,
        },
        UniversoMeta {
            id: "poker",
            name: "Pôquer",
            version: "1.0",
            max_players: 6,
            api_version: 1,
        },
        UniversoMeta {
            id: "vim",
            name: "Vim",
            version: "1.0",
            max_players: 1,
            api_version: 1,
        },
        // YG-85: neuro — atlas 3D de anatomia. Não é tick-based; a página é o
        // viewer Godot em /universos/neuro. max_players alto = colaborativo.
        UniversoMeta {
            id: "neuro",
            name: "Neuro — Atlas 3D",
            version: "1.0",
            max_players: 999,
            api_version: 1,
        },
    ]
}

fn is_known(id: &str) -> bool {
    matches!(
        id,
        "snake" | "tetris" | "invaders" | "pointset" | "poker" | "vim"
    )
}

// ─── Session trait ───────────────────────────────────────────────────────────

trait UniversoSession: Send {
    fn tick_key(&mut self, key: &str);
    fn render_json(&self) -> serde_json::Value;
    fn score(&self) -> u32;
    fn is_over(&self) -> bool;
}

// ─── Snake ───────────────────────────────────────────────────────────────────

struct SnakeSession {
    game: YggSnake,
}

impl SnakeSession {
    fn new() -> Self {
        Self {
            game: YggSnake::new(Universe::snake()),
        }
    }

    fn parse_key(key: &str) -> Input {
        match key {
            "ArrowUp" | "k" | "Up" => Input::Move(Direction::Up),
            "ArrowDown" | "j" | "Down" => Input::Move(Direction::Down),
            "ArrowLeft" | "h" | "Left" => Input::Move(Direction::Left),
            "ArrowRight" | "l" | "Right" => Input::Move(Direction::Right),
            "q" | "Quit" => Input::Quit,
            _ => Input::None,
        }
    }
}

impl UniversoSession for SnakeSession {
    fn tick_key(&mut self, key: &str) {
        self.game.tick(Self::parse_key(key));
    }
    fn render_json(&self) -> serde_json::Value {
        serde_json::to_value(self.game.render()).unwrap_or_default()
    }
    fn score(&self) -> u32 {
        self.game.score()
    }
    fn is_over(&self) -> bool {
        self.game.is_over()
    }
}

// ─── Tetris ──────────────────────────────────────────────────────────────────

struct TetrisSession {
    game: YggTetris,
}

impl TetrisSession {
    fn new() -> Self {
        Self {
            game: YggTetris::new(Universe::tetris()),
        }
    }

    fn parse_key(key: &str) -> Input {
        match key {
            "ArrowLeft" | "h" | "Left" => Input::Move(Direction::Left),
            "ArrowRight" | "l" | "Right" => Input::Move(Direction::Right),
            "ArrowDown" | "j" | "Down" => Input::Move(Direction::Down),
            "ArrowUp" | "k" | "Rotate" => Input::Move(Direction::Up),
            " " | "HardDrop" => Input::Action,
            "q" | "Quit" => Input::Quit,
            _ => Input::None,
        }
    }
}

impl UniversoSession for TetrisSession {
    fn tick_key(&mut self, key: &str) {
        self.game.tick(Self::parse_key(key));
    }
    fn render_json(&self) -> serde_json::Value {
        serde_json::to_value(self.game.render()).unwrap_or_default()
    }
    fn score(&self) -> u32 {
        self.game.score()
    }
    fn is_over(&self) -> bool {
        self.game.is_over()
    }
}

// ─── Invaders ────────────────────────────────────────────────────────────────

struct InvadersSession {
    game: YggInvaders,
}

impl InvadersSession {
    fn new() -> Self {
        Self {
            game: YggInvaders::new(Universe::invaders()),
        }
    }

    fn parse_key(key: &str) -> Input {
        match key {
            "ArrowLeft" | "h" | "Left" => Input::Move(Direction::Left),
            "ArrowRight" | "l" | "Right" => Input::Move(Direction::Right),
            " " | "z" | "Shoot" => Input::Action,
            "q" | "Quit" => Input::Quit,
            _ => Input::None,
        }
    }
}

impl UniversoSession for InvadersSession {
    fn tick_key(&mut self, key: &str) {
        self.game.tick(Self::parse_key(key));
    }
    fn render_json(&self) -> serde_json::Value {
        serde_json::to_value(self.game.render()).unwrap_or_default()
    }
    fn score(&self) -> u32 {
        self.game.score()
    }
    fn is_over(&self) -> bool {
        self.game.is_over()
    }
}

// ─── Stub (pointset, vim) ────────────────────────────────────────────────────

struct StubSession {
    ticks: u32,
}

impl StubSession {
    fn new() -> Self {
        Self { ticks: 0 }
    }
}

impl UniversoSession for StubSession {
    fn tick_key(&mut self, _key: &str) {
        self.ticks = self.ticks.saturating_add(1);
    }
    fn render_json(&self) -> serde_json::Value {
        serde_json::json!({ "ticks": self.ticks })
    }
    fn score(&self) -> u32 {
        0
    }
    fn is_over(&self) -> bool {
        false
    }
}

// ─── Session factory ─────────────────────────────────────────────────────────

fn make_session(id: &str) -> Option<Box<dyn UniversoSession>> {
    match id {
        "snake" => Some(Box::new(SnakeSession::new())),
        "tetris" => Some(Box::new(TetrisSession::new())),
        "invaders" => Some(Box::new(InvadersSession::new())),
        "pointset" => Some(Box::new(StubSession::new())),
        "vim" => Some(Box::new(StubSession::new())),
        _ => None,
    }
}

// ─── Shared state ────────────────────────────────────────────────────────────

struct SessionEntry {
    universo_id: String,
    session: Box<dyn UniversoSession>,
    started_at: i64,
    last_tick_at: tokio::time::Instant,
}

pub struct UniversosState {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    pub scores: Arc<dyn ScoresStore>,
    pub telemetria: Arc<TelemetriaDb>,
    admin_token: Option<String>,
}

impl UniversosState {
    pub fn new(scores: Arc<dyn ScoresStore>, telemetria: Arc<TelemetriaDb>) -> Arc<Self> {
        let admin_token = std::env::var("YGGDRASIL_ADMIN_TOKEN").ok();
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            scores,
            telemetria,
            admin_token,
        })
    }

    #[cfg(test)]
    pub fn for_test(
        scores: Arc<dyn ScoresStore>,
        telemetria: Arc<TelemetriaDb>,
        admin_token: Option<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            scores,
            telemetria,
            admin_token,
        })
    }
}

// ─── Cleanup job ─────────────────────────────────────────────────────────────

/// Removes sessions inactive for > 30 minutes and records them as abandoned.
/// Exposed for testing; in production use [`spawn_cleanup_job`].
pub async fn run_cleanup_once(state: &Arc<UniversosState>) {
    let cutoff = Duration::from_secs(30 * 60);
    let now = tokio::time::Instant::now();

    let abandoned: Vec<(String, String)> = {
        let mut sessions = state.sessions.lock().unwrap();
        let stale: Vec<String> = sessions
            .iter()
            .filter(|(_, e)| now.duration_since(e.last_tick_at) > cutoff)
            .map(|(k, _)| k.clone())
            .collect();
        stale
            .into_iter()
            .filter_map(|sid| sessions.remove(&sid).map(|e| (sid, e.universo_id)))
            .collect()
    };

    if !abandoned.is_empty() {
        let ended_at = chrono::Utc::now().timestamp_millis();
        for (sid, universe_id) in abandoned {
            state
                .telemetria
                .session_abandon(&sid, &universe_id, ended_at);
        }
    }
}

/// Spawns a background task that calls [`run_cleanup_once`] every 5 minutes.
pub fn spawn_cleanup_job(state: Arc<UniversosState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            run_cleanup_once(&state).await;
        }
    });
}

// ─── Request / response types ────────────────────────────────────────────────

#[derive(Serialize)]
struct CreateSessaoResponse {
    session_id: String,
    state: serde_json::Value,
}

#[derive(Deserialize, ToSchema)]
pub struct TickInput {
    pub key: String,
    #[serde(default)]
    pub timestamp_ms: Option<u64>,
}

#[derive(Deserialize, ToSchema)]
pub struct TickBody {
    pub inputs: Vec<TickInput>,
}

#[derive(Serialize)]
struct TickResponse {
    state: serde_json::Value,
    session_ended: bool,
    score: u32,
}

#[derive(Serialize)]
struct DeleteResponse {
    final_score: u32,
}

#[derive(Deserialize)]
struct WsMessage {
    key: String,
}

#[derive(Serialize)]
struct WsStateMessage {
    state: serde_json::Value,
}

// ─── Route handlers ──────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/v1/universos",
    responses(
        (status = 200, description = "Lista de universos disponíveis", body = Vec<UniversoMeta>),
    ),
    tag = "universos"
)]
pub async fn list_universos(_state: State<Arc<UniversosState>>) -> impl IntoResponse {
    Json(universo_list())
}

#[utoipa::path(
    get,
    path = "/api/v1/universos/{id}",
    params(("id" = String, Path, description = "ID do universo")),
    responses(
        (status = 200, description = "Metadados e schema do universo", body = crate::openapi::UniversoMetaSchemaDoc),
        (status = 404, description = "Universo não encontrado", body = crate::openapi::ErrorDoc),
    ),
    tag = "universos"
)]
pub async fn get_universo(
    State(state): State<Arc<UniversosState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !is_known(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "erro": format!("Universo '{id}' não encontrado") })),
        )
            .into_response();
    }
    state.telemetria.universe_view(&id);
    let meta = serde_json::json!({
        "id": id,
        "api_version": 1,
        "sessoes": format!("/api/v1/universos/{id}/sessoes"),
        "state_schema": { "type": "object" },
        "input_schema": {
            "type": "object",
            "properties": { "key": { "type": "string" } },
            "required": ["key"]
        }
    });
    (StatusCode::OK, Json(meta)).into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/universos/{id}/sessoes",
    params(("id" = String, Path, description = "ID do universo")),
    responses(
        (status = 200, description = "Sessão criada", body = crate::openapi::CreateSessaoDoc),
        (status = 404, description = "Universo não encontrado", body = crate::openapi::ErrorDoc),
        (status = 422, description = "Pôquer usa /api/v1/poker/lobbies"),
    ),
    tag = "universos"
)]
pub async fn create_sessao(
    State(state): State<Arc<UniversosState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if id == "poker" {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "erro": "Sessões de pôquer via /api/v1/poker/lobbies"
            })),
        )
            .into_response();
    }

    let session = match make_session(&id) {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "erro": format!("Universo '{id}' não encontrado") })),
            )
                .into_response();
        }
    };

    let initial_state = session.render_json();
    let session_id = nanoid!();
    let started_at = chrono::Utc::now().timestamp_millis();
    let now = tokio::time::Instant::now();

    state.sessions.lock().unwrap().insert(
        session_id.clone(),
        SessionEntry {
            universo_id: id.clone(),
            session,
            started_at,
            last_tick_at: now,
        },
    );

    state
        .telemetria
        .session_create(&session_id, &id, started_at);

    (
        StatusCode::OK,
        Json(CreateSessaoResponse {
            session_id,
            state: initial_state,
        }),
    )
        .into_response()
}

#[utoipa::path(
    post,
    path = "/api/v1/universos/{id}/sessoes/{sid}/tick",
    params(
        ("id" = String, Path, description = "ID do universo"),
        ("sid" = String, Path, description = "ID da sessão"),
    ),
    request_body = TickBody,
    responses(
        (status = 200, description = "Estado após os inputs", body = crate::openapi::TickResponseDoc),
        (status = 404, description = "Sessão não encontrada", body = crate::openapi::ErrorDoc),
    ),
    tag = "universos"
)]
pub async fn tick_sessao(
    State(state): State<Arc<UniversosState>>,
    Path((id, sid)): Path<(String, String)>,
    Json(body): Json<TickBody>,
) -> impl IntoResponse {
    let (game_state, score, session_ended, completed_opt) = {
        let mut sessions = state.sessions.lock().unwrap();

        let entry = match sessions.get_mut(&sid) {
            Some(e) => e,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "erro": "Sessão não encontrada" })),
                )
                    .into_response();
            }
        };

        if entry.universo_id != id {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "erro": "Sessão não pertence a este universo" })),
            )
                .into_response();
        }

        for input in &body.inputs {
            if !entry.session.is_over() {
                entry.session.tick_key(&input.key);
            }
        }
        entry.last_tick_at = tokio::time::Instant::now();

        let game_state = entry.session.render_json();
        let score = entry.session.score();
        let is_over = entry.session.is_over();
        let universo_id = entry.universo_id.clone();
        let started_at = entry.started_at;

        if is_over {
            sessions.remove(&sid);
        }

        (
            game_state,
            score,
            is_over,
            if is_over {
                Some((universo_id, started_at))
            } else {
                None
            },
        )
    };

    if let Some((uid, started_at)) = completed_opt {
        state.scores.save_score("anonymous", &uid, score);
        let ended_at = chrono::Utc::now().timestamp_millis();
        state
            .telemetria
            .session_complete(&sid, &uid, ended_at, ended_at - started_at, score);
    }

    (
        StatusCode::OK,
        Json(TickResponse {
            state: game_state,
            session_ended,
            score,
        }),
    )
        .into_response()
}

#[utoipa::path(
    delete,
    path = "/api/v1/universos/{id}/sessoes/{sid}",
    params(
        ("id" = String, Path, description = "ID do universo"),
        ("sid" = String, Path, description = "ID da sessão"),
    ),
    responses(
        (status = 200, description = "Sessão encerrada e score persistido", body = crate::openapi::DeleteSessaoDoc),
        (status = 404, description = "Sessão não encontrada", body = crate::openapi::ErrorDoc),
    ),
    tag = "universos"
)]
pub async fn delete_sessao(
    State(state): State<Arc<UniversosState>>,
    Path((id, sid)): Path<(String, String)>,
) -> impl IntoResponse {
    let (final_score, universo_id, started_at) = {
        let mut sessions = state.sessions.lock().unwrap();

        let entry = match sessions.remove(&sid) {
            Some(e) => e,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "erro": "Sessão não encontrada" })),
                )
                    .into_response();
            }
        };

        if entry.universo_id != id {
            sessions.insert(sid.clone(), entry);
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "erro": "Sessão não pertence a este universo" })),
            )
                .into_response();
        }

        (entry.session.score(), entry.universo_id, entry.started_at)
    };

    state
        .scores
        .save_score("anonymous", &universo_id, final_score);

    let ended_at = chrono::Utc::now().timestamp_millis();
    state.telemetria.session_complete(
        &sid,
        &universo_id,
        ended_at,
        ended_at - started_at,
        final_score,
    );

    (StatusCode::OK, Json(DeleteResponse { final_score })).into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/universos/{id}/sessoes/{sid}/ws",
    params(
        ("id" = String, Path, description = "ID do universo"),
        ("sid" = String, Path, description = "ID da sessão"),
    ),
    responses(
        (status = 101, description = "WebSocket upgrade — stream de estado em tempo real"),
        (status = 404, description = "Sessão não encontrada"),
    ),
    tag = "universos"
)]
pub async fn ws_sessao(
    State(state): State<Arc<UniversosState>>,
    Path((id, sid)): Path<(String, String)>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let exists = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .get(&sid)
            .map(|e| e.universo_id == id)
            .unwrap_or(false)
    };

    if !exists {
        return (StatusCode::NOT_FOUND, "Sessão não encontrada").into_response();
    }

    ws.on_upgrade(move |socket| ws_loop(socket, state, sid))
        .into_response()
}

async fn ws_loop(mut socket: WebSocket, state: Arc<UniversosState>, sid: String) {
    loop {
        let msg = match socket.recv().await {
            Some(Ok(m)) => m,
            _ => break,
        };

        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let ws_msg: WsMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let (game_state, session_ended, score, completed_opt) = {
            let mut sessions = state.sessions.lock().unwrap();

            let entry = match sessions.get_mut(&sid) {
                Some(e) => e,
                None => break,
            };

            entry.session.tick_key(&ws_msg.key);
            entry.last_tick_at = tokio::time::Instant::now();

            let game_state = entry.session.render_json();
            let score = entry.session.score();
            let is_over = entry.session.is_over();
            let universo_id = entry.universo_id.clone();
            let started_at = entry.started_at;

            if is_over {
                sessions.remove(&sid);
            }

            (
                game_state,
                is_over,
                score,
                if is_over {
                    Some((universo_id, started_at))
                } else {
                    None
                },
            )
        };

        if let Some((uid, started_at)) = completed_opt {
            state.scores.save_score("anonymous", &uid, score);
            let ended_at = chrono::Utc::now().timestamp_millis();
            state
                .telemetria
                .session_complete(&sid, &uid, ended_at, ended_at - started_at, score);
        }

        let Ok(json) = serde_json::to_string(&WsStateMessage { state: game_state }) else {
            break;
        };

        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }

        if session_ended {
            let _ = socket.send(Message::Close(None)).await;
            break;
        }
    }
}

// ─── Analytics endpoint ──────────────────────────────────────────────────────

pub async fn get_analytics(
    State(state): State<Arc<UniversosState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let admin_token = match &state.admin_token {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "erro": "Token de administração não configurado" })),
            )
                .into_response();
        }
    };

    let provided = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");

    if provided != admin_token {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "erro": "Token inválido" })),
        )
            .into_response();
    }

    let active_counts: HashMap<String, i64> = {
        let sessions = state.sessions.lock().unwrap();
        let mut counts: HashMap<String, i64> = HashMap::new();
        for entry in sessions.values() {
            *counts.entry(entry.universo_id.clone()).or_insert(0) += 1;
        }
        counts
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let since_ms = now_ms - 24 * 60 * 60 * 1000;

    let report = state.telemetria.get_analytics(since_ms, &active_counts);
    (StatusCode::OK, Json(report)).into_response()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::{delete, get, post},
    };
    use tower::ServiceExt;

    use super::*;
    use crate::scores_store::InMemoryScoresStore;
    use crate::telemetria::TelemetriaDb;

    fn make_app() -> (Router, Arc<UniversosState>) {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, None);
        let app = Router::new()
            .route("/api/v1/universos", get(list_universos))
            .route("/api/v1/universos/{id}", get(get_universo))
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}/tick",
                post(tick_sessao),
            )
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}",
                delete(delete_sessao),
            )
            .route("/api/v1/universos/{id}/sessoes/{sid}/ws", get(ws_sessao))
            .route("/api/v1/admin/analytics", get(get_analytics))
            .with_state(state.clone());
        (app, state)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── GET /api/v1/universos ────────────────────────────────────────────────

    #[tokio::test]
    async fn lista_retorna_7_universos() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universos")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 7, "esperava 7 universos, got {}", arr.len());

        let ids: Vec<&str> = arr.iter().filter_map(|u| u["id"].as_str()).collect();
        for expected in [
            "snake", "tetris", "invaders", "pointset", "poker", "vim", "neuro",
        ] {
            assert!(ids.contains(&expected), "'{expected}' ausente na lista");
        }

        for u in arr {
            assert_eq!(u["api_version"], 1);
            assert!(u["max_players"].is_number());
        }
    }

    // ── GET /api/v1/universos/{id} ───────────────────────────────────────────

    #[tokio::test]
    async fn get_universo_retorna_metadata() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universos/snake")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["id"], "snake");
        assert_eq!(v["api_version"], 1);
        assert!(v["sessoes"].is_string());
    }

    #[tokio::test]
    async fn get_universo_desconhecido_retorna_404() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universos/nao-existe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_universo_emite_universe_view() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);
        let app = Router::new()
            .route("/api/v1/universos/{id}", get(get_universo))
            .with_state(state);

        app.oneshot(
            Request::builder()
                .uri("/api/v1/universos/snake")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(
            telemetria.count_events("UNIVERSE_VIEW"),
            1,
            "UNIVERSE_VIEW deve ser emitido"
        );
    }

    // ── POST /api/v1/universos/{id}/sessoes ──────────────────────────────────

    #[tokio::test]
    async fn create_sessao_snake_retorna_session_id_e_state() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["session_id"].is_string(), "session_id ausente");
        assert!(v["state"].is_object(), "state ausente");
        assert!(v["state"]["width"].is_number());
        assert!(v["state"]["height"].is_number());
        assert!(v["state"]["tiles"].is_array());
    }

    #[tokio::test]
    async fn create_sessao_registra_session_create_event() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .with_state(state);

        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/universos/snake/sessoes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(telemetria.count_events("SESSION_CREATE"), 1);
        assert_eq!(telemetria.count_session_records(), 1);
    }

    #[tokio::test]
    async fn create_sessao_tetris_retorna_state_com_board() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/tetris/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["session_id"].is_string());
        assert!(v["state"]["board"].is_array());
    }

    #[tokio::test]
    async fn create_sessao_invaders_retorna_state_com_aliens() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/invaders/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["session_id"].is_string());
        assert!(v["state"]["aliens"].is_array());
    }

    #[tokio::test]
    async fn create_sessao_poker_retorna_422() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/poker/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_sessao_universo_desconhecido_retorna_404() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/nao-existe/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── POST /api/v1/universos/{id}/sessoes/{sid}/tick ───────────────────────

    #[tokio::test]
    async fn tick_snake_com_arrow_right_retorna_state_valido() {
        let (app, state) = make_app();
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state2 = UniversosState::for_test(scores, telemetria, None);
        let app2 = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}/tick",
                post(tick_sessao),
            )
            .with_state(state2.clone());

        let resp = app2
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "inputs": [{ "key": "ArrowRight" }] }).to_string();
        let resp = app2
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/universos/snake/sessoes/{sid}/tick"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["state"].is_object());
        assert!(v["state"]["tiles"].is_array());
        assert!(v["score"].is_number());
        assert_eq!(v["session_ended"], false);

        drop(app);
        drop(state);
    }

    #[tokio::test]
    async fn tick_snake_com_vim_key_l_retorna_state_valido() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}/tick",
                post(tick_sessao),
            )
            .with_state(state);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "inputs": [{ "key": "l" }] }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/universos/snake/sessoes/{sid}/tick"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["state"]["tiles"].is_array());
    }

    #[tokio::test]
    async fn tick_sessao_inexistente_retorna_404() {
        let (app, _) = make_app();
        let body = serde_json::json!({ "inputs": [{ "key": "ArrowRight" }] }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes/nope/tick")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn tick_snake_quit_termina_sessao_e_session_ended_true() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}/tick",
                post(tick_sessao),
            )
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "inputs": [{ "key": "q" }] }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/universos/snake/sessoes/{sid}/tick"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["session_ended"], true);

        // Score deve ter sido persistida
        let count = state
            .scores
            .recent_scores(10)
            .into_iter()
            .filter(|r| r.game == "snake")
            .count();
        assert_eq!(count, 1, "score não foi persistido");

        // SESSION_COMPLETE deve ter sido emitido
        assert_eq!(
            telemetria.count_events("SESSION_COMPLETE"),
            1,
            "SESSION_COMPLETE deve ser emitido ao encerrar via tick"
        );
    }

    // ── DELETE /api/v1/universos/{id}/sessoes/{sid} ──────────────────────────

    #[tokio::test]
    async fn delete_sessao_persiste_score_e_remove_sessao() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}",
                delete(delete_sessao),
            )
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        let resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/universos/snake/sessoes/{sid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["final_score"].is_number());

        let count = state
            .scores
            .recent_scores(10)
            .into_iter()
            .filter(|r| r.game == "snake")
            .count();
        assert_eq!(count, 1, "score não foi persistido via DELETE");

        assert_eq!(
            telemetria.count_events("SESSION_COMPLETE"),
            1,
            "SESSION_COMPLETE deve ser emitido ao deletar sessão"
        );
    }

    #[tokio::test]
    async fn delete_sessao_registra_duration_ms_correto() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}",
                delete(delete_sessao),
            )
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        app.oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/universos/snake/sessoes/{sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        let duration = telemetria.session_duration(&sid);
        assert!(duration.is_some(), "duration_ms deve ser registrado");
        assert!(duration.unwrap() >= 0, "duration_ms deve ser não-negativo");
    }

    #[tokio::test]
    async fn delete_sessao_inexistente_retorna_404() {
        let (app, _) = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/universos/snake/sessoes/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── WS /api/v1/universos/{id}/sessoes/{sid}/ws ───────────────────────────

    #[tokio::test]
    async fn ws_sessao_existente_pode_ser_localizada_no_estado() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .with_state(state.clone());

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        let exists = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&sid)
                .map(|e| e.universo_id == "snake")
                .unwrap_or(false)
        };
        assert!(exists, "ws_sessao deveria encontrar a sessão");

        let not_exists = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get("nope")
                .map(|e| e.universo_id == "snake")
                .unwrap_or(false)
        };
        assert!(!not_exists, "ws_sessao não deve achar 'nope'");
    }

    #[tokio::test]
    async fn ws_loop_envia_state_e_fecha_ao_encerrar_sessao() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, None);
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route(
                "/api/v1/universos/{id}/sessoes/{sid}/tick",
                post(tick_sessao),
            )
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/universos/snake/sessoes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let sid = v["session_id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "inputs": [{ "key": "q" }] }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/universos/snake/sessoes/{sid}/tick"))
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(
            v["session_ended"], true,
            "ws_loop fecha quando session_ended=true"
        );

        let still_there = state.sessions.lock().unwrap().contains_key(&sid);
        assert!(!still_there, "sessão deve ter sido removida após terminar");
    }

    // ── Cleanup job ──────────────────────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn cleanup_abandona_sessoes_inativas_apos_30min() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);

        let session_id = "cleanup-test-session";
        let now_ms = chrono::Utc::now().timestamp_millis();

        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(
                session_id.to_string(),
                SessionEntry {
                    universo_id: "snake".to_string(),
                    session: Box::new(StubSession::new()),
                    started_at: now_ms,
                    last_tick_at: tokio::time::Instant::now(),
                },
            );
        }
        state.telemetria.session_create(session_id, "snake", now_ms);

        // Advance virtual time past the 30-minute threshold
        tokio::time::advance(Duration::from_secs(31 * 60)).await;

        run_cleanup_once(&state).await;

        assert!(
            !state.sessions.lock().unwrap().contains_key(session_id),
            "sessão deve ser removida da memória"
        );
        assert!(
            telemetria.session_abandoned(session_id),
            "session_records.abandoned deve ser 1"
        );
        assert_eq!(
            telemetria.count_events("SESSION_ABANDON"),
            1,
            "SESSION_ABANDON deve ser emitido"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cleanup_nao_abandona_sessoes_recentes() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria.clone(), None);

        let session_id = "recent-session";
        let now_ms = chrono::Utc::now().timestamp_millis();

        {
            let mut sessions = state.sessions.lock().unwrap();
            sessions.insert(
                session_id.to_string(),
                SessionEntry {
                    universo_id: "tetris".to_string(),
                    session: Box::new(StubSession::new()),
                    started_at: now_ms,
                    last_tick_at: tokio::time::Instant::now(),
                },
            );
        }

        // Only 10 minutes elapsed — should NOT be abandoned
        tokio::time::advance(Duration::from_secs(10 * 60)).await;
        run_cleanup_once(&state).await;

        assert!(
            state.sessions.lock().unwrap().contains_key(session_id),
            "sessão recente não deve ser removida"
        );
        assert_eq!(telemetria.count_events("SESSION_ABANDON"), 0);
    }

    // ── GET /api/v1/admin/analytics ──────────────────────────────────────────

    #[tokio::test]
    async fn analytics_sem_token_retorna_401() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, Some("secret".to_string()));
        let app = Router::new()
            .route("/api/v1/admin/analytics", get(get_analytics))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/analytics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn analytics_token_errado_retorna_401() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, Some("secret".to_string()));
        let app = Router::new()
            .route("/api/v1/admin/analytics", get(get_analytics))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/analytics")
                    .header("authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn analytics_sem_admin_token_configurado_retorna_401() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        // admin_token = None → always 401
        let state = UniversosState::for_test(scores, telemetria, None);
        let app = Router::new()
            .route("/api/v1/admin/analytics", get(get_analytics))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/analytics")
                    .header("authorization", "Bearer anything")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn analytics_com_token_correto_retorna_json() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, Some("test-token".to_string()));
        let app = Router::new()
            .route("/api/v1/admin/analytics", get(get_analytics))
            .with_state(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/analytics")
                    .header("authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["period"], "24h");
        assert!(v["generated_at"].is_string());
        assert!(v["universos"].is_array());
        assert!(v["funnel_24h"].is_object());
        assert!(v["funnel_24h"]["universe_views"].is_number());
        assert!(v["funnel_24h"]["session_creates"].is_number());
        assert!(v["funnel_24h"]["session_completions"].is_number());
        assert!(v["funnel_24h"]["conversion_view_to_create_pct"].is_number());
        assert!(v["funnel_24h"]["conversion_create_to_complete_pct"].is_number());
    }

    #[tokio::test]
    async fn analytics_active_now_reflete_sessoes_em_memoria() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let telemetria = Arc::new(TelemetriaDb::in_memory().unwrap());
        let state = UniversosState::for_test(scores, telemetria, Some("tok".to_string()));
        let app = Router::new()
            .route("/api/v1/universos/{id}/sessoes", post(create_sessao))
            .route("/api/v1/admin/analytics", get(get_analytics))
            .with_state(state.clone());

        // Create two snake sessions
        for _ in 0..2 {
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/universos/snake/sessoes")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/admin/analytics")
                    .header("authorization", "Bearer tok")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let v = body_json(resp).await;
        let snake = v["universos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["id"] == "snake")
            .unwrap();
        assert_eq!(
            snake["active_now"], 2,
            "active_now deve refletir sessões em memória"
        );
    }

    // ── Legacy alias snapshot tests ──────────────────────────────────────────

    #[tokio::test]
    async fn legacy_snake_start_retorna_formato_esperado() {
        use crate::games::snake_routes::{make_snake_state, start_game};
        use crate::scores_store::InMemoryScoresStore as Mem;

        let store: Arc<dyn ScoresStore> = Arc::new(Mem::new());
        let snake_state = make_snake_state(store);
        let app = Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .with_state(snake_state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/snake/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(v["id"].is_string(), "campo 'id' ausente ou não-string");
        assert!(
            v["state"]["width"].is_number(),
            "campo 'state.width' ausente"
        );
        assert!(
            v["state"]["height"].is_number(),
            "campo 'state.height' ausente"
        );
        assert!(
            v["state"]["tiles"].is_array(),
            "campo 'state.tiles' ausente"
        );
        assert_eq!(v["score"], 0, "score inicial deve ser 0");
    }
}
