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
use yggdrasil_core::games::snake::SnakeOptions;
use yggdrasil_core::games::{YggGame, YggSnake};

use super::common::{InputRequest, StartResponse, TickResponse, VariantQuery};
use crate::scores_store::ScoresStore;

pub struct SnakeState {
    sessions: Mutex<HashMap<String, YggSnake>>,
    scores: Arc<dyn ScoresStore>,
}

pub fn make_snake_state(scores: Arc<dyn ScoresStore>) -> Arc<SnakeState> {
    Arc::new(SnakeState {
        sessions: Mutex::new(HashMap::new()),
        scores,
    })
}

fn parse_direction(s: &str) -> Input {
    match s {
        "Up" => Input::Move(Direction::Up),
        "Down" => Input::Move(Direction::Down),
        "Left" => Input::Move(Direction::Left),
        "Right" => Input::Move(Direction::Right),
        "Quit" => Input::Quit,
        _ => Input::None,
    }
}

/// `GET /api/v1/games/snake/start[?variant=<slug>]` — cria sessão e retorna estado inicial.
/// Variantes suportadas: `snake/walls` adiciona paredes internas (YG-37).
pub async fn start_game(
    State(state): State<Arc<SnakeState>>,
    Query(q): Query<VariantQuery>,
) -> impl IntoResponse {
    let universe = Universe::snake();
    let opts = snake_opts_for_variant(
        q.variant.as_deref(),
        universe.map.width,
        universe.map.height,
    );
    let game = YggSnake::with_options(universe, opts);
    let game_state = game.render();
    let id = nanoid!();

    state.sessions.lock().unwrap().insert(id.clone(), game);

    Json(StartResponse {
        id,
        state: game_state,
        score: 0,
    })
}

/// Mapeia variant slug → `SnakeOptions`. Inline em vez de consultar o registry
/// global — variantes são parte do contrato deste universo, registry é só o
/// catálogo público.
fn snake_opts_for_variant(variant: Option<&str>, width: usize, height: usize) -> SnakeOptions {
    match variant {
        Some("snake/walls") => SnakeOptions {
            walls: YggSnake::walls_pattern(width, height),
        },
        _ => SnakeOptions::default(),
    }
}

/// `POST /api/v1/games/snake/:id/input` — avança um tick com o input recebido.
pub async fn send_input(
    Path(id): Path<String>,
    State(state): State<Arc<SnakeState>>,
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
        state.scores.save_score(&body.user_id, "snake", score);
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
        let state = make_snake_state(store);
        Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .route("/api/v1/games/snake/{id}/input", post(send_input))
            .with_state(state)
    }

    #[tokio::test]
    async fn start_returns_id_and_state() {
        let app = make_app();

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
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["id"].is_string());
        assert!(v["state"]["width"].is_number());
        assert!(v["state"]["tiles"].is_array());
        assert_eq!(v["score"], 0);
    }

    #[tokio::test]
    async fn send_input_right_returns_continue() {
        let store: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = make_snake_state(store);
        let app = Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .route("/api/v1/games/snake/{id}/input", post(send_input))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/snake/start")
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

        let body = serde_json::json!({ "direction": "Right" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/games/snake/{id}/input"))
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
        assert!(v["state"]["tiles"].is_array());
        assert!(v["score"].is_number());
    }

    #[tokio::test]
    async fn send_quit_saves_score_and_returns_quit() {
        let store: Arc<dyn ScoresStore> = Arc::new(InMemoryScoresStore::new());
        let state = make_snake_state(store);
        let app = Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .route("/api/v1/games/snake/{id}/input", post(send_input))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/snake/start")
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

        let body = serde_json::json!({ "direction": "Quit" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/games/snake/{id}/input"))
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
            .filter(|r| r.game == "snake")
            .count();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn variant_walls_adiciona_paredes_internas() {
        // YG-37: ?variant=snake/walls deve produzir Tile::Wall em posições
        // internas (não-borda) no estado retornado.
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/snake/start?variant=snake/walls")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let tiles = v["state"]["tiles"].as_array().unwrap();
        let width = v["state"]["width"].as_u64().unwrap() as usize;
        let height = v["state"]["height"].as_u64().unwrap() as usize;
        let mut internal_walls = 0;
        for (y, row) in tiles.iter().enumerate() {
            for (x, cell) in row.as_array().unwrap().iter().enumerate() {
                // ignora bordas
                if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                    continue;
                }
                if cell == "Wall" {
                    internal_walls += 1;
                }
            }
        }
        assert!(
            internal_walls > 0,
            "variant snake/walls deveria gerar ≥ 1 wall interno, got {internal_walls}"
        );
    }

    #[tokio::test]
    async fn root_snake_sem_paredes_internas() {
        // Regressão: comportamento root permanece sem paredes internas.
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/snake/start")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let tiles = v["state"]["tiles"].as_array().unwrap();
        let width = v["state"]["width"].as_u64().unwrap() as usize;
        let height = v["state"]["height"].as_u64().unwrap() as usize;
        for (y, row) in tiles.iter().enumerate() {
            for (x, cell) in row.as_array().unwrap().iter().enumerate() {
                if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                    continue;
                }
                assert_ne!(
                    cell, "Wall",
                    "root snake não deve ter Wall interno em ({x},{y})"
                );
            }
        }
    }

    #[tokio::test]
    async fn send_input_unknown_game_returns_404() {
        let app = make_app();

        let body = serde_json::json!({ "direction": "Right" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/games/snake/nope/input")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
