use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use game_core::{
    engine::{Direction, Input, Universe},
    games::GameAction,
};
use nanoid::nanoid;
use yggdrasil_core::games::snake::SnakeOptions;
use yggdrasil_core::games::{YggGame, YggSnake};

use super::common::{self, InputRequest, StartResponse, TickResponse, VariantQuery, map_to_value};

pub struct SnakeState {
    sessions: Mutex<HashMap<String, YggSnake>>,
    db: Mutex<rusqlite::Connection>,
    jwt_secret: String,
}

pub fn make_snake_state(db_path: &str, jwt_secret: String) -> rusqlite::Result<Arc<SnakeState>> {
    let conn = common::init_db(db_path)?;
    Ok(Arc::new(SnakeState {
        sessions: Mutex::new(HashMap::new()),
        db: Mutex::new(conn),
        jwt_secret,
    }))
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
    let state_val = map_to_value(&game.render_json());
    let id = nanoid!();

    state.sessions.lock().unwrap().insert(id.clone(), game);

    Json(StartResponse {
        id,
        state: state_val,
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
/// `user_id` para o score vem do JWT (Bearer header); ausência ou JWT inválido
/// = "anonymous".
pub async fn send_input(
    Path(id): Path<String>,
    State(state): State<Arc<SnakeState>>,
    headers: HeaderMap,
    Json(body): Json<InputRequest>,
) -> impl IntoResponse {
    let input = parse_direction(&body.direction);
    let user_id = common::user_id_from_jwt(&headers, &state.jwt_secret);

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
        common::save_score_locked(&state.db, &user_id, "snake", score);
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
        let state = make_snake_state(db_path, "t".to_string()).unwrap();
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
        let state = make_snake_state(&db, "t".to_string()).unwrap();
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
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_snake_state(&db, "t".to_string()).unwrap();
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
    async fn variant_walls_adiciona_paredes_internas() {
        // YG-37: ?variant=snake/walls deve produzir Tile::Wall em posições
        // internas (não-borda) no estado retornado.
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);
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
