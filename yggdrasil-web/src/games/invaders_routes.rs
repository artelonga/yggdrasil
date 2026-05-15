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
use yggdrasil_core::games::invaders::InvadersOptions;
use yggdrasil_core::games::{YggGame, YggInvaders};

use super::common::{self, InputRequest, StartResponse, TickResponse, VariantQuery, map_to_value};

pub struct InvadersState {
    sessions: Mutex<HashMap<String, YggInvaders>>,
    db: Mutex<rusqlite::Connection>,
    jwt_secret: String,
}

pub fn make_invaders_state(
    db_path: &str,
    jwt_secret: String,
) -> rusqlite::Result<Arc<InvadersState>> {
    let conn = common::init_db(db_path)?;
    Ok(Arc::new(InvadersState {
        sessions: Mutex::new(HashMap::new()),
        db: Mutex::new(conn),
        jwt_secret,
    }))
}

fn parse_input(s: &str) -> Input {
    match s {
        "Left" => Input::Move(Direction::Left),
        "Right" => Input::Move(Direction::Right),
        "Shoot" => Input::Action,
        "Quit" => Input::Quit,
        _ => Input::None, // "Tick" and unknown
    }
}

/// `GET /api/v1/games/invaders/start[?variant=<slug>]` — cria sessão e retorna estado inicial.
/// Variantes suportadas: `invaders/swarm` reduz vidas para 1 (YG-37).
pub async fn start_game(
    State(state): State<Arc<InvadersState>>,
    Query(q): Query<VariantQuery>,
) -> impl IntoResponse {
    let universe = Universe::invaders();
    let opts = invaders_opts_for_variant(q.variant.as_deref());
    let game = YggInvaders::with_options(universe, opts);
    let state_val = map_to_value(&game.render_json());
    let id = nanoid!();
    state.sessions.lock().unwrap().insert(id.clone(), game);
    Json(StartResponse {
        id,
        state: state_val,
        score: 0,
    })
}

fn invaders_opts_for_variant(variant: Option<&str>) -> InvadersOptions {
    match variant {
        Some("invaders/swarm") => InvadersOptions { lives: Some(1) },
        _ => InvadersOptions::default(),
    }
}

/// `POST /api/v1/games/invaders/:id/input` — avança um tick com o input recebido.
/// `user_id` para o score vem do JWT (Bearer header); ausência ou JWT inválido
/// = "anonymous".
pub async fn send_input(
    Path(id): Path<String>,
    State(state): State<Arc<InvadersState>>,
    headers: HeaderMap,
    Json(body): Json<InputRequest>,
) -> impl IntoResponse {
    let input = parse_input(&body.direction);
    let (user_id, email) = common::user_info_from_jwt(&headers, &state.jwt_secret);
    common::lazy_upsert_profile(&state.db, &user_id, &email);

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
        common::save_score_locked(&state.db, &user_id, "invaders", score);
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
        let state = make_invaders_state(db_path, "t".to_string()).unwrap();
        Router::new()
            .route("/api/v1/games/invaders/start", get(start_game))
            .route("/api/v1/games/invaders/{id}/input", post(send_input))
            .with_state(state)
    }

    #[tokio::test]
    async fn variant_swarm_inicia_com_uma_vida() {
        // YG-37: ?variant=invaders/swarm deve produzir lives=1.
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/invaders/start?variant=invaders/swarm")
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
        assert_eq!(v["state"]["lives"], 1);
    }

    #[tokio::test]
    async fn root_invaders_inicia_com_tres_vidas() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/invaders/start")
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
        assert_eq!(v["state"]["lives"], 3);
    }

    #[tokio::test]
    async fn start_returns_id_and_state() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let app = make_app(&db);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/invaders/start")
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
        assert!(v["state"]["player_x"].is_number());
        assert!(v["state"]["aliens"].is_array());
        assert_eq!(v["score"], 0);
    }

    #[tokio::test]
    async fn tick_input_returns_continue() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_invaders_state(&db, "t".to_string()).unwrap();
        let app = Router::new()
            .route("/api/v1/games/invaders/start", get(start_game))
            .route("/api/v1/games/invaders/{id}/input", post(send_input))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/invaders/start")
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

        let body = serde_json::json!({ "direction": "Tick" }).to_string();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/games/invaders/{id}/input"))
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
        assert!(v["state"]["aliens"].is_array());
        assert!(v["score"].is_number());
    }

    #[tokio::test]
    async fn quit_saves_score_and_returns_quit() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("t.db").to_string_lossy().to_string();
        let state = make_invaders_state(&db, "t".to_string()).unwrap();
        let app = Router::new()
            .route("/api/v1/games/invaders/start", get(start_game))
            .route("/api/v1/games/invaders/{id}/input", post(send_input))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/games/invaders/start")
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
                    .uri(format!("/api/v1/games/invaders/{id}/input"))
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
                "SELECT COUNT(*) FROM scores WHERE game = 'invaders'",
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
                    .uri("/api/v1/games/invaders/nope/input")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
