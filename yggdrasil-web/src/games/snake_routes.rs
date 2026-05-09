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
use chrono::Utc;
use game_core::{
    engine::{Direction, Input, Universe},
    games::GameAction,
};
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use yggdrasil_core::games::{YggGame, YggSnake};

pub struct SnakeState {
    sessions: Mutex<HashMap<String, YggSnake>>,
    db: Mutex<rusqlite::Connection>,
}

pub fn make_snake_state(db_path: &str) -> rusqlite::Result<Arc<SnakeState>> {
    let conn = rusqlite::Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS scores (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            game    TEXT NOT NULL,
            score   INTEGER NOT NULL,
            ts      TEXT NOT NULL
        );",
    )?;
    Ok(Arc::new(SnakeState {
        sessions: Mutex::new(HashMap::new()),
        db: Mutex::new(conn),
    }))
}

#[derive(Serialize)]
struct StartResponse {
    id: String,
    state: serde_json::Value,
    score: u32,
}

#[derive(Deserialize)]
pub struct InputRequest {
    direction: String,
    #[serde(default = "default_user_id")]
    user_id: String,
}

fn default_user_id() -> String {
    "anonymous".to_string()
}

#[derive(Serialize)]
struct TickResponse {
    action: String,
    state: serde_json::Value,
    score: u32,
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

fn map_to_value(json: &str) -> serde_json::Value {
    serde_json::from_str(json).unwrap_or(serde_json::Value::Null)
}

/// `GET /api/v1/games/snake/start` — cria sessão e retorna estado inicial.
pub async fn start_game(State(state): State<Arc<SnakeState>>) -> impl IntoResponse {
    let universe = Universe::snake();
    let game = YggSnake::new(universe);
    let state_val = map_to_value(&game.render_json());
    let id = nanoid!();

    state.sessions.lock().unwrap().insert(id.clone(), game);

    Json(StartResponse {
        id,
        state: state_val,
        score: 0,
    })
}

/// `POST /api/v1/games/snake/:id/input` — avança um tick com o input recebido.
pub async fn send_input(
    Path(id): Path<String>,
    State(state): State<Arc<SnakeState>>,
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
        // Persist score on quit (wall hit, self-collision, or explicit Quit)
        let ts = Utc::now().to_rfc3339();
        let _ = state.db.lock().unwrap().execute(
            "INSERT INTO scores (user_id, game, score, ts) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![body.user_id, "snake", score, ts],
        );
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
        let state = make_snake_state(db_path).unwrap();
        Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .route("/api/v1/games/snake/{id}/input", post(send_input))
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
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_snake_state(&db).unwrap();
        let app = Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .route("/api/v1/games/snake/{id}/input", post(send_input))
            .with_state(state.clone());

        // Start
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

        // Input
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
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_snake_state(&db).unwrap();
        let app = Router::new()
            .route("/api/v1/games/snake/start", get(start_game))
            .route("/api/v1/games/snake/{id}/input", post(send_input))
            .with_state(state.clone());

        // Start
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

        // Quit
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

        // Verify score persisted
        let count: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM scores WHERE game = 'snake'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn send_input_unknown_game_returns_404() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);

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
