//! OpenAPI 3.x specification — `/openapi.json` + `/openapi.yaml`.
//!
//! Doc-only schema types are defined here for external types (game-core,
//! yggdrasil-core, universe-vim) that can't have ToSchema added directly.
//! Handlers annotated with `#[utoipa::path]` reference these types.

use axum::{Json, response::IntoResponse};
use serde::Serialize;
use utoipa::{
    Modify, OpenApi, ToSchema,
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
};

// ─── Game-state doc types ─────────────────────────────────────────────────────

/// Snake map state (grid of tiles).
#[derive(ToSchema, Serialize)]
pub struct SnakeMapDoc {
    pub width: u32,
    pub height: u32,
    /// 2-D grid. Each cell is "Empty", "Wall", or entity tag.
    pub tiles: Vec<Vec<String>>,
}

/// Active Tetris piece.
#[derive(ToSchema, Serialize)]
pub struct TetrisPieceDoc {
    pub piece_type: u32,
    pub x: i32,
    pub y: i32,
    pub shape: Vec<Vec<u32>>,
}

/// Tetris board state.
#[derive(ToSchema, Serialize)]
pub struct TetrisStateDoc {
    pub board: Vec<Vec<u32>>,
    pub piece: TetrisPieceDoc,
    pub score: u32,
    pub level: u32,
    pub game_over: bool,
}

/// Space Invaders bullet.
#[derive(ToSchema, Serialize)]
pub struct InvadersBulletDoc {
    pub x: i32,
    pub y: i32,
    pub vy: i32,
    pub alien: bool,
}

/// Space Invaders alien.
#[derive(ToSchema, Serialize)]
pub struct InvadersAlienDoc {
    pub x: i32,
    pub y: i32,
    pub alive: bool,
    pub row: u32,
}

/// Space Invaders board state.
#[derive(ToSchema, Serialize)]
pub struct InvadersStateDoc {
    pub player_x: i32,
    pub bullets: Vec<InvadersBulletDoc>,
    pub aliens: Vec<InvadersAlienDoc>,
    pub score: u32,
    pub lives: u32,
    pub game_over: bool,
    pub won: bool,
}

/// Vim game state (universe-vim).
#[derive(ToSchema, Serialize)]
pub struct VimGameStateDoc {
    pub lines: Vec<String>,
    pub cursor: Vec<u32>,
    pub mode: String,
    pub level: u32,
    pub score: u32,
    pub hint: String,
}

// ─── Game response doc types ─────────────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct SnakeStartDoc {
    pub id: String,
    pub state: SnakeMapDoc,
    pub score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct SnakeTickDoc {
    pub action: String,
    pub state: SnakeMapDoc,
    pub score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct TetrisStartDoc {
    pub id: String,
    pub state: TetrisStateDoc,
    pub score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct TetrisTickDoc {
    pub action: String,
    pub state: TetrisStateDoc,
    pub score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct InvadersStartDoc {
    pub id: String,
    pub state: InvadersStateDoc,
    pub score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct InvadersTickDoc {
    pub action: String,
    pub state: InvadersStateDoc,
    pub score: u32,
}

// ─── Auth doc types ───────────────────────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct AuthTokenDoc {
    pub token: String,
}

#[derive(ToSchema, Serialize)]
pub struct ErrorDoc {
    pub erro: String,
}

// ─── Me doc types ─────────────────────────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct SaldoDoc {
    pub saldo: u64,
    pub moeda: String,
    pub atualizado_em: String,
}

// ─── Scores doc types ─────────────────────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct ScoresListDoc {
    pub scores: Vec<crate::scores_store::ScoreRow>,
}

// ─── Universe doc types ───────────────────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct ApiContractDoc {
    pub start: String,
    pub input: String,
    pub page: String,
}

#[derive(ToSchema, Serialize)]
pub struct UniverseNodeDoc {
    pub slug: String,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub api: ApiContractDoc,
}

#[derive(ToSchema, Serialize)]
pub struct UniverseEdgeDoc {
    pub from: String,
    pub to: String,
}

#[derive(ToSchema, Serialize)]
pub struct UniverseGraphDoc {
    pub nodes: Vec<UniverseNodeDoc>,
    pub edges: Vec<UniverseEdgeDoc>,
}

#[derive(ToSchema, Serialize)]
pub struct UniversesListDoc {
    pub universes: Vec<UniverseNodeDoc>,
}

// ─── Poker doc types ──────────────────────────────────────────────────────────

/// Poker table snapshot (public lobby view).
#[derive(ToSchema, Serialize)]
pub struct PokerLobbyDoc {
    pub id: String,
    pub name: String,
    pub max_seats: u32,
    pub big_blind: u32,
    pub small_blind: u32,
    pub seats: Vec<Option<PokerSeatDoc>>,
}

#[derive(ToSchema, Serialize)]
pub struct PokerSeatDoc {
    pub user_id: String,
    pub stack: u32,
}

#[derive(ToSchema, Serialize)]
pub struct PokerLobbiesDoc {
    pub lobbies: Vec<PokerLobbyDoc>,
}

#[derive(ToSchema, Serialize)]
pub struct PokerHoleCardsDoc {
    pub cards: Option<Vec<String>>,
}

/// Public hand snapshot (community cards, pot, actor).
#[derive(ToSchema, Serialize)]
pub struct PokerHandDoc {
    pub community_cards: Vec<String>,
    pub pot: u32,
    pub current_actor: Option<String>,
    pub stage: String,
}

// ─── Universos unified API doc types ─────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct CreateSessaoDoc {
    pub session_id: String,
    /// Initial game state (shape depends on universo id).
    #[schema(value_type = Object)]
    pub state: serde_json::Value,
}

#[derive(ToSchema, Serialize)]
pub struct TickResponseDoc {
    /// Game state after processing inputs.
    #[schema(value_type = Object)]
    pub state: serde_json::Value,
    pub session_ended: bool,
    pub score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct DeleteSessaoDoc {
    pub final_score: u32,
}

#[derive(ToSchema, Serialize)]
pub struct UniversoMetaSchemaDoc {
    pub id: String,
    pub api_version: u32,
    pub sessoes: String,
}

// ─── Vim session doc types ────────────────────────────────────────────────────

#[derive(ToSchema, Serialize)]
pub struct VimCreateSessionDoc {
    pub session_id: String,
    pub state: VimGameStateDoc,
}

#[derive(ToSchema, Serialize)]
pub struct VimSendKeyResponseDoc {
    pub state: VimGameStateDoc,
}

// ─── Security modifier ────────────────────────────────────────────────────────

pub struct SecurityAddon;
impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearerAuth",
                SecurityScheme::Http(
                    Http::builder()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

// ─── ApiDoc ───────────────────────────────────────────────────────────────────

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Yggdrasil API",
        version = "1.0.0",
        description = "API pública — lobby, jogos, autenticação e universos. Spec em `/openapi.json`."
    ),
    paths(
        // Auth
        crate::auth::magic_link::request_code,
        crate::auth::magic_link::verify_code,
        // Me
        crate::api::me::get_sementes,
        // Scores
        crate::api::scores::get_top,
        crate::api::scores::get_recent,
        // Universe registry
        crate::api::universes::list_universes,
        crate::api::universes::get_graph,
        crate::api::universes::get_universe,
        // Snake
        crate::games::snake_routes::start_game,
        crate::games::snake_routes::send_input,
        // Tetris
        crate::games::tetris_routes::start_game,
        crate::games::tetris_routes::send_input,
        // Invaders
        crate::games::invaders_routes::start_game,
        crate::games::invaders_routes::send_input,
        // Vim
        crate::games::vim_routes::create_session,
        crate::games::vim_routes::send_key,
        // Poker
        crate::games::poker::routes::list_lobbies,
        crate::games::poker::routes::get_lobby,
        crate::games::poker::routes::sit,
        crate::games::poker::routes::stand,
        crate::games::poker::routes::get_hand,
        crate::games::poker::routes::get_hole_cards,
        crate::games::poker::routes::post_action,
        // Universos unified
        crate::universos_routes::list_universos,
        crate::universos_routes::get_universo,
        crate::universos_routes::create_sessao,
        crate::universos_routes::tick_sessao,
        crate::universos_routes::delete_sessao,
        crate::universos_routes::ws_sessao,
    ),
    components(
        schemas(
            // Auth
            crate::auth::magic_link::CodeRequest,
            crate::auth::magic_link::VerifyRequest,
            AuthTokenDoc,
            ErrorDoc,
            // Me
            SaldoDoc,
            // Scores
            crate::scores_store::ScoreRow,
            ScoresListDoc,
            // Universes
            UniversesListDoc,
            UniverseNodeDoc,
            UniverseEdgeDoc,
            UniverseGraphDoc,
            ApiContractDoc,
            // Games — request types
            crate::games::common::InputRequest,
            // Snake
            SnakeMapDoc,
            SnakeStartDoc,
            SnakeTickDoc,
            // Tetris
            TetrisPieceDoc,
            TetrisStateDoc,
            TetrisStartDoc,
            TetrisTickDoc,
            // Invaders
            InvadersBulletDoc,
            InvadersAlienDoc,
            InvadersStateDoc,
            InvadersStartDoc,
            InvadersTickDoc,
            // Vim
            VimGameStateDoc,
            VimCreateSessionDoc,
            VimSendKeyResponseDoc,
            crate::games::vim_routes::SendKeyRequest,
            // Poker
            crate::games::poker::routes::SitRequest,
            crate::games::poker::routes::ActionRequest,
            PokerLobbyDoc,
            PokerSeatDoc,
            PokerLobbiesDoc,
            PokerHoleCardsDoc,
            PokerHandDoc,
            // Universos unified
            crate::universos_routes::UniversoMeta,
            crate::universos_routes::TickInput,
            crate::universos_routes::TickBody,
            UniversoMetaSchemaDoc,
            CreateSessaoDoc,
            TickResponseDoc,
            DeleteSessaoDoc,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "auth", description = "Autenticação via magic-link"),
        (name = "me", description = "Dados do usuário autenticado"),
        (name = "scores", description = "Placar público"),
        (name = "universes", description = "Registro de universos (grafo)"),
        (name = "games", description = "Sessões de jogo single-player (legacy API)"),
        (name = "poker", description = "Pôquer multiplayer"),
        (name = "universos", description = "API unificada de sessões v2"),
    )
)]
pub struct ApiDoc;

// ─── Route handlers ───────────────────────────────────────────────────────────

pub async fn serve_openapi_json() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}

pub async fn serve_openapi_yaml() -> impl IntoResponse {
    let spec = ApiDoc::openapi();
    match serde_yaml::to_string(&spec) {
        Ok(yaml) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "application/x-yaml; charset=utf-8",
            )],
            yaml,
        )
            .into_response(),
        Err(e) => {
            tracing::error!("openapi yaml serialization failed: {e}");
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    use super::{serve_openapi_json, serve_openapi_yaml};

    fn make_app() -> Router {
        Router::new()
            .route("/openapi.json", get(serve_openapi_json))
            .route("/openapi.yaml", get(serve_openapi_yaml))
    }

    #[tokio::test]
    async fn openapi_json_retorna_200_e_spec_valida() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let openapi_ver = spec["openapi"].as_str().unwrap_or("");
        assert!(
            openapi_ver.starts_with("3."),
            "openapi version should start with '3.', got '{openapi_ver}'"
        );

        let paths = spec["paths"]
            .as_object()
            .expect("spec.paths must be an object");
        assert!(
            paths.len() >= 20,
            "spec should have ≥20 paths, got {}",
            paths.len()
        );
    }

    #[tokio::test]
    async fn openapi_json_inclui_rotas_obrigatorias() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let paths = spec["paths"].as_object().unwrap();

        let required_prefixes = [
            "/api/v1/auth/",
            "/api/v1/me",
            "/api/v1/universes",
            "/api/v1/games/",
            "/api/v1/poker/",
        ];
        for prefix in required_prefixes {
            assert!(
                paths.keys().any(|k| k.starts_with(prefix)),
                "spec missing paths for prefix '{prefix}'"
            );
        }
    }

    #[tokio::test]
    async fn openapi_yaml_retorna_200() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.yaml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("yaml"),
            "content-type should contain 'yaml', got '{ct}'"
        );
    }
}
