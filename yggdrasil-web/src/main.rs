//! Yggdrasil web server — entrypoint.

mod api;
mod auth;
mod auth_co;
mod games;
mod lobby;
mod mail;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    Router,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use games::invaders_routes::{
    make_invaders_state, send_input as invaders_input, start_game as invaders_start,
};
use games::poker::{
    PokerState, get_hand as poker_get_hand, get_hole_cards as poker_hole_cards,
    get_lobby as poker_get_lobby, list_lobbies as poker_list_lobbies,
    post_action as poker_post_action, sit as poker_sit, stand as poker_stand,
    ws_handler as poker_ws,
};
use games::snake_routes::{make_snake_state, send_input as snake_input, start_game as snake_start};
use games::tetris_routes::{
    make_tetris_state, send_input as tetris_input, start_game as tetris_start,
};
use lobby::router as lobby_router;
use tower_http::services::ServeDir;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let jwt_secret = std::env::var("YGGDRASIL_JWT_SECRET").map_err(|_| {
        anyhow::anyhow!(
            "YGGDRASIL_JWT_SECRET não configurado — defina esta variável de ambiente para iniciar o servidor"
        )
    })?;

    let db_path = std::env::var("YGGDRASIL_DB").unwrap_or_else(|_| "yggdrasil.db".to_string());
    let auth_conn = rusqlite::Connection::open(&db_path)?;
    auth::init_auth_db(&auth_conn)?;
    let auth_state = Arc::new(auth::AuthState {
        db: Arc::new(Mutex::new(auth_conn)),
        mail: mail::build_mail_provider(),
        jwt_secret,
    });

    let snake_state = make_snake_state(&db_path).expect("sqlite init (snake)");
    let tetris_state = make_tetris_state(&db_path).expect("sqlite init (tetris)");
    let invaders_state = make_invaders_state(&db_path).expect("sqlite init (invaders)");

    // Scores DB compartilhada (mesma tabela `scores` que snake/tetris/invaders gravam).
    let scores_conn = rusqlite::Connection::open(&db_path)?;
    let scores_state = Arc::new(api::scores::ScoresState {
        db: Arc::new(Mutex::new(scores_conn)),
    });

    let sementes_db = std::env::var("YGGDRASIL_SEMENTES_DB")
        .unwrap_or_else(|_| "yggdrasil-sementes.db".to_string());
    let sementes_storage = Arc::new(
        game_core::storage::Storage::open(std::path::Path::new(&sementes_db))
            .map_err(|e| anyhow::anyhow!("Erro ao abrir storage de sementes: {e}"))?,
    );
    let sementes = Arc::new(yggdrasil_core::sementes::Sementes::new(sementes_storage));
    let me_state = Arc::new(api::me::MeState {
        jwt_secret: auth_state.jwt_secret.clone(),
        sementes: sementes.clone(),
    });

    let me_router = Router::new()
        .route(
            "/api/v1/me/sementes",
            axum::routing::get(api::me::get_sementes),
        )
        .with_state(me_state);

    let scores_router = Router::new()
        .route("/api/v1/scores/top", get(api::scores::get_top))
        .route("/api/v1/scores/recent", get(api::scores::get_recent))
        .with_state(scores_state);

    // Universe graph — registro estático de universos com variantes e composições.
    let universes_state = Arc::new(api::universes::UniversesState {
        registry: Arc::new(yggdrasil_core::universes::default_registry()),
    });
    let universes_router = Router::new()
        .route("/api/v1/universes", get(api::universes::list_universes))
        .route("/api/v1/universes/graph", get(api::universes::get_graph))
        .route(
            "/api/v1/universes/{*slug}",
            get(api::universes::get_universe),
        )
        .with_state(universes_state);

    let co_handover_state = Arc::new(CoHandoverState {
        jwt_secret: auth_state.jwt_secret.clone(),
        jwks: Arc::new(auth_co::JwksCache::new()),
    });
    let co_handover_router = Router::new()
        .route("/auth/co-handover-receive", get(receive_co_handover))
        .route("/auth/co-login", get(redirect_to_co_login))
        .with_state(co_handover_state);

    // YG-29: poker persiste seating + stacks na mesma SQLite controlada por
    // `YGGDRASIL_DB`. Restart do servidor mantém quem está sentado e quanto
    // tem em chips; mãos em curso são forfeit.
    let poker_state = Arc::new(PokerState::with_persistence(
        auth_state.jwt_secret.clone(),
        sementes.clone(),
        std::path::Path::new(&db_path),
    ));

    let auth_router = Router::new()
        .route("/api/v1/auth/code", post(auth::request_code))
        .route("/api/v1/auth/verify", post(auth::verify_code))
        .with_state(auth_state);

    let snake_router = Router::new()
        .route("/api/v1/games/snake/start", get(snake_start))
        .route("/api/v1/games/snake/{id}/input", post(snake_input))
        .with_state(snake_state);

    let tetris_router = Router::new()
        .route("/api/v1/games/tetris/start", get(tetris_start))
        .route("/api/v1/games/tetris/{id}/input", post(tetris_input))
        .with_state(tetris_state);

    let invaders_router = Router::new()
        .route("/api/v1/games/invaders/start", get(invaders_start))
        .route("/api/v1/games/invaders/{id}/input", post(invaders_input))
        .with_state(invaders_state);

    let poker_router = Router::new()
        .route("/api/v1/poker/lobbies", get(poker_list_lobbies))
        .route("/api/v1/poker/lobbies/{id}", get(poker_get_lobby))
        .route("/api/v1/poker/lobbies/{id}/sit", post(poker_sit))
        .route("/api/v1/poker/lobbies/{id}/stand", post(poker_stand))
        .route("/api/v1/poker/lobbies/{id}/hand", get(poker_get_hand))
        .route(
            "/api/v1/poker/lobbies/{id}/hole-cards",
            get(poker_hole_cards),
        )
        .route("/api/v1/poker/lobbies/{id}/action", post(poker_post_action))
        .route("/api/v1/poker/lobbies/{id}/ws", get(poker_ws))
        .with_state(poker_state);

    let app = Router::new()
        .route("/", get(root))
        .merge(lobby_router())
        .route("/login", get(serve_login))
        .route("/universos/snake", get(serve_snake))
        .route("/universos/tetris", get(serve_tetris))
        .route("/universos/invaders", get(serve_invaders))
        .route("/universos/poker", get(serve_poker))
        // 301 redirects para preservar bookmarks/links externos com a URL
        // antiga `/games/<slug>`. Remover quando todos os universos ativos
        // estiverem na nova URL por ≥ 1 release.
        .route("/games/snake", get(redirect_to_universo_snake))
        .route("/games/tetris", get(redirect_to_universo_tetris))
        .route("/games/invaders", get(redirect_to_universo_invaders))
        .route("/games/poker", get(redirect_to_universo_poker))
        .route("/health", get(health))
        .merge(auth_router)
        .merge(co_handover_router)
        .merge(me_router)
        .merge(scores_router)
        .merge(universes_router)
        .merge(snake_router)
        .merge(tetris_router)
        .merge(invaders_router)
        .merge(poker_router)
        .nest_service("/static", ServeDir::new("yggdrasil-web/static"));

    let addr: SocketAddr = "0.0.0.0:3030".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("yggdrasil-web ouvindo em http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> impl IntoResponse {
    Redirect::to("/lobby")
}

async fn serve_login() -> impl IntoResponse {
    Html(include_str!("../static/login.html"))
}

async fn serve_snake() -> impl IntoResponse {
    Html(include_str!("../static/universos/snake.html"))
}

async fn serve_tetris() -> impl IntoResponse {
    Html(include_str!("../static/universos/tetris.html"))
}

async fn serve_invaders() -> impl IntoResponse {
    Html(include_str!("../static/universos/invaders.html"))
}

async fn serve_poker() -> impl IntoResponse {
    Html(include_str!("../static/universos/poker.html"))
}

// ── Legacy redirects (YG-N rename `/games/*` → `/universos/*`) ─────────────

async fn redirect_to_universo_snake() -> impl IntoResponse {
    Redirect::permanent("/universos/snake")
}
async fn redirect_to_universo_tetris() -> impl IntoResponse {
    Redirect::permanent("/universos/tetris")
}
async fn redirect_to_universo_invaders() -> impl IntoResponse {
    Redirect::permanent("/universos/invaders")
}
async fn redirect_to_universo_poker() -> impl IntoResponse {
    Redirect::permanent("/universos/poker")
}

async fn health() -> impl IntoResponse {
    "ok"
}

// ── CO handover receiver ───────────────────────────────────────────────────

#[derive(Clone)]
struct CoHandoverState {
    jwt_secret: String,
    jwks: Arc<auth_co::JwksCache>,
}

#[derive(serde::Deserialize)]
struct CoHandoverParams {
    co_token: String,
    #[serde(default)]
    next: Option<String>,
}

#[derive(serde::Deserialize)]
struct CoLoginParams {
    #[serde(default)]
    next: Option<String>,
}

/// `GET /auth/co-login?next=<path>` — redirect 302 para CO. Server-side
/// para que `CO_BASE_URL` (env var) seja resolvido em runtime sem precisar
/// passar para o frontend.
async fn redirect_to_co_login(
    axum::extract::Query(params): axum::extract::Query<CoLoginParams>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Construct yggdrasil base from Host header (preserves dev vs prod).
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("yggdrasil.artelonga.com.br");
    let scheme = if host.contains("localhost") || host.starts_with("127.") {
        "http"
    } else {
        "https"
    };
    let base = format!("{scheme}://{host}");
    let url = auth_co::co_login_url(&base, params.next.as_deref());
    Redirect::temporary(&url)
}

/// `GET /auth/co-handover-receive?co_token=<es256_jwt>&next=<path>`
///
/// Recebe token assinado por CO, valida via JWKS de CO, e mintar JWT local
/// HS256 com o mesmo `sub`/`email`. Responde com HTML que armazena o JWT em
/// `localStorage.yggdrasil-jwt` e navega para `next` (ou `/lobby` se ausente).
async fn receive_co_handover(
    axum::extract::State(state): axum::extract::State<Arc<CoHandoverState>>,
    axum::extract::Query(params): axum::extract::Query<CoHandoverParams>,
) -> axum::response::Response {
    let claims = match auth_co::verify_co_token(&params.co_token, &state.jwks).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("co-handover verify failed: {e}");
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Html(co_handover_error_page("Token de CO inválido ou expirado")),
            )
                .into_response();
        }
    };
    let local_jwt = match auth::sign_jwt(&claims.sub, &claims.email, &state.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("co-handover sign local jwt failed: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Html(co_handover_error_page("Erro interno ao gerar sessão local")),
            )
                .into_response();
        }
    };
    let next = params
        .next
        .as_deref()
        .filter(|n| n.starts_with('/'))
        .unwrap_or("/lobby");
    Html(co_handover_success_page(&local_jwt, next)).into_response()
}

fn co_handover_success_page(token: &str, next: &str) -> String {
    // Token e next vão para o HTML; ambos validados antes (token é base64-url+ascii,
    // next começa com /). Escapagem mínima já basta — não há control chars.
    let token_escaped = token.replace('<', "&lt;").replace('>', "&gt;");
    let next_escaped = next
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('"', "");
    format!(
        r#"<!DOCTYPE html>
<html lang="pt-BR"><head><meta charset="utf-8"><title>Entrando…</title>
<style>body{{background:#0d0d12;color:#e8e3d3;font-family:system-ui,monospace;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0}}</style></head>
<body><div>Concluindo login com CO…</div>
<script>
try {{
  localStorage.setItem('yggdrasil-jwt', '{token_escaped}');
  location.replace('{next_escaped}');
}} catch (_) {{
  document.body.textContent = 'Não foi possível armazenar sessão local.';
}}
</script></body></html>"#
    )
}

fn co_handover_error_page(msg: &str) -> String {
    let msg_esc = msg.replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<!DOCTYPE html>
<html lang="pt-BR"><head><meta charset="utf-8"><title>Erro no login</title>
<style>body{{background:#0d0d12;color:#e8e3d3;font-family:system-ui,monospace;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;text-align:center}}.box{{max-width:30rem;padding:2rem}}a{{color:#d4af37}}</style></head>
<body><div class="box"><h1 style="font-weight:300;letter-spacing:0.2em">ERRO</h1><p style="opacity:0.7;margin:1rem 0">{msg_esc}</p><p><a href="/login">Tentar novamente</a></p></div></body></html>"#
    )
}
