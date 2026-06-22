use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use super::state::AuthState;

const JWT_EXPIRY_SECS: i64 = 604_800;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)]
    pub email: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct UserId(pub String);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for UserId {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let result = parts
            .extensions
            .get::<UserId>()
            .cloned()
            .ok_or_else(|| unauthorized("Não autenticado"));
        std::future::ready(result)
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
struct ErrorBody {
    error: String,
}

#[allow(dead_code)]
pub(super) fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorBody {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(token_data.claims.sub)
}

/// Como [`verify_jwt`], mas devolve os claims completos (sub + email) — usado no
/// registro de passkey (YG-174), que amarra a credencial ao e-mail do usuário.
pub fn verify_jwt_full(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(token_data.claims)
}

pub fn sign_jwt(user_id: &str, email: &str, secret: &str) -> anyhow::Result<String> {
    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now.timestamp() + JWT_EXPIRY_SECS) as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        exp,
        iat,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

#[allow(dead_code)]
pub async fn require_auth(
    State(state): State<Arc<AuthState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| unauthorized("Cabeçalho Authorization ausente ou inválido"))?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &validation,
    )
    .map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => unauthorized("Token expirado"),
        _ => unauthorized("Token inválido"),
    })?;

    req.extensions_mut().insert(UserId(token_data.claims.sub));
    Ok(next.run(req).await)
}
