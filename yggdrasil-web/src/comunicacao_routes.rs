//! Rotas do universo **comunicação** — salas interativas de léxico.
//!
//! Contrato HTTP (auth via `sub` do JWT, mesmo padrão de [`crate::api::instances`]):
//!
//! ```text
//! POST   /api/v1/comunicacao/salas?template=yoruba|mbya|blank&title=&lang=
//! GET    /api/v1/comunicacao/salas                  (do dono; ?published=true = feed)
//! GET    /api/v1/comunicacao/salas/{id}
//! PATCH  /api/v1/comunicacao/salas/{id}             (RoomEdit)
//! DELETE /api/v1/comunicacao/salas/{id}
//! POST   /api/v1/comunicacao/salas/{id}/elementos/{eid}/publicar
//! GET    /api/v1/comunicacao/lexico?lang=&q=
//! GET    /api/v1/comunicacao/templates
//! GET    /api/v1/comunicacao/revisao
//! POST   /api/v1/comunicacao/revisao/nota           ({ term_path, correct })
//! ```

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use nanoid::nanoid;
use serde::Deserialize;
use tokio::sync::broadcast;
use yggdrasil_core::comunicacao::{
    Caderno, CadernoStore, LexiconError, LexiconStore, ReviewItem, Room, RoomEdit, RoomStore,
    Writeback,
    lexicon::{Contribution, slugify},
    room::EditError,
    store::StoreError,
    template_instantiate, template_summaries,
};
use yggdrasil_core::instance::{InstanceStore, NoteStore, UniverseInstance};

use crate::auth::verify_jwt;
use crate::co_bridge_producer::{
    AYVU_INSTANCE, FederatedSource, NoteKind, NoteWritten, ObservabilityEvent, RoomActivity,
    comunicacao_term_event,
};

/// Estado compartilhado das rotas de comunicação.
pub struct ComunicacaoState {
    pub jwt_secret: String,
    pub store: Arc<RoomStore>,
    pub lexicon: Arc<LexiconStore>,
    /// Motor de write-back das contribuições `_users/` → git (YG-100).
    pub writeback: Arc<Writeback>,
    /// YG-103: canal de conteúdo do producer da federated bus — emite
    /// `entry.*` `universe_key=comunicacao` por termo publicado. `None` quando
    /// o producer está desligado (gate de env) — emitir vira no-op.
    pub term_events: Option<broadcast::Sender<NoteWritten>>,
    /// YG-103: canal de observability — emite `yggdrasil.sala.*` (atividade de
    /// sala, não-conteúdo). `None` ⇒ no-op.
    pub room_events: Option<broadcast::Sender<ObservabilityEvent>>,
    /// YG-101: token de curador (`YGGDRASIL_ADMIN_TOKEN`, mesmo do feedback/
    /// analytics) que libera promover stubs → léxico curado. `None` ⇒ curadoria
    /// desabilitada (401).
    pub admin_token: Option<String>,
    /// YG-112: store per-user do Caderno do Ayvu Rapyta (favoritos/notas/
    /// progresso) — `<root>/_caderno/<user-slug>.json`, espelhando o per-user de
    /// revisão.
    pub caderno: Arc<CadernoStore>,
    /// YG-112/114: store de instâncias, p/ gravar as **notas** do Caderno via
    /// `NoteStore` sob a instância canônica do Ayvu ([`AYVU_INSTANCE`]) — assim o
    /// producer staged (YG-93/97) as federa. O `term_events` (canal de conteúdo
    /// `NoteWritten`) é reusado para emitir essas notas (fonte `Notes`).
    pub instance_store: Arc<InstanceStore>,
}

impl ComunicacaoState {
    /// Liga os canais do producer do CO (YG-103). Sem isto, os campos ficam
    /// `None` e a federação é no-op.
    pub fn with_bridge(
        mut self,
        terms: broadcast::Sender<NoteWritten>,
        rooms: broadcast::Sender<ObservabilityEvent>,
    ) -> Self {
        self.term_events = Some(terms);
        self.room_events = Some(rooms);
        self
    }

    /// Emite os termos de uma sala como `entry.*` na bus (YG-103). No-op se o
    /// producer estiver desligado. `kind` distingue created/updated/deleted —
    /// reusa [`comunicacao_term_event`] (mesmo path/corpo do backfill).
    fn emit_terms(&self, room: &Room, kind: NoteKind) {
        let Some(tx) = &self.term_events else { return };
        let user_slug = slugify(&room.owner);
        for elem in &room.elements {
            if let Some(mut ev) = comunicacao_term_event(room, elem, &user_slug) {
                ev.kind = kind;
                let _ = tx.send(ev);
            }
        }
    }

    /// Emite um evento de atividade de sala (`yggdrasil.sala.*`, YG-103). No-op
    /// se o producer estiver desligado.
    fn emit_room_activity(&self, room: &Room, activity: RoomActivity, actor: &str) {
        let Some(tx) = &self.room_events else { return };
        let _ = tx.send(
            ObservabilityEvent::room(activity, &room.id, &room.lang, actor)
                .at(room.updated_at.to_rfc3339()),
        );
    }

    /// `NoteStore` das notas do Caderno, enraizado na instância canônica do Ayvu
    /// ([`AYVU_INSTANCE`]) — o mesmo padrão de [`crate::api::instances`]. Garante
    /// que o *shell* da instância exista em disco (cria se ausente) para que a
    /// nota tenha onde morar e o producer a enxergue no backfill.
    fn ayvu_note_store(&self) -> NoteStore {
        if !self.instance_store.exists(AYVU_INSTANCE) {
            let inst = UniverseInstance::empty(
                AYVU_INSTANCE,
                crate::co_bridge_producer::ORIGIN_DEPLOYMENT,
                "Ayvu Rapyta",
            );
            if let Err(e) = self.instance_store.save(&inst) {
                tracing::warn!(erro = %e, "caderno: não foi possível criar a instância do Ayvu");
            }
        }
        NoteStore::for_instance(self.instance_store.root(), AYVU_INSTANCE)
    }

    /// Emite um [`NoteWritten`] (fonte `Notes`) no canal de conteúdo do producer,
    /// se ligado — espelha o `emit_note` de [`crate::api::instances`]. No-op
    /// quando o producer está desligado (sem assinantes).
    fn emit_note(&self, event: NoteWritten) {
        if let Some(tx) = &self.term_events {
            let _ = tx.send(event);
        }
    }
}

type ApiState = State<Arc<ComunicacaoState>>;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn err_json(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({ "erro": msg }))).into_response()
}

fn unauthorized() -> axum::response::Response {
    err_json(StatusCode::UNAUTHORIZED, "nao_autenticado")
}

#[allow(clippy::result_large_err)]
fn require_user(
    state: &ComunicacaoState,
    headers: &HeaderMap,
) -> Result<String, axum::response::Response> {
    let token = extract_bearer(headers).ok_or_else(unauthorized)?;
    verify_jwt(&token, &state.jwt_secret).map_err(|_| unauthorized())
}

/// Exige um **curador** (YG-101): `Authorization: Bearer <YGGDRASIL_ADMIN_TOKEN>`
/// (mesmo padrão do feedback/analytics). 401 se a curadoria não está configurada;
/// 403 se o token não confere (não-curador).
#[allow(clippy::result_large_err)]
fn require_curator(
    state: &ComunicacaoState,
    headers: &HeaderMap,
) -> Result<(), axum::response::Response> {
    let admin = state
        .admin_token
        .as_deref()
        .ok_or_else(|| err_json(StatusCode::UNAUTHORIZED, "curador_nao_configurado"))?;
    let provided = extract_bearer(headers).unwrap_or_default();
    if provided == admin {
        Ok(())
    } else {
        Err(err_json(StatusCode::FORBIDDEN, "nao_e_curador"))
    }
}

#[allow(clippy::result_large_err)]
fn load_owned(
    state: &ComunicacaoState,
    id: &str,
    user: &str,
) -> Result<Room, axum::response::Response> {
    let room = state.store.load(id).map_err(map_store_err)?;
    if room.owner != user {
        return Err(err_json(StatusCode::FORBIDDEN, "nao_e_dono"));
    }
    Ok(room)
}

fn map_store_err(e: StoreError) -> axum::response::Response {
    match e {
        StoreError::NotFound(_) => err_json(StatusCode::NOT_FOUND, "sala_nao_encontrada"),
        StoreError::Serde(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "sala_corrompida"),
        StoreError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "erro_io"),
    }
}

fn map_edit_err(e: EditError) -> axum::response::Response {
    let status = match e {
        EditError::ElementExists(_) | EditError::LinkExists(_) => StatusCode::CONFLICT,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (status, Json(serde_json::json!({ "erro": e.to_string() }))).into_response()
}

fn map_lexicon_err(e: LexiconError) -> axum::response::Response {
    match e {
        LexiconError::UnsupportedLang(_) => {
            err_json(StatusCode::UNPROCESSABLE_ENTITY, "lingua_sem_plano")
        }
        LexiconError::EmptyWord => err_json(StatusCode::UNPROCESSABLE_ENTITY, "palavra_vazia"),
        LexiconError::NotFound(_) => err_json(StatusCode::NOT_FOUND, "termo_nao_encontrado"),
        LexiconError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "erro_io_lexico"),
    }
}

// ─── CRUD de salas ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateQuery {
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Língua da sala em branco (default `pt`). Ignorado quando há template.
    #[serde(default)]
    pub lang: Option<String>,
}

/// `POST /api/v1/comunicacao/salas` — cria sala (de template ou em branco).
pub async fn create_sala(
    State(state): ApiState,
    headers: HeaderMap,
    Query(q): Query<CreateQuery>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let id = nanoid!();
    let mut room = match q.template.as_deref() {
        Some(slug) => match template_instantiate(slug, &id, &user) {
            Some(r) => r,
            None => return err_json(StatusCode::NOT_FOUND, "template_nao_encontrado"),
        },
        None => Room::empty(
            &id,
            &user,
            q.title.clone().unwrap_or_else(|| "Nova sala".into()),
            q.lang.clone().unwrap_or_else(|| "pt".into()),
        ),
    };
    if let Some(title) = q.title {
        room.title = title;
    }
    if let Err(e) = state.store.save(&room) {
        return map_store_err(e);
    }
    (StatusCode::CREATED, Json(room)).into_response()
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub published: bool,
}

/// `GET /api/v1/comunicacao/salas` — do dono, ou feed público.
pub async fn list_salas(
    State(state): ApiState,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    if q.published {
        return match state.store.list_published() {
            Ok(list) => Json(list).into_response(),
            Err(e) => map_store_err(e),
        };
    }
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match state.store.list_owner(&user) {
        Ok(list) => Json(list).into_response(),
        Err(e) => map_store_err(e),
    }
}

/// `GET /api/v1/comunicacao/salas/{id}` — público se `published`, senão owner-only.
pub async fn get_sala(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let room = match state.store.load(&id) {
        Ok(r) => r,
        Err(e) => return map_store_err(e),
    };
    if !room.published {
        match require_user(&state, &headers) {
            Ok(u) if u == room.owner => {}
            Ok(_) => return err_json(StatusCode::FORBIDDEN, "nao_e_dono"),
            Err(r) => return r,
        }
    }
    Json(room).into_response()
}

/// `PATCH /api/v1/comunicacao/salas/{id}` — aplica um [`RoomEdit`] (owner-only).
pub async fn patch_sala(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(op): Json<RoomEdit>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut room = match load_owned(&state, &id, &user) {
        Ok(r) => r,
        Err(r) => return r,
    };
    let was_published = room.published;
    if let Err(e) = op.apply(&mut room) {
        return map_edit_err(e);
    }
    if let Err(e) = state.store.save(&room) {
        return map_store_err(e);
    }
    // YG-103: federa a transição de publicação ao CO.
    match (was_published, room.published) {
        (false, true) => {
            // Sala recém-publicada: anuncia atividade + federa seus termos.
            state.emit_room_activity(&room, RoomActivity::Published, &user);
            state.emit_terms(&room, NoteKind::Created);
        }
        (true, false) => {
            state.emit_room_activity(&room, RoomActivity::Unpublished, &user);
        }
        (true, true) => {
            // Edição de uma sala já pública: propaga o conteúdo atualizado.
            state.emit_terms(&room, NoteKind::Updated);
        }
        (false, false) => {}
    }
    Json(room).into_response()
}

/// `DELETE /api/v1/comunicacao/salas/{id}` — owner-only.
pub async fn delete_sala(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    match state.store.delete(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => map_store_err(e),
    }
}

/// `POST /api/v1/comunicacao/salas/{id}/fork` — "Sugerir": cria uma **cópia
/// pessoal editável** de uma sala pública (ou própria). Login obrigatório.
pub async fn fork_sala(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let src = match state.store.load(&id) {
        Ok(r) => r,
        Err(e) => return map_store_err(e),
    };
    // pode forkar salas públicas (published) ou as próprias
    if !src.published && src.owner != user {
        return err_json(StatusCode::FORBIDDEN, "nao_pode_forkar");
    }
    let mut copy = src;
    copy.id = nanoid!();
    copy.owner = user;
    copy.published = false;
    copy.template = format!("fork:{id}");
    copy.title = format!("{} — minha cópia", copy.title);
    if let Err(e) = state.store.save(&copy) {
        return map_store_err(e);
    }
    (StatusCode::CREATED, Json(copy)).into_response()
}

// ─── Publicar elemento no léxico ────────────────────────────────────────────────

/// `POST /api/v1/comunicacao/salas/{id}/elementos/{eid}/publicar`
///
/// O "PUT" que liga o universo do usuário ao léxico geral: se a palavra já
/// existe no léxico compartilhado, **liga** a ela; senão **cria** uma entrada
/// nova atribuída ao usuário. Em ambos os casos enfileira o termo para revisão.
pub async fn publicar_elemento(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, eid)): Path<(String, String)>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut room = match load_owned(&state, &id, &user) {
        Ok(r) => r,
        Err(r) => return r,
    };
    let Some(element) = room.element(&eid).cloned() else {
        return err_json(StatusCode::NOT_FOUND, "elemento_nao_encontrado");
    };

    // parents (composição fractal): palavras dos termos-parente ligados por
    // `compoe` a partir deste elemento → viram `parts:` na entrada de léxico.
    let parts: Vec<String> = room
        .links
        .iter()
        .filter(|l| l.is_compose() && l.from == eid)
        .filter_map(|l| room.element(&l.to).map(|p| p.word.clone()))
        .collect();

    let contribution: Contribution = match state.lexicon.contribute(&user, &element, &parts) {
        Ok(c) => c,
        Err(e) => return map_lexicon_err(e),
    };

    // liga o elemento à entrada de léxico
    if let Some(el) = room.element_mut(&eid) {
        el.lexicon = contribution.link.clone();
    }
    if let Err(e) = state.store.save(&room) {
        return map_store_err(e);
    }

    // enfileira para revisão (idempotente por term_path)
    let mut queue = match state.store.load_review(&user) {
        Ok(q) => q,
        Err(e) => return map_store_err(e),
    };
    let item = ReviewItem::new(
        contribution.relative_path.clone(),
        element.word.clone(),
        element.lang.clone(),
        element.gloss.clone(),
        chrono::Utc::now(),
    );
    let review_added = queue.upsert(item);
    if let Err(e) = state.store.save_review(&user, &queue) {
        return map_store_err(e);
    }

    // YG-100: write-back não-bloqueante. Só dispara quando o `publicar` CRIOU
    // um arquivo novo em `_users/` — ligar a termo já existente não muda nada
    // em disco. O git roda em `spawn_blocking` (Command é síncrono) dentro de
    // `tokio::spawn` p/ não atrasar a resposta ao cliente.
    if contribution.created {
        spawn_writeback(state.writeback.clone(), contribution.relative_path.clone());
    }

    // YG-118: sinal de atividade de sala ao hub do CO.
    state.emit_room_activity(&room, RoomActivity::TermContributed, &user);

    let scope = match contribution.scope {
        yggdrasil_core::comunicacao::LexiconScope::Shared => "compartilhado",
        yggdrasil_core::comunicacao::LexiconScope::User => "usuario",
    };
    Json(serde_json::json!({
        "caminho": contribution.relative_path,
        "criado": contribution.created,
        "escopo": scope,
        "revisao_adicionada": review_added,
        "elemento": room.element(&eid),
    }))
    .into_response()
}

// ─── Curadoria de léxico (YG-101) ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PromoverBody {
    /// Código de língua da entrada (`yo`, `gn-mbya`, …).
    pub lang: String,
    /// Usuário contribuidor (dono do stub em `_users/<user>/`).
    pub user: String,
    /// Slug do termo a promover.
    pub slug: String,
    /// Identidade do curador (`reviewed_by`); default `"curador"`.
    #[serde(default)]
    pub curator: Option<String>,
}

/// `POST /api/v1/comunicacao/lexico/promover` — um **curador** promove um stub
/// `_users/<u>/<slug>.md` ao léxico curado `terms/<slug>.md` (YG-101). Vira o
/// frontmatter p/ `reviewed`, arquiva o stub, repõe a fila de revisão do
/// contribuidor (não reaparece como stub) e versiona a curadoria (git, não-bloq).
pub async fn promover_termo(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<PromoverBody>,
) -> impl IntoResponse {
    if let Err(r) = require_curator(&state, &headers) {
        return r;
    }
    let curator = body.curator.as_deref().unwrap_or("curador");
    let promotion = match state
        .lexicon
        .promote(&body.lang, &body.user, &body.slug, curator)
    {
        Ok(p) => p,
        Err(e) => return map_lexicon_err(e),
    };

    // a fila do contribuidor reflete o novo status: o item passa a apontar p/ o
    // termo curado em vez do stub arquivado (não reaparece como pendente).
    if let Ok(mut queue) = state.store.load_review(&body.user)
        && let Some(item) = queue.get_mut(&promotion.from_rel)
    {
        item.term_path = promotion.to_rel.clone();
        let _ = state.store.save_review(&body.user, &queue);
    }

    // versiona a curadoria (stub removido + termo curado) num commit, não-bloq.
    spawn_curation_writeback(
        state.writeback.clone(),
        promotion.from_rel.clone(),
        promotion.to_rel.clone(),
        promotion.slug.clone(),
    );

    Json(serde_json::json!({
        "promovido": promotion.created,
        "arquivado": promotion.archived,
        "de": promotion.from_rel,
        "para": promotion.to_rel,
        "slug": promotion.slug,
    }))
    .into_response()
}

// ─── Write-back de contribuições → git (YG-100) ─────────────────────────────────

/// Dispara o write-back de UMA contribuição recém-criada, sem bloquear o
/// request. O motor é env-gated: desligado → no-op silencioso. Falhas (git
/// ausente, dir não-git, conflito) são logadas em `warn` e nunca propagadas —
/// a publicação já gravou o arquivo em disco; o git é a camada de persistência.
fn spawn_writeback(writeback: Arc<Writeback>, relative_path: String) {
    tokio::spawn(async move {
        let message = format!(
            "feat(comunicacao): contribuição de léxico {relative_path}\n\n\
             Write-back automático via Yggdrasil (YG-100).\n\
             Co-Authored-By: Claude <noreply@anthropic.com>"
        );
        let res =
            tokio::task::spawn_blocking(move || writeback.run(&[relative_path], &message)).await;
        match res {
            Ok(Ok(outcome)) => {
                tracing::debug!(?outcome, "comunicacao write-back");
            }
            Ok(Err(e)) => {
                tracing::warn!(erro = %e, "comunicacao write-back falhou");
            }
            Err(e) => {
                tracing::warn!(erro = %e, "comunicacao write-back: task join falhou");
            }
        }
    });
}

/// Versiona uma **curadoria** (YG-101): stub removido + termo curado num único
/// commit, sem bloquear o request. Usa `commit_paths` (curador-autorizado) p/
/// versionar também o caminho curado `terms/<slug>.md` — fora do filtro `_users/`
/// do write-back automático. Env-gated: desligado → no-op.
fn spawn_curation_writeback(
    writeback: Arc<Writeback>,
    from_rel: String,
    to_rel: String,
    slug: String,
) {
    tokio::spawn(async move {
        let message = format!(
            "feat(comunicacao): curar léxico {slug} ({from_rel} → {to_rel})\n\n\
             Promoção stub → reviewed via Yggdrasil (YG-101).\n\
             Co-Authored-By: Claude <noreply@anthropic.com>"
        );
        let paths = vec![from_rel, to_rel];
        let res =
            tokio::task::spawn_blocking(move || writeback.commit_paths(&paths, &message)).await;
        match res {
            Ok(Ok(outcome)) => tracing::debug!(?outcome, "comunicacao curadoria write-back"),
            Ok(Err(e)) => tracing::warn!(erro = %e, "comunicacao curadoria write-back falhou"),
            Err(e) => tracing::warn!(erro = %e, "comunicacao curadoria: task join falhou"),
        }
    });
}

/// Rede de segurança periódica: percorre todas as contribuições `_users/` em
/// disco e faz write-back das que ainda não estão commitadas. Pega casos onde o
/// disparo por evento falhou (ex.: git temporariamente indisponível) ou
/// arquivos que entraram no volume por outra via. Idempotente: o próprio
/// [`Writeback::run`] não cria commit vazio.
pub async fn run_writeback_sweep(state: &ComunicacaoState) {
    let lex_root = state.lexicon.root().to_path_buf();
    let writeback = state.writeback.clone();
    // gate barato: se desligado, nem varre o disco.
    if !writeback.config().enabled {
        return;
    }
    let res = tokio::task::spawn_blocking(move || {
        let paths = collect_user_contributions(&lex_root);
        if paths.is_empty() {
            return Ok(yggdrasil_core::comunicacao::WritebackOutcome::NothingToCommit);
        }
        writeback.run(
            &paths,
            "feat(comunicacao): sync periódico de contribuições de léxico\n\n\
             Rede de segurança (YG-100).\n\
             Co-Authored-By: Claude <noreply@anthropic.com>",
        )
    })
    .await;
    match res {
        Ok(Ok(outcome)) => tracing::debug!(?outcome, "comunicacao write-back sweep"),
        Ok(Err(e)) => tracing::warn!(erro = %e, "comunicacao write-back sweep falhou"),
        Err(e) => tracing::warn!(erro = %e, "comunicacao write-back sweep: join falhou"),
    }
}

/// Spawna a rede de segurança periódica do write-back (default a cada 5 min).
pub fn spawn_writeback_sweeper(state: Arc<ComunicacaoState>) {
    if !state.writeback.config().enabled {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            run_writeback_sweep(&state).await;
        }
    });
}

/// Lista os caminhos relativos (`<lang>/terms/_users/.../*.md`) de todas as
/// contribuições de usuário sob a raiz do léxico. Tolera ausência de pastas.
fn collect_user_contributions(lex_root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(langs) = std::fs::read_dir(lex_root) else {
        return out;
    };
    for lang in langs.flatten() {
        let users_dir = lang.path().join("terms").join("_users");
        collect_md_under(&users_dir, lex_root, &mut out);
    }
    out
}

/// Coleta recursivamente os `*.md` sob `dir`, devolvendo caminhos relativos a
/// `base` com separador `/`.
fn collect_md_under(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_md_under(&p, base, out);
        } else if p.extension().is_some_and(|x| x == "md")
            && let Ok(rel) = p.strip_prefix(base)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

// ─── Léxico (lookup) ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LexicoQuery {
    pub lang: String,
    pub q: String,
}

/// `GET /api/v1/comunicacao/lexico?lang=&q=` — consulta o léxico compartilhado.
pub async fn consultar_lexico(
    State(state): ApiState,
    Query(q): Query<LexicoQuery>,
) -> impl IntoResponse {
    let supported = LexiconStore::lang_dir(&q.lang).is_some();
    match state.lexicon.lookup_shared(&q.lang, &q.q) {
        Ok(found) => Json(serde_json::json!({
            "lingua_suportada": supported,
            "encontrado": found.is_some(),
            "caminho": found,
        }))
        .into_response(),
        Err(LexiconError::UnsupportedLang(_)) => Json(serde_json::json!({
            "lingua_suportada": false,
            "encontrado": false,
            "caminho": serde_json::Value::Null,
        }))
        .into_response(),
        Err(e) => map_lexicon_err(e),
    }
}

// ─── Léxico paginado ("carregar mais") ──────────────────────────────────────────

fn default_lista_limit() -> usize {
    100
}

#[derive(Deserialize)]
pub struct ListaQuery {
    pub lang: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_lista_limit")]
    pub limit: usize,
}

/// `GET /api/v1/comunicacao/lexico/lista?lang=&offset=&limit=` — fatia do léxico
/// por popularidade (sem auth — navegação pública). `limit=0` devolve só o total.
pub async fn lexico_lista(
    State(state): ApiState,
    Query(q): Query<ListaQuery>,
) -> impl IntoResponse {
    let limit = q.limit.min(500);
    let slice = yggdrasil_core::comunicacao::public::lexicon_slice(
        state.lexicon.root(),
        &q.lang,
        q.offset,
        limit,
    );
    Json(slice).into_response()
}

/// `GET /api/v1/comunicacao/corpus/{slug}` — JSON de corpus baked (sem auth —
/// navegação pública). Serve a superfície de exploração do Ayvu Rapyta:
/// capítulos → versos Mbyá ⟷ Español + glosas/partículas + NOTAS de Cadogan.
pub async fn corpus(State(state): ApiState, Path(slug): Path<String>) -> impl IntoResponse {
    match yggdrasil_core::comunicacao::public::corpus_json(state.lexicon.root(), &slug) {
        Some(body) => (
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "corpus não encontrado").into_response(),
    }
}

// ─── Templates ──────────────────────────────────────────────────────────────────

/// `GET /api/v1/comunicacao/templates` — lista os templates-semente.
pub async fn list_templates() -> impl IntoResponse {
    Json(template_summaries())
}

// ─── Revisão ──────────────────────────────────────────────────────────────────

/// `GET /api/v1/comunicacao/revisao` — fila completa + itens vencidos.
pub async fn get_revisao(State(state): ApiState, headers: HeaderMap) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let queue = match state.store.load_review(&user) {
        Ok(q) => q,
        Err(e) => return map_store_err(e),
    };
    let now = chrono::Utc::now();
    let due: Vec<&ReviewItem> = queue.due(now);
    Json(serde_json::json!({
        "total": queue.items.len(),
        "vencidos": due.len(),
        "itens": queue.items,
        "vencidos_agora": due,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct NotaBody {
    pub term_path: String,
    pub correct: bool,
}

/// `POST /api/v1/comunicacao/revisao/nota` — registra acerto/erro e reagenda.
pub async fn nota_revisao(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<NotaBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut queue = match state.store.load_review(&user) {
        Ok(q) => q,
        Err(e) => return map_store_err(e),
    };
    let now = chrono::Utc::now();
    let Some(item) = queue.get_mut(&body.term_path) else {
        return err_json(StatusCode::NOT_FOUND, "termo_nao_esta_na_fila");
    };
    item.grade(body.correct, now);
    let updated = item.clone();
    if let Err(e) = state.store.save_review(&user, &queue) {
        return map_store_err(e);
    }
    Json(updated).into_response()
}

// ─── Caderno do Ayvu Rapyta (YG-112) ────────────────────────────────────────────

/// `GET /api/v1/comunicacao/caderno` — devolve o Caderno completo do usuário
/// logado (favoritos + notas + progresso). Recuperável em qualquer dispositivo.
pub async fn get_caderno(State(state): ApiState, headers: HeaderMap) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    match state.caderno.load(&user) {
        Ok(c) => Json(c).into_response(),
        Err(e) => map_store_err(e),
    }
}

/// `PUT /api/v1/comunicacao/caderno/favoritos/{key}` — favorita um verso.
pub async fn add_favorito(
    State(state): ApiState,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut caderno = match state.caderno.load(&user) {
        Ok(c) => c,
        Err(e) => return map_store_err(e),
    };
    let added = caderno.add_fav(&key, chrono::Utc::now());
    if let Err(e) = state.caderno.save(&user, &caderno) {
        return map_store_err(e);
    }
    Json(serde_json::json!({ "chave": key, "favoritado": true, "novo": added })).into_response()
}

/// `DELETE /api/v1/comunicacao/caderno/favoritos/{key}` — desfavorita.
pub async fn remove_favorito(
    State(state): ApiState,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut caderno = match state.caderno.load(&user) {
        Ok(c) => c,
        Err(e) => return map_store_err(e),
    };
    let removed = caderno.remove_fav(&key);
    if let Err(e) = state.caderno.save(&user, &caderno) {
        return map_store_err(e);
    }
    Json(serde_json::json!({ "chave": key, "favoritado": false, "removido": removed }))
        .into_response()
}

#[derive(Deserialize)]
pub struct NotaCadernoBody {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub markdown: String,
}

/// `PUT /api/v1/comunicacao/caderno/notas/{key}` — cria/atualiza uma nota do
/// Caderno. Persiste **duas** vezes: (1) no Caderno per-user (recuperação rápida
/// na UI) e (2) via `NoteStore` sob a instância canônica do Ayvu ([`AYVU_INSTANCE`],
/// YG-114) + `emit_note` — exatamente como `put_note` em `instances.rs` — para o
/// producer staged (YG-93/97) federar de graça (contrato CO-389).
pub async fn put_nota_caderno(
    State(state): ApiState,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(body): Json<NotaCadernoBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let title = if body.title.trim().is_empty() {
        key.clone()
    } else {
        body.title.clone()
    };

    // (1) persiste no Caderno per-user
    let mut caderno = match state.caderno.load(&user) {
        Ok(c) => c,
        Err(e) => return map_store_err(e),
    };
    caderno.upsert_note(&key, &title, &body.markdown, chrono::Utc::now());
    if let Err(e) = state.caderno.save(&user, &caderno) {
        return map_store_err(e);
    }

    // (2) grava via NoteStore sob a instância do Ayvu + emite no producer
    //     (federação YG-114). Falha aqui é logada mas não derruba a resposta: o
    //     Caderno já está durável; a federação é a camada extra.
    let note_store = state.ayvu_note_store();
    let existed = note_store.load(&key).is_ok();
    match note_store.save(&key, &title, &body.markdown) {
        Ok(n) => {
            state.emit_note(NoteWritten {
                instance: AYVU_INSTANCE.to_string(),
                slug: n.slug.clone(),
                kind: if existed {
                    NoteKind::Updated
                } else {
                    NoteKind::Created
                },
                source: FederatedSource::Notes,
                title: n.title.clone(),
                body: n.body.clone(),
                updated_at: Some(n.updated.to_rfc3339()),
                frontmatter: None,
                actor: None,
                visibility: crate::co_bridge_producer::Visibility::Public,
            });
            Json(serde_json::json!({
                "chave": key,
                "slug": n.slug,
                "title": n.title,
                "federado": true,
            }))
            .into_response()
        }
        Err(e) => {
            tracing::warn!(erro = %e, "caderno: nota não federada (NoteStore falhou)");
            // o Caderno per-user já gravou — devolve sucesso parcial.
            Json(serde_json::json!({
                "chave": key,
                "slug": key,
                "title": title,
                "federado": false,
            }))
            .into_response()
        }
    }
}

/// `DELETE /api/v1/comunicacao/caderno/notas/{key}` — remove a nota do Caderno
/// per-user **e** da instância do Ayvu (federando o `entry.deleted`).
pub async fn delete_nota_caderno(
    State(state): ApiState,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut caderno = match state.caderno.load(&user) {
        Ok(c) => c,
        Err(e) => return map_store_err(e),
    };
    let removed = caderno.remove_note(&key);
    if let Err(e) = state.caderno.save(&user, &caderno) {
        return map_store_err(e);
    }
    // remove também da instância do Ayvu + federa o delete (best-effort).
    let note_store = state.ayvu_note_store();
    if note_store.delete(&key).is_ok() {
        state.emit_note(NoteWritten {
            instance: AYVU_INSTANCE.to_string(),
            slug: key.clone(),
            kind: NoteKind::Deleted,
            source: FederatedSource::Notes,
            title: String::new(),
            body: String::new(),
            updated_at: None,
            frontmatter: None,
            actor: None,
            visibility: crate::co_bridge_producer::Visibility::Public,
        });
    }
    Json(serde_json::json!({ "chave": key, "removido": removed })).into_response()
}

#[derive(Deserialize)]
pub struct ProgressoBody {
    pub verse: String,
}

/// `PUT /api/v1/comunicacao/caderno/progresso/{key}` — registra o avanço de
/// leitura (último verso lido) de um capítulo/seção.
pub async fn set_progresso(
    State(state): ApiState,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(body): Json<ProgressoBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut caderno = match state.caderno.load(&user) {
        Ok(c) => c,
        Err(e) => return map_store_err(e),
    };
    caderno.set_progress(&key, &body.verse);
    if let Err(e) = state.caderno.save(&user, &caderno) {
        return map_store_err(e);
    }
    Json(serde_json::json!({ "chave": key, "verse": body.verse })).into_response()
}

/// `POST /api/v1/comunicacao/caderno/migrar` — recebe o blob do `localStorage`
/// (o Caderno local-first de antes da YG-112) e o **funde** no servidor
/// (idempotente, sem perda — [`Caderno::merge_from`]). É o caminho do primeiro
/// login: o cliente envia `{ fav, notes, progress }` e recebe o Caderno
/// consolidado. As notas migradas **não** re-federam em massa aqui (evita
/// duplicar o backfill); a federação acende nas escritas seguintes via
/// `put_nota_caderno`.
pub async fn migrar_caderno(
    State(state): ApiState,
    headers: HeaderMap,
    Json(local): Json<Caderno>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut server = match state.caderno.load(&user) {
        Ok(c) => c,
        Err(e) => return map_store_err(e),
    };
    server.merge_from(local);
    if let Err(e) = state.caderno.save(&user, &server) {
        return map_store_err(e);
    }
    Json(server).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::Request,
        routing::{get, post},
    };
    use tempfile::TempDir;
    use tower::ServiceExt;

    use crate::auth::sign_jwt;

    const SECRET: &str = "test-secret";
    const ADMIN: &str = "admin-secret-token";

    fn app() -> (Router, TempDir, TempDir) {
        let (router, rd, ld, _terms, _rooms) = app_full();
        (router, rd, ld)
    }

    /// Como [`app`], mas liga os canais do producer da bus (YG-103) e devolve os
    /// `receiver`s para assertar os eventos emitidos (`entry.*` de termos +
    /// `yggdrasil.sala.*` de atividade).
    #[allow(clippy::type_complexity)]
    fn app_full() -> (
        Router,
        TempDir,
        TempDir,
        broadcast::Receiver<NoteWritten>,
        broadcast::Receiver<ObservabilityEvent>,
    ) {
        let rooms_dir = tempfile::tempdir().unwrap();
        let lex_dir = tempfile::tempdir().unwrap();
        // semeia o léxico compartilhado: yoruba/terms/iemanja.md já existe
        let shared = lex_dir.path().join("yoruba/terms");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("iemanja.md"), "---\nword: iemanjá\n---\n").unwrap();

        let store = Arc::new(RoomStore::new(rooms_dir.path()).unwrap());
        let lexicon = Arc::new(LexiconStore::new(lex_dir.path()));
        // write-back desligado nos testes de rota (não há repo git no tempdir);
        // o motor é exercitado em `writeback.rs`. Aqui só garante no-op.
        let mut wb_cfg = yggdrasil_core::comunicacao::WritebackConfig::for_testing(lex_dir.path());
        wb_cfg.enabled = false;
        let writeback = Arc::new(Writeback::new(wb_cfg));
        let (term_tx, term_rx) = broadcast::channel(64);
        let (room_tx, room_rx) = broadcast::channel(64);
        // YG-112: Caderno per-user + store de instâncias (notas do Ayvu) sob a
        // mesma raiz de salas, num tempdir isolado por teste.
        let caderno = Arc::new(CadernoStore::new(rooms_dir.path()).unwrap());
        let instance_store = Arc::new(InstanceStore::new(rooms_dir.path().join("_inst")).unwrap());
        let state = Arc::new(
            ComunicacaoState {
                jwt_secret: SECRET.into(),
                store,
                lexicon,
                writeback,
                term_events: None,
                room_events: None,
                admin_token: Some(ADMIN.into()),
                caderno,
                instance_store,
            }
            .with_bridge(term_tx, room_tx),
        );
        let router = Router::new()
            .route(
                "/api/v1/comunicacao/salas",
                post(create_sala).get(list_salas),
            )
            .route(
                "/api/v1/comunicacao/salas/{id}",
                get(get_sala).patch(patch_sala).delete(delete_sala),
            )
            .route(
                "/api/v1/comunicacao/salas/{id}/elementos/{eid}/publicar",
                post(publicar_elemento),
            )
            .route("/api/v1/comunicacao/salas/{id}/fork", post(fork_sala))
            .route("/api/v1/comunicacao/lexico", get(consultar_lexico))
            .route("/api/v1/comunicacao/lexico/lista", get(lexico_lista))
            .route("/api/v1/comunicacao/lexico/promover", post(promover_termo))
            .route("/api/v1/comunicacao/templates", get(list_templates))
            .route("/api/v1/comunicacao/revisao", get(get_revisao))
            .route("/api/v1/comunicacao/revisao/nota", post(nota_revisao))
            .route("/api/v1/comunicacao/caderno", get(get_caderno))
            .route(
                "/api/v1/comunicacao/caderno/favoritos/{key}",
                axum::routing::put(add_favorito).delete(remove_favorito),
            )
            .route(
                "/api/v1/comunicacao/caderno/notas/{key}",
                axum::routing::put(put_nota_caderno).delete(delete_nota_caderno),
            )
            .route(
                "/api/v1/comunicacao/caderno/progresso/{key}",
                axum::routing::put(set_progresso),
            )
            .route("/api/v1/comunicacao/caderno/migrar", post(migrar_caderno))
            .with_state(state);
        (router, rooms_dir, lex_dir, term_rx, room_rx)
    }

    fn token(user: &str) -> String {
        sign_jwt(user, "u@test.com", SECRET).unwrap()
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn create_yoruba(app: &Router, user: &str) -> String {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/salas?template=yoruba")
                    .header("authorization", format!("Bearer {}", token(user)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = body_json(resp).await;
        v["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn create_sem_jwt_401() {
        let (app, _r, _l) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/salas")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_yoruba_tem_elementos_semeados() {
        let (app, _r, _l) = app();
        let id = create_yoruba(&app, "alice").await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/comunicacao/salas/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["lang"], "yo");
        assert_eq!(v["template"], "yoruba");
        assert!(v["elements"].as_array().unwrap().len() >= 10);
    }

    #[tokio::test]
    async fn patch_add_element_persiste() {
        let (app, _r, _l) = app();
        let id = create_yoruba(&app, "alice").await;
        let op = serde_json::json!({
            "op": "add_element",
            "element": { "id": "novo", "word": "àbúrò", "lang": "yo", "x": 50.0, "y": 50.0, "gloss": "irmão mais novo" }
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/comunicacao/salas/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(op.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert!(
            v["elements"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == "novo")
        );
    }

    #[tokio::test]
    async fn publicar_palavra_nova_cria_lexico_e_enfileira_revisao() {
        let (app, _r, _l) = app();
        let id = create_yoruba(&app, "alice").await;
        // àṣẹ não está no léxico compartilhado (só iemanja) → cria entrada de usuário
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/v1/comunicacao/salas/{id}/elementos/ase/publicar"
                    ))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["criado"], true);
        assert_eq!(v["escopo"], "usuario");
        assert_eq!(v["caminho"], "yoruba/terms/_users/alice/ase.md");
        assert_eq!(v["revisao_adicionada"], true);
        assert_eq!(v["elemento"]["lexicon"]["state"], "contributed");

        // a fila de revisão agora tem o termo
        let rev = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/comunicacao/revisao")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let rv = body_json(rev).await;
        assert_eq!(rv["total"], 1);
        assert_eq!(rv["vencidos"], 1);
    }

    // ── YG-101: curadoria (promover stub → curado) ────────────────────────

    async fn publicar_ase(app: &Router, user: &str, id: &str) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/v1/comunicacao/salas/{id}/elementos/ase/publicar"
                    ))
                    .header("authorization", format!("Bearer {}", token(user)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    fn promover_req(admin: Option<&str>, lang: &str, user: &str, slug: &str) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri("/api/v1/comunicacao/lexico/promover")
            .header("content-type", "application/json");
        if let Some(a) = admin {
            b = b.header("authorization", format!("Bearer {a}"));
        }
        b.body(Body::from(
            serde_json::json!({ "lang": lang, "user": user, "slug": slug }).to_string(),
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn promover_curador_move_stub_para_curado_e_repoe_fila() {
        let (app, rooms_dir, lex_dir, _t, _r) = app_full();
        let id = create_yoruba(&app, "alice").await;
        publicar_ase(&app, "alice", &id).await;
        assert!(
            lex_dir
                .path()
                .join("yoruba/terms/_users/alice/ase.md")
                .is_file(),
            "stub criado"
        );

        let resp = app
            .clone()
            .oneshot(promover_req(Some(ADMIN), "yo", "alice", "ase"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["promovido"], true);
        assert_eq!(v["para"], "yoruba/terms/ase.md");
        assert_eq!(v["de"], "yoruba/terms/_users/alice/ase.md");

        // disco: curado existe (reviewed), stub arquivado
        let curado = std::fs::read_to_string(lex_dir.path().join("yoruba/terms/ase.md")).unwrap();
        assert!(curado.contains("seed_status: reviewed"));
        assert!(
            !lex_dir
                .path()
                .join("yoruba/terms/_users/alice/ase.md")
                .is_file()
        );

        // fila de revisão da alice: o item aponta p/ o curado, não o stub
        let rstore = RoomStore::new(rooms_dir.path()).unwrap();
        let queue = rstore.load_review("alice").unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].term_path, "yoruba/terms/ase.md");
    }

    #[tokio::test]
    async fn promover_sem_curador_403() {
        let (app, _r, lex_dir, _t, _ro) = app_full();
        let id = create_yoruba(&app, "alice").await;
        publicar_ase(&app, "alice", &id).await;
        // token de usuário comum (não é o admin) → 403
        let resp = app
            .clone()
            .oneshot(promover_req(Some(&token("alice")), "yo", "alice", "ase"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        // nada foi promovido: stub permanece, curado não existe
        assert!(
            lex_dir
                .path()
                .join("yoruba/terms/_users/alice/ase.md")
                .is_file()
        );
        assert!(!lex_dir.path().join("yoruba/terms/ase.md").is_file());
    }

    #[tokio::test]
    async fn promover_termo_inexistente_404() {
        let (app, _r, _l, _t, _ro) = app_full();
        let resp = app
            .clone()
            .oneshot(promover_req(Some(ADMIN), "yo", "alice", "inexistente"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn publicar_palavra_existente_liga_sem_duplicar() {
        let (app, _r, _l) = app();
        let id = create_yoruba(&app, "alice").await;
        // adiciona elemento "iemanjá" que JÁ existe no compartilhado
        let op = serde_json::json!({
            "op": "add_element",
            "element": { "id": "iem", "word": "iemanjá", "lang": "yo", "x": 0.0, "y": 0.0 }
        });
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/comunicacao/salas/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(op.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/v1/comunicacao/salas/{id}/elementos/iem/publicar"
                    ))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["criado"], false);
        assert_eq!(v["escopo"], "compartilhado");
        assert_eq!(v["caminho"], "yoruba/terms/iemanja.md");
        assert_eq!(v["elemento"]["lexicon"]["state"], "linked");
    }

    #[tokio::test]
    async fn nota_revisao_reagenda() {
        let (app, _r, _l) = app();
        let id = create_yoruba(&app, "alice").await;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/v1/comunicacao/salas/{id}/elementos/ase/publicar"
                    ))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let nota =
            serde_json::json!({ "term_path": "yoruba/terms/_users/alice/ase.md", "correct": true });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/revisao/nota")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(nota.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["interval_days"], 1);
        assert_eq!(v["reps"], 1);
    }

    #[tokio::test]
    async fn outro_usuario_nao_acessa_sala_403() {
        let (app, _r, _l) = app();
        let id = create_yoruba(&app, "alice").await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/comunicacao/salas/{id}"))
                    .header("authorization", format!("Bearer {}", token("bob")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn fork_de_sala_publica_cria_copia_do_usuario() {
        let (app, _r, _l) = app();
        // alice cria e publica uma sala
        let id = create_yoruba(&app, "alice").await;
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/comunicacao/salas/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "op": "set_published", "published": true }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        // bob forka → cópia own de bob, id novo, não publicada
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/comunicacao/salas/{id}/fork"))
                    .header("authorization", format!("Bearer {}", token("bob")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = body_json(resp).await;
        assert_eq!(v["owner"], "bob");
        assert_eq!(v["published"], false);
        assert_ne!(v["id"].as_str().unwrap(), id);
        assert!(v["elements"].as_array().unwrap().len() >= 10);
    }

    /// YG-103: publicar uma sala federa seus termos como `entry.created`
    /// `universe_key=comunicacao` **e** emite `yggdrasil.sala.published`.
    #[tokio::test]
    async fn publicar_sala_federa_termos_e_atividade() {
        let (app, _r, _l, mut term_rx, mut room_rx) = app_full();
        let id = create_yoruba(&app, "alice").await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/comunicacao/salas/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "op": "set_published", "published": true }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // atividade de sala: yggdrasil.sala.published
        let act = room_rx.try_recv().expect("evento de atividade emitido");
        assert_eq!(act.event_type, "yggdrasil.sala.published");
        assert_eq!(act.room_id, id);
        assert_eq!(act.actor, "alice");
        // observability carrega o código da sala (`yo`); o path canônico
        // (`yoruba/terms/...`) é resolvido só no evento de conteúdo.
        assert_eq!(act.lang, "yo");

        // termos federados como entry.created universe_key=comunicacao
        let mut n = 0;
        while let Ok(ev) = term_rx.try_recv() {
            assert_eq!(ev.universe_key(), "comunicacao");
            assert_eq!(ev.kind, NoteKind::Created);
            assert!(ev.path().starts_with("yoruba/terms/"));
            n += 1;
        }
        assert!(
            n >= 10,
            "todos os termos semeados da sala yoruba federados (got {n})"
        );
    }

    /// YG-118: despublicar emite `yggdrasil.sala.published{published:false}` (wire
    /// aposentado) e **não** re-federa termos.
    #[tokio::test]
    async fn despublicar_sala_emite_unpublished_sem_termos() {
        let (app, _r, _l, mut term_rx, mut room_rx) = app_full();
        let id = create_yoruba(&app, "alice").await;
        let publish = |published: bool| {
            let app = app.clone();
            let id = id.clone();
            async move {
                app.oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(format!("/api/v1/comunicacao/salas/{id}"))
                        .header("authorization", format!("Bearer {}", token("alice")))
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({ "op": "set_published", "published": published })
                                .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
            }
        };
        publish(true).await; // published
        // drena os eventos da publicação
        while term_rx.try_recv().is_ok() {}
        while room_rx.try_recv().is_ok() {}

        publish(false).await; // unpublished
        let act = room_rx.try_recv().expect("evento de despublicação");
        // YG-118: Unpublished é aposentado do wire — mapeia p/ published{published:false}
        assert_eq!(act.event_type, "yggdrasil.sala.published");
        assert_eq!(
            act.published,
            Some(false),
            "payload deve ter published=false"
        );
        assert!(term_rx.try_recv().is_err(), "despublicar não federa termos");
    }

    #[tokio::test]
    async fn fork_sem_jwt_401() {
        let (app, _r, _l) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/comunicacao/salas/qualquer/fork")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn templates_lista_tres() {
        let (app, _r, _l) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/comunicacao/templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    // ── YG-112: Caderno (favoritos / notas / progresso / migração) ─────────

    fn caderno_req(
        method: &str,
        uri: &str,
        user: &str,
        body: Option<serde_json::Value>,
    ) -> Request<Body> {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {}", token(user)));
        let body = match body {
            Some(v) => {
                b = b.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        b.body(body).unwrap()
    }

    #[tokio::test]
    async fn caderno_sem_jwt_401() {
        let (app, _r, _l) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/comunicacao/caderno")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn favoritar_e_recuperar_em_outro_request() {
        let (app, _r, _l) = app();
        let put = app
            .clone()
            .oneshot(caderno_req(
                "PUT",
                "/api/v1/comunicacao/caderno/favoritos/1-2",
                "alice",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);
        assert_eq!(body_json(put).await["novo"], true);

        // recupera o Caderno: favorito persistido
        let get = app
            .clone()
            .oneshot(caderno_req(
                "GET",
                "/api/v1/comunicacao/caderno",
                "alice",
                None,
            ))
            .await
            .unwrap();
        let v = body_json(get).await;
        assert!(v["fav"].get("1-2").is_some(), "favorito persistido");

        // outro usuário não vê o favorito da alice (isolamento per-user)
        let bob = app
            .clone()
            .oneshot(caderno_req(
                "GET",
                "/api/v1/comunicacao/caderno",
                "bob",
                None,
            ))
            .await
            .unwrap();
        assert!(body_json(bob).await["fav"].as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn desfavoritar_remove() {
        let (app, _r, _l) = app();
        app.clone()
            .oneshot(caderno_req(
                "PUT",
                "/api/v1/comunicacao/caderno/favoritos/1-2",
                "alice",
                None,
            ))
            .await
            .unwrap();
        let del = app
            .clone()
            .oneshot(caderno_req(
                "DELETE",
                "/api/v1/comunicacao/caderno/favoritos/1-2",
                "alice",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(body_json(del).await["removido"], true);
        let get = app
            .clone()
            .oneshot(caderno_req(
                "GET",
                "/api/v1/comunicacao/caderno",
                "alice",
                None,
            ))
            .await
            .unwrap();
        assert!(body_json(get).await["fav"].as_object().unwrap().is_empty());
    }

    /// YG-112/114: gravar uma nota do Caderno persiste no per-user **e** federa
    /// via `NoteStore` da instância do Ayvu — o `NoteWritten` emitido bate o
    /// contrato CO-389 (`instances/ayvu-rapyta/notes/<slug>.md`).
    #[tokio::test]
    async fn nota_caderno_persiste_e_federa_pelo_ayvu() {
        let (app, _r, _l, mut term_rx, _room_rx) = app_full();
        let resp = app
            .clone()
            .oneshot(caderno_req(
                "PUT",
                "/api/v1/comunicacao/caderno/notas/1-2",
                "alice",
                Some(serde_json::json!({ "title": "Verso 1.2", "markdown": "minha leitura" })),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["slug"], "1-2");
        assert_eq!(v["federado"], true);

        // evento federado: fonte Notes, instância do Ayvu, path contratado
        let ev = term_rx.try_recv().expect("nota federada");
        assert_eq!(ev.kind, NoteKind::Created);
        assert_eq!(ev.instance, AYVU_INSTANCE);
        assert_eq!(ev.universe_key(), "yggdrasil");
        assert_eq!(ev.path(), "instances/ayvu-rapyta/notes/1-2.md");

        // recupera a nota do Caderno per-user
        let get = app
            .clone()
            .oneshot(caderno_req(
                "GET",
                "/api/v1/comunicacao/caderno",
                "alice",
                None,
            ))
            .await
            .unwrap();
        let c = body_json(get).await;
        assert_eq!(c["notes"]["1-2"]["markdown"], "minha leitura");
    }

    #[tokio::test]
    async fn nota_caderno_segunda_escrita_e_updated() {
        let (app, _r, _l, mut term_rx, _room_rx) = app_full();
        for _ in 0..1 {
            app.clone()
                .oneshot(caderno_req(
                    "PUT",
                    "/api/v1/comunicacao/caderno/notas/1-2",
                    "alice",
                    Some(serde_json::json!({ "markdown": "v1" })),
                ))
                .await
                .unwrap();
        }
        assert_eq!(term_rx.try_recv().unwrap().kind, NoteKind::Created);
        app.clone()
            .oneshot(caderno_req(
                "PUT",
                "/api/v1/comunicacao/caderno/notas/1-2",
                "alice",
                Some(serde_json::json!({ "markdown": "v2" })),
            ))
            .await
            .unwrap();
        assert_eq!(term_rx.try_recv().unwrap().kind, NoteKind::Updated);
    }

    #[tokio::test]
    async fn progresso_persiste() {
        let (app, _r, _l) = app();
        let put = app
            .clone()
            .oneshot(caderno_req(
                "PUT",
                "/api/v1/comunicacao/caderno/progresso/cap-1",
                "alice",
                Some(serde_json::json!({ "verse": "1-7" })),
            ))
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);
        let get = app
            .clone()
            .oneshot(caderno_req(
                "GET",
                "/api/v1/comunicacao/caderno",
                "alice",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(body_json(get).await["progress"]["cap-1"], "1-7");
    }

    /// YG-112: migração do `localStorage` é idempotente e sem perda — funde o
    /// blob local no Caderno do servidor, e re-enviar o mesmo blob não muda nada.
    #[tokio::test]
    async fn migrar_localstorage_idempotente_sem_perda() {
        let (app, _r, _l) = app();
        // pré-existe um favorito no servidor
        app.clone()
            .oneshot(caderno_req(
                "PUT",
                "/api/v1/comunicacao/caderno/favoritos/srv-fav",
                "alice",
                None,
            ))
            .await
            .unwrap();

        let blob = serde_json::json!({
            "fav": { "local-fav": "2026-06-01T00:00:00Z" },
            "notes": {
                "1-2": {
                    "key": "1-2",
                    "title": "Local",
                    "markdown": "nota local",
                    "updated_at": "2026-06-07T00:00:00Z"
                }
            },
            "progress": { "cap-1": "1-9" }
        });
        let m1 = app
            .clone()
            .oneshot(caderno_req(
                "POST",
                "/api/v1/comunicacao/caderno/migrar",
                "alice",
                Some(blob.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(m1.status(), StatusCode::OK);
        let c1 = body_json(m1).await;
        // sem perda: o favorito do servidor E o do local coexistem
        assert!(c1["fav"].get("srv-fav").is_some());
        assert!(c1["fav"].get("local-fav").is_some());
        assert_eq!(c1["notes"]["1-2"]["markdown"], "nota local");
        assert_eq!(c1["progress"]["cap-1"], "1-9");

        // idempotente: re-migrar o mesmo blob dá o mesmo Caderno
        let m2 = app
            .clone()
            .oneshot(caderno_req(
                "POST",
                "/api/v1/comunicacao/caderno/migrar",
                "alice",
                Some(blob),
            ))
            .await
            .unwrap();
        assert_eq!(body_json(m2).await, c1);
    }
}
