//! REST API de instâncias de universo autoradas (YG-76/77/79).
//!
//! Conceito host-neutral: o contrato JSON + endpoints abaixo é a fronteira
//! estável que web e Godot consomem. Auth via `sub` do JWT (mesmo padrão de
//! [`crate::api::me`]). Persistência via [`InstanceStore`] (disco + blobs).

use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use nanoid::nanoid;
use serde::Deserialize;
use yggdrasil_core::instance::{
    AttachmentKind, ContentRef, DraftStore, EditError, EditOp, InstanceStore, NoteError, NoteStore,
    StoreError, UniverseInstance, note, template_by_slug, template_summaries,
};

use crate::auth::verify_jwt;
use crate::co_bridge_producer::{NoteKind, NoteWritten};

/// Estado compartilhado das rotas de instâncias.
pub struct InstancesState {
    pub jwt_secret: String,
    pub store: Arc<InstanceStore>,
    /// Tamanho máximo de anexo em bytes.
    pub max_attachment_bytes: usize,
    /// Canal de eventos de nota para o producer do CO (YG-93). `None` quando o
    /// producer está desligado (gate de env não configurado) — emitir é no-op.
    pub note_events: Option<tokio::sync::broadcast::Sender<NoteWritten>>,
}

impl InstancesState {
    pub fn new(jwt_secret: String, store: Arc<InstanceStore>) -> Self {
        let max_attachment_bytes = std::env::var("YGGDRASIL_MAX_ATTACHMENT_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(25 * 1024 * 1024);
        Self {
            jwt_secret,
            store,
            max_attachment_bytes,
            note_events: None,
        }
    }

    /// Liga o canal de eventos de nota do producer do CO (YG-93).
    pub fn with_note_events(mut self, sender: tokio::sync::broadcast::Sender<NoteWritten>) -> Self {
        self.note_events = Some(sender);
        self
    }

    /// Publica um [`NoteWritten`] no canal do producer, se ligado. Falha de
    /// envio (sem assinantes) é benigna e ignorada — não afeta a resposta HTTP.
    fn emit_note(&self, event: NoteWritten) {
        if let Some(tx) = &self.note_events {
            let _ = tx.send(event);
        }
    }
}

type ApiState = State<Arc<InstancesState>>;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

fn unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "erro": "nao_autenticado" })),
    )
        .into_response()
}

fn err_json(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({ "erro": msg }))).into_response()
}

/// Extrai e valida o `sub` do JWT, ou devolve 401.
#[allow(clippy::result_large_err)] // o `Err` é uma `Response` axum por design
fn require_user(
    state: &InstancesState,
    headers: &HeaderMap,
) -> Result<String, axum::response::Response> {
    let token = extract_bearer(headers).ok_or_else(unauthorized)?;
    verify_jwt(&token, &state.jwt_secret).map_err(|_| unauthorized())
}

/// Carrega a instância exigindo que o caller seja o dono.
#[allow(clippy::result_large_err)] // o `Err` é uma `Response` axum por design
fn load_owned(
    state: &InstancesState,
    id: &str,
    user: &str,
) -> Result<UniverseInstance, axum::response::Response> {
    let inst = state.store.load(id).map_err(map_store_err)?;
    if inst.owner != user {
        return Err(err_json(StatusCode::FORBIDDEN, "nao_e_dono"));
    }
    Ok(inst)
}

fn map_store_err(e: StoreError) -> axum::response::Response {
    match e {
        StoreError::NotFound(_) => err_json(StatusCode::NOT_FOUND, "instancia_nao_encontrada"),
        StoreError::BlobNotFound(_) => err_json(StatusCode::NOT_FOUND, "anexo_nao_encontrado"),
        StoreError::Serde(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "instancia_corrompida"),
        StoreError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "erro_io"),
    }
}

fn map_edit_err(e: EditError) -> axum::response::Response {
    let status = match e {
        EditError::BlockExists(_) | EditError::LayerExists(_) => StatusCode::CONFLICT,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    (status, Json(serde_json::json!({ "erro": e.to_string() }))).into_response()
}

// ─── CRUD ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateQuery {
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// `POST /api/v1/instances` — cria (opcionalmente a partir de `?template=`).
pub async fn create_instance(
    State(state): ApiState,
    headers: HeaderMap,
    Query(q): Query<CreateQuery>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let id = nanoid!();
    let mut inst = match q.template.as_deref() {
        Some(slug) => match template_by_slug(slug) {
            Some(t) => {
                let mut inst = t.instantiate(&id, &user);
                if let Some(title) = q.title {
                    inst.title = title;
                }
                inst
            }
            None => return err_json(StatusCode::NOT_FOUND, "template_nao_encontrado"),
        },
        None => UniverseInstance::empty(
            &id,
            &user,
            q.title.unwrap_or_else(|| "Novo universo".into()),
        ),
    };
    // Semeia assets de fundo embutidos para templates que os têm (ex.
    // neuroanatomia: silhueta + SNC) — assim o toggle de transparência já tem
    // o que revelar/esconder antes de qualquer upload do usuário.
    seed_template_assets(&state.store, &mut inst);
    if let Err(e) = state.store.save(&inst) {
        return map_store_err(e);
    }
    (StatusCode::CREATED, Json(inst)).into_response()
}

// Assets de fundo embutidos por template (placeholders originais, CC0).
const NEURO_CORPO_SVG: &[u8] =
    include_bytes!("../../static/universos/assets/neuroanatomia/corpo.svg");
const NEURO_SNC_SVG: &[u8] = include_bytes!("../../static/universos/assets/neuroanatomia/snc.svg");

/// Grava os assets de fundo embutidos do template no blob store e liga-os às
/// camadas correspondentes. No-op para templates sem assets.
fn seed_template_assets(store: &InstanceStore, inst: &mut UniverseInstance) {
    let assets: &[(&str, &str, &[u8])] = match inst.template.as_str() {
        "neuroanatomia" => &[
            ("corpo", "corpo.svg", NEURO_CORPO_SVG),
            ("snc", "snc.svg", NEURO_SNC_SVG),
        ],
        _ => return,
    };
    for (layer_id, filename, bytes) in assets {
        let Ok((hash, size)) = store.write_blob(bytes) else {
            continue;
        };
        if let Some(layer) = inst.layer_mut(layer_id) {
            layer.background = Some(ContentRef {
                kind: AttachmentKind::Image,
                hash,
                filename: (*filename).to_string(),
                mime: "image/svg+xml".to_string(),
                size,
            });
        }
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub published: bool,
}

/// `GET /api/v1/instances` — lista do caller, ou feed público (`?published=true`).
pub async fn list_instances(
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

/// `GET /api/v1/instances/{id}` — público se `published`, senão owner-only.
pub async fn get_instance(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let inst = match state.store.load(&id) {
        Ok(i) => i,
        Err(e) => return map_store_err(e),
    };
    if !inst.published {
        match require_user(&state, &headers) {
            Ok(u) if u == inst.owner => {}
            Ok(_) => return err_json(StatusCode::FORBIDDEN, "nao_e_dono"),
            Err(r) => return r,
        }
    }
    Json(inst).into_response()
}

/// Resumo leve de um universo alcançável por portal (cross-universe, YG-157).
/// Só metadados (id/título/dono/publicado) — o **mundo** do destino é carregado
/// lazy quando o usuário atravessa (`GET /api/v1/instances/{id}` + notas).
#[derive(serde::Serialize)]
pub struct PortalSummary {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub published: bool,
}

/// `GET /api/v1/instances/{id}/portals` — universos para os quais o usuário pode
/// **atravessar** a partir de `{id}` (YG-157, "universe-as-node" / CO-400).
///
/// Reusa a visibilidade do feed: um destino entra se for **público** ou **do
/// próprio caller**. Sempre exclui a própria origem `{id}`. Devolve só metadados
/// (lazy: o mundo do destino só é buscado ao cruzar o portal). A origem precisa
/// ser visível ao caller — mesmo gate de [`get_instance`].
pub async fn list_portals(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let src = match state.store.load(&id) {
        Ok(i) => i,
        Err(e) => return map_store_err(e),
    };
    let caller = extract_bearer(&headers).and_then(|t| verify_jwt(&t, &state.jwt_secret).ok());
    if !src.published {
        match &caller {
            Some(u) if *u == src.owner => {}
            Some(_) => return err_json(StatusCode::FORBIDDEN, "nao_e_dono"),
            None => return unauthorized(),
        }
    }
    // candidatos: feed público + (se autenticado) os do próprio caller.
    let mut all = match state.store.list_published() {
        Ok(l) => l,
        Err(e) => return map_store_err(e),
    };
    if let Some(u) = &caller {
        match state.store.list_owner(u) {
            Ok(owned) => all.extend(owned),
            Err(e) => return map_store_err(e),
        }
    }
    // dedupe por id, exclui a origem; preserva a ordem (updated_at desc das listas).
    let mut seen = std::collections::HashSet::new();
    let portals: Vec<PortalSummary> = all
        .into_iter()
        .filter(|i| i.id != id && seen.insert(i.id.clone()))
        .map(|i| PortalSummary {
            id: i.id,
            title: i.title,
            owner: i.owner,
            published: i.published,
        })
        .collect();
    Json(serde_json::json!({ "portals": portals })).into_response()
}

/// `PUT /api/v1/instances/{id}` — salva a instância inteira (owner-only).
pub async fn put_instance(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut body): Json<UniverseInstance>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    // garante existência + posse antes de sobrescrever
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    // id e owner são imutáveis pelo corpo
    body.id = id;
    body.owner = user;
    body.updated_at = chrono::Utc::now();
    if let Err(e) = state.store.save(&body) {
        return map_store_err(e);
    }
    Json(body).into_response()
}

/// `PATCH /api/v1/instances/{id}` — aplica um [`EditOp`] (owner-only).
pub async fn patch_instance(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(op): Json<EditOp>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    let mut inst = match load_owned(&state, &id, &user) {
        Ok(i) => i,
        Err(r) => return r,
    };
    if let Err(e) = op.apply(&mut inst) {
        return map_edit_err(e);
    }
    if let Err(e) = state.store.save(&inst) {
        return map_store_err(e);
    }
    Json(inst).into_response()
}

/// `DELETE /api/v1/instances/{id}` — apaga (owner-only).
pub async fn delete_instance(
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

// ─── Anexos ─────────────────────────────────────────────────────────────────

/// `POST /api/v1/instances/{id}/attachments` — upload multipart → [`ContentRef`].
pub async fn upload_attachment(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }

    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return err_json(StatusCode::BAD_REQUEST, "arquivo_ausente"),
        Err(_) => return err_json(StatusCode::BAD_REQUEST, "multipart_invalido"),
    };

    let filename = field.file_name().unwrap_or("anexo").to_string();
    let mime = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_string();

    let kind = match AttachmentKind::from_mime(&mime) {
        Some(k) => k,
        None => {
            return err_json(StatusCode::UNSUPPORTED_MEDIA_TYPE, "tipo_nao_suportado");
        }
    };

    let bytes = match field.bytes().await {
        Ok(b) => b,
        Err(_) => return err_json(StatusCode::BAD_REQUEST, "leitura_falhou"),
    };
    if bytes.len() > state.max_attachment_bytes {
        return err_json(StatusCode::PAYLOAD_TOO_LARGE, "arquivo_grande_demais");
    }

    let (hash, size) = match state.store.write_blob(&bytes) {
        Ok(r) => r,
        Err(e) => return map_store_err(e),
    };

    let content = ContentRef {
        kind,
        hash,
        filename,
        mime,
        size,
    };
    (StatusCode::CREATED, Json(content)).into_response()
}

/// `GET /api/v1/instances/{id}/attachments/{hash}` — serve bytes content-addressed.
pub async fn serve_attachment(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, hash)): Path<(String, String)>,
) -> impl IntoResponse {
    let inst = match state.store.load(&id) {
        Ok(i) => i,
        Err(e) => return map_store_err(e),
    };
    if !inst.published {
        match require_user(&state, &headers) {
            Ok(u) if u == inst.owner => {}
            Ok(_) => return err_json(StatusCode::FORBIDDEN, "nao_e_dono"),
            Err(r) => return r,
        }
    }

    // descobre o mime/filename do ContentRef referenciado (se houver)
    let cref = inst
        .layers
        .iter()
        .flat_map(|l| {
            l.background
                .iter()
                .chain(l.blocks.iter().flat_map(|b| b.attachments.iter()))
        })
        .find(|c| c.hash == hash);

    let bytes = match state.store.read_blob(&hash) {
        Ok(b) => b,
        Err(e) => return map_store_err(e),
    };

    let mime = cref
        .map(|c| c.mime.clone())
        .unwrap_or_else(|| "application/octet-stream".into());
    let filename = cref
        .map(|c| c.filename.clone())
        .unwrap_or_else(|| hash.clone());

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{filename}\""),
            ),
            (
                header::CACHE_CONTROL,
                "public, max-age=31536000, immutable".into(),
            ),
        ],
        bytes,
    )
        .into_response()
}

// ─── Templates ──────────────────────────────────────────────────────────────

/// `GET /api/v1/templates` — lista resumida dos templates.
pub async fn list_templates() -> impl IntoResponse {
    Json(template_summaries())
}

/// `GET /api/v1/templates/{slug}` — detalhe (seed + palette + render_hints).
pub async fn get_template(Path(slug): Path<String>) -> impl IntoResponse {
    match template_by_slug(&slug) {
        Some(t) => Json(t).into_response(),
        None => err_json(StatusCode::NOT_FOUND, "template_nao_encontrado"),
    }
}

// ─── Notas (jardim — Markdown canônico ligado por wikilinks) ──────────────────

fn map_note_err(e: NoteError) -> axum::response::Response {
    match e {
        NoteError::NotFound(_) => err_json(StatusCode::NOT_FOUND, "nota_nao_encontrada"),
        NoteError::InvalidSlug => err_json(StatusCode::UNPROCESSABLE_ENTITY, "slug_invalido"),
        NoteError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "erro_io"),
    }
}

fn note_store(state: &InstancesState, id: &str) -> NoteStore {
    NoteStore::for_instance(state.store.root(), id)
}

/// Frontmatter JSON federado de uma nota (YG-149): status (tarefa) + layout do
/// Mundo (`parent`, `pos{room,x,y}`). É o patch que o CO recebe no write-back.
/// `None` quando a nota não tem nenhum desses campos (nota pura sem layout).
fn note_frontmatter(n: &note::Note) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    if let Some(st) = &n.status {
        map.insert("status".into(), serde_json::Value::String(st.clone()));
    }
    if let Some(p) = &n.parent {
        map.insert("parent".into(), serde_json::Value::String(p.clone()));
    }
    if let Some(pos) = &n.pos {
        map.insert(
            "pos".into(),
            serde_json::json!({ "room": pos.room, "x": pos.x, "y": pos.y }),
        );
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

// ─── Rascunhos (YG-125): branch cross-device do editor; NUNCA federam ────────

fn draft_store(state: &InstancesState, id: &str, user: &str) -> DraftStore {
    DraftStore::for_user(state.store.root(), id, user)
}

fn map_draft_err(e: NoteError) -> axum::response::Response {
    match e {
        NoteError::NotFound(_) => err_json(StatusCode::NOT_FOUND, "rascunho_nao_encontrado"),
        NoteError::InvalidSlug => err_json(StatusCode::UNPROCESSABLE_ENTITY, "slug_invalido"),
        NoteError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "erro_io"),
    }
}

#[derive(Deserialize)]
pub struct DraftBody {
    #[serde(default)]
    pub markdown: String,
}

/// `GET /api/v1/instances/{id}/notes/{slug}/draft` — rascunho do dono (404 se não há).
pub async fn get_draft(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    match draft_store(&state, &id, &user).load(&slug) {
        Ok(d) => Json(d).into_response(),
        Err(e) => map_draft_err(e),
    }
}

/// `PUT /api/v1/instances/{id}/notes/{slug}/draft` — grava o rascunho. Não toca
/// a nota canônica e **não emite** nada ao producer do bridge (privacidade por
/// construção: rascunho não federa).
pub async fn put_draft(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, slug)): Path<(String, String)>,
    Json(body): Json<DraftBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    match draft_store(&state, &id, &user).save(&slug, &body.markdown) {
        Ok(d) => Json(d).into_response(),
        Err(e) => map_draft_err(e),
    }
}

/// `DELETE /api/v1/instances/{id}/notes/{slug}/draft` — descarta o rascunho.
pub async fn delete_draft(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    match draft_store(&state, &id, &user).delete(&slug) {
        Ok(()) => Json(serde_json::json!({ "slug": slug, "descartado": true })).into_response(),
        Err(e) => map_draft_err(e),
    }
}

/// Carrega a instância para **leitura**: pública se `published`, senão owner-only.
/// Espelha o gating de [`get_instance`]/[`serve_attachment`].
#[allow(clippy::result_large_err)] // o `Err` é uma `Response` axum por design
fn load_readable(
    state: &InstancesState,
    id: &str,
    headers: &HeaderMap,
) -> Result<UniverseInstance, axum::response::Response> {
    let inst = state.store.load(id).map_err(map_store_err)?;
    if !inst.published {
        match require_user(state, headers) {
            Ok(u) if u == inst.owner => {}
            Ok(_) => return Err(err_json(StatusCode::FORBIDDEN, "nao_e_dono")),
            Err(r) => return Err(r),
        }
    }
    Ok(inst)
}

#[derive(Deserialize)]
pub struct NoteBody {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub markdown: String,
    /// Tarefa por composição (YG-130): ausente = preserva o status atual;
    /// `""` limpa (volta a nota); `todo|doing|done` define.
    #[serde(default)]
    pub status: Option<String>,
}

/// `PUT /api/v1/instances/{id}/notes/{slug}` — cria/atualiza uma nota (owner-only).
/// O `slug` é normalizado server-side; a resposta traz o slug canônico.
pub async fn put_note(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, slug)): Path<(String, String)>,
    Json(body): Json<NoteBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    let title = if body.title.trim().is_empty() {
        slug.clone()
    } else {
        body.title
    };
    let store_for_notes = note_store(&state, &id);
    // created vs updated: a nota já existia em disco antes deste save?
    let existed = store_for_notes.load(&slug).is_ok();
    // YG-130: status ausente preserva (save); presente define/limpa por composição
    let saved = match &body.status {
        None => store_for_notes.save(&slug, &title, &body.markdown),
        Some(st) => store_for_notes.save_with_status(&slug, &title, &body.markdown, Some(st)),
    };
    match saved {
        Ok(n) => {
            state.emit_note(NoteWritten {
                instance: id.clone(),
                slug: n.slug.clone(),
                kind: if existed {
                    NoteKind::Updated
                } else {
                    NoteKind::Created
                },
                source: crate::co_bridge_producer::FederatedSource::Notes,
                title: n.title.clone(),
                body: n.body.clone(),
                updated_at: Some(n.updated.to_rfc3339()),
                // federa o frontmatter: status (tarefa) + layout do Mundo (YG-149)
                frontmatter: note_frontmatter(&n),
                actor: None,
                visibility: crate::co_bridge_producer::Visibility::Public,
            });
            Json(n).into_response()
        }
        Err(e) => map_note_err(e),
    }
}

/// `GET /api/v1/instances/{id}/notes/{slug}` — lê uma nota (gating de leitura).
pub async fn get_note(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(r) = load_readable(&state, &id, &headers) {
        return r;
    }
    match note_store(&state, &id).load(&slug) {
        Ok(n) => Json(n).into_response(),
        Err(e) => map_note_err(e),
    }
}

/// `GET /api/v1/instances/{id}/notes` — lista + backlinks + grafo de wikilinks.
pub async fn list_notes(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(r) = load_readable(&state, &id, &headers) {
        return r;
    }
    match note_store(&state, &id).list() {
        Ok(notes) => {
            let backlinks = note::backlinks(&notes);
            let graph = note::graph(&notes);
            Json(serde_json::json!({
                "notes": notes,
                "backlinks": backlinks,
                "graph": graph,
            }))
            .into_response()
        }
        Err(e) => map_note_err(e),
    }
}

/// `DELETE /api/v1/instances/{id}/notes/{slug}` — remove uma nota (owner-only).
pub async fn delete_note(
    State(state): ApiState,
    headers: HeaderMap,
    Path((id, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    match note_store(&state, &id).delete(&slug) {
        Ok(()) => {
            state.emit_note(NoteWritten {
                instance: id.clone(),
                slug: slug.clone(),
                kind: NoteKind::Deleted,
                source: crate::co_bridge_producer::FederatedSource::Notes,
                title: String::new(),
                body: String::new(),
                updated_at: None,
                frontmatter: None,
                actor: None,
                visibility: crate::co_bridge_producer::Visibility::Public,
            });
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => map_note_err(e),
    }
}

// ─── Layout do Mundo (YG-149): commit em lote de drag-drop ────────────────────

#[derive(Deserialize)]
pub struct LayoutBody {
    /// Movimentos do arraste, aplicados em lote (não por-pixel).
    #[serde(default)]
    pub moves: Vec<note::LayoutMove>,
}

/// `POST /api/v1/instances/{id}/layout` — persiste posições/reparent do drag-drop
/// no frontmatter `.md` (via `NoteStore`) em **lote** e federa cada patch ao CO
/// (write-back CO-413). Owner-only.
pub async fn put_layout(
    State(state): ApiState,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<LayoutBody>,
) -> impl IntoResponse {
    let user = match require_user(&state, &headers) {
        Ok(u) => u,
        Err(r) => return r,
    };
    if let Err(r) = load_owned(&state, &id, &user) {
        return r;
    }
    let updated = match note_store(&state, &id).apply_layout(&body.moves) {
        Ok(v) => v,
        Err(e) => return map_note_err(e),
    };
    // write-back ao CO: um patch (NoteWritten::Updated) por nota movida, com o
    // frontmatter de layout (pos/parent) no payload.
    for n in &updated {
        state.emit_note(NoteWritten {
            instance: id.clone(),
            slug: n.slug.clone(),
            kind: NoteKind::Updated,
            source: crate::co_bridge_producer::FederatedSource::Notes,
            title: n.title.clone(),
            body: n.body.clone(),
            updated_at: Some(n.updated.to_rfc3339()),
            frontmatter: note_frontmatter(n),
            actor: Some(user.clone()),
            visibility: crate::co_bridge_producer::Visibility::Public,
        });
    }
    Json(serde_json::json!({ "updated": updated })).into_response()
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

    fn app() -> (Router, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(InstanceStore::new(dir.path()).unwrap());
        let state = Arc::new(InstancesState {
            jwt_secret: SECRET.into(),
            store,
            max_attachment_bytes: 1024,
            note_events: None,
        });
        let router = Router::new()
            .route(
                "/api/v1/instances",
                post(create_instance).get(list_instances),
            )
            .route(
                "/api/v1/instances/{id}",
                get(get_instance)
                    .put(put_instance)
                    .patch(patch_instance)
                    .delete(delete_instance),
            )
            .route("/api/v1/instances/{id}/portals", get(list_portals))
            .route("/api/v1/instances/{id}/notes", get(list_notes))
            .route(
                "/api/v1/instances/{id}/notes/{slug}",
                get(get_note).put(put_note).delete(delete_note),
            )
            .route("/api/v1/instances/{id}/layout", post(put_layout))
            .route(
                "/api/v1/instances/{id}/notes/{slug}/draft",
                get(get_draft).put(put_draft).delete(delete_draft),
            )
            .route("/api/v1/templates", get(list_templates))
            .route("/api/v1/templates/{slug}", get(get_template))
            .with_state(state);
        (router, dir)
    }

    /// Variante de [`app`] com o canal do producer do CO ligado (YG-93): devolve
    /// também um receiver para assertar os [`NoteWritten`] emitidos.
    fn app_with_events() -> (
        Router,
        TempDir,
        tokio::sync::broadcast::Receiver<NoteWritten>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(InstanceStore::new(dir.path()).unwrap());
        let (tx, rx) = tokio::sync::broadcast::channel(16);
        let state = Arc::new(InstancesState {
            jwt_secret: SECRET.into(),
            store,
            max_attachment_bytes: 1024,
            note_events: Some(tx),
        });
        let router = Router::new()
            .route("/api/v1/instances", post(create_instance))
            .route(
                "/api/v1/instances/{id}/notes/{slug}",
                get(get_note).put(put_note).delete(delete_note),
            )
            .route("/api/v1/instances/{id}/layout", post(put_layout))
            .route(
                "/api/v1/instances/{id}/notes/{slug}/draft",
                get(get_draft).put(put_draft).delete(delete_draft),
            )
            .with_state(state);
        (router, dir, rx)
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

    #[tokio::test]
    async fn create_sem_jwt_401() {
        let (app, _d) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/instances")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_com_template_neuroanatomia() {
        let (app, _d) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/instances?template=neuroanatomia")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let v = body_json(resp).await;
        assert_eq!(v["template"], "neuroanatomia");
        assert_eq!(v["owner"], "alice");
        let layers = v["layers"].as_array().unwrap();
        assert_eq!(layers.len(), 3);
        // assets de fundo (silhueta + SNC) semeados → o toggle de transparência
        // tem o que revelar/esconder sem upload prévio.
        for id in ["corpo", "snc"] {
            let layer = layers.iter().find(|l| l["id"] == id).unwrap();
            assert!(
                layer["background"]["hash"]
                    .as_str()
                    .is_some_and(|h| !h.is_empty()),
                "camada {id} deveria ter background semeado"
            );
            assert_eq!(layer["background"]["mime"], "image/svg+xml");
        }
    }

    #[tokio::test]
    async fn create_template_invalido_404() {
        let (app, _d) = app();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/instances?template=fantasma")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    async fn create_blank(app: &Router, user: &str) -> String {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/instances")
                    .header("authorization", format!("Bearer {}", token(user)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        v["id"].as_str().unwrap().to_string()
    }

    /// Publica a instância (PUT do corpo inteiro com `published=true`, owner-only).
    async fn publish(app: &Router, id: &str, user: &str) {
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token(user)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mut inst = body_json(get).await;
        inst["published"] = serde_json::Value::Bool(true);
        let put = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token(user)))
                    .header("content-type", "application/json")
                    .body(Body::from(inst.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::OK);
    }

    async fn portals_of(app: &Router, id: &str, user: Option<&str>) -> serde_json::Value {
        let mut req = Request::builder().uri(format!("/api/v1/instances/{id}/portals"));
        if let Some(u) = user {
            req = req.header("authorization", format!("Bearer {}", token(u)));
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        body_json(resp).await
    }

    fn portal_ids(v: &serde_json::Value) -> Vec<String> {
        v["portals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_str().unwrap().to_string())
            .collect()
    }

    // YG-157: a partir de um universo, os portais são os outros universos VISÍVEIS
    // ao caller (públicos + os dele), excluindo a própria origem.
    #[tokio::test]
    async fn portals_respeita_visibilidade() {
        let (app, _d) = app();
        let a = create_blank(&app, "alice").await; // origem (de alice)
        let a_pub = create_blank(&app, "alice").await; // outro de alice, publicado
        let alice_priv = create_blank(&app, "alice").await; // outro de alice, privado
        let bob_pub = create_blank(&app, "bob").await; // publicado por bob
        let bob_priv = create_blank(&app, "bob").await; // privado de bob
        publish(&app, &a_pub, "alice").await;
        publish(&app, &bob_pub, "bob").await;

        // alice (dona da origem) vê: o público dela, o privado dela, o público do bob.
        // NÃO vê o privado do bob, nem a própria origem.
        let mut ids = portal_ids(&portals_of(&app, &a, Some("alice")).await);
        ids.sort();
        let mut esperado = vec![a_pub.clone(), alice_priv.clone(), bob_pub.clone()];
        esperado.sort();
        assert_eq!(ids, esperado);
        assert!(!ids.contains(&a), "exclui a própria origem");
        assert!(
            !ids.contains(&bob_priv),
            "privado de outro dono é invisível"
        );
    }

    // Anônimo só atravessa para universos públicos; origem privada é 401.
    #[tokio::test]
    async fn portals_anonimo_so_ve_publicos() {
        let (app, _d) = app();
        let a_pub = create_blank(&app, "alice").await;
        let outro_pub = create_blank(&app, "alice").await;
        let _priv = create_blank(&app, "alice").await;
        publish(&app, &a_pub, "alice").await;
        publish(&app, &outro_pub, "alice").await;

        let ids = portal_ids(&portals_of(&app, &a_pub, None).await);
        assert_eq!(ids, vec![outro_pub], "anônimo só vê o outro público");

        // origem privada sem token → 401.
        let priv_src = create_blank(&app, "alice").await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{priv_src}/portals"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn patch_place_block_persiste() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        let op = serde_json::json!({
            "op": "place_block",
            "layer": "base",
            "block": { "id": "n1", "block_type": "note", "pos": { "x": 2, "y": 3 } }
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(op.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["layers"][0]["blocks"][0]["id"], "n1");
    }

    #[tokio::test]
    async fn patch_connection_endpoint_invalido_422() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        let op = serde_json::json!({
            "op": "add_connection",
            "connection": { "id": "c1", "from": "nada", "to": "nada2" }
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(op.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn owner_b_nao_acessa_instancia_de_a_403() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token("bob")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_depois_get_404() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        let del = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del.status(), StatusCode::NO_CONTENT);
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/instances/{id}"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn templates_lista_e_detalhe() {
        let (app, _d) = app();
        let list = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/templates")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let detail = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/templates/neuroanatomia")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let v = body_json(detail).await;
        assert_eq!(v["slug"], "neuroanatomia");
        assert!(v["palette"].as_array().unwrap().len() >= 1);
    }

    // ─── Notas ────────────────────────────────────────────────────────────────

    async fn put_note_req(
        app: &Router,
        id: &str,
        user: &str,
        slug: &str,
        body: serde_json::Value,
    ) -> axum::response::Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/instances/{id}/notes/{slug}"))
                    .header("authorization", format!("Bearer {}", token(user)))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    /// YG-93: o hook de emissão publica exatamente 1 `NoteWritten` por
    /// save/delete, com o `kind` correto (created → updated → deleted).
    #[tokio::test]
    async fn save_delete_emite_um_note_written_por_operacao() {
        let (app, _d, mut rx) = app_with_events();
        let id = create_blank(&app, "alice").await;

        // CREATE
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "Minha-Nota",
            serde_json::json!({ "title": "Minha Nota", "markdown": "corpo [[outra]]" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let ev = rx.try_recv().expect("1 evento no create");
        assert_eq!(ev.kind, NoteKind::Created);
        assert_eq!(ev.slug, "minha-nota");
        assert_eq!(ev.instance, id);
        assert_eq!(ev.title, "Minha Nota");
        assert!(ev.body.contains("[[outra]]"));
        assert!(ev.updated_at.is_some());
        // YG-97: path instance-qualified p/ o round-trip de volta do CO.
        assert_eq!(ev.path(), format!("instances/{id}/notes/minha-nota.md"));
        assert!(rx.try_recv().is_err(), "exatamente 1 evento por save");

        // UPDATE (mesma nota já existe → updated)
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "minha-nota",
            serde_json::json!({ "title": "Minha Nota", "markdown": "corpo v2" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let ev = rx.try_recv().expect("1 evento no update");
        assert_eq!(ev.kind, NoteKind::Updated);
        assert!(rx.try_recv().is_err());

        // DELETE
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/instances/{id}/notes/minha-nota"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let ev = rx.try_recv().expect("1 evento no delete");
        assert_eq!(ev.kind, NoteKind::Deleted);
        assert_eq!(ev.slug, "minha-nota");
        assert!(rx.try_recv().is_err());
    }

    /// Sem o producer ligado (`note_events: None`, gate de env desligado) o
    /// save/delete segue funcionando — emitir é no-op (não quebra o boot).
    #[tokio::test]
    async fn save_sem_producer_ligado_nao_quebra() {
        let (app, _d) = app(); // note_events: None
        let id = create_blank(&app, "alice").await;
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "n",
            serde_json::json!({ "title": "N", "markdown": "x" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn put_get_nota_round_trip() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        // slug do path em maiúsculas → normalizado server-side
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "Minha-Nota",
            serde_json::json!({ "title": "Minha Nota", "markdown": "corpo [[outra]]" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["slug"], "minha-nota"); // slug canônico server-side
        assert_eq!(v["links"][0]["target"], "outra");

        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}/notes/minha-nota"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get.status(), StatusCode::OK);
        assert_eq!(body_json(get).await["title"], "Minha Nota");
    }

    #[tokio::test]
    async fn list_notas_com_backlinks_e_grafo() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        put_note_req(
            &app,
            &id,
            "alice",
            "a",
            serde_json::json!({ "markdown": "liga [[b]]" }),
        )
        .await;
        put_note_req(
            &app,
            &id,
            "alice",
            "b",
            serde_json::json!({ "markdown": "folha" }),
        )
        .await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}/notes"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["notes"].as_array().unwrap().len(), 2);
        assert_eq!(v["backlinks"]["b"][0]["source"], "a");
        assert_eq!(v["graph"]["a"][0], "b");
    }

    #[tokio::test]
    async fn put_nota_sem_jwt_401() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/instances/{id}/notes/x"))
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn put_nota_de_outro_dono_403() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        let resp = put_note_req(
            &app,
            &id,
            "bob",
            "x",
            serde_json::json!({ "markdown": "hack" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_nota_instancia_nao_publicada_de_outro_403() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        put_note_req(
            &app,
            &id,
            "alice",
            "n",
            serde_json::json!({ "markdown": "x" }),
        )
        .await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}/notes/n"))
                    .header("authorization", format!("Bearer {}", token("bob")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ─── Tarefa por composição (YG-130) ──────────────────────────────────────

    #[tokio::test]
    async fn nota_vira_tarefa_por_status_e_put_sem_status_preserva() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;

        // cria já como tarefa (composer `[] …` manda status=todo)
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "comprar-tinta",
            serde_json::json!({ "title": "Comprar tinta", "markdown": "", "status": "todo" }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["status"], "todo");

        // editar o corpo SEM mandar status preserva a tarefa
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "comprar-tinta",
            serde_json::json!({ "title": "Comprar tinta", "markdown": "acrílica 500ml" }),
        )
        .await;
        let v = body_json(resp).await;
        assert_eq!(v["status"], "todo", "PUT sem status não destarefa");

        // status:"" limpa — volta a nota pura (sem o campo na resposta)
        let resp = put_note_req(
            &app,
            &id,
            "alice",
            "comprar-tinta",
            serde_json::json!({ "title": "Comprar tinta", "markdown": "acrílica 500ml", "status": "" }),
        )
        .await;
        let v = body_json(resp).await;
        assert!(v.get("status").is_none() || v["status"].is_null());
    }

    // ─── Layout do Mundo: drag-drop persiste + write-back CO (YG-149) ─────────

    async fn layout_req(
        app: &Router,
        id: &str,
        user: &str,
        body: serde_json::Value,
    ) -> axum::response::Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/instances/{id}/layout"))
                    .header("authorization", format!("Bearer {}", token(user)))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn layout_persiste_pos_e_reparent_e_round_trip() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        put_note_req(
            &app,
            &id,
            "alice",
            "bem-vindo",
            serde_json::json!({ "title": "Bem-vindo", "markdown": "oi" }),
        )
        .await;

        // commit em lote: reposiciona + reparent numa só requisição
        let resp = layout_req(
            &app,
            &id,
            "alice",
            serde_json::json!({ "moves": [
                { "slug": "bem-vindo", "pos": { "room": "jardim", "x": 4, "y": 6 }, "parent": "jardim" }
            ] }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // posição = estado persistido no .md (reabrir mantém o layout)
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}/notes/bem-vindo"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(get).await;
        assert_eq!(v["pos"]["room"], "jardim");
        assert_eq!(v["pos"]["x"], 4);
        assert_eq!(v["pos"]["y"], 6);
        assert_eq!(v["parent"], "jardim");
    }

    /// CRUD ao pisar: editar o corpo grava no .md e NÃO move o objeto no Mundo.
    #[tokio::test]
    async fn editar_nota_preserva_layout() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;
        put_note_req(
            &app,
            &id,
            "alice",
            "n",
            serde_json::json!({ "title": "N", "markdown": "v1" }),
        )
        .await;
        layout_req(
            &app,
            &id,
            "alice",
            serde_json::json!({ "moves": [
                { "slug": "n", "pos": { "room": "raiz", "x": 2, "y": 3 } }
            ] }),
        )
        .await;
        // editar o corpo
        put_note_req(
            &app,
            &id,
            "alice",
            "n",
            serde_json::json!({ "title": "N", "markdown": "v2" }),
        )
        .await;
        let get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}/notes/n"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(get).await;
        assert_eq!(v["body"], "v2\n");
        assert_eq!(v["pos"]["x"], 2, "editar corpo não move o objeto");
    }

    /// Write-back ao CO (CO-413): cada nota movida emite 1 NoteWritten::Updated
    /// com o frontmatter de layout (pos/parent) — é o patch que o CO recebe.
    #[tokio::test]
    async fn layout_federa_patch_de_pos_ao_co() {
        let (app, _d, mut rx) = app_with_events();
        let id = create_blank(&app, "alice").await;
        put_note_req(
            &app,
            &id,
            "alice",
            "obj",
            serde_json::json!({ "title": "Obj", "markdown": "x" }),
        )
        .await;
        let _ = rx.try_recv(); // descarta o evento do create

        let resp = layout_req(
            &app,
            &id,
            "alice",
            serde_json::json!({ "moves": [
                { "slug": "obj", "pos": { "room": "jardim", "x": 7, "y": 2 }, "parent": "jardim" }
            ] }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let ev = rx.try_recv().expect("1 patch de layout ao CO");
        assert_eq!(ev.kind, NoteKind::Updated);
        assert_eq!(ev.slug, "obj");
        let fm = ev.frontmatter.expect("frontmatter com layout");
        assert_eq!(fm["parent"], "jardim");
        assert_eq!(fm["pos"]["room"], "jardim");
        assert_eq!(fm["pos"]["x"], 7);
        assert!(rx.try_recv().is_err(), "1 evento por nota movida");
    }

    // ─── Rascunhos (YG-125) ──────────────────────────────────────────────────

    async fn draft_req(
        app: &Router,
        method: &str,
        id: &str,
        user: Option<&str>,
        slug: &str,
        body: Option<serde_json::Value>,
    ) -> axum::response::Response {
        let mut b = Request::builder()
            .method(method)
            .uri(format!("/api/v1/instances/{id}/notes/{slug}/draft"));
        if let Some(u) = user {
            b = b.header("authorization", format!("Bearer {}", token(u)));
        }
        let req = match body {
            Some(v) => b
                .header("content-type", "application/json")
                .body(Body::from(v.to_string())),
            None => b.body(Body::empty()),
        }
        .unwrap();
        app.clone().oneshot(req).await.unwrap()
    }

    #[tokio::test]
    async fn draft_round_trip_owner_only() {
        let (app, _d) = app();
        let id = create_blank(&app, "alice").await;

        // sem rascunho ainda → 404
        let resp = draft_req(&app, "GET", &id, Some("alice"), "n", None).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // grava e lê de volta
        let resp = draft_req(
            &app,
            "PUT",
            &id,
            Some("alice"),
            "n",
            Some(serde_json::json!({ "markdown": "rascunho v1" })),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = draft_req(&app, "GET", &id, Some("alice"), "n", None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["markdown"], "rascunho v1");

        // não-dono e anônimo não acessam (link é seguro por construção)
        let resp = draft_req(&app, "GET", &id, Some("bob"), "n", None).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let resp = draft_req(&app, "GET", &id, None, "n", None).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // descarta → some
        let resp = draft_req(&app, "DELETE", &id, Some("alice"), "n", None).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = draft_req(&app, "GET", &id, Some("alice"), "n", None).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Privacidade por construção: rascunho NÃO emite NoteWritten (não federa)
    /// e não toca a nota canônica.
    #[tokio::test]
    async fn draft_nao_federa_nem_toca_a_nota() {
        let (app, _d, mut rx) = app_with_events();
        let id = create_blank(&app, "alice").await;

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/v1/instances/{id}/notes/n/draft"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "markdown": "segredo" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(rx.try_recv().is_err(), "rascunho nunca passa pelo producer");

        // a nota canônica continua inexistente
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/instances/{id}/notes/n"))
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
