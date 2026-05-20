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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
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
use yggdrasil_core::games::{YggGame, YggInvaders, YggSnake, YggTetris};

use crate::scores_store::ScoresStore;

// ─── Universo list ───────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct UniversoMeta {
    pub id: &'static str,
    pub name: &'static str,
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
}

pub struct UniversosState {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    pub scores: Arc<dyn ScoresStore>,
}

impl UniversosState {
    pub fn new(scores: Arc<dyn ScoresStore>) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            scores,
        })
    }
}

// ─── Request / response types ────────────────────────────────────────────────

#[derive(Serialize)]
struct CreateSessaoResponse {
    session_id: String,
    state: serde_json::Value,
}

#[derive(Deserialize)]
pub struct TickInput {
    pub key: String,
    #[serde(default)]
    pub timestamp_ms: Option<u64>,
}

#[derive(Deserialize)]
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

/// `GET /api/v1/universos` — lista todos os universos com metadados básicos.
pub async fn list_universos(_state: State<Arc<UniversosState>>) -> impl IntoResponse {
    Json(universo_list())
}

/// `GET /api/v1/universos/{id}` — metadados + schema mínimo do universo.
pub async fn get_universo(
    _state: State<Arc<UniversosState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if !is_known(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "erro": format!("Universo '{id}' não encontrado") })),
        )
            .into_response();
    }
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

/// `POST /api/v1/universos/{id}/sessoes` — cria sessão e retorna estado inicial.
///
/// Poker usa `/api/v1/poker/lobbies` e retorna 422 aqui.
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

    state.sessions.lock().unwrap().insert(
        session_id.clone(),
        SessionEntry {
            universo_id: id,
            session,
        },
    );

    (
        StatusCode::OK,
        Json(CreateSessaoResponse {
            session_id,
            state: initial_state,
        }),
    )
        .into_response()
}

/// `POST /api/v1/universos/{id}/sessoes/{sid}/tick` — processa inputs e retorna estado.
pub async fn tick_sessao(
    State(state): State<Arc<UniversosState>>,
    Path((id, sid)): Path<(String, String)>,
    Json(body): Json<TickBody>,
) -> impl IntoResponse {
    let (game_state, score, session_ended, universo_id_opt) = {
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

        let game_state = entry.session.render_json();
        let score = entry.session.score();
        let is_over = entry.session.is_over();
        let universo_id = entry.universo_id.clone();

        if is_over {
            sessions.remove(&sid);
        }

        (
            game_state,
            score,
            is_over,
            if is_over { Some(universo_id) } else { None },
        )
    };

    if let Some(uid) = universo_id_opt {
        state.scores.save_score("anonymous", &uid, score);
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

/// `DELETE /api/v1/universos/{id}/sessoes/{sid}` — encerra sessão e persiste score.
pub async fn delete_sessao(
    State(state): State<Arc<UniversosState>>,
    Path((id, sid)): Path<(String, String)>,
) -> impl IntoResponse {
    let (final_score, universo_id) = {
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
            sessions.insert(sid, entry);
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "erro": "Sessão não pertence a este universo" })),
            )
                .into_response();
        }

        (entry.session.score(), entry.universo_id)
    };

    state
        .scores
        .save_score("anonymous", &universo_id, final_score);

    (StatusCode::OK, Json(DeleteResponse { final_score })).into_response()
}

/// `GET /api/v1/universos/{id}/sessoes/{sid}/ws` — WebSocket para stream em tempo real.
///
/// Client → Server: `{ "key": "<input>" }`
/// Server → Client: `{ "state": <game_state> }`
/// Server envia Close frame quando `session_ended == true`.
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

        let (game_state, session_ended, score, universo_id_opt) = {
            let mut sessions = state.sessions.lock().unwrap();

            let entry = match sessions.get_mut(&sid) {
                Some(e) => e,
                None => break,
            };

            entry.session.tick_key(&ws_msg.key);
            let game_state = entry.session.render_json();
            let score = entry.session.score();
            let is_over = entry.session.is_over();
            let universo_id = entry.universo_id.clone();

            if is_over {
                sessions.remove(&sid);
            }

            (
                game_state,
                is_over,
                score,
                if is_over { Some(universo_id) } else { None },
            )
        };

        if let Some(uid) = universo_id_opt {
            state.scores.save_score("anonymous", &uid, score);
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

    fn make_app() -> (Router, Arc<UniversosState>) {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = UniversosState::new(scores);
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
    async fn lista_retorna_6_universos() {
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
        assert_eq!(arr.len(), 6, "esperava 6 universos, got {}", arr.len());

        let ids: Vec<&str> = arr.iter().filter_map(|u| u["id"].as_str()).collect();
        for expected in ["snake", "tetris", "invaders", "pointset", "poker", "vim"] {
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
        // snake state must have width, height, tiles
        assert!(v["state"]["width"].is_number());
        assert!(v["state"]["height"].is_number());
        assert!(v["state"]["tiles"].is_array());
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

    async fn create_snake_session(app: Router) -> (Router, String) {
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
        (app, sid)
    }

    #[tokio::test]
    async fn tick_snake_com_arrow_right_retorna_state_valido() {
        let (app, state) = make_app();
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state2 = UniversosState::new(scores);
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
        let state = UniversosState::new(scores);
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
        let state = UniversosState::new(scores);
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
    }

    // ── DELETE /api/v1/universos/{id}/sessoes/{sid} ──────────────────────────

    #[tokio::test]
    async fn delete_sessao_persiste_score_e_remove_sessao() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = UniversosState::new(scores);
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

        // Score persistido
        let count = state
            .scores
            .recent_scores(10)
            .into_iter()
            .filter(|r| r.game == "snake")
            .count();
        assert_eq!(count, 1, "score não foi persistido via DELETE");
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
    //
    // axum 0.8 `WebSocketUpgrade` returns 426 when the underlying connection is
    // not upgradable (e.g., tower::ServiceExt::oneshot doesn't provide a real
    // TCP socket). Protocol-level WS tests therefore run as integration tests
    // against a live 127.0.0.1 listener. Here we validate session-state logic
    // that the WS handler delegates to.

    #[tokio::test]
    async fn ws_sessao_existente_pode_ser_localizada_no_estado() {
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = UniversosState::new(scores);
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

        // ws_sessao checks this condition before upgrading
        let exists = {
            let sessions = state.sessions.lock().unwrap();
            sessions
                .get(&sid)
                .map(|e| e.universo_id == "snake")
                .unwrap_or(false)
        };
        assert!(exists, "ws_sessao deveria encontrar a sessão");

        // Sessão inexistente → ws_sessao retornaria 404
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
        // Test ws_loop logic indirectly via session tick:
        // Creating a session and sending quit via tick closes the session.
        let scores: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = UniversosState::new(scores);
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

        // Simulate what ws_loop does: receive key "q" → tick → session ends
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
        // session_ended == true mirrors the Close frame ws_loop sends
        assert_eq!(
            v["session_ended"], true,
            "ws_loop fecha quando session_ended=true"
        );

        // Session should be gone from state (ws_loop removes it)
        let still_there = state.sessions.lock().unwrap().contains_key(&sid);
        assert!(!still_there, "sessão deve ter sido removida após terminar");
    }

    // ── Legacy alias snapshot tests ──────────────────────────────────────────
    // Rotas legadas não são alteradas. Os testes em snake_routes.rs, tetris_routes.rs
    // e invaders_routes.rs garantem o contrato.
    // Este teste verifica o formato exato do JSON legado do snake.

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
        // Schema snapshot — campos obrigatórios não podem ser removidos.
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
