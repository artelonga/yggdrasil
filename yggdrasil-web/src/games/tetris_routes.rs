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
use game_core::{
    engine::{Direction, Input, Universe},
    games::GameAction,
};
use nanoid::nanoid;
use yggdrasil_core::games::{YggGame, YggTetris};

use super::common::{self, InputRequest, StartResponse, TickResponse, map_to_value};

pub struct TetrisState {
    sessions: Mutex<HashMap<String, YggTetris>>,
    db: Mutex<rusqlite::Connection>,
}

pub fn make_tetris_state(db_path: &str) -> rusqlite::Result<Arc<TetrisState>> {
    let conn = common::init_db(db_path)?;
    Ok(Arc::new(TetrisState {
        sessions: Mutex::new(HashMap::new()),
        db: Mutex::new(conn),
    }))
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

/// `GET /api/v1/games/tetris/start` — cria sessão e retorna estado inicial.
pub async fn start_game(State(state): State<Arc<TetrisState>>) -> impl IntoResponse {
    let universe = Universe::tetris();
    let game = YggTetris::new(universe);
    let state_val = map_to_value(&game.render_json());
    let id = nanoid!();

    state.sessions.lock().unwrap().insert(id.clone(), game);

    Json(StartResponse {
        id,
        state: state_val,
        score: 0,
    })
}

/// `POST /api/v1/games/tetris/:id/input` — avança um tick com o input recebido.
pub async fn send_input(
    Path(id): Path<String>,
    State(state): State<Arc<TetrisState>>,
    Json(body): Json<InputRequest>,
) -> impl IntoResponse {
    let input = parse_direction(&body.direction);

    let (action, state_val, score) = {
        let mut sessions = state.sessions.lock().unwrap();
        let game = match sessions.get_mut(&id) {
            Some(g) => g,
            None => return StatusCode::NOT_FOUND.into_response(),
        };

        let action = game.tick(input);
        let state_val = map_to_value(&game.render_json());
        let score = game.score();

        if action != GameAction::Continue {
            sessions.remove(&id);
        }

        (action, state_val, score)
    };

    let action_str = if action == GameAction::Continue {
        "continue"
    } else {
        common::save_score_locked(&state.db, &body.user_id, "tetris", score);
        "quit"
    };

    Json(TickResponse {
        action: action_str.to_string(),
        state: state_val,
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
    use tempfile::tempdir;
    use tower::ServiceExt;

    fn make_app(db_path: &str) -> Router {
        let state = make_tetris_state(db_path).unwrap();
        Router::new()
            .route("/api/v1/games/tetris/start", get(start_game))
            .route("/api/v1/games/tetris/{id}/input", post(send_input))
            .with_state(state)
    }

    #[tokio::test]
    async fn start_returns_id_and_state() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);

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
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_tetris_state(&db).unwrap();
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
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_tetris_state(&db).unwrap();
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

        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM scores WHERE game = 'tetris'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn unknown_session_returns_404() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);

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
