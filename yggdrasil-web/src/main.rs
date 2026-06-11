//! Yggdrasil web server — entrypoint.

mod api;
mod auth;
mod auth_co;
pub mod catalog;
pub mod co_bridge_inbound;
pub mod co_bridge_producer;
pub mod comunicacao_routes;
pub mod feedback;
mod games;
pub mod hint_engine;
mod lobby;
mod mail;
pub mod openapi;
mod scores_store;
pub mod telemetria;
pub mod universos_routes;
pub mod wasm_runtime;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use scores_store::{ScoresStore, SqliteScoresStore};

use axum::{
    Router,
    response::{Html, IntoResponse, Redirect},
    routing::{delete, get, post},
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
use games::vim_routes::{
    create_session as vim_create_session, make_vim_state, send_key as vim_send_key,
};
use lobby::router as lobby_router;
use tower_http::cors::CorsLayer;
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

    let scores_store: Arc<dyn ScoresStore> =
        Arc::new(SqliteScoresStore::open(&db_path).map_err(|e| anyhow::anyhow!("scores db: {e}"))?);

    let snake_state = make_snake_state(scores_store.clone());
    let tetris_state = make_tetris_state(scores_store.clone());
    let invaders_state = make_invaders_state(scores_store.clone());
    let vim_state = make_vim_state();

    let scores_state = Arc::new(api::scores::ScoresState {
        scores: scores_store,
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
        .with_state(auth_state.clone());

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

    let vim_router = Router::new()
        .route("/api/v1/universos/vim/sessoes", post(vim_create_session))
        .route(
            "/api/v1/universos/vim/sessoes/{id}/tecla",
            post(vim_send_key),
        )
        .with_state(vim_state);

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

    // YG-60: unified /api/v1/universos session routes + WebSocket
    // YG-62: telemetria — funnel_events + session_records + analytics
    let telemetria = Arc::new(
        telemetria::TelemetriaDb::open(&db_path)
            .map_err(|e| anyhow::anyhow!("telemetria db: {e}"))?,
    );
    let universos_state = universos_routes::UniversosState::new(
        Arc::new(
            SqliteScoresStore::open(&db_path)
                .map_err(|e| anyhow::anyhow!("universos scores db: {e}"))?,
        ),
        telemetria,
    );
    universos_routes::spawn_cleanup_job(universos_state.clone());
    let universos_router = Router::new()
        .route("/api/v1/universos", get(universos_routes::list_universos))
        .route("/api/v1/stats", get(universos_routes::get_stats))
        .route(
            "/api/v1/universos/{id}",
            get(universos_routes::get_universo),
        )
        .route(
            "/api/v1/universos/{id}/sessoes",
            post(universos_routes::create_sessao),
        )
        .route(
            "/api/v1/universos/{id}/sessoes/{sid}/tick",
            post(universos_routes::tick_sessao),
        )
        .route(
            "/api/v1/universos/{id}/sessoes/{sid}",
            delete(universos_routes::delete_sessao),
        )
        .route(
            "/api/v1/universos/{id}/sessoes/{sid}/ws",
            get(universos_routes::ws_sessao),
        )
        .route(
            "/api/v1/admin/analytics",
            get(universos_routes::get_analytics),
        )
        .with_state(universos_state);

    // YG-73: editor de universos data-driven — instâncias autoradas + anexos +
    // templates. Conceito paralelo ao runtime WASM; nada de arcade é tocado.
    let instances_dir =
        std::env::var("YGGDRASIL_INSTANCES_DIR").unwrap_or_else(|_| "data/instances".to_string());
    let instance_store = Arc::new(
        yggdrasil_core::instance::InstanceStore::new(&instances_dir)
            .map_err(|e| anyhow::anyhow!("instance store: {e}"))?,
    );
    // YG-93/YG-103: producer de eventos p/ a federated bus do CO. O canal
    // `broadcast` é sempre criado (barato); a task de fundo só sobe se
    // `YGG_CO_BRIDGE_URL` + `YGG_CO_BRIDGE_TOKEN` estiverem setados (gate). Sem
    // eles, `spawn` é no-op e os `sender`s ficam sem assinantes — emitir é
    // benigno. `spawn` é adiado até o `comunicacao_store` existir (semeia os
    // termos publicados, YG-103). E2E aguarda os hubs CO-384/389.
    let co_bridge = co_bridge_producer::Producer::new();
    let instance_store_for_bridge = instance_store.clone();
    // YG-112/114: as notas do Caderno do Ayvu Rapyta são gravadas via `NoteStore`
    // sob a instância canônica do Ayvu (`AYVU_INSTANCE`) no mesmo store de
    // instâncias — o producer staged então as federa de graça.
    let instance_store_for_caderno = instance_store.clone();

    let instances_state = Arc::new(
        api::instances::InstancesState::new(auth_state.jwt_secret.clone(), instance_store)
            .with_note_events(co_bridge.sender()),
    );
    let instances_router = Router::new()
        .route(
            "/api/v1/instances",
            post(api::instances::create_instance).get(api::instances::list_instances),
        )
        .route(
            "/api/v1/instances/{id}",
            get(api::instances::get_instance)
                .put(api::instances::put_instance)
                .patch(api::instances::patch_instance)
                .delete(api::instances::delete_instance),
        )
        .route(
            "/api/v1/instances/{id}/attachments",
            post(api::instances::upload_attachment),
        )
        .route(
            "/api/v1/instances/{id}/attachments/{hash}",
            get(api::instances::serve_attachment),
        )
        .route("/api/v1/instances/{id}/play", get(serve_instance_player))
        // Notas (jardim): Markdown canônico ligado por wikilinks, referenciado
        // pelo grafo da instância via `Block.props.note_slug`.
        .route(
            "/api/v1/instances/{id}/notes",
            get(api::instances::list_notes),
        )
        .route(
            "/api/v1/instances/{id}/notes/{slug}",
            get(api::instances::get_note)
                .put(api::instances::put_note)
                .delete(api::instances::delete_note),
        )
        .route("/api/v1/templates", get(api::instances::list_templates))
        .route(
            "/api/v1/templates/{slug}",
            get(api::instances::get_template),
        )
        .with_state(instances_state);

    // Fale conosco — canal de feedback/dúvida/sugestão por universo e na raiz.
    // JWT opcional (anônimo vs usuário); grava na mesma SQLite (`YGGDRASIL_DB`).
    let feedback_state = Arc::new(api::feedback::FeedbackState {
        jwt_secret: auth_state.jwt_secret.clone(),
        db: Arc::new(
            feedback::FeedbackDb::open(&db_path)
                .map_err(|e| anyhow::anyhow!("feedback db: {e}"))?,
        ),
        // YG-92: mesma chave de admin do analytics libera resolver feedback.
        admin_token: std::env::var("YGGDRASIL_ADMIN_TOKEN").ok(),
    });
    let feedback_router = Router::new()
        .route(
            "/api/v1/feedback",
            post(api::feedback::submit_feedback).get(api::feedback::list_feedback),
        )
        .route(
            "/api/v1/feedback/{id}/resolve",
            post(api::feedback::resolve_feedback),
        )
        .with_state(feedback_state);

    // Universo `comunicacao` — salas interativas de léxico cross-linguístico
    // (Mbyá Guaraní × Iorubá). Auto-contido: salas em disco + write-back de
    // termos novos no repo `comunicacao` (markdown) + fila de revisão.
    let comunicacao_rooms_dir = std::env::var("YGGDRASIL_COMUNICACAO_DIR")
        .unwrap_or_else(|_| "data/comunicacao".to_string());
    let comunicacao_lexicon_dir =
        std::env::var("COMUNICACAO_DIR").unwrap_or_else(|_| "../comunicacao".to_string());
    let comunicacao_store = Arc::new(
        yggdrasil_core::comunicacao::RoomStore::new(&comunicacao_rooms_dir)
            .map_err(|e| anyhow::anyhow!("comunicacao store: {e}"))?,
    );
    // YG-112: Caderno do Ayvu Rapyta — store per-user (favoritos/notas/progresso)
    // sob a mesma raiz das salas (`_caderno/<user-slug>.json`), espelhando o
    // per-user da fila de revisão.
    let comunicacao_caderno = Arc::new(
        yggdrasil_core::comunicacao::CadernoStore::new(&comunicacao_rooms_dir)
            .map_err(|e| anyhow::anyhow!("comunicacao caderno store: {e}"))?,
    );
    // (Re)gera as duas salas públicas (Iorubá + Mbyá) do léxico completo baked-in.
    yggdrasil_core::comunicacao::ensure_public_rooms(
        &comunicacao_store,
        std::path::Path::new(&comunicacao_lexicon_dir),
    );
    // YG-100: write-back das contribuições `_users/` → git (env-gated por
    // YGGDRASIL_COMUNICACAO_WRITEBACK). Convive com as rotas de notas (Phase 0).
    let comunicacao_writeback = Arc::new(yggdrasil_core::comunicacao::Writeback::new(
        yggdrasil_core::comunicacao::WritebackConfig::from_env(),
    ));
    // YG-103: sobe a task de fundo do producer agora que ambos os stores existem;
    // semeia notas (Fase 0) + termos das salas publicadas, e segue emitindo.
    let co_bridge_spawned = co_bridge_producer::spawn(
        &co_bridge,
        instance_store_for_bridge,
        comunicacao_store.clone(),
    );
    if co_bridge_spawned {
        info!("co-bridge producer ativo — federando notas (P-A) + comunicação (YG-103) ao CO");
    }

    let comunicacao_state = Arc::new(
        comunicacao_routes::ComunicacaoState {
            jwt_secret: auth_state.jwt_secret.clone(),
            store: comunicacao_store,
            lexicon: Arc::new(yggdrasil_core::comunicacao::LexiconStore::new(
                &comunicacao_lexicon_dir,
            )),
            writeback: comunicacao_writeback,
            term_events: None,
            room_events: None,
            // YG-101: curadoria de léxico — mesma chave de admin do feedback/analytics.
            admin_token: std::env::var("YGGDRASIL_ADMIN_TOKEN").ok(),
            // YG-112: Caderno per-user + store de instâncias p/ federar as notas
            // do Ayvu (YG-114).
            caderno: comunicacao_caderno,
            instance_store: instance_store_for_caderno,
        }
        .with_bridge(co_bridge.sender(), co_bridge.obs_sender()),
    );
    // Rede de segurança periódica (no-op se o write-back estiver desligado).
    comunicacao_routes::spawn_writeback_sweeper(comunicacao_state.clone());
    let comunicacao_router = Router::new()
        .route(
            "/api/v1/comunicacao/salas",
            post(comunicacao_routes::create_sala).get(comunicacao_routes::list_salas),
        )
        .route(
            "/api/v1/comunicacao/salas/{id}",
            get(comunicacao_routes::get_sala)
                .patch(comunicacao_routes::patch_sala)
                .delete(comunicacao_routes::delete_sala),
        )
        .route(
            "/api/v1/comunicacao/salas/{id}/elementos/{eid}/publicar",
            post(comunicacao_routes::publicar_elemento),
        )
        .route(
            "/api/v1/comunicacao/salas/{id}/fork",
            post(comunicacao_routes::fork_sala),
        )
        .route(
            "/api/v1/comunicacao/lexico",
            get(comunicacao_routes::consultar_lexico),
        )
        .route(
            "/api/v1/comunicacao/lexico/lista",
            get(comunicacao_routes::lexico_lista),
        )
        .route(
            "/api/v1/comunicacao/lexico/promover",
            post(comunicacao_routes::promover_termo),
        )
        .route(
            "/api/v1/comunicacao/templates",
            get(comunicacao_routes::list_templates),
        )
        .route(
            "/api/v1/comunicacao/revisao",
            get(comunicacao_routes::get_revisao),
        )
        .route(
            "/api/v1/comunicacao/revisao/nota",
            post(comunicacao_routes::nota_revisao),
        )
        // YG-112: Caderno do Ayvu Rapyta — favoritos/notas/progresso por usuário
        // (JWT-gated), com migração do localStorage. Notas federam pelo Ayvu (YG-114).
        .route(
            "/api/v1/comunicacao/caderno",
            get(comunicacao_routes::get_caderno),
        )
        .route(
            "/api/v1/comunicacao/caderno/favoritos/{key}",
            axum::routing::put(comunicacao_routes::add_favorito)
                .delete(comunicacao_routes::remove_favorito),
        )
        .route(
            "/api/v1/comunicacao/caderno/notas/{key}",
            axum::routing::put(comunicacao_routes::put_nota_caderno)
                .delete(comunicacao_routes::delete_nota_caderno),
        )
        .route(
            "/api/v1/comunicacao/caderno/progresso/{key}",
            axum::routing::put(comunicacao_routes::set_progresso),
        )
        .route(
            "/api/v1/comunicacao/caderno/migrar",
            post(comunicacao_routes::migrar_caderno),
        )
        // YG-111: superfície de exploração do corpus (Ayvu Rapyta).
        .route(
            "/api/v1/comunicacao/corpus/{slug}",
            get(comunicacao_routes::corpus),
        )
        .with_state(comunicacao_state);

    let app = Router::new()
        .route("/openapi.json", get(openapi::serve_openapi_json))
        .route("/openapi.yaml", get(openapi::serve_openapi_yaml))
        .route("/", get(root))
        .merge(lobby_router())
        .route("/login", get(serve_login))
        .route("/universos", get(serve_universos_index))
        .route("/universos/snake", get(serve_snake))
        .route("/universos/tetris", get(serve_tetris))
        .route("/universos/invaders", get(serve_invaders))
        .route("/universos/poker", get(serve_poker))
        .route("/universos/vim", get(serve_vim))
        .route("/universos/comunicacao", get(serve_comunicacao))
        .route("/universos/corpus", get(serve_corpus))
        // Fallback: o catálogo (YG-68) lista 41 universos mas só os embedded
        // têm página própria — qualquer outro slug (planned/external/shandara)
        // volta ao catálogo em vez de 404. Segmentos estáticos têm precedência
        // sobre a captura, então as rotas acima não são afetadas.
        .route("/universos/{slug}", get(redirect_to_catalogo))
        // 301 redirects para preservar bookmarks/links externos com a URL
        // antiga `/games/<slug>`. Remover quando todos os universos ativos
        // estiverem na nova URL por ≥ 1 release.
        .route("/games/snake", get(redirect_to_universo_snake))
        .route("/games/tetris", get(redirect_to_universo_tetris))
        .route("/games/invaders", get(redirect_to_universo_invaders))
        .route("/games/poker", get(redirect_to_universo_poker))
        .route("/health", get(health))
        .route("/version", get(version))
        .merge(auth_router)
        .merge(co_handover_router)
        .merge(me_router)
        .merge(scores_router)
        .merge(universes_router)
        .merge(snake_router)
        .merge(tetris_router)
        .merge(invaders_router)
        .merge(vim_router)
        .merge(poker_router)
        .merge(universos_router)
        .merge(instances_router)
        .merge(feedback_router)
        .merge(comunicacao_router)
        .route("/universos/instance/{id}", get(serve_instance_player))
        // YG-87: neuro = viewer multiescala Neuroglancer (atlas subcortical
        // Precomputed servido mesmo-origem). O viewer Godot macro fica em /anatomia.
        .route("/neuro", get(serve_neuro))
        .route("/universos/neuro", get(serve_neuro))
        // Mural público de feedback (Fale conosco) — nome sim, e-mail nunca.
        .route("/feedback", get(serve_feedback))
        // YG-84: visualizador 3D de anatomia (Godot Web export, single-thread →
        // serve de qualquer host estático). Os arquivos do export ficam em
        // static/anatomia/ e são servidos pelo ServeDir abaixo; /anatomia é só
        // um atalho amigável.
        .route("/anatomia", get(serve_anatomia))
        // CORS aberto no estático: o Neuroglancer (hosted, outra origem) precisa
        // buscar os dados Precomputed em /static/neuro-data/ via fetch cross-origin.
        .nest_service(
            "/static",
            tower::ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .service(ServeDir::new("yggdrasil-web/static")),
        );

    let addr: SocketAddr = "0.0.0.0:3030".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("yggdrasil-web ouvindo em http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Landing page (Relic Archive) — universos na barra esquerda, stats anônimas e
/// placares por jogo. O mapa em canvas continua em `/lobby`.
async fn root() -> impl IntoResponse {
    Html(include_str!("../static/landing.html"))
}

async fn serve_login() -> impl IntoResponse {
    Html(include_str!("../static/login.html"))
}

/// YG-78: player genérico de instâncias autoradas. A página busca a instância
/// por `id` via `GET /api/v1/instances/{id}` e renderiza client-side.
async fn serve_instance_player() -> impl IntoResponse {
    Html(include_str!("../static/universos/instance.html"))
}

/// YG-84: atalho `/anatomia` → bundle do Godot Web export servido em
/// `/static/anatomia/`.
/// YG-87: viewer Neuroglancer (atlas multiescala). A página monta o estado do NG
/// para a origem atual e carrega o bundle self-hosted em /static/ng/.
async fn serve_neuro() -> impl IntoResponse {
    Html(include_str!("../static/neuro.html"))
}

/// Mural público de feedback. Busca `GET /api/v1/feedback` (sem e-mail) e
/// renderiza client-side.
async fn serve_feedback() -> impl IntoResponse {
    Html(include_str!("../static/feedback-mural.html"))
}

async fn serve_anatomia() -> impl IntoResponse {
    Redirect::permanent("/static/anatomia/")
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

async fn serve_vim() -> impl IntoResponse {
    Html(include_str!("../static/universos/vim.html"))
}

/// Página do universo `comunicacao` — mapa interativo de léxico (pan/zoom +
/// elementos multilíngues). Busca a sala via `/api/v1/comunicacao/salas/{id}`.
async fn serve_comunicacao() -> impl IntoResponse {
    Html(include_str!("../static/universos/comunicacao.html"))
}

/// Superfície de exploração do **Ayvu Rapyta** — leitura verso a verso, capítulo
/// a capítulo: Mbyá ⟷ Español + NOTAS de Cadogan + partículas do léxico.
async fn serve_corpus() -> impl IntoResponse {
    Html(include_str!("../static/universos/corpus.html"))
}

/// Índice `/universos` — catálogo unificado e filtrável de todos os universos
/// (arcade + atlas + salas de comunicação + instâncias autoradas). A agregação
/// das fontes acontece no cliente (`/static/universos/index.js`).
async fn serve_universos_index() -> impl IntoResponse {
    Html(include_str!("../static/universos/index.html"))
}

/// Slug sem página própria (planned/external do catálogo) → catálogo.
/// Temporário de propósito: quando o universo ganhar página, a rota
/// específica passa a vencer a captura e o redirect deixa de disparar.
async fn redirect_to_catalogo() -> impl IntoResponse {
    Redirect::temporary("/universos")
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

/// `GET /version` — versão do binário (`CARGO_PKG_VERSION`), para verificar
/// qual release está no ar sem depender do contador interno do Fly.
async fn version() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
    }))
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
