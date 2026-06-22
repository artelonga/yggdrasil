//! Passkey (WebAuthn) ceremonies (YG-174) — biometria/security-key real.
//!
//! - `POST /api/v1/auth/passkey/register/start`  (autenticado) → opções p/ criar
//! - `POST /api/v1/auth/passkey/register/finish` (autenticado) → grava a credencial
//! - `POST /api/v1/auth/passkey/login/start`     (anon, e-mail dica) → desafio
//! - `POST /api/v1/auth/passkey/login/finish`    (anon) → verifica → emite JWT
//!
//! O estado efêmero das cerimônias vive em memória (curto TTL), chaveado por um
//! id devolvido ao cliente. O login por passkey **emite o JWT local** via
//! `auth::sign_jwt` (mesmo token do handover do CO) — sem round-trip ao CO.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webauthn_rs::Webauthn;
use webauthn_rs::prelude::{
    PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
};

use crate::auth::jwt::verify_jwt_full;
use crate::auth::passkey::PasskeyDb;

/// Namespace estável p/ derivar o `user_unique_id` (Uuid) a partir do `sub`.
const NS: Uuid = Uuid::from_bytes([
    0x7e, 0x67, 0x67, 0x64, 0x72, 0x61, 0x73, 0x69, 0x6c, 0x70, 0x61, 0x73, 0x73, 0x6b, 0x65, 0x79,
]);

fn uid_for(sub: &str) -> Uuid {
    Uuid::new_v5(&NS, sub.as_bytes())
}

pub struct PasskeyState {
    pub webauthn: Arc<Webauthn>,
    pub db: Arc<PasskeyDb>,
    pub jwt_secret: String,
    /// Estado das cerimônias em curso (id → (estado, instante)). TTL curto.
    pub reg: Mutex<HashMap<String, (PasskeyRegistration, std::time::Instant)>>,
    pub auth: Mutex<HashMap<String, (PasskeyAuthentication, std::time::Instant)>>,
}

const CEREMONY_TTL: std::time::Duration = std::time::Duration::from_secs(300);

impl PasskeyState {
    fn new_id() -> String {
        Uuid::new_v4().simple().to_string()
    }
    fn gc<T>(map: &mut HashMap<String, (T, std::time::Instant)>) {
        map.retain(|_, (_, t)| t.elapsed() < CEREMONY_TTL);
    }
}

#[derive(Serialize)]
struct Err_ {
    error: String,
}
fn err(code: StatusCode, msg: &str) -> axum::response::Response {
    (
        code,
        Json(Err_ {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

/// `sub`+`email` do JWT no header `Authorization: Bearer`, ou `None` se ausente/inválido.
fn caller(state_secret: &str, headers: &HeaderMap) -> Option<(String, String)> {
    let tok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;
    verify_jwt_full(tok, state_secret)
        .ok()
        .map(|c| (c.sub, c.email))
}

// ── REGISTRO (autenticado) ──────────────────────────────────────────────────

pub async fn register_start(
    State(st): State<Arc<PasskeyState>>,
    headers: HeaderMap,
) -> axum::response::Response {
    let (sub, email) = match caller(&st.jwt_secret, &headers) {
        Some(c) => c,
        None => return err(StatusCode::UNAUTHORIZED, "nao_autenticado"),
    };
    // exclui credenciais já registradas p/ este usuário (evita duplicar)
    let existing: Vec<_> = st
        .db
        .for_email(&email)
        .iter()
        .map(|p| p.cred_id().clone())
        .collect();
    let exclude = if existing.is_empty() {
        None
    } else {
        Some(existing)
    };
    match st
        .webauthn
        .start_passkey_registration(uid_for(&sub), &email, &email, exclude)
    {
        Ok((ccr, reg_state)) => {
            let id = PasskeyState::new_id();
            let mut map = st.reg.lock().unwrap();
            PasskeyState::gc(&mut map);
            map.insert(id.clone(), (reg_state, std::time::Instant::now()));
            (
                StatusCode::OK,
                Json(serde_json::json!({ "id": id, "options": ccr })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("passkey register start: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "erro_registro")
        }
    }
}

#[derive(Deserialize)]
pub struct RegisterFinish {
    pub id: String,
    pub credential: RegisterPublicKeyCredential,
    #[serde(default)]
    pub label: Option<String>,
}

pub async fn register_finish(
    State(st): State<Arc<PasskeyState>>,
    headers: HeaderMap,
    Json(body): Json<RegisterFinish>,
) -> axum::response::Response {
    let (sub, email) = match caller(&st.jwt_secret, &headers) {
        Some(c) => c,
        None => return err(StatusCode::UNAUTHORIZED, "nao_autenticado"),
    };
    let reg_state = {
        let mut map = st.reg.lock().unwrap();
        PasskeyState::gc(&mut map);
        match map.remove(&body.id) {
            Some((s, _)) => s,
            None => return err(StatusCode::BAD_REQUEST, "cerimonia_expirada"),
        }
    };
    match st
        .webauthn
        .finish_passkey_registration(&body.credential, &reg_state)
    {
        Ok(pk) => {
            st.db.save(&sub, &email, &pk, body.label.as_deref());
            (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        Err(e) => {
            tracing::warn!("passkey register finish: {e}");
            err(StatusCode::BAD_REQUEST, "registro_invalido")
        }
    }
}

// ── LOGIN (anônimo, dica de e-mail) ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginStart {
    pub email: String,
}

pub async fn login_start(
    State(st): State<Arc<PasskeyState>>,
    Json(body): Json<LoginStart>,
) -> axum::response::Response {
    let email = body.email.trim().to_lowercase();
    let creds = st.db.for_email(&email);
    if creds.is_empty() {
        return err(StatusCode::NOT_FOUND, "sem_passkey");
    }
    match st.webauthn.start_passkey_authentication(&creds) {
        Ok((rcr, auth_state)) => {
            let id = PasskeyState::new_id();
            let mut map = st.auth.lock().unwrap();
            PasskeyState::gc(&mut map);
            map.insert(id.clone(), (auth_state, std::time::Instant::now()));
            (
                StatusCode::OK,
                Json(serde_json::json!({ "id": id, "options": rcr })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("passkey login start: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "erro_login")
        }
    }
}

#[derive(Deserialize)]
pub struct LoginFinish {
    pub id: String,
    pub credential: PublicKeyCredential,
}

pub async fn login_finish(
    State(st): State<Arc<PasskeyState>>,
    Json(body): Json<LoginFinish>,
) -> axum::response::Response {
    let auth_state = {
        let mut map = st.auth.lock().unwrap();
        PasskeyState::gc(&mut map);
        match map.remove(&body.id) {
            Some((s, _)) => s,
            None => return err(StatusCode::BAD_REQUEST, "cerimonia_expirada"),
        }
    };
    let res = match st
        .webauthn
        .finish_passkey_authentication(&body.credential, &auth_state)
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("passkey login finish: {e}");
            return err(StatusCode::UNAUTHORIZED, "login_invalido");
        }
    };
    // identifica o dono pela credencial e emite o JWT local
    let (owner, mut pk) = match st.db.owner_of(res.cred_id()) {
        Some(x) => x,
        None => return err(StatusCode::UNAUTHORIZED, "credencial_desconhecida"),
    };
    // atualiza o contador de assinatura (detecta clonagem); persiste se mudou
    if pk.update_credential(&res).unwrap_or(false) {
        st.db.update(&pk);
    }
    match crate::auth::sign_jwt(&owner.sub, &owner.email, &st.jwt_secret) {
        Ok(token) => (StatusCode::OK, Json(serde_json::json!({ "token": token }))).into_response(),
        Err(e) => {
            tracing::error!("passkey sign jwt: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, "erro_sessao")
        }
    }
}
