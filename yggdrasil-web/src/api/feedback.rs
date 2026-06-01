//! `POST /api/v1/feedback` — Fale conosco. Aceita mensagens anônimas ou de
//! usuários logados (JWT opcional: se presente e válido → `user_sub`). Nome e
//! e-mail são opcionais. Validação num único ponto antes de gravar.

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::auth::verify_jwt;
use crate::feedback::{FeedbackDb, KINDS, NewFeedback, PublicFeedback};

/// Máximo de mensagens devolvidas na listagem pública.
const LIST_LIMIT: usize = 200;

const MSG_MAX: usize = 5_000;
const NAME_MAX: usize = 120;
const EMAIL_MAX: usize = 200;
const UNIVERSE_MAX: usize = 64;

#[derive(Clone)]
pub struct FeedbackState {
    pub jwt_secret: String,
    pub db: Arc<FeedbackDb>,
}

#[derive(Deserialize)]
pub struct FeedbackBody {
    #[serde(default)]
    pub universe: String,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Serialize)]
pub struct FeedbackOk {
    pub ok: bool,
    pub id: String,
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

/// Limpa um campo opcional: trim, vira `None` se vazio, corta no limite.
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

pub async fn submit_feedback(
    State(state): State<Arc<FeedbackState>>,
    headers: HeaderMap,
    Json(body): Json<FeedbackBody>,
) -> axum::response::Response {
    // Auth opcional → distingue anônimo de usuário.
    let user_sub = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| verify_jwt(t, &state.jwt_secret).ok());

    let kind = body.kind.trim().to_lowercase();
    if !KINDS.contains(&kind.as_str()) {
        return bad("tipo_invalido");
    }

    let message = body.message.trim().to_string();
    if message.is_empty() {
        return bad("mensagem_vazia");
    }
    if message.chars().count() > MSG_MAX {
        return bad("mensagem_muito_longa");
    }

    let name = clean(body.name, NAME_MAX);
    let email = clean(body.email, EMAIL_MAX);
    // Validação leve de e-mail (só se informado): tem `@` com algo dos dois lados.
    if let Some(ref e) = email {
        let parts: Vec<&str> = e.split('@').collect();
        if parts.len() != 2 || parts[0].is_empty() || !parts[1].contains('.') {
            return bad("email_invalido");
        }
    }

    let universe = clean(Some(body.universe), UNIVERSE_MAX).unwrap_or_else(|| "root".to_string());

    let id = state.db.submit(&NewFeedback {
        universe: &universe,
        kind: &kind,
        message: &message,
        name: name.as_deref(),
        email: email.as_deref(),
        user_sub: user_sub.as_deref(),
    });

    (StatusCode::CREATED, Json(FeedbackOk { ok: true, id })).into_response()
}

/// `GET /api/v1/feedback` — listagem pública (mais recentes primeiro).
/// Sem auth; **nunca** devolve e-mail nem `user_sub` (ver [`PublicFeedback`]).
pub async fn list_feedback(State(state): State<Arc<FeedbackState>>) -> Json<Vec<PublicFeedback>> {
    Json(state.db.list_public(LIST_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::sign_jwt;
    use crate::feedback::FeedbackDb;
    use axum::{Router, body::Body, http::Request, routing::post};
    use tower::ServiceExt;

    fn app() -> (Router, Arc<FeedbackState>) {
        let state = Arc::new(FeedbackState {
            jwt_secret: "dev".to_string(),
            db: Arc::new(FeedbackDb::in_memory().unwrap()),
        });
        let router = Router::new()
            .route("/api/v1/feedback", post(submit_feedback).get(list_feedback))
            .with_state(state.clone());
        (router, state)
    }

    async fn post_json(app: &Router, json: serde_json::Value, auth: Option<&str>) -> StatusCode {
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/v1/feedback")
            .header("content-type", "application/json");
        if let Some(t) = auth {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::from(json.to_string())).unwrap())
            .await
            .unwrap();
        resp.status()
    }

    #[tokio::test]
    async fn anonimo_201_e_grava_anonymous() {
        let (app, state) = app();
        let st = post_json(
            &app,
            serde_json::json!({"universe":"neuro","kind":"sugestao","message":"oi","email":"a@b.com"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        assert_eq!(state.db.count(), 1);
    }

    #[tokio::test]
    async fn logado_grava_user_sub() {
        let (app, state) = app();
        let token = sign_jwt("user-9", "u@e.com", "dev").unwrap();
        let st = post_json(
            &app,
            serde_json::json!({"universe":"root","kind":"feedback","message":"top"}),
            Some(&token),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        assert_eq!(state.db.count(), 1);
    }

    #[tokio::test]
    async fn tipo_invalido_400() {
        let (app, _) = app();
        let st = post_json(
            &app,
            serde_json::json!({"universe":"root","kind":"spam","message":"x"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn mensagem_vazia_400() {
        let (app, _) = app();
        let st = post_json(
            &app,
            serde_json::json!({"universe":"root","kind":"duvida","message":"   "}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn email_invalido_400() {
        let (app, _) = app();
        let st = post_json(
            &app,
            serde_json::json!({"universe":"root","kind":"feedback","message":"x","email":"nope"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_lista_publica_200_sem_email() {
        let (app, _) = app();
        post_json(
            &app,
            serde_json::json!({"universe":"root","kind":"feedback","message":"oi","name":"Ana","email":"ana@x.com"}),
            None,
        )
        .await;
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/feedback")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("Ana"), "deve mostrar o nome");
        assert!(!body.contains("ana@x.com"), "não pode vazar e-mail");
        assert!(!body.contains("email"));
    }

    #[tokio::test]
    async fn universe_default_root_quando_ausente() {
        let (app, state) = app();
        let st = post_json(
            &app,
            serde_json::json!({"kind":"feedback","message":"sem universo"}),
            None,
        )
        .await;
        assert_eq!(st, StatusCode::CREATED);
        assert_eq!(state.db.count(), 1);
    }
}
