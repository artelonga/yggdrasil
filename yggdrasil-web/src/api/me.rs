use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use yggdrasil_core::sementes::Sementes;
use yggdrasil_core::universes::{UniverseKind, UniverseRegistry};

use crate::auth::verify_jwt;

pub struct MeState {
    pub jwt_secret: String,
    pub sementes: Arc<Sementes>,
}

pub struct UniversosMeState {
    pub jwt_secret: String,
    pub db_path: PathBuf,
    pub registry: Arc<UniverseRegistry>,
}

#[derive(Serialize)]
struct SaldoResponse {
    saldo: u64,
    moeda: &'static str,
    atualizado_em: String,
}

pub async fn get_sementes(
    State(state): State<Arc<MeState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = match extract_bearer(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "nao_autenticado"})),
            )
                .into_response();
        }
    };

    let user_id = match verify_jwt(&token, &state.jwt_secret) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "nao_autenticado"})),
            )
                .into_response();
        }
    };

    match state.sementes.saldo_info(&user_id) {
        Ok(info) => (
            StatusCode::OK,
            Json(SaldoResponse {
                saldo: info.saldo,
                moeda: "sementes",
                atualizado_em: info.atualizado_em.to_rfc3339(),
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Erro ao buscar saldo de sementes: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "Erro interno"})),
            )
                .into_response()
        }
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

#[derive(Deserialize, Default)]
pub struct UniversosQuery {
    pub visibilidade: Option<String>,
    pub papel: Option<String>,
    pub cursor: Option<String>,
    #[serde(default = "default_limite")]
    pub limite: usize,
}

fn default_limite() -> usize {
    50
}

#[derive(Serialize, Debug, Clone)]
pub struct UniversoEntry {
    pub slug: String,
    pub nome: String,
    pub visibilidade: &'static str,
    pub papel: &'static str,
    pub ultima_visita: String,
}

#[derive(Serialize)]
struct UniversosResponse {
    universos: Vec<UniversoEntry>,
    proximo_cursor: Option<String>,
}

/// `GET /api/v1/me/universos` — lista universos que o usuário interagiu.
///
/// Hoje "interagiu" significa: scores em snake/tetris/invaders, ou mãos
/// favoritadas em pôquer. Quando YG-15 (plugin loader) e YG-N (criação de
/// universos) chegarem, `papel` poderá ser `"criador"` e `visibilidade` virá
/// do registro de cada universo. Por enquanto: todos são `publico` + `jogador`.
///
/// Filtros: `?visibilidade=publico|privado`, `?papel=criador|jogador`.
/// Paginação cursor-based: `?cursor=<slug>&limite=50` (slug é o último item
/// retornado; ordem estável por slug).
pub async fn get_universos(
    State(state): State<Arc<UniversosMeState>>,
    headers: HeaderMap,
    Query(q): Query<UniversosQuery>,
) -> impl IntoResponse {
    let token = match extract_bearer(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "nao_autenticado"})),
            )
                .into_response();
        }
    };
    let user_id = match verify_jwt(&token, &state.jwt_secret) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"erro": "nao_autenticado"})),
            )
                .into_response();
        }
    };

    // Filtros estáticos: hoje todos os universos são públicos e o papel é
    // sempre 'jogador'. Filtros que pedem o oposto retornam lista vazia.
    let papel_filter = q.papel.as_deref();
    let vis_filter = q.visibilidade.as_deref();
    if matches!(papel_filter, Some(p) if p != "jogador")
        || matches!(vis_filter, Some(v) if v != "publico")
    {
        return (
            StatusCode::OK,
            Json(UniversosResponse {
                universos: vec![],
                proximo_cursor: None,
            }),
        )
            .into_response();
    }

    let entries = match query_universos(&state.db_path, &user_id, &state.registry) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("me/universos query: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"erro": "db_unavailable"})),
            )
                .into_response();
        }
    };

    // Cursor: pula tudo até depois do slug informado. Ordem estável por slug.
    let mut iter = entries.into_iter().peekable();
    if let Some(c) = &q.cursor {
        while let Some(e) = iter.peek() {
            if e.slug.as_str() <= c.as_str() {
                iter.next();
            } else {
                break;
            }
        }
    }
    let limite = q.limite.clamp(1, 100);
    let page: Vec<UniversoEntry> = iter.by_ref().take(limite).collect();
    let proximo_cursor = if iter.peek().is_some() {
        page.last().map(|e| e.slug.clone())
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(UniversosResponse {
            universos: page,
            proximo_cursor,
        }),
    )
        .into_response()
}

/// Roda uma única query SQL (UNION ALL) para evitar N+1: agrega scores por
/// game + adiciona pôquer se há mãos favoritadas. Depois faz join in-memory
/// com o registro estático de universos.
fn query_universos(
    db_path: &std::path::Path,
    user_id: &str,
    registry: &UniverseRegistry,
) -> rusqlite::Result<Vec<UniversoEntry>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT game AS slug, MAX(ts) AS ultima
           FROM scores
          WHERE user_id = ?1
          GROUP BY game
         UNION ALL
         SELECT 'poker' AS slug,
                strftime('%Y-%m-%dT%H:%M:%SZ', MAX(favorited_at), 'unixepoch') AS ultima
           FROM poker_favorite_hands
          WHERE user_id = ?1
          GROUP BY user_id",
    )?;
    let rows = stmt.query_map([user_id], |row| {
        let slug: String = row.get(0)?;
        let ultima: String = row.get(1)?;
        Ok((slug, ultima))
    })?;

    let mut entries: Vec<UniversoEntry> = Vec::new();
    for r in rows.flatten() {
        let (slug, ultima_visita) = r;
        let nome = registry
            .get(&slug)
            .filter(|n| matches!(n.kind, UniverseKind::Root))
            .map(|n| n.title.clone())
            .unwrap_or_else(|| slug.clone());
        entries.push(UniversoEntry {
            slug,
            nome,
            visibilidade: "publico",
            papel: "jogador",
            ultima_visita,
        });
    }
    entries.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use game_core::storage::{Storage, schema};
    use tempfile::tempdir;
    use tower::ServiceExt;

    use crate::auth::sign_jwt;

    fn make_state_with_storage(secret: &str, storage: Arc<Storage>) -> Arc<MeState> {
        Arc::new(MeState {
            jwt_secret: secret.to_string(),
            sementes: Arc::new(Sementes::new(storage)),
        })
    }

    fn make_state(secret: &str) -> (Arc<MeState>, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());
        (make_state_with_storage(secret, storage), dir)
    }

    fn make_app(state: Arc<MeState>) -> Router {
        Router::new()
            .route("/api/v1/me/sementes", get(get_sementes))
            .with_state(state)
    }

    #[tokio::test]
    async fn sem_jwt_retorna_401_nao_autenticado() {
        let (state, _dir) = make_state("test-secret");
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["erro"], "nao_autenticado");
    }

    #[tokio::test]
    async fn jwt_invalido_retorna_401() {
        let (state, _dir) = make_state("test-secret");
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", "Bearer token-invalido")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["erro"], "nao_autenticado");
    }

    #[tokio::test]
    async fn jwt_valido_retorna_200_com_saldo_e_timestamp() {
        let secret = "test-secret";
        let (state, _dir) = make_state(secret);
        let token = sign_jwt("user-1", "user@test.com", secret).unwrap();
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["saldo"].is_number(), "saldo deve ser número");
        assert_eq!(v["moeda"], "sementes");
        assert!(
            v["atualizado_em"].is_string(),
            "atualizado_em deve ser string ISO 8601"
        );
    }

    #[tokio::test]
    async fn saldo_u64_correto_para_usuario_com_carteira() {
        let secret = "test-secret";
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.db");
        let storage = Arc::new(Storage::open(&path).unwrap());

        let wallet = schema::Wallet {
            user_id: "user-42".to_string(),
            balance: 9_750,
            last_updated: 1_700_000_000,
        };
        storage.save_wallet_for_user("user-42", &wallet).unwrap();

        let state = make_state_with_storage(secret, storage);
        let token = sign_jwt("user-42", "u@test.com", secret).unwrap();
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["saldo"].as_u64().unwrap(), 9_750u64);
        assert!(v["atualizado_em"].as_str().unwrap().starts_with("2023-"));
    }

    fn seed_universos_db(path: &std::path::Path) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS scores (
                id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, game TEXT NOT NULL,
                score INTEGER NOT NULL, ts TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS poker_favorite_hands (
                id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, hand_id TEXT NOT NULL,
                favorited_at INTEGER NOT NULL, snapshot TEXT NOT NULL
             );
             INSERT INTO scores (user_id, game, score, ts) VALUES
                ('u1', 'snake', 100, '2026-05-15T10:00:00Z'),
                ('u1', 'snake',  50, '2026-05-14T10:00:00Z'),
                ('u1', 'tetris', 500, '2026-05-14T09:00:00Z'),
                ('u2', 'invaders', 10, '2026-05-15T08:00:00Z');
             INSERT INTO poker_favorite_hands (user_id, hand_id, favorited_at, snapshot) VALUES
                ('u1', 'h1', 1747299600, '{}'),
                ('u1', 'h2', 1747386000, '{}');",
        )
        .unwrap();
    }

    fn make_universos_state(secret: &str, db: PathBuf) -> Arc<UniversosMeState> {
        Arc::new(UniversosMeState {
            jwt_secret: secret.to_string(),
            db_path: db,
            registry: Arc::new(yggdrasil_core::universes::default_registry()),
        })
    }

    fn make_universos_app(state: Arc<UniversosMeState>) -> Router {
        Router::new()
            .route("/api/v1/me/universos", get(get_universos))
            .with_state(state)
    }

    #[tokio::test]
    async fn universos_sem_auth_retorna_401() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed_universos_db(&path);
        let app = make_universos_app(make_universos_state("s", path));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/universos")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn universos_autenticado_agrega_scores_e_poker() {
        let secret = "s";
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed_universos_db(&path);
        let app = make_universos_app(make_universos_state(secret, path));
        let token = sign_jwt("u1", "u1@test.com", secret).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/universos")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = v["universos"].as_array().unwrap();
        // u1 jogou snake + tetris e tem 2 mãos favoritas em poker → 3 universos.
        assert_eq!(arr.len(), 3);
        let slugs: Vec<&str> = arr.iter().map(|e| e["slug"].as_str().unwrap()).collect();
        assert_eq!(slugs, vec!["poker", "snake", "tetris"]);
        // Nome humano vem do registry.
        let snake = arr.iter().find(|e| e["slug"] == "snake").unwrap();
        assert_eq!(snake["nome"], "Snake");
        // Todos públicos + jogador por enquanto.
        assert!(arr.iter().all(|e| e["visibilidade"] == "publico"));
        assert!(arr.iter().all(|e| e["papel"] == "jogador"));
        // ultima_visita é o MAX(ts) do snake (2026-05-15 não 2026-05-14).
        assert_eq!(snake["ultima_visita"], "2026-05-15T10:00:00Z");
    }

    #[tokio::test]
    async fn universos_sem_atividade_retorna_lista_vazia() {
        let secret = "s";
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed_universos_db(&path);
        let app = make_universos_app(make_universos_state(secret, path));
        let token = sign_jwt("u-novo", "novo@test.com", secret).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/universos")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["universos"].as_array().unwrap().is_empty());
        assert!(v["proximo_cursor"].is_null());
    }

    #[tokio::test]
    async fn universos_filtro_visibilidade_privado_retorna_vazio() {
        let secret = "s";
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed_universos_db(&path);
        let app = make_universos_app(make_universos_state(secret, path));
        let token = sign_jwt("u1", "u1@test.com", secret).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/universos?visibilidade=privado")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(v["universos"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn universos_paginacao_cursor_avanca_pela_lista() {
        let secret = "s";
        let dir = tempdir().unwrap();
        let path = dir.path().join("t.db");
        seed_universos_db(&path);
        let app = make_universos_app(make_universos_state(secret, path));
        let token = sign_jwt("u1", "u1@test.com", secret).unwrap();

        // Página 1: limite=2 → poker, snake. proximo_cursor=snake.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/universos?limite=2")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["universos"].as_array().unwrap().len(), 2);
        assert_eq!(v["proximo_cursor"], "snake");

        // Página 2: cursor=snake → tetris. Cursor null no fim.
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/universos?limite=2&cursor=snake")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = v["universos"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["slug"], "tetris");
        assert!(v["proximo_cursor"].is_null());
    }

    #[tokio::test]
    async fn usuario_sem_carteira_retorna_saldo_zero() {
        let secret = "test-secret";
        let (state, _dir) = make_state(secret);
        let token = sign_jwt("novo-usuario", "novo@test.com", secret).unwrap();
        let app = make_app(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/me/sementes")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["saldo"].as_u64().unwrap(), 0u64);
        assert_eq!(v["moeda"], "sementes");
    }
}
