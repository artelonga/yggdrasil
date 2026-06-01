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
use yggdrasil_core::comunicacao::{
    LexiconError, LexiconStore, ReviewItem, Room, RoomEdit, RoomStore, lexicon::Contribution,
    room::EditError, store::StoreError, template_instantiate, template_summaries,
};

use crate::auth::verify_jwt;

/// Estado compartilhado das rotas de comunicação.
pub struct ComunicacaoState {
    pub jwt_secret: String,
    pub store: Arc<RoomStore>,
    pub lexicon: Arc<LexiconStore>,
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
    if let Err(e) = op.apply(&mut room) {
        return map_edit_err(e);
    }
    if let Err(e) = state.store.save(&room) {
        return map_store_err(e);
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

    fn app() -> (Router, TempDir, TempDir) {
        let rooms_dir = tempfile::tempdir().unwrap();
        let lex_dir = tempfile::tempdir().unwrap();
        // semeia o léxico compartilhado: yoruba/terms/iemanja.md já existe
        let shared = lex_dir.path().join("yoruba/terms");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("iemanja.md"), "---\nword: iemanjá\n---\n").unwrap();

        let store = Arc::new(RoomStore::new(rooms_dir.path()).unwrap());
        let lexicon = Arc::new(LexiconStore::new(lex_dir.path()));
        let state = Arc::new(ComunicacaoState {
            jwt_secret: SECRET.into(),
            store,
            lexicon,
        });
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
            .route("/api/v1/comunicacao/templates", get(list_templates))
            .route("/api/v1/comunicacao/revisao", get(get_revisao))
            .route("/api/v1/comunicacao/revisao/nota", post(nota_revisao))
            .with_state(state);
        (router, rooms_dir, lex_dir)
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
}
