//! Cross-apex SSO via CO handover (YG-N).
//!
//! Fluxo:
//!   1. User clica "Entrar com CO" no /login do Yggdrasil.
//!   2. Browser → `https://co.artelonga.com.br/auth/co-handover?return_to=https://<ygg>/auth/co-handover-receive?next=/lobby`
//!   3. CO autentica (se necessário), assina JWT ES256 60s, redirect:
//!      `https://<ygg>/auth/co-handover-receive?next=/lobby&co_token=<jwt>`
//!   4. Yggdrasil verifica `co_token` via JWKS de CO (fetch + cache em
//!      `JwksCache`), e mintar JWT local HS256 com o mesmo `sub`/`email`.
//!   5. Browser recebe HTML que armazena JWT em localStorage e navega
//!      para `next`.
//!
//! Sem segredo compartilhado entre CO e Yggdrasil — só a public key
//! exposta pelo JWKS de CO. Rotação de chave em CO requer apenas que o
//! cache do Yggdrasil expire (default 1 hora) ou seja explicitly invalidado.

use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// URL pública do servidor CO. Pode ser sobrescrito via env var `CO_BASE_URL`
/// para apontar a UAT, staging, ou um servidor local em dev.
pub fn co_base_url() -> String {
    std::env::var("CO_BASE_URL").unwrap_or_else(|_| "https://co.artelonga.com.br".to_string())
}

pub fn co_jwks_url() -> String {
    format!("{}/.well-known/jwks.json", co_base_url())
}

/// Feature-gate do round-trip editável (YG-124). O botão "Editar no CO" só
/// aparece quando o CO está **bidirecional** — aceita writes nas entradas
/// federadas e re-emite `entry.updated` de volta ao hub (CO-413). Enquanto o CO
/// segue read-only (`AppError::ReadOnlyUniverse`) o gate fica `false` e o botão
/// nem é exposto, evitando prometer um round-trip que ainda não fecha.
///
/// Liga via `YGG_CO_EDITOR=1` (também aceita `true`/`yes`/`on`). Default off:
/// ops liga só depois que CO-413 estiver em prod.
pub fn co_editor_enabled() -> bool {
    std::env::var("YGG_CO_EDITOR")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// TTL do cache de JWKS. Curto o suficiente para pegar rotação de chave em CO,
/// longo o suficiente para não martelar `/.well-known/jwks.json` a cada request.
const JWKS_TTL: Duration = Duration::from_secs(3600);

/// Timeout do fetch de JWKS. Servidor CO está em GRU; deveria ser <100ms
/// numa boa rede. 5s é generoso para tolerar transientes.
const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwk {
    pub kty: String,
    pub crv: String,
    pub kid: String,
    pub x: String,
    pub y: String,
    #[serde(default)]
    pub alg: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct JwksResponse {
    pub keys: Vec<Jwk>,
}

#[derive(Debug)]
struct CachedJwks {
    keys: Vec<Jwk>,
    fetched_at: Instant,
}

/// Claims que esperamos no JWT de handover do CO. Mantemos compatibilidade
/// com o shape de `co::Claims` mas usamos `#[serde(default)]` para os campos
/// que podem estar vazios. `tier`/`iat` deserializam mas não são usados
/// hoje — guardados para futura propagação no JWT local.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct CoHandoverClaims {
    pub sub: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub tier: String,
    pub exp: usize,
    #[serde(default)]
    pub iat: usize,
}

/// Cache thread-safe de JWKS. Compartilhado via `Arc<JwksCache>` no
/// estado da aplicação.
pub struct JwksCache {
    inner: RwLock<Option<CachedJwks>>,
    http: reqwest::Client,
    jwks_url: String,
}

impl JwksCache {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(JWKS_FETCH_TIMEOUT)
            .build()
            .expect("reqwest client");
        Self {
            inner: RwLock::new(None),
            http,
            jwks_url: co_jwks_url(),
        }
    }

    /// Constructor para testes — permite URL customizado (mock server).
    #[cfg(test)]
    pub fn with_url(url: String) -> Self {
        Self {
            inner: RwLock::new(None),
            http: reqwest::Client::builder()
                .timeout(JWKS_FETCH_TIMEOUT)
                .build()
                .expect("reqwest client"),
            jwks_url: url,
        }
    }

    /// Retorna o JWK que corresponde ao `kid` do header de um JWT. Refresca
    /// o cache se estiver expirado ou se o `kid` não estiver no cache atual
    /// (cobre rotação imediata).
    pub async fn get_key(&self, kid: &str) -> Result<Jwk, JwksError> {
        // Try cache first
        {
            let guard = self.inner.read().await;
            if let Some(cached) = guard.as_ref() {
                let fresh = cached.fetched_at.elapsed() < JWKS_TTL;
                if let Some(k) = cached.keys.iter().find(|k| k.kid == kid)
                    && fresh
                {
                    return Ok(k.clone());
                }
                // Either expired OR kid not found — refresh below.
            }
        }
        // Refresh
        self.refresh().await?;
        let guard = self.inner.read().await;
        let cached = guard.as_ref().ok_or(JwksError::CacheEmpty)?;
        cached
            .keys
            .iter()
            .find(|k| k.kid == kid)
            .cloned()
            .ok_or_else(|| JwksError::KidNotFound(kid.to_string()))
    }

    async fn refresh(&self) -> Result<(), JwksError> {
        let resp = self
            .http
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| JwksError::Fetch(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(JwksError::Fetch(format!("status {}", resp.status())));
        }
        let body: JwksResponse = resp
            .json()
            .await
            .map_err(|e| JwksError::Parse(e.to_string()))?;
        let mut guard = self.inner.write().await;
        *guard = Some(CachedJwks {
            keys: body.keys,
            fetched_at: Instant::now(),
        });
        tracing::info!("JWKS refreshed from {}", self.jwks_url);
        Ok(())
    }
}

impl Default for JwksCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JwksError {
    #[error("Erro buscando JWKS: {0}")]
    Fetch(String),
    #[error("JWKS inválido: {0}")]
    Parse(String),
    #[error("kid '{0}' não encontrado em JWKS")]
    KidNotFound(String),
    #[error("Cache de JWKS vazio")]
    CacheEmpty,
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("Header JWT inválido: {0}")]
    BadHeader(String),
    #[error("kid ausente no JWT")]
    MissingKid,
    #[error(transparent)]
    Jwks(#[from] JwksError),
    #[error("Assinatura inválida: {0}")]
    Signature(String),
    #[error("JWK não-EC ou crv ≠ P-256 (got kty={kty}, crv={crv})")]
    UnsupportedJwk { kty: String, crv: String },
}

/// Verifica um JWT ES256 emitido por CO. Retorna as claims se válido.
pub async fn verify_co_token(
    token: &str,
    cache: &Arc<JwksCache>,
) -> Result<CoHandoverClaims, VerifyError> {
    let header = decode_header(token).map_err(|e| VerifyError::BadHeader(e.to_string()))?;
    let kid = header.kid.ok_or(VerifyError::MissingKid)?;
    let jwk = cache.get_key(&kid).await?;
    if jwk.kty != "EC" || jwk.crv != "P-256" {
        return Err(VerifyError::UnsupportedJwk {
            kty: jwk.kty,
            crv: jwk.crv,
        });
    }
    let decoding_key = DecodingKey::from_ec_components(&jwk.x, &jwk.y)
        .map_err(|e| VerifyError::Signature(e.to_string()))?;
    let mut validation = Validation::new(Algorithm::ES256);
    validation.validate_exp = true;
    let data = decode::<CoHandoverClaims>(token, &decoding_key, &validation)
        .map_err(|e| VerifyError::Signature(e.to_string()))?;
    Ok(data.claims)
}

/// URL para o botão "Continuar com Google" do Yggdrasil.
///
/// Aponta para o endpoint de início de OAuth Google do CO (não para a tela
/// de login do CO). CO faz a dança OAuth com Google, cria/atualiza a conta
/// no CO, e — porque `return_to` contém `/auth/co-handover` — anexa um
/// `co_token` ES256 no redirect final via `maybe_attach_co_handover_token`.
/// Nosso receiver `/auth/co-handover-receive` valida via JWKS e mintar a
/// sessão local.
///
/// Espelha o padrão usado em `quilomboaraucaria/web/src/routes/cadastro/+page.svelte`:
/// usuário entra direto via Google sem ter passado pela tela de login do CO.
pub fn co_login_url(yggdrasil_base: &str, next: Option<&str>) -> String {
    let return_to = match next {
        Some(n) => format!(
            "{}/auth/co-handover-receive?next={}",
            yggdrasil_base,
            urlencoding(n)
        ),
        None => format!("{}/auth/co-handover-receive", yggdrasil_base),
    };
    format!(
        "{}/api/v1/auth/google/start?return_to={}",
        co_base_url(),
        urlencoding(&return_to)
    )
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn co_base_url_default() {
        // Não setando env var
        unsafe {
            std::env::remove_var("CO_BASE_URL");
        }
        assert_eq!(co_base_url(), "https://co.artelonga.com.br");
    }

    #[test]
    fn co_jwks_url_default() {
        unsafe {
            std::env::remove_var("CO_BASE_URL");
        }
        assert_eq!(
            co_jwks_url(),
            "https://co.artelonga.com.br/.well-known/jwks.json"
        );
    }

    #[test]
    fn co_editor_gate_off_por_padrao_on_com_flag() {
        // YG-124: read-only por padrão (botão "Editar no CO" não aparece);
        // liga só com a flag explícita, em variantes truthy.
        unsafe {
            std::env::remove_var("YGG_CO_EDITOR");
        }
        assert!(!co_editor_enabled(), "default deve ser off (CO read-only)");
        for truthy in ["1", "true", "TRUE", " yes ", "on"] {
            unsafe {
                std::env::set_var("YGG_CO_EDITOR", truthy);
            }
            assert!(co_editor_enabled(), "{truthy:?} deveria ligar o gate");
        }
        for falsy in ["0", "false", "", "nope"] {
            unsafe {
                std::env::set_var("YGG_CO_EDITOR", falsy);
            }
            assert!(!co_editor_enabled(), "{falsy:?} não deveria ligar o gate");
        }
        unsafe {
            std::env::remove_var("YGG_CO_EDITOR");
        }
    }

    #[test]
    fn urlencoding_escapa_caracteres_especiais() {
        assert_eq!(urlencoding("/lobby"), "%2Flobby");
        assert_eq!(urlencoding("foo bar"), "foo%20bar");
        assert_eq!(urlencoding("ASCII-letters_99.~"), "ASCII-letters_99.~");
    }

    #[test]
    fn co_login_url_com_next_codifica_corretamente() {
        unsafe {
            std::env::remove_var("CO_BASE_URL");
        }
        let url = co_login_url("https://yggdrasil.test", Some("/lobby"));
        assert!(
            url.starts_with("https://co.artelonga.com.br/api/v1/auth/google/start?return_to="),
            "url={url}"
        );
        // `next=/lobby` é encodado uma vez (%2Flobby) e depois encodado de
        // novo ao ser embarcado no return_to externo (%252Flobby).
        assert!(url.contains("next%3D%252Flobby"), "url={url}");
    }

    #[test]
    fn co_login_url_sem_next_omite_query_string() {
        unsafe {
            std::env::remove_var("CO_BASE_URL");
        }
        let url = co_login_url("https://yggdrasil.test", None);
        assert!(url.contains("co-handover-receive"));
        assert!(!url.contains("next%3D"));
    }

    #[test]
    fn co_login_url_aponta_para_google_start_nao_para_login() {
        // Regressão: usuário não deve ser redirecionado para tela de login
        // do CO. O fluxo correto é Google OAuth → cria conta CO → handover.
        unsafe {
            std::env::remove_var("CO_BASE_URL");
        }
        let url = co_login_url("https://yggdrasil.test", None);
        assert!(url.contains("/api/v1/auth/google/start"), "url={url}");
        assert!(!url.contains("/auth/co-handover?"), "url={url}");
    }
}
