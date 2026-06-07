//! Universe graph endpoints — expõe o registro de universos como API
//! queryable. Lê o `default_registry` na inicialização do servidor.
//!
//! Anônimo (sem auth) — o grafo de universos é público.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use yggdrasil_core::universes::UniverseRegistry;

pub struct UniversesState {
    pub registry: Arc<UniverseRegistry>,
}

#[utoipa::path(
    get,
    path = "/api/v1/universes",
    responses(
        (status = 200, description = "Lista de todos os universos", body = crate::openapi::UniversesListDoc),
    ),
    tag = "universes"
)]
pub async fn list_universes(State(state): State<Arc<UniversesState>>) -> impl IntoResponse {
    let nodes: Vec<&yggdrasil_core::universes::UniverseNode> = state.registry.list();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "universes": nodes })),
    )
        .into_response()
}

#[utoipa::path(
    get,
    path = "/api/v1/universes/{slug}",
    params(
        ("slug" = String, Path, description = "Slug do universo (pode conter '/', ex: tetris/sprint-40)")
    ),
    responses(
        (status = 200, description = "Nó do universo", body = crate::openapi::UniverseNodeDoc),
        (status = 404, description = "Universo não encontrado", body = crate::openapi::ErrorDoc),
    ),
    tag = "universes"
)]
pub async fn get_universe(
    State(state): State<Arc<UniversesState>>,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    match state.registry.get(&slug) {
        Some(node) => (StatusCode::OK, Json(node.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"erro": format!("Universo '{slug}' não encontrado")})),
        )
            .into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/universes/graph",
    responses(
        (status = 200, description = "Grafo de universos (nós + arestas)", body = crate::openapi::UniverseGraphDoc),
    ),
    tag = "universes"
)]
pub async fn get_graph(State(state): State<Arc<UniversesState>>) -> impl IntoResponse {
    (StatusCode::OK, Json(state.registry.graph())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;
    use yggdrasil_core::universes::default_registry;

    fn make_app() -> Router {
        let state = Arc::new(UniversesState {
            registry: Arc::new(default_registry()),
        });
        Router::new()
            .route("/api/v1/universes", get(list_universes))
            .route("/api/v1/universes/graph", get(get_graph))
            .route("/api/v1/universes/{*slug}", get(get_universe))
            .with_state(state)
    }

    async fn parse_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn list_retorna_todos_os_universos() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        let universes = v["universes"].as_array().unwrap();
        // 4 roots + 8 variantes no default registry.
        assert!(universes.len() >= 12, "got {}", universes.len());
    }

    #[tokio::test]
    async fn get_root_inclui_children() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universes/tetris")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert_eq!(v["slug"], "tetris");
        assert_eq!(v["kind"], "root");
        let children = v["children"].as_array().unwrap();
        assert!(
            children.iter().any(|c| c == "tetris/sprint-40"),
            "children: {children:?}"
        );
    }

    #[tokio::test]
    async fn get_variant_aponta_para_root_via_parent() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universes/tetris/sprint-40")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert_eq!(v["slug"], "tetris/sprint-40");
        assert_eq!(v["parent"], "tetris");
        assert_eq!(v["kind"], "variant");
        // Parâmetros documentados
        assert_eq!(v["parameters"]["lines_to_clear"], 40);
        // API contract aponta para o root com query string
        assert_eq!(
            v["api"]["start"],
            "/api/v1/games/tetris/start?variant=tetris/sprint-40"
        );
    }

    #[tokio::test]
    async fn get_universo_inexistente_retorna_404() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universes/nao-existe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn graph_retorna_nodes_e_edges() {
        let app = make_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/universes/graph")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = parse_body(resp).await;
        assert!(v["nodes"].is_array());
        assert!(v["edges"].is_array());
        let edges = v["edges"].as_array().unwrap();
        // Cada variante gera uma edge — pelo menos 8 no default registry.
        assert!(edges.len() >= 8, "edges: {}", edges.len());
        // Edge canônica: tetris → tetris/sprint-40
        assert!(
            edges
                .iter()
                .any(|e| e["from"] == "tetris" && e["to"] == "tetris/sprint-40"),
            "edges: {edges:?}"
        );
    }
}
