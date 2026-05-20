//! OpenAPI spec wiring — serve `/openapi.json` e `/openapi.yaml`.
//!
//! YG-50: utoipa derive + handler annotations espalhadas em cada módulo.
//! Este arquivo agrega tudo no `ApiDoc` e expõe os handlers de rota.

use axum::{Json, response::IntoResponse};
use serde::Serialize;
use utoipa::OpenApi;

// ── Schema proxy para game_core::engine::Map (orphan rule impede ToSchema lá) ──

/// Representação do mapa do Snake para o OpenAPI spec.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SnakeMapSchema {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Vec<String>>,
}

/// Resposta de start para Snake.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SnakeStartResponse {
    pub id: String,
    pub state: SnakeMapSchema,
    pub score: u32,
}

/// Resposta de tick para Snake.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SnakeTickResponse {
    pub action: String,
    pub state: SnakeMapSchema,
    pub score: u32,
}

// ── Concrete aliases para Tetris e Invaders ────────────────────────────────

/// Resposta de start para Tetris.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TetrisStartResponse {
    pub id: String,
    pub state: yggdrasil_core::games::tetris::TetrisRender,
    pub score: u32,
}

/// Resposta de tick para Tetris.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TetrisTickResponse {
    pub action: String,
    pub state: yggdrasil_core::games::tetris::TetrisRender,
    pub score: u32,
}

/// Resposta de start para Invaders.
#[derive(Serialize, utoipa::ToSchema)]
pub struct InvadersStartResponse {
    pub id: String,
    pub state: yggdrasil_core::games::invaders::InvadersRender,
    pub score: u32,
}

/// Resposta de tick para Invaders.
#[derive(Serialize, utoipa::ToSchema)]
pub struct InvadersTickResponse {
    pub action: String,
    pub state: yggdrasil_core::games::invaders::InvadersRender,
    pub score: u32,
}

// ── ApiDoc ─────────────────────────────────────────────────────────────────

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Yggdrasil API",
        version = "0.9.2",
        description = "API pública da plataforma Yggdrasil — auth, universos e jogos."
    ),
    paths(
        crate::auth::request_code,
        crate::auth::verify_code,
        crate::api::me::get_sementes,
        crate::api::universes::list_universes,
        crate::api::universes::get_graph,
        crate::api::universes::get_universe,
        crate::api::scores::get_top,
        crate::api::scores::get_recent,
        crate::lobby_routes::get_lobby,
        crate::lobby_routes::post_enter,
        crate::games::snake_routes::start_game,
        crate::games::snake_routes::send_input,
        crate::games::tetris_routes::start_game,
        crate::games::tetris_routes::send_input,
        crate::games::invaders_routes::start_game,
        crate::games::invaders_routes::send_input,
        crate::games::poker::routes::list_lobbies,
        crate::games::poker::routes::get_lobby,
        crate::games::poker::routes::sit,
        crate::games::poker::routes::stand,
        crate::games::poker::routes::get_hand,
        crate::games::poker::routes::get_hole_cards,
        crate::games::poker::routes::post_action,
    ),
    components(schemas(
        // Snake proxy schemas
        SnakeMapSchema,
        SnakeStartResponse,
        SnakeTickResponse,
        // Tetris concrete aliases
        TetrisStartResponse,
        TetrisTickResponse,
        yggdrasil_core::games::tetris::TetrisRender,
        yggdrasil_core::games::tetris::ActivePiece,
        // Invaders concrete aliases
        InvadersStartResponse,
        InvadersTickResponse,
        yggdrasil_core::games::invaders::InvadersRender,
        yggdrasil_core::games::invaders::Bullet,
        yggdrasil_core::games::invaders::Alien,
        // Poker schemas
        crate::games::poker::routes::SitRequest,
        crate::games::poker::routes::ActionRequest,
        yggdrasil_core::games::poker::game::HandState,
        yggdrasil_core::games::poker::game::CardView,
        yggdrasil_core::games::poker::game::PublicPlayer,
        yggdrasil_core::games::poker::lobby::PokerLobby,
        yggdrasil_core::games::poker::lobby::SeatOccupant,
        // Shared request/response schemas
        crate::games::common::InputRequest,
        crate::api::me::SaldoResponse,
        crate::api::scores::ScoreRow,
        crate::lobby_routes::EnterResponse,
        crate::auth::CodeRequest,
        crate::auth::VerifyRequest,
        crate::auth::TokenResponse,
        // Universe schemas
        yggdrasil_core::universes::UniverseNode,
        yggdrasil_core::universes::UniverseKind,
        yggdrasil_core::universes::ApiContract,
    )),
    security(
        ("bearer_auth" = [])
    ),
    tags(
        (name = "auth", description = "Autenticação magic-link"),
        (name = "me", description = "Dados do usuário autenticado"),
        (name = "universes", description = "Grafo de universos"),
        (name = "scores", description = "Placar público"),
        (name = "lobby", description = "Lobby central 40×20"),
        (name = "snake", description = "Jogo Snake"),
        (name = "tetris", description = "Jogo Tetris"),
        (name = "invaders", description = "Jogo Space Invaders"),
        (name = "poker", description = "Pôquer multiplayer"),
    )
)]
pub struct ApiDoc;

// ── Route handlers ─────────────────────────────────────────────────────────

pub async fn get_openapi_json() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}

pub async fn get_openapi_yaml() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/yaml; charset=utf-8",
        )],
        ApiDoc::openapi().to_yaml().unwrap_or_default(),
    )
}

// ── Integration tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    fn make_openapi_app() -> Router {
        Router::new()
            .route("/openapi.json", get(get_openapi_json))
            .route("/openapi.yaml", get(get_openapi_yaml))
    }

    #[tokio::test]
    async fn openapi_json_returns_200_and_correct_version() {
        let app = make_openapi_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            spec["openapi"].as_str().unwrap().starts_with("3."),
            "openapi version deve começar com '3.', got: {}",
            spec["openapi"]
        );
    }

    #[tokio::test]
    async fn openapi_json_has_at_least_20_paths() {
        let app = make_openapi_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let paths = spec["paths"].as_object().unwrap();
        assert!(
            paths.len() >= 20,
            "esperado ≥20 paths no spec, mas got {}. Paths: {:?}",
            paths.len(),
            paths.keys().collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn openapi_yaml_returns_200_with_yaml_content_type() {
        let app = make_openapi_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.yaml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("yaml"),
            "content-type deve conter 'yaml', got: {ct}"
        );
    }
}
