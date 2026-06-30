//! Yggdrasil web server — entrypoint.

pub mod analytics_stream;
mod api;
mod auth;
mod auth_co;
pub mod campanha;
pub mod catalog;
pub mod co_bridge_inbound;
pub mod co_bridge_producer;
pub mod comunicacao_routes;
pub mod corpus_nlp;
pub mod feedback;
mod games;
pub mod hint_engine;
mod lobby;
mod mail;
pub mod openapi;
pub mod pix;
pub mod presenca;
mod scores_store;
pub mod shandara;
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
    stream_handler as poker_stream,
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
        .route("/api/v1/config", get(frontend_config))
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

    // YG-174: passkeys (WebAuthn) — login biométrico/security-key real. RP id +
    // origin do ambiente (default localhost p/ dev). Yggdrasil emite o próprio JWT.
    let rp_id = std::env::var("YGGDRASIL_RP_ID").unwrap_or_else(|_| "localhost".to_string());
    let rp_origin = std::env::var("YGGDRASIL_RP_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:3030".to_string());
    let passkey_router = match webauthn_rs::prelude::Url::parse(&rp_origin)
        .map_err(|e| anyhow::anyhow!("rp_origin: {e}"))
        .and_then(|o| {
            webauthn_rs::WebauthnBuilder::new(&rp_id, &o)
                .map_err(|e| anyhow::anyhow!("webauthn: {e}"))
                .and_then(|b| {
                    b.rp_name("Yggdrasil")
                        .build()
                        .map_err(|e| anyhow::anyhow!("webauthn build: {e}"))
                })
        }) {
        Ok(webauthn) => {
            let passkey_state = Arc::new(api::passkey::PasskeyState {
                webauthn: Arc::new(webauthn),
                db: Arc::new(
                    auth::passkey::PasskeyDb::open(&db_path)
                        .map_err(|e| anyhow::anyhow!("passkey db: {e}"))?,
                ),
                jwt_secret: auth_state.jwt_secret.clone(),
                reg: std::sync::Mutex::new(std::collections::HashMap::new()),
                auth: std::sync::Mutex::new(std::collections::HashMap::new()),
            });
            Router::new()
                .route(
                    "/api/v1/auth/passkey/register/start",
                    post(api::passkey::register_start),
                )
                .route(
                    "/api/v1/auth/passkey/register/finish",
                    post(api::passkey::register_finish),
                )
                .route(
                    "/api/v1/auth/passkey/login/start",
                    post(api::passkey::login_start),
                )
                .route(
                    "/api/v1/auth/passkey/login/finish",
                    post(api::passkey::login_finish),
                )
                .with_state(passkey_state)
        }
        Err(e) => {
            tracing::error!("passkeys desligados (config inválida): {e}");
            Router::new()
        }
    };

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
        .route("/api/v1/poker/lobbies/{id}/stream", get(poker_stream))
        .with_state(poker_state);

    // YG-60: unified /api/v1/universos session routes + WebSocket
    // YG-62: telemetria — funnel_events + session_records + analytics
    let telemetria = Arc::new(
        telemetria::TelemetriaDb::open(&db_path)
            .map_err(|e| anyhow::anyhow!("telemetria db: {e}"))?,
    );
    // YG-145: telemetria → CO segue o modelo da ArteLonga — eventos anônimos
    // nomeados via tracker client-side (static/telemetria.js → POST co/.../events).
    // Não há mais push server-side de rollups (removido o gate de token).

    // YG-128: retenção 90d agendada (espelha o CO) — 1×/dia, best-effort.
    {
        let t = telemetria.clone();
        tokio::spawn(async move {
            loop {
                let corte = chrono::Utc::now().timestamp_millis() - 90 * 24 * 60 * 60 * 1000;
                let (ev, ses) = t.cleanup_older_than(corte);
                if ev + ses > 0 {
                    tracing::info!(
                        eventos = ev,
                        sessoes = ses,
                        "telemetria: retenção 90d aplicada"
                    );
                }
                tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
            }
        });
    }

    // YG-128: pulso ao vivo — ring + broadcast p/ o WS /api/v1/analytics/stream.
    // Frames anônimos: stats a cada 10s + nota.escrita (sem slug/título/path).
    let pulso = analytics_stream::Pulso::new();
    // YG-145: presença/atividade ao vivo por universo (registro in-memory efêmero,
    // alimentado pelo ping client-side de telemetria.js).
    let presenca = presenca::Presenca::new();
    let universos_state = universos_routes::UniversosState::new(
        Arc::new(
            SqliteScoresStore::open(&db_path)
                .map_err(|e| anyhow::anyhow!("universos scores db: {e}"))?,
        ),
        telemetria,
    );
    universos_routes::spawn_cleanup_job(universos_state.clone());
    // YG-128: frame de stats anônimas no pulso (jogando agora / sessões 24h)
    {
        let p = pulso.clone();
        let us = universos_state.clone();
        let pres = presenca.clone();
        tokio::spawn(async move {
            loop {
                let agora = us.jogando_agora();
                let now_ms = chrono::Utc::now().timestamp_millis();
                let desde = now_ms - 24 * 60 * 60 * 1000;
                let s24 = us.telemetria.sessions_since(desde);
                // YG-145: presença por universo no mesmo frame (GC dos expirados).
                let por_universo = pres.ativos_por_universo(now_ms);
                p.push(serde_json::json!({
                    "ev": "stats",
                    "jogando_agora": agora,
                    "sessoes_24h": s24,
                    "presenca": por_universo,
                    "ts": now_ms,
                }));
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        });
    }
    let stream_router = axum::Router::new()
        .route("/api/v1/analytics/stream", get(analytics_stream::ws_stream))
        .with_state(pulso.clone());
    // YG-145: ingest de presença + leitura por universo / lobby.
    let atividade_router = Router::new()
        .route("/api/v1/presenca", post(presenca::ingest))
        .route("/api/v1/atividade", get(presenca::por_universo))
        .route(
            "/api/v1/universos/{id}/atividade",
            get(presenca::do_universo),
        )
        .with_state(presenca::AtividadeState {
            presenca: presenca.clone(),
            pulso: pulso.clone(),
        });
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

    // YG-137: perfil universal de usuário — UM por usuário, universos compõem.
    let profiles_dir =
        std::env::var("YGGDRASIL_PROFILES_DIR").unwrap_or_else(|_| "data/profiles".to_string());
    let profile_router = Router::new()
        .route(
            "/api/v1/profile",
            get(api::profile::get_profile).put(api::profile::put_profile),
        )
        .route(
            "/api/v1/profile/universos/{slug}",
            axum::routing::put(api::profile::join_universe).delete(api::profile::leave_universe),
        )
        .with_state(Arc::new(api::profile::ProfileApiState {
            jwt_secret: auth_state.jwt_secret.clone(),
            store: Arc::new(yggdrasil_core::profile::ProfileStore::new(&profiles_dir)),
        }));
    // YG-93/YG-103: producer de eventos p/ a federated bus do CO. O canal
    // `broadcast` é sempre criado (barato); a task de fundo só sobe se
    // `YGG_CO_BRIDGE_URL` + `YGG_CO_BRIDGE_TOKEN` estiverem setados (gate). Sem
    // eles, `spawn` é no-op e os `sender`s ficam sem assinantes — emitir é
    // benigno. `spawn` é adiado até o `comunicacao_store` existir (semeia os
    // termos publicados, YG-103). E2E aguarda os hubs CO-384/389.
    let co_bridge = co_bridge_producer::Producer::new();
    // YG-128: cada nota escrita vira um tique anônimo no pulso (sem slug/título)
    {
        let mut rx_notas = co_bridge.sender().subscribe();
        let p = pulso.clone();
        tokio::spawn(async move {
            while let Ok(_n) = rx_notas.recv().await {
                p.push(serde_json::json!({
                    "ev": "nota.escrita",
                    "ts": chrono::Utc::now().timestamp_millis(),
                }));
            }
        });
    }
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
        // YG-157: universos alcançáveis por portal a partir desta instância
        // (cross-universe / "universe-as-node"). Só metadados; o mundo do destino
        // é lazy (buscado ao cruzar).
        .route(
            "/api/v1/instances/{id}/portals",
            get(api::instances::list_portals),
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
        // YG-149: manipulação direta — commit em lote de posições/reparent do
        // drag-drop no frontmatter `.md` + write-back ao CO.
        .route(
            "/api/v1/instances/{id}/layout",
            post(api::instances::put_layout),
        )
        // YG-125: rascunho server-side (branch cross-device do editor popup) —
        // owner-only, fora de notes/, nunca passa pelo producer do bridge.
        .route(
            "/api/v1/instances/{id}/notes/{slug}/draft",
            get(api::instances::get_draft)
                .put(api::instances::put_draft)
                .delete(api::instances::delete_draft),
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

    // Campanha (YG-161) — crowdfunding independente: ledger de apoios (pledges),
    // tiers canônicos e página de créditos. Pledge nasce `pendente`; a confirmação
    // (admin, fora de banda) credita as sementes do tier. Mesma SQLite + sementes.
    let campanha_state = Arc::new(api::campanha::CampanhaState {
        jwt_secret: auth_state.jwt_secret.clone(),
        db: Arc::new(
            campanha::PledgeDb::open(&db_path).map_err(|e| anyhow::anyhow!("campanha db: {e}"))?,
        ),
        sementes: sementes.clone(),
        admin_token: std::env::var("YGGDRASIL_ADMIN_TOKEN").ok(),
        // YG-163: PIX independente — ligado só se YGGDRASIL_PIX_KEY estiver setada.
        pix: pix::PixConfig::from_env(),
        // YG-164: meta de arrecadação (BRL). 0/ausente = sem meta (barra mostra só total).
        meta: std::env::var("YGGDRASIL_CAMPANHA_META")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0),
        // YG-172: onde os comprovantes de PIX são gravados (volume persistente).
        comprovantes_dir: std::path::PathBuf::from(
            std::env::var("YGGDRASIL_COMPROVANTES_DIR")
                .unwrap_or_else(|_| "data/comprovantes".to_string()),
        ),
    });
    let campanha_router = Router::new()
        .route("/api/v1/campanha/tiers", get(api::campanha::list_tiers))
        .route("/api/v1/campanha/apoiar", post(api::campanha::apoiar))
        .route("/api/v1/campanha/progresso", get(api::campanha::progresso))
        .route("/api/v1/creditos", get(api::campanha::list_creditos))
        // YG-165: admin de apoios (gated por YGGDRASIL_ADMIN_TOKEN no handler)
        .route("/api/v1/campanha/pledges", get(api::campanha::list_pledges))
        .route(
            "/api/v1/campanha/pledges/{id}/confirmar",
            post(api::campanha::confirmar_pledge),
        )
        // YG-172: apoiador anexa comprovante (público, por id); operador baixa (admin)
        .route(
            "/api/v1/campanha/pledges/{id}/comprovante",
            post(api::campanha::enviar_comprovante),
        )
        .route(
            "/api/v1/campanha/pledges/{id}/comprovante/arquivo",
            get(api::campanha::baixar_comprovante),
        )
        .with_state(campanha_state);

    // Universo `comunicacao` — salas interativas de léxico cross-linguístico
    // (Mbyá Guaraní × Iorubá). Auto-contido: salas em disco + write-back de
    // termos novos no repo `comunicacao` (markdown) + fila de revisão.
    let comunicacao_rooms_dir = std::env::var("YGGDRASIL_COMUNICACAO_DIR")
        .unwrap_or_else(|_| "data/comunicacao".to_string());
    let comunicacao_lexicon_dir =
        std::env::var("COMUNICACAO_DIR").unwrap_or_else(|_| "../comunicacao".to_string());

    // YG-139: motor NLP de corpus (DuckDB em memória, montado do canônico no
    // boot). Best-effort: se o DuckDB falhar, o router de corpus não sobe.
    // YG-146: NPC endpoint (Ollama env-gated + fallback determinístico).
    let npc_state = Arc::new(api::npc::NpcState::from_env());
    let npc_router = Router::new()
        .route("/api/v1/npc", axum::routing::post(api::npc::post_npc))
        .with_state(npc_state);

    let corpus_router = match corpus_nlp::CorpusDb::build(std::path::Path::new(
        &comunicacao_lexicon_dir,
    )) {
        Ok(db) => {
            tracing::info!("corpus-nlp: índice DuckDB montado");
            Router::new()
                .route("/api/v1/corpus", get(serve_corpus_list))
                .route("/api/v1/corpus/compare", get(serve_corpus_compare))
                .route("/api/v1/corpus/{name}/freq", get(serve_corpus_freq))
                .with_state(Arc::new(db))
        }
        Err(e) => {
            tracing::warn!(erro = %e, "corpus-nlp: DuckDB indisponível — rotas /corpus desligadas");
            Router::new()
        }
    };

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

    // YG-168: ledger de bits por usuário (camada Shannon — score do ÑE'Ẽ).
    let comunicacao_score = Arc::new(
        yggdrasil_core::comunicacao::BitsLedgerStore::new(&comunicacao_rooms_dir)
            .map_err(|e| anyhow::anyhow!("score store: {e}"))?,
    );
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
            // YG-168: ledger de bits por usuário.
            score: comunicacao_score,
        }
        .with_bridge(co_bridge.sender(), co_bridge.obs_sender()),
    );
    // Rede de segurança periódica (no-op se o write-back estiver desligado).
    comunicacao_routes::spawn_writeback_sweeper(comunicacao_state.clone());

    // YG-170: sequencer espacial — salvar motivos gravados pelo front.
    // Subdiretório do mesmo comunicacao_rooms_dir (policy consistente).
    let motivos_dir = std::path::Path::new(&comunicacao_rooms_dir).join("motivos");
    let motivos_router = Router::new()
        .route(
            "/api/v1/comunicacao/motivos",
            post(api::motivos::save_motivo),
        )
        .with_state(Arc::new(api::motivos::MotivosState {
            jwt_secret: auth_state.jwt_secret.clone(),
            motivos_dir,
        }));

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
            "/api/v1/comunicacao/lexico/busca",
            get(comunicacao_routes::lexico_busca),
        )
        .route(
            "/api/v1/comunicacao/lexico/indice",
            get(comunicacao_routes::lexico_indice),
        )
        .route(
            "/api/v1/comunicacao/lexico/promover",
            post(comunicacao_routes::promover_termo),
        )
        .route(
            "/api/v1/comunicacao/templates",
            get(comunicacao_routes::list_templates),
        )
        // YG-155: LexiconPack — música/idioma como pacotes jogáveis que soam.
        .route(
            "/api/v1/comunicacao/packs",
            get(comunicacao_routes::list_packs),
        )
        // YG-169: projeção CO-universe → pack de língua (somente leitura).
        .route(
            "/api/v1/comunicacao/packs/lang/{plano}",
            get(comunicacao_routes::get_lang_pack),
        )
        .route(
            "/api/v1/comunicacao/packs/{id}",
            get(comunicacao_routes::get_pack),
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
        // YG-113: sugestões do Caderno → pipeline de curadoria
        .route(
            "/api/v1/comunicacao/caderno/sugestoes",
            post(comunicacao_routes::sugerir_verso),
        )
        .route(
            "/api/v1/comunicacao/corpus/{slug}/correcoes",
            get(comunicacao_routes::corpus_correcoes),
        )
        // YG-168: score / camada Shannon — ledger de bits por usuário.
        .route(
            "/api/v1/comunicacao/score",
            get(comunicacao_routes::get_score),
        )
        .route(
            "/api/v1/comunicacao/score/descobrir",
            post(comunicacao_routes::score_descobrir),
        )
        .route(
            "/api/v1/comunicacao/score/identificar",
            post(comunicacao_routes::score_identificar),
        )
        .route(
            "/api/v1/comunicacao/score/revelar",
            post(comunicacao_routes::score_revelar),
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
        .route("/universos/nee", get(serve_nee))
        .route("/universos/nee/world", get(serve_nee_world))
        .route("/universos/corpus", get(serve_corpus))
        .route("/universos/corpus-lab", get(serve_corpus_lab))
        .route("/universos/dino", get(serve_dino))
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
        .merge(passkey_router)
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
        .merge(profile_router)
        .merge(feedback_router)
        .merge(campanha_router)
        .merge(comunicacao_router)
        .merge(motivos_router)
        .merge(stream_router)
        .merge(atividade_router)
        .merge(corpus_router)
        .merge(npc_router)
        // Criar universo autorado (template picker + meus universos). O segmento
        // estático "new" vence a captura {id} da rota do player abaixo.
        .route("/universos/instance/new", get(serve_instance_new))
        .route("/universos/instance/{id}", get(serve_instance_player))
        // YG-87: neuro = viewer multiescala Neuroglancer (atlas subcortical
        // Precomputed servido mesmo-origem). O viewer Godot macro fica em /anatomia.
        .route("/neuro", get(serve_neuro))
        .route("/universos/neuro", get(serve_neuro))
        // Mural público de feedback (Fale conosco) — nome sim, e-mail nunca.
        .route("/feedback", get(serve_feedback))
        // YG-128: analytics ao vivo — agregados anônimos (hub do CO + stats locais)
        .route("/analytics", get(serve_analytics))
        // YG-143: landing de campanha (tiers de REWARDS.md + stats ao vivo)
        .route("/campanha", get(serve_campanha))
        // YG-161: créditos — apoiadores que optaram por aparecer (público)
        .route("/creditos", get(serve_creditos))
        // YG-165: admin de apoios (a página pede o admin token; API é gated)
        .route("/campanha/admin", get(serve_campanha_admin))
        // YG-144: reader do SRD de Shandara (estático vence o fallback {slug})
        .route("/universos/shandara", get(serve_shandara))
        .route("/api/v1/shandara/srd", get(serve_shandara_tree))
        .route("/api/v1/shandara/srd/{*path}", get(serve_shandara_doc))
        // YG-84: visualizador 3D de anatomia (Godot Web export, single-thread →
        // serve de qualquer host estático). Os arquivos do export ficam em
        // static/anatomia/ e são servidos pelo ServeDir abaixo; /anatomia é só
        // um atalho amigável.
        .route("/anatomia", get(serve_anatomia))
        // YG-146: protótipo do Mundo walkable (5 temas, p/ feedback). Atalho ao
        // estático; `?tema=<id>` entra direto numa versão.
        .route("/mundo", get(serve_mundo))
        // YG-153: navegar/editar um universo do CO dentro do Mundo (federação
        // inbound, cliente). `?u=<universe_key>`. Lê/escreve a API do CO com o
        // cookie compartilhado — sem trabalho server-side aqui.
        .route("/co-mundo", get(serve_co_mundo))
        // CORS aberto no estático: o Neuroglancer (hosted, outra origem) precisa
        // buscar os dados Precomputed em /static/neuro-data/ via fetch cross-origin.
        // `Cache-Control: no-cache` ≠ "não cachear": o browser guarda mas REVALIDA
        // (If-Modified-Since → 304). Sem isso o heuristic caching segurava JS
        // velho por horas depois de cada deploy — paleta/editor "sumiam".
        .nest_service(
            "/static",
            tower::ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .layer(tower_http::set_header::SetResponseHeaderLayer::overriding(
                    axum::http::header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("no-cache"),
                ))
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

/// Criar universo autorado: template picker + lista "meus universos". Fecha o
/// loop do CTA "Criar universo" da landing (antes apontava para cá sem a rota
/// existir).
async fn serve_instance_new() -> impl IntoResponse {
    Html(include_str!("../static/universos/new.html"))
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
/// YG-143: landing de campanha (da semente ao topo) — tiers + stats ao vivo.
async fn serve_campanha() -> impl IntoResponse {
    Html(include_str!("../static/campanha.html"))
}

/// YG-161: créditos — rol de apoiadores (lê `GET /api/v1/creditos`, sem e-mail).
async fn serve_creditos() -> impl IntoResponse {
    Html(include_str!("../static/creditos.html"))
}

/// YG-165: admin de apoios — a página pede o `YGGDRASIL_ADMIN_TOKEN` e lê
/// `GET /api/v1/campanha/pledges` (gated). Fora da nav pública.
async fn serve_campanha_admin() -> impl IntoResponse {
    Html(include_str!("../static/campanha-admin.html"))
}

// ─── YG-144: reader do SRD de Shandara (conteúdo embutido) ───────────────────

/// Página do reader.
async fn serve_shandara() -> impl IntoResponse {
    Html(include_str!("../static/universos/shandara.html"))
}

/// `GET /api/v1/shandara/srd` — árvore de docs (seções → docs).
async fn serve_shandara_tree() -> impl IntoResponse {
    axum::Json(shandara::tree())
}

/// `GET /api/v1/shandara/srd/{*path}` — Markdown cru de um doc.
async fn serve_shandara_doc(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    match shandara::doc(&path) {
        Some(md) => (
            [(
                axum::http::header::CONTENT_TYPE,
                "text/markdown; charset=utf-8",
            )],
            md,
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "doc não encontrado").into_response(),
    }
}

async fn serve_feedback() -> impl IntoResponse {
    Html(include_str!("../static/feedback-mural.html"))
}

/// YG-128: seção pública de analytics ao vivo — só agregados anônimos
/// (summary do CO PII-stripped + stats locais de jogo + placares).
async fn serve_analytics() -> impl IntoResponse {
    Html(include_str!("../static/analytics.html"))
}

async fn serve_anatomia() -> impl IntoResponse {
    Redirect::permanent("/static/anatomia/")
}

// YG-146: serve a página do protótipo direto (preserva `?tema=` na URL /mundo).
async fn serve_mundo() -> impl IntoResponse {
    Html(include_str!("../static/universos/mundo-proto.html"))
}

// YG-153: navegar/editar um universo do CO no Mundo (preserva `?u=` em /co-mundo).
async fn serve_co_mundo() -> impl IntoResponse {
    Html(include_str!("../static/universos/co-mundo.html"))
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

async fn serve_dino() -> impl IntoResponse {
    Html(include_str!("../static/universos/dino.html"))
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

/// YG-155: ÑE'Ẽ — pacotes de léxico que **soam** (música/idioma via Web Audio).
async fn serve_nee() -> impl IntoResponse {
    Html(include_str!("../static/universos/nee.html"))
}

/// YG-167: sala caminhável de um LexiconPack — pisar numa entrada toca o som.
async fn serve_nee_world() -> impl IntoResponse {
    Html(include_str!("../static/universos/nee-world.html"))
}

/// Superfície de exploração do **Ayvu Rapyta** — leitura verso a verso, capítulo
/// a capítulo: Mbyá ⟷ Español + NOTAS de Cadogan + partículas do léxico.
async fn serve_corpus() -> impl IntoResponse {
    Html(include_str!("../static/universos/corpus.html"))
}

/// YG-139: laboratório de corpus — frequência + comparação cross-linguística.
async fn serve_corpus_lab() -> impl IntoResponse {
    Html(include_str!("../static/universos/corpus-lab.html"))
}

// ─── YG-139: API do framework NLP de corpus (DuckDB) ─────────────────────────

#[derive(serde::Deserialize)]
struct FreqQuery {
    #[serde(default = "freq_limit")]
    limit: usize,
}
fn freq_limit() -> usize {
    50
}

#[derive(serde::Deserialize)]
struct CompareQuery {
    a: String,
    b: String,
    #[serde(default = "compare_mode")]
    mode: String,
    #[serde(default = "freq_limit")]
    limit: usize,
}
fn compare_mode() -> String {
    "inner".into()
}

async fn serve_corpus_list(
    axum::extract::State(db): axum::extract::State<Arc<corpus_nlp::CorpusDb>>,
) -> impl IntoResponse {
    match db.list_corpora() {
        Ok(v) => axum::Json(v).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "erro").into_response(),
    }
}

async fn serve_corpus_freq(
    axum::extract::State(db): axum::extract::State<Arc<corpus_nlp::CorpusDb>>,
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<FreqQuery>,
) -> impl IntoResponse {
    match db.freq(&name, q.limit.min(500)) {
        Ok(v) => axum::Json(v).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "erro").into_response(),
    }
}

async fn serve_corpus_compare(
    axum::extract::State(db): axum::extract::State<Arc<corpus_nlp::CorpusDb>>,
    axum::extract::Query(q): axum::extract::Query<CompareQuery>,
) -> impl IntoResponse {
    let mode = if q.mode == "left" { "left" } else { "inner" };
    match db.compare(&q.a, &q.b, mode, q.limit.min(500)) {
        Ok(v) => axum::Json(v).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "erro").into_response(),
    }
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

/// `GET /api/v1/config` — config pública do frontend, resolvida em runtime no
/// servidor (sem hardcode no JS). Hoje expõe a base do CO (`CO_BASE_URL`, p/ os
/// deep-links) e o feature-gate do round-trip editável (YG-124): `co_editor_enabled`
/// só vira `true` quando o CO está bidirecional (CO-413), e só então o botão
/// "Editar no CO" do inspetor de nota aparece.
async fn frontend_config() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "co_base_url": auth_co::co_base_url(),
        "co_editor_enabled": auth_co::co_editor_enabled(),
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
