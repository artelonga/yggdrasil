use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use game_core::{
    engine::{Direction, Input, Universe},
    games::GameAction,
};
use nanoid::nanoid;
use yggdrasil_core::games::tetris::TetrisOptions;
use yggdrasil_core::games::{YggGame, YggTetris};

use super::common::{InputRequest, StartResponse, TickResponse, VariantQuery};
use crate::scores_store::ScoresStore;

pub struct TetrisState {
    sessions: Mutex<HashMap<String, YggTetris>>,
    scores: Arc<dyn ScoresStore>,
}

pub fn make_tetris_state(scores: Arc<dyn ScoresStore>) -> Arc<TetrisState> {
    Arc::new(TetrisState {
        sessions: Mutex::new(HashMap::new()),
        scores,
    })
}

fn parse_direction(s: &str) -> Input {
    match s {
        "Left" => Input::Move(Direction::Left),
        "Right" => Input::Move(Direction::Right),
        "Down" => Input::Move(Direction::Down),
        "Rotate" => Input::Move(Direction::Up),
        "HardDrop" => Input::Action,
        "Quit" => Input::Quit,
        _ => Input::None, // "Drop" (gravity tick) and unknown inputs
    }
}

/// `GET /api/v1/games/tetris/start[?variant=<slug>]` — cria sessão e retorna estado inicial.
/// Variantes suportadas: `tetris/sprint-40` encerra ao limpar 40 linhas (YG-37).
#[utoipa::path(
    get,
    path = "/api/v1/games/tetris/start",
    params(
        ("variant" = Option<String>, Query, description = "Variante do jogo (ex: tetris/sprint-40)")
    ),
    responses(
        (status = 200, description = "Sessão criada", body = crate::openapi::TetrisStartResponse)
    ),
    tag = "tetris"
)]
pub async fn start_game(
    State(state): State<Arc<TetrisState>>,
    Query(q): Query<VariantQuery>,
) -> impl IntoResponse {
    let universe = Universe::tetris();
    let opts = tetris_opts_for_variant(q.variant.as_deref());
    let game = YggTetris::with_options(universe, opts);
    let game_state = game.render();
    let id = nanoid!();

    state.sessions.lock().unwrap().insert(id.clone(), game);

    Json(StartResponse {
        id,
        state: game_state,
        score: 0,
    })
}

fn tetris_opts_for_variant(variant: Option<&str>) -> TetrisOptions {
    match variant {
        Some("tetris/sprint-40") => TetrisOptions {
            sprint_lines: Some(40),
        },
        _ => TetrisOptions::default(),
    }
}

/// `POST /api/v1/games/tetris/:id/input` — avança um tick com o input recebido.
#[utoipa::path(
    post,
    path = "/api/v1/games/tetris/{id}/input",
    params(
        ("id" = String, Path, description = "ID da sessão de jogo")
    ),
    request_body = crate::games::common::InputRequest,
    responses(
        (status = 200, description = "Tick processado", body = crate::openapi::TetrisTickResponse),
        (status = 404, description = "Sessão não encontrada")
    ),
    tag = "tetris"
)]
pub async fn send_input(
    Path(id): Path<String>,
    State(state): State<Arc<TetrisState>>,
    Json(body): Json<InputRequest>,
) -> impl IntoResponse {
    let input = parse_direction(&body.direction);

    let (action, game_state, score) = {
        let mut sessions = state.sessions.lock().unwrap();
        let game = match sessions.get_mut(&id) {
            Some(g) => g,
            None => return StatusCode::NOT_FOUND.into_response(),
        };

        let action = game.tick(input);
        let game_state = game.render();
        let score = game.score();

        if action != GameAction::Continue {
            sessions.remove(&id);
        }

        (action, game_state, score)
    };

    let action_str = if action == GameAction::Continue {
        "continue"
    } else {
        state.scores.save_score(&body.user_id, "tetris", score);
        "quit"
    };

    Json(TickResponse {
        action: action_str.to_string(),
        state: game_state,
        score,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::{get, post},
    };
    use tower::ServiceExt;

    use crate::scores_store::InMemoryScoresStore;

    fn make_app() -> Router {
        let store: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = make_tetris_state(store);
        Router::new()
            .route("/api/v1/games/tetris/start", get(start_game))
            .route("/api/v1/games/tetris/{id}/input", post(send_input))
            .with_state(state)
    }

    #[tokio::test]
    async fn start_returns_id_and_state() {
        let app = make_app();

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/tetris/start")
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
        assert!(v["id"].is_string());
        assert!(v["state"]["board"].is_array());
        assert_eq!(v["state"]["board"].as_array().unwrap().len(), 20);
        assert_eq!(v["score"], 0);
    }

    #[tokio::test]
    async fn drop_input_returns_continue() {
        let store: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = make_tetris_state(store);
        let app = Router::new()
            .route("/api/v1/games/tetris/start", get(start_game))
            .route("/api/v1/games/tetris/{id}/input", post(send_input))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/tetris/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let start: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = start["id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "direction": "Drop" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/games/tetris/{id}/input"))
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
        assert_eq!(v["action"], "continue");
        assert!(v["state"]["board"].is_array());
        assert!(v["score"].is_number());
    }

    #[tokio::test]
    async fn quit_saves_score_and_returns_quit() {
        let store: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = make_tetris_state(store);
        let app = Router::new()
            .route("/api/v1/games/tetris/start", get(start_game))
            .route("/api/v1/games/tetris/{id}/input", post(send_input))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/tetris/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let start: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = start["id"].as_str().unwrap().to_string();

        let body = serde_json::json!({ "direction": "Quit", "user_id": "jogador1" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/games/tetris/{id}/input"))
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
        assert_eq!(v["action"], "quit");

        let count = state
            .scores
            .recent_scores(10)
            .into_iter()
            .filter(|r| r.game == "tetris")
            .count();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn unknown_session_returns_404() {
        let app = make_app();

        let body = serde_json::json!({ "direction": "Left" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/games/tetris/nope/input")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
