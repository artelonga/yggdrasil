//! Rotas do **universo centralizado** — topologia de sentido cross-linguística (YG-175).
//!
//! - `GET  /api/v1/topologia/no/{id}`              — nó + vizinhos + refs (público).
//! - `GET  /api/v1/topologia/grafo?around=&depth=` — subgrafo p/ render (público).
//! - `POST /api/v1/topologia/explorar`             — co-visitação → `weight++` (logado).
//! - `POST /api/v1/topologia/aresta`               — promove a relação nomeada (logado).
//! - `POST /api/v1/topologia/no/{id}/ref`          — anexa referência por link (logado).
//!
//! Anônimo **lê** o grafo; criar/promover aresta e anexar ref **exigem login**
//! (atribuição = autoria, como o `publicar` do comunicação). Toda escrita valida
//! que os nós **existem** num pack ([`resolve_node`]) — o grafo não acumula nós
//! fantasma.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::auth::verify_jwt;
use crate::topologia::{Neighbor, RefRow, ResolvedNode, TopoError, TopologiaDb, resolve_node};
use yggdrasil_core::comunicacao::topologia::RefKind;

const HREF_MAX: usize = 2000;
const LABEL_MAX: usize = 200;
const RELATION_MAX: usize = 120;

pub struct TopologiaState {
    pub jwt_secret: String,
    pub db: Arc<TopologiaDb>,
    /// Curadoria (YG-176): recomputar o overlay semântico é admin-gated. `None`
    /// → recomputar = 401.
    pub admin_token: Option<String>,
}

/// Limiar do overlay semântico (cosseno) e nº de vizinhos por nó.
const SEM_THRESHOLD: f64 = 0.10;
const SEM_K_PER_NODE: usize = 8;

type ApiState = State<Arc<TopologiaState>>;

// ─── helpers ────────────────────────────────────────────────────────────────

fn err(status: StatusCode, msg: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({ "erro": msg }))).into_response()
}

/// `sub` do JWT (`Authorization: Bearer …`) ou `None` (anônimo).
fn caller(state: &TopologiaState, headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| verify_jwt(t, &state.jwt_secret).ok())
}

fn clean(s: Option<String>, max: usize) -> Option<String> {
    s.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|mut s| {
            if s.chars().count() > max {
                s = s.chars().take(max).collect();
            }
            s
        })
}

fn map_write_err(e: TopoError) -> axum::response::Response {
    match e {
        TopoError::SelfLoop => err(StatusCode::BAD_REQUEST, "self_loop"),
        TopoError::UnknownNode(_) => err(StatusCode::NOT_FOUND, "no_inexistente"),
        TopoError::Db(e) => {
            tracing::error!("topologia db: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "erro_interno")
        }
    }
}

// ─── GET /no/{id} ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct NodeView {
    node: ResolvedNode,
    neighbors: Vec<Neighbor>,
    refs: Vec<RefRow>,
    /// Instâncias reais no Ayvu Rapytã (versos, com hierarquia) — YG-177.
    versos: Vec<crate::topologia::VerseRef>,
    /// Exemplos do dicionário (sentenças bilíngues) — YG-177.
    exemplos: Vec<crate::topologia::ExampleRef>,
}

pub async fn get_no(State(state): ApiState, Path(id): Path<String>) -> axum::response::Response {
    let Some(node) = resolve_node(&id) else {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    };
    Json(NodeView {
        node,
        neighbors: state.db.neighbors(&id),
        refs: state.db.refs_for(&id),
        versos: crate::topologia::verses_for(&id),
        exemplos: crate::topologia::examples_for(&id),
    })
    .into_response()
}

// ─── GET /nos?pack= ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct NosQuery {
    #[serde(default)]
    pack: Option<String>,
}

/// Catálogo de nós (paleta do front): todas as entradas resolvidas, opcionalmente
/// filtradas por `pack`. Público.
pub async fn get_nos(
    State(_state): ApiState,
    Query(q): Query<NosQuery>,
) -> axum::response::Response {
    Json(crate::topologia::all_nodes(q.pack.as_deref())).into_response()
}

// ─── GET /sentencas?lang=&q=&limit= ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct SentencasQuery {
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// Sentenças (versos do Ayvu Rapytã) com seus termos — a entrada "ler primeiro"
/// (YG-177): o front escolhe uma sentença e o grafo se filtra às palavras dela.
pub async fn get_sentencas(
    State(_state): ApiState,
    Query(q): Query<SentencasQuery>,
) -> axum::response::Response {
    let limit = q.limit.unwrap_or(200).min(1000);
    Json(crate::topologia::sentences(
        q.lang.as_deref(),
        q.q.as_deref(),
        limit,
    ))
    .into_response()
}

// ─── GET /grafo?around=&depth= ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GrafoQuery {
    around: String,
    #[serde(default)]
    depth: Option<u32>,
    /// YG-176: incluir o overlay de arestas semânticas incidentes ao subgrafo.
    #[serde(default)]
    semantica: bool,
    /// YG-177: quais overlays incluir (lista CSV de `lexico`/`corpus`). Default
    /// `corpus` quando `semantica=true`.
    #[serde(default)]
    overlay: Option<String>,
}

#[derive(Serialize)]
struct GrafoView {
    nodes: Vec<ResolvedNode>,
    edges: Vec<crate::topologia::EdgeRow>,
}

pub async fn get_grafo(
    State(state): ApiState,
    Query(q): Query<GrafoQuery>,
) -> axum::response::Response {
    if resolve_node(&q.around).is_none() {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    }
    let depth = q.depth.unwrap_or(2).clamp(1, crate::topologia::MAX_DEPTH);
    let (mut node_ids, mut edges) = state.db.subgraph(&q.around, depth);
    // YG-176: overlay semântico — arestas de sentido incidentes ao subgrafo, com
    // seus extremos adicionados (assim focar um nó revela vizinhos por SENTIDO,
    // mesmo sem ninguém ter navegado entre eles).
    if q.semantica {
        let overlays = q.overlay.as_deref().unwrap_or("corpus");
        for method in overlays
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            if !matches!(method, "lexico" | "corpus" | "neural") {
                continue;
            }
            let sem = state.db.semantic_incident(&node_ids, method);
            for e in &sem {
                for end in [&e.a, &e.b] {
                    if !node_ids.contains(end) {
                        node_ids.push(end.clone());
                    }
                }
            }
            edges.extend(sem);
        }
    }
    // resolve cada nó alcançado; descarta os que não casam mais um pack (defensivo)
    let nodes = node_ids.iter().filter_map(|id| resolve_node(id)).collect();
    Json(GrafoView { nodes, edges }).into_response()
}

// ─── POST /explorar ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ExplorarBody {
    from: String,
    to: String,
}

pub async fn explorar(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<ExplorarBody>,
) -> axum::response::Response {
    if caller(&state, &headers).is_none() {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    }
    if resolve_node(&body.from).is_none() || resolve_node(&body.to).is_none() {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    }
    match state.db.bump_edge(&body.from, &body.to) {
        Ok(edge) => Json(edge).into_response(),
        Err(e) => map_write_err(e),
    }
}

// ─── POST /aresta ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ArestaBody {
    a: String,
    b: String,
    #[serde(default)]
    relation: Option<String>,
}

pub async fn promover_aresta(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<ArestaBody>,
) -> axum::response::Response {
    if caller(&state, &headers).is_none() {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    }
    if resolve_node(&body.a).is_none() || resolve_node(&body.b).is_none() {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    }
    let relation = clean(body.relation, RELATION_MAX);
    match state.db.promote_edge(&body.a, &body.b, relation.as_deref()) {
        Ok(edge) => Json(edge).into_response(),
        Err(e) => map_write_err(e),
    }
}

// ─── POST /no/{id}/ref ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RefBody {
    kind: String,
    href: String,
    #[serde(default)]
    label: Option<String>,
}

pub async fn anexar_ref(
    State(state): ApiState,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RefBody>,
) -> axum::response::Response {
    let Some(owner) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    if resolve_node(&id).is_none() {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    }
    let Some(kind) = RefKind::parse(&body.kind) else {
        return err(StatusCode::BAD_REQUEST, "kind_invalido");
    };
    let href = match clean(Some(body.href), HREF_MAX) {
        Some(h) => h,
        None => return err(StatusCode::BAD_REQUEST, "href_vazio"),
    };
    let label = clean(body.label, LABEL_MAX);
    match state
        .db
        .add_ref(&id, kind, &href, label.as_deref(), Some(&owner))
    {
        Ok(r) => Json(r).into_response(),
        Err(e) => map_write_err(e),
    }
}

// ─── camada PESSOAL (YG-178): minhas palavras ─────────────────────────────────

#[derive(Deserialize)]
pub struct VisitarBody {
    node: String,
}

/// Reivindica/aprende um termo (clicar = vira "minha"; rever avança o status).
/// Exige login; o termo precisa existir no léxico (`resolve_node`).
pub async fn visitar(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<VisitarBody>,
) -> axum::response::Response {
    let Some(sub) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    if resolve_node(&body.node).is_none() {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    }
    match state.db.visit(&sub, &body.node) {
        Ok(w) => Json(w).into_response(),
        Err(e) => map_write_err(e),
    }
}

#[derive(Serialize)]
struct MyView {
    words: Vec<crate::topologia::WordRow>,
    edges: Vec<crate::topologia::EdgeRow>,
    nodes: Vec<ResolvedNode>,
}

/// O subconjunto pessoal do usuário (palavras + arestas + nós próprios). Login.
pub async fn my_topologia(State(state): ApiState, headers: HeaderMap) -> axum::response::Response {
    let Some(sub) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    Json(MyView {
        words: state.db.my_words(&sub),
        edges: state.db.my_edges(&sub),
        nodes: state.db.my_nodes(&sub),
    })
    .into_response()
}

/// `true` se `id` é referenciável pelo usuário: existe no léxico OU é nó próprio.
fn mine_or_lexico(state: &TopologiaState, sub: &str, id: &str) -> bool {
    resolve_node(id).is_some() || state.db.owns_node(sub, id)
}

// ─── EXPRESSÃO (slice 2): + palavra própria, + conexão, + texto ───────────────

#[derive(Deserialize)]
pub struct NoBody {
    term: String,
    #[serde(default)]
    gloss: Option<String>,
}

/// Adiciona uma **palavra própria** (fora do léxico) ao meu vocabulário.
pub async fn add_meu_no(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<NoBody>,
) -> axum::response::Response {
    let Some(sub) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    let term = match clean(Some(body.term), 120) {
        Some(t) => t,
        None => return err(StatusCode::BAD_REQUEST, "term_vazio"),
    };
    let gloss = clean(body.gloss, 500);
    Json(state.db.add_user_node(&sub, &term, gloss.as_deref())).into_response()
}

#[derive(Deserialize)]
pub struct MeuArestaBody {
    a: String,
    b: String,
    #[serde(default)]
    relation: Option<String>,
}

/// Cria uma **conexão minha** entre dois termos (léxico ou meus). Privada.
pub async fn add_minha_aresta(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<MeuArestaBody>,
) -> axum::response::Response {
    let Some(sub) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    if !mine_or_lexico(&state, &sub, &body.a) || !mine_or_lexico(&state, &sub, &body.b) {
        return err(StatusCode::NOT_FOUND, "no_inexistente");
    }
    let relation = clean(body.relation, RELATION_MAX);
    match state
        .db
        .add_user_edge(&sub, &body.a, &body.b, relation.as_deref())
    {
        Ok(e) => Json(e).into_response(),
        Err(e) => map_write_err(e),
    }
}

#[derive(Deserialize)]
pub struct TextoBody {
    #[serde(default)]
    title: Option<String>,
    text: String,
}

/// Grava um **texto próprio** (corpus pessoal — a EXPRESSÃO). Devolve o id.
pub async fn add_meu_texto(
    State(state): ApiState,
    headers: HeaderMap,
    Json(body): Json<TextoBody>,
) -> axum::response::Response {
    let Some(sub) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    let text = match clean(Some(body.text), 4000) {
        Some(t) => t,
        None => return err(StatusCode::BAD_REQUEST, "texto_vazio"),
    };
    let title = clean(body.title, 200);
    let id = state.db.add_user_text(&sub, title.as_deref(), &text);
    Json(serde_json::json!({ "id": id })).into_response()
}

/// Meus textos como sentenças (palavras resolvidas no léxico ou nos meus nós) —
/// entram no "Ler" do front como o meu corpus pessoal.
pub async fn meus_textos(State(state): ApiState, headers: HeaderMap) -> axum::response::Response {
    let Some(sub) = caller(&state, &headers) else {
        return err(StatusCode::UNAUTHORIZED, "nao_autenticado");
    };
    let out: Vec<serde_json::Value> = state
        .db
        .my_texts_raw(&sub)
        .into_iter()
        .map(|(id, title, text)| {
            let terms: Vec<String> = text
                .split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '\u{2019}')
                .filter(|w| !w.is_empty())
                .filter_map(|w| match_term(&state, &sub, w))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            serde_json::json!({
                "id": format!("meu#{id}"),
                "lang": "meu",
                "loc": title.unwrap_or_else(|| "meu texto".to_string()),
                "text": text,
                "terms": terms,
            })
        })
        .collect();
    Json(out).into_response()
}

/// Casa uma palavra escrita a um NodeId: léxico (`gn-mbya`/`yo`) ou nó próprio.
fn match_term(state: &TopologiaState, sub: &str, word: &str) -> Option<String> {
    use yggdrasil_core::comunicacao::lexicon::slugify;
    let s = slugify(word);
    for lang in ["gn-mbya", "yo"] {
        let id = format!("{lang}:{s}");
        if resolve_node(&id).is_some() {
            return Some(id);
        }
    }
    let uid = format!("user:{sub}:{s}");
    state.db.owns_node(sub, &uid).then_some(uid)
}

// ─── POST /semantica/recomputar ───────────────────────────────────────────────

/// Recalcula o overlay semântico (cosseno de sentido por contexto). Admin-gated
/// (`YGGDRASIL_ADMIN_TOKEN`) — é um job, não uma ação de usuário.
pub async fn recomputar_semantica(
    State(state): ApiState,
    headers: HeaderMap,
) -> axum::response::Response {
    let admin = match state.admin_token.as_deref() {
        Some(t) => t,
        None => return err(StatusCode::UNAUTHORIZED, "admin_token_nao_configurado"),
    };
    let ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == admin);
    if !ok {
        return err(StatusCode::UNAUTHORIZED, "nao_autorizado");
    }
    match state.db.recompute_semantic(SEM_THRESHOLD, SEM_K_PER_NODE) {
        Ok(pares) => {
            Json(serde_json::json!({ "pares": pares, "limiar": SEM_THRESHOLD })).into_response()
        }
        Err(e) => map_write_err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::sign_jwt;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use tower::ServiceExt;

    const SECRET: &str = "test-secret";

    fn app() -> Router {
        let state = Arc::new(TopologiaState {
            jwt_secret: SECRET.to_string(),
            db: Arc::new(TopologiaDb::in_memory().unwrap()),
            admin_token: Some("admin-secret".to_string()),
        });
        Router::new()
            .route("/api/v1/topologia/no/{id}", get(get_no))
            .route("/api/v1/topologia/grafo", get(get_grafo))
            .route("/api/v1/topologia/explorar", post(explorar))
            .route("/api/v1/topologia/aresta", post(promover_aresta))
            .route("/api/v1/topologia/no/{id}/ref", post(anexar_ref))
            .with_state(state)
    }

    fn token(sub: &str) -> String {
        sign_jwt(sub, &format!("{sub}@e2e.test"), SECRET).unwrap()
    }

    /// Dois ids de nós REAIS do catálogo (../comunicacao). `None` se ausente → o
    /// teste é pulado (sem dado fabricado de fallback).
    fn two_real_ids() -> Option<(String, String)> {
        let ns = crate::topologia::all_nodes(None);
        (ns.len() >= 2).then(|| (ns[0].id.clone(), ns[1].id.clone()))
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    #[tokio::test]
    async fn explorar_exige_login_e_liga_nos_reais() {
        let Some((a, b)) = two_real_ids() else {
            eprintln!("(skip) catálogo real ausente");
            return;
        };
        let app = app();
        // anônimo → 401
        let anon = app
            .clone()
            .oneshot(
                Request::post("/api/v1/topologia/explorar")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"from":"{a}","to":"{b}"}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);

        // logado → cria a aresta (weight 1)
        let ok = app
            .clone()
            .oneshot(
                Request::post("/api/v1/topologia/explorar")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::from(format!(r#"{{"from":"{a}","to":"{b}"}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        let v = body_json(ok).await;
        assert_eq!(v["weight"], 1);
        assert_eq!(v["source"], "explore");
    }

    #[tokio::test]
    async fn explorar_no_inexistente_404() {
        let app = app();
        let resp = app
            .oneshot(
                Request::post("/api/v1/topologia/explorar")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::from(
                        r#"{"from":"gn-mbya:naoexiste-zzz","to":"gn-mbya:tampouco-zzz"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn no_publico_traz_vizinhos_e_refs() {
        let Some((a, b)) = two_real_ids() else {
            eprintln!("(skip) catálogo real ausente");
            return;
        };
        let app = app();
        // semeia uma aresta + ref via rotas autenticadas
        let _ = app
            .clone()
            .oneshot(
                Request::post("/api/v1/topologia/explorar")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::from(format!(r#"{{"from":"{a}","to":"{b}"}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap();
        let _ = app
            .clone()
            .oneshot(
                Request::post(format!("/api/v1/topologia/no/{a}/ref"))
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::from(
                        r#"{"kind":"etymology","href":"/comunicacao/lexico/x"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // leitura anônima do nó
        let resp = app
            .oneshot(
                Request::get(format!("/api/v1/topologia/no/{a}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["node"]["id"], a);
        assert_eq!(v["neighbors"][0]["node"], b);
        assert_eq!(v["refs"][0]["kind"], "etymology");
    }

    #[tokio::test]
    async fn promover_aresta_nomeia_relacao() {
        let Some((a, b)) = two_real_ids() else {
            eprintln!("(skip) catálogo real ausente");
            return;
        };
        let app = app();
        let resp = app
            .oneshot(
                Request::post("/api/v1/topologia/aresta")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token("alice")))
                    .body(Body::from(format!(
                        r#"{{"a":"{a}","b":"{b}","relation":"rima sonora"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["source"], "user");
        assert_eq!(v["relation"], "rima sonora");
    }

    #[tokio::test]
    async fn grafo_no_inexistente_404() {
        let app = app();
        let resp = app
            .oneshot(
                Request::get("/api/v1/topologia/grafo?around=pack:fantasma")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
