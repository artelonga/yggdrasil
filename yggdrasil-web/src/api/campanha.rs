//! Rotas da campanha (YG-161) — crowdfunding **independente** de Catarse.
//!
//! - `GET  /api/v1/campanha/tiers`            — tiers canônicos (público).
//! - `POST /api/v1/campanha/apoiar`           — registra um apoio (JWT opcional).
//! - `GET  /api/v1/creditos`                  — apoiadores que optaram por aparecer.
//! - `POST /api/v1/campanha/pledges/{id}/confirmar` — admin: confirma pagamento
//!   (fora de banda) e credita as sementes do tier ao usuário, se logado.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::auth::verify_jwt;
use crate::campanha::{Credito, NewPledge, PledgeDb, TIERS, Tier, tier};
use yggdrasil_core::sementes::Sementes;

const NAME_MAX: usize = 120;
const EMAIL_MAX: usize = 200;
const MSG_MAX: usize = 500;

pub struct CampanhaState {
    pub jwt_secret: String,
    pub db: Arc<PledgeDb>,
    pub sementes: Arc<Sementes>,
    /// Mesma chave de admin do feedback/analytics. `None` → confirmar = 401.
    pub admin_token: Option<String>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn bad(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn clean(v: Option<String>, max: usize) -> Option<String> {
    v.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|mut s| {
            if s.chars().count() > max {
                s = s.chars().take(max).collect();
            }
            s
        })
}

/// `GET /api/v1/campanha/tiers` — tiers canônicos (público).
pub async fn list_tiers() -> Json<Vec<Tier>> {
    Json(TIERS.to_vec())
}

#[derive(Deserialize)]
pub struct ApoiarBody {
    pub tier: String,
    #[serde(default)]
    pub nome: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub mensagem: Option<String>,
    /// Opt-in para aparecer nos créditos. Default `true` (apoio é celebrado).
    #[serde(default = "default_true")]
    pub mostrar_creditos: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
pub struct ApoiarOk {
    pub ok: bool,
    pub id: String,
    pub tier: String,
    pub valor: u32,
    pub status: String,
    /// Próximos passos honestos: nada foi cobrado ainda.
    pub proximos_passos: String,
}

/// `POST /api/v1/campanha/apoiar` — registra um apoio (pledge). JWT opcional:
/// se presente e válido, amarra ao `user_sub` (para creditar sementes na
/// confirmação). Nenhuma cobrança acontece aqui — o pledge nasce `pendente`.
pub async fn apoiar(
    State(state): State<Arc<CampanhaState>>,
    headers: HeaderMap,
    Json(body): Json<ApoiarBody>,
) -> axum::response::Response {
    let user_sub = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| verify_jwt(t, &state.jwt_secret).ok());

    let slug = body.tier.trim().to_lowercase();
    let t = match tier(&slug) {
        Some(t) => t,
        None => return bad("tier_invalido"),
    };

    let nome = clean(body.nome, NAME_MAX);
    let email = clean(body.email, EMAIL_MAX);
    if let Some(ref e) = email {
        let parts: Vec<&str> = e.split('@').collect();
        if parts.len() != 2 || parts[0].is_empty() || !parts[1].contains('.') {
            return bad("email_invalido");
        }
    }
    let mensagem = clean(body.mensagem, MSG_MAX);

    let id = state.db.apoiar(&NewPledge {
        tier: t.slug,
        valor: t.preco,
        nome: nome.as_deref(),
        email: email.as_deref(),
        user_sub: user_sub.as_deref(),
        mensagem: mensagem.as_deref(),
        mostrar_creditos: body.mostrar_creditos,
    });

    (
        StatusCode::CREATED,
        Json(ApoiarOk {
            ok: true,
            id,
            tier: t.slug.to_string(),
            valor: t.preco,
            status: crate::campanha::STATUS_PENDENTE.to_string(),
            proximos_passos:
                "Apoio registrado! Em breve enviaremos as instruções de pagamento (PIX). \
                 Nada foi cobrado ainda."
                    .to_string(),
        }),
    )
        .into_response()
}

/// `GET /api/v1/creditos` — apoiadores que optaram por aparecer (público).
/// Nunca devolve e-mail nem `user_sub`.
pub async fn list_creditos(State(state): State<Arc<CampanhaState>>) -> Json<Vec<Credito>> {
    Json(state.db.creditos())
}

/// `POST /api/v1/campanha/pledges/{id}/confirmar` — admin confirma o pagamento
/// (registrado fora de banda) e credita as sementes do tier ao usuário, se
/// logado. Gated por `YGGDRASIL_ADMIN_TOKEN`. Só credita na primeira confirmação.
pub async fn confirmar_pledge(
    State(state): State<Arc<CampanhaState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    let admin_token = match &state.admin_token {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody {
                    error: "admin_token_nao_configurado".to_string(),
                }),
            )
                .into_response();
        }
    };
    let provided = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    if provided != admin_token {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody {
                error: "token_invalido".to_string(),
            }),
        )
            .into_response();
    }

    let info = match state.db.confirmar(&id) {
        Some(i) => i,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: "nao_encontrado".to_string(),
                }),
            )
                .into_response();
        }
    };

    // Credita as sementes do tier — só na primeira confirmação e só se logado.
    let mut sementes_creditadas = 0u64;
    if info.was_pendente
        && let Some(ref sub) = info.user_sub
        && let Some(t) = tier(&info.tier)
        && t.sementes > 0
    {
        match state.sementes.creditar(sub, t.sementes) {
            Ok(()) => sementes_creditadas = t.sementes,
            Err(e) => tracing::error!("falha ao creditar sementes do tier {}: {e}", info.tier),
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "id": id,
            "status": crate::campanha::STATUS_CONFIRMADO,
            "sementes_creditadas": sementes_creditadas,
        })),
    )
        .into_response()
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
    use tower::ServiceExt;

    // game-core Storage é redb (file-backed, sem in-memory) → arquivo temporário
    // único por teste. O TempDir vive enquanto o app durar (devolvido junto).
    fn sementes_tmp() -> (Arc<Sementes>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage =
            Arc::new(game_core::storage::Storage::open(&dir.path().join("sementes.redb")).unwrap());
        (Arc::new(Sementes::new(storage)), dir)
    }

    fn app() -> (Router, Arc<CampanhaState>, tempfile::TempDir) {
        let (sementes, dir) = sementes_tmp();
        let state = Arc::new(CampanhaState {
            jwt_secret: "dev".to_string(),
            db: Arc::new(PledgeDb::in_memory().unwrap()),
            sementes,
            admin_token: Some("sekret".to_string()),
        });
        let router = Router::new()
            .route("/api/v1/campanha/tiers", get(list_tiers))
            .route("/api/v1/campanha/apoiar", post(apoiar))
            .route("/api/v1/creditos", get(list_creditos))
            .route(
                "/api/v1/campanha/pledges/{id}/confirmar",
                post(confirmar_pledge),
            )
            .with_state(state.clone());
        (router, state, dir)
    }

    async fn post_json(
        app: &Router,
        uri: &str,
        json: serde_json::Value,
        auth: Option<&str>,
    ) -> (StatusCode, String) {
        let mut req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(t) = auth {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::from(json.to_string())).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    async fn get_body(app: &Router, uri: &str) -> (StatusCode, String) {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    #[tokio::test]
    async fn tiers_listados() {
        let (app, _s, _dir) = app();
        let (st, body) = get_body(&app, "/api/v1/campanha/tiers").await;
        assert_eq!(st, StatusCode::OK);
        assert!(body.contains("Semente"));
        assert!(body.contains("yggdrasil"));
    }

    #[tokio::test]
    async fn apoiar_tier_invalido_400() {
        let (app, _s, _dir) = app();
        let (st, _) = post_json(
            &app,
            "/api/v1/campanha/apoiar",
            serde_json::json!({"tier":"ouro"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn apoiar_anonimo_201_pendente() {
        let (app, state, _dir) = app();
        let (st, body) = post_json(
            &app,
            "/api/v1/campanha/apoiar",
            serde_json::json!({"tier":"raiz","nome":"Ana","mensagem":"vamo"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        assert!(body.contains("pendente"));
        assert!(body.contains("\"valor\":60"));
        assert_eq!(state.db.count(), 1);
    }

    #[tokio::test]
    async fn apoiar_email_invalido_400() {
        let (app, _s, _dir) = app();
        let (st, _) = post_json(
            &app,
            "/api/v1/campanha/apoiar",
            serde_json::json!({"tier":"semente","email":"xis"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn creditos_mostra_opt_in_e_esconde_opt_out() {
        let (app, _s, _dir) = app();
        post_json(
            &app,
            "/api/v1/campanha/apoiar",
            serde_json::json!({"tier":"semente","nome":"Apareço","mostrar_creditos":true,"email":"a@b.com"}),
            None,
        )
        .await;
        post_json(
            &app,
            "/api/v1/campanha/apoiar",
            serde_json::json!({"tier":"raiz","nome":"Sumido","mostrar_creditos":false}),
            None,
        )
        .await;
        let (st, body) = get_body(&app, "/api/v1/creditos").await;
        assert_eq!(st, StatusCode::OK);
        assert!(body.contains("Apareço"));
        assert!(!body.contains("Sumido"));
        assert!(!body.contains("a@b.com"), "nunca vaza e-mail");
    }

    #[tokio::test]
    async fn confirmar_sem_token_401() {
        let (app, state, _dir) = app();
        let id = state.db.apoiar(&NewPledge {
            tier: "raiz",
            valor: 60,
            nome: None,
            email: None,
            user_sub: Some("u-1"),
            mensagem: None,
            mostrar_creditos: true,
        });
        let (st, _) = post_json(
            &app,
            &format!("/api/v1/campanha/pledges/{id}/confirmar"),
            serde_json::json!({}),
            Some("errado"),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn confirmar_credita_sementes_do_tier_uma_vez() {
        let (app, state, _dir) = app();
        let id = state.db.apoiar(&NewPledge {
            tier: "raiz",
            valor: 60,
            nome: Some("Ana"),
            email: None,
            user_sub: Some("u-1"),
            mensagem: None,
            mostrar_creditos: true,
        });
        assert_eq!(state.sementes.saldo("u-1").unwrap(), 0);

        let (st, body) = post_json(
            &app,
            &format!("/api/v1/campanha/pledges/{id}/confirmar"),
            serde_json::json!({}),
            Some("sekret"),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert!(body.contains("\"sementes_creditadas\":1000"));
        assert_eq!(state.sementes.saldo("u-1").unwrap(), 1_000);

        // reconfirmar não recredita
        post_json(
            &app,
            &format!("/api/v1/campanha/pledges/{id}/confirmar"),
            serde_json::json!({}),
            Some("sekret"),
        )
        .await;
        assert_eq!(state.sementes.saldo("u-1").unwrap(), 1_000, "não recredita");
    }

    #[tokio::test]
    async fn confirmar_id_inexistente_404() {
        let (app, _s, _dir) = app();
        let (st, _) = post_json(
            &app,
            "/api/v1/campanha/pledges/nada/confirmar",
            serde_json::json!({}),
            Some("sekret"),
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }
}
