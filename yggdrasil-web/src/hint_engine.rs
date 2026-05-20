//! Host-side LLM bridge — Claude hint engine for the Vim universo.
//!
//! When the Vim WASM module calls `request_hint(ctx_json_ptr, len)`, the host
//! enqueues the raw JSON via `HostState::hint_tx`. A background task picks it
//! up, calls [`HintEngine::request`], and writes the result into
//! `HostState::hint_result` for the next tick to return.
//!
//! If `ANTHROPIC_API_KEY` is absent **or** the user exceeds the rate limit
//! (5 hints / user / hour) the engine silently falls back to a static,
//! per-level hint in PT-BR. No error is logged in either case — the absence
//! of an API key is an expected configuration state.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Maximum Claude API calls per user per hour before falling back to static hints.
pub const MAX_HINTS_PER_HOUR: u32 = 5;

const MODEL: &str = "claude-sonnet-4-6";

// ---------------------------------------------------------------------------
// HintContext
// ---------------------------------------------------------------------------

/// Context emitted by the Vim universo WASM via `request_hint`.
///
/// The WASM module serialises this as JSON and passes the pointer + length to
/// the `request_hint` host import. The host deserialises it here.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HintContext {
    pub puzzle_id: String,
    pub puzzle_description: String,
    pub buffer_content: String,
    pub cursor: serde_json::Value,
    #[serde(default)]
    pub recent_commands: Vec<String>,
    pub attempt_count: u32,
    /// "pt" → PT-BR hint; anything else → English hint.
    #[serde(default)]
    pub description_lang: String,
}

// ---------------------------------------------------------------------------
// HintApi — injectable HTTP abstraction for testability
// ---------------------------------------------------------------------------

/// Trait over the Claude REST call so tests can inject a stub without
/// starting a real HTTP listener or setting `ANTHROPIC_API_KEY`.
pub trait HintApi: Send + Sync {
    fn complete(
        &self,
        prompt: String,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// ReqwestHintApi — production implementation
// ---------------------------------------------------------------------------

/// Calls the Anthropic Messages API using an existing `reqwest::Client`.
pub struct ReqwestHintApi {
    client: reqwest::Client,
    api_key: String,
}

impl ReqwestHintApi {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
        }
    }
}

impl HintApi for ReqwestHintApi {
    fn complete(
        &self,
        prompt: String,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>> {
        Box::pin(async move {
            let body = serde_json::json!({
                "model": MODEL,
                "max_tokens": 150,
                "messages": [{"role": "user", "content": prompt}]
            });

            let response = self
                .client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                anyhow::bail!("Claude API error: HTTP {}", response.status());
            }

            let resp: serde_json::Value = response.json().await?;
            let text = resp["content"][0]["text"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("unexpected Claude API response shape"))?
                .to_string();

            Ok(text)
        })
    }
}

// ---------------------------------------------------------------------------
// HintEngine
// ---------------------------------------------------------------------------

/// Host-side LLM bridge for the Vim universo hint system.
///
/// Create once at startup and share via `Arc<HintEngine>`. The engine is
/// `Send + Sync` and safe to call from multiple concurrent hint tasks.
pub struct HintEngine {
    api: Option<Box<dyn HintApi>>,
    rate_limits: Arc<DashMap<String, (u32, SystemTime)>>,
    /// Injectable clock; defaults to `SystemTime::now`. Override in tests.
    now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync>,
}

impl HintEngine {
    /// Creates an engine by reading `ANTHROPIC_API_KEY` from the environment.
    ///
    /// Logs `INFO` (not `WARN`) when the key is absent — falling back to
    /// static hints is a valid, expected configuration.
    pub fn from_env() -> Self {
        let api = std::env::var("ANTHROPIC_API_KEY").ok().map(|key| {
            tracing::info!("ANTHROPIC_API_KEY configured — Vim hints will use Claude API");
            Box::new(ReqwestHintApi::new(key)) as Box<dyn HintApi>
        });
        if api.is_none() {
            tracing::info!("ANTHROPIC_API_KEY not set — Vim hints will use static fallback");
        }
        Self {
            api,
            rate_limits: Arc::new(DashMap::new()),
            now_fn: Arc::new(SystemTime::now),
        }
    }

    /// Constructor with injectable API impl and clock — used in tests and
    /// future integration wiring (e.g. from YG-60 session routes).
    pub fn with_components(
        api: Option<Box<dyn HintApi>>,
        now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync>,
    ) -> Self {
        Self {
            api,
            rate_limits: Arc::new(DashMap::new()),
            now_fn,
        }
    }

    /// Returns a hint for `user_id` given the current puzzle `ctx`.
    ///
    /// Falls back to a static per-level hint (PT-BR) when:
    /// - `ANTHROPIC_API_KEY` is absent
    /// - the user has exceeded the rate limit (5 / hour)
    /// - the Claude API returns an error
    ///
    /// Never panics; errors are logged at `WARN` and swallowed.
    pub async fn request(&self, user_id: &str, ctx: &HintContext) -> String {
        if !self.check_rate_limit(user_id) {
            return static_hint(&ctx.puzzle_id);
        }

        match &self.api {
            None => static_hint(&ctx.puzzle_id),
            Some(api) => match api.complete(build_prompt(ctx)).await {
                Ok(hint) => hint,
                Err(e) => {
                    warn!("Claude API error for hint (user={user_id}): {e}");
                    static_hint(&ctx.puzzle_id)
                }
            },
        }
    }

    /// Returns `true` and increments the counter when under the rate limit.
    /// Returns `false` (without incrementing) when the limit is reached.
    fn check_rate_limit(&self, user_id: &str) -> bool {
        let now = (self.now_fn)();
        let window = Duration::from_secs(3600);

        let mut entry = self
            .rate_limits
            .entry(user_id.to_string())
            .or_insert_with(|| (0u32, now));

        let (count, window_start) = &mut *entry;

        if now.duration_since(*window_start).unwrap_or_default() >= window {
            *count = 0;
            *window_start = now;
        }

        if *count >= MAX_HINTS_PER_HOUR {
            return false;
        }

        *count += 1;
        true
    }
}

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

/// Builds the Claude prompt including all required context fields.
pub fn build_prompt(ctx: &HintContext) -> String {
    let lang = if ctx.description_lang == "pt" {
        "PT-BR"
    } else {
        "English"
    };
    let commands = ctx
        .recent_commands
        .iter()
        .take(5)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "You are a helpful Vim tutor. The player is stuck on a Vim puzzle.\n\n\
         Puzzle: {description}\n\
         Buffer:\n```\n{buffer}\n```\n\
         Cursor: {cursor}\n\
         Recent commands: {commands}\n\
         Attempts: {attempts}\n\n\
         Give a concise hint in {lang} (2-3 sentences). \
         Guide without revealing the full solution.",
        description = ctx.puzzle_description,
        buffer = ctx.buffer_content,
        cursor = ctx.cursor,
        commands = commands,
        attempts = ctx.attempt_count,
        lang = lang,
    )
}

// ---------------------------------------------------------------------------
// Static per-level hints (PT-BR) — Vim universo 10 levels
// ---------------------------------------------------------------------------

/// Returns a hardcoded PT-BR hint for the given puzzle ID.
///
/// Covers levels 1–10 of the Vim universo. Unknown IDs get a generic hint.
pub fn static_hint(puzzle_id: &str) -> String {
    let level: u32 = puzzle_id
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .unwrap_or(0);

    match level {
        1 => {
            "Use `i` para entrar no modo de inserção e digitar o texto. \
              Pressione `Esc` para voltar ao modo normal."
        }
        2 => {
            "Use `dd` para deletar a linha inteira, \
              ou `x` para deletar um caractere de cada vez."
        }
        3 => {
            "Navegue com `h`, `j`, `k`, `l` — esquerda, baixo, cima, direita. \
              Combine com números: `5j` avança 5 linhas."
        }
        4 => {
            "Use `w` para avançar uma palavra, `b` para voltar. \
              `0` vai ao início da linha, `$` ao fim."
        }
        5 => {
            "Use `yy` para copiar a linha e `p` para colar abaixo do cursor. \
              `P` cola acima."
        }
        6 => {
            "Use `u` para desfazer (undo) e `Ctrl-r` para refazer (redo). \
              Você pode desfazer várias vezes seguidas."
        }
        7 => {
            "O comando `ciw` muda a palavra inteira sob o cursor. \
              `cw` muda do cursor até o fim da palavra."
        }
        8 => {
            "Use `/palavra` para buscar no arquivo. \
              `n` vai para a próxima ocorrência, `N` para a anterior."
        }
        9 => {
            "`:s/velho/novo/g` substitui todas as ocorrências na linha. \
              `:%s/velho/novo/g` substitui no arquivo inteiro."
        }
        10 => {
            "Macros: `qa` inicia gravação na macro `a`; execute os comandos; \
               `q` para. Use `@a` para reproduzir a macro."
        }
        _ => {
            "Lembre dos modos do Vim: Normal (Esc), Inserção (i/a/o), Visual (v). \
              Em modo Normal, cada tecla é um comando."
        }
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn fixed_clock(t: SystemTime) -> Arc<dyn Fn() -> SystemTime + Send + Sync> {
        Arc::new(move || t)
    }

    fn make_ctx(puzzle_id: &str) -> HintContext {
        HintContext {
            puzzle_id: puzzle_id.to_string(),
            puzzle_description: "Delete the word under the cursor".to_string(),
            buffer_content: "hello world\n".to_string(),
            cursor: serde_json::json!({"line": 1, "col": 1}),
            recent_commands: vec!["w".to_string(), "b".to_string()],
            attempt_count: 3,
            description_lang: "pt".to_string(),
        }
    }

    // ------------------------------------------------------------------
    // Static fallback
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn no_api_key_returns_static_hint() {
        let engine = HintEngine::with_components(None, fixed_clock(SystemTime::UNIX_EPOCH));
        let hint = engine.request("user-a", &make_ctx("vim_1")).await;
        assert!(!hint.is_empty(), "static hint must not be empty");
        assert!(
            hint.contains('`'),
            "static hint should contain Vim command notation"
        );
    }

    #[tokio::test]
    async fn static_hint_covers_all_10_levels() {
        for level in 1u32..=10 {
            let id = format!("vim_{level}");
            let hint = static_hint(&id);
            assert!(
                !hint.is_empty(),
                "level {level} must have a non-empty static hint"
            );
        }
    }

    // ------------------------------------------------------------------
    // Rate limiting (clock injected to avoid real time dependency)
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn rate_limit_blocks_sixth_request_without_calling_api() {
        struct CountingApi(Arc<AtomicU32>);
        impl HintApi for CountingApi {
            fn complete(
                &self,
                _prompt: String,
            ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + '_>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok("dica do claude".to_string()) })
            }
        }

        let call_count = Arc::new(AtomicU32::new(0));
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let engine = HintEngine::with_components(
            Some(Box::new(CountingApi(call_count.clone()))),
            fixed_clock(now),
        );
        let ctx = make_ctx("vim_2");

        for _ in 0..5 {
            engine.request("user-rl", &ctx).await;
        }
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            5,
            "first 5 requests must call the API"
        );

        // 6th request must NOT call the API
        let hint = engine.request("user-rl", &ctx).await;
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            5,
            "6th request must not call Claude API"
        );
        assert!(!hint.is_empty(), "6th request still returns a static hint");
    }

    #[tokio::test]
    async fn rate_limit_resets_after_one_hour() {
        let base = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
        let clock = Arc::new(std::sync::Mutex::new(base));
        let clock2 = clock.clone();
        let now_fn: Arc<dyn Fn() -> SystemTime + Send + Sync> =
            Arc::new(move || *clock2.lock().unwrap());

        let engine = HintEngine::with_components(None, now_fn);
        let ctx = make_ctx("vim_3");

        for _ in 0..MAX_HINTS_PER_HOUR {
            engine.request("user-reset", &ctx).await;
        }

        // Advance clock past the 1-hour window
        *clock.lock().unwrap() = base + Duration::from_secs(3601);

        engine.request("user-reset", &ctx).await;
        let count = engine
            .rate_limits
            .get("user-reset")
            .map(|e| (*e).0)
            .unwrap_or(0);
        assert_eq!(count, 1, "count must reset to 1 after window expiry");
    }

    #[tokio::test]
    async fn rate_limit_independent_per_user() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_000_000);
        let engine = HintEngine::with_components(None, fixed_clock(now));
        let ctx = make_ctx("vim_4");

        // Exhaust user-a
        for _ in 0..MAX_HINTS_PER_HOUR {
            engine.request("user-a", &ctx).await;
        }

        // user-b is unaffected
        let count_b = engine
            .rate_limits
            .get("user-b")
            .map(|e| (*e).0)
            .unwrap_or(0);
        assert_eq!(count_b, 0, "rate limit must be per-user");
    }

    // ------------------------------------------------------------------
    // Prompt content
    // ------------------------------------------------------------------

    #[test]
    fn prompt_contains_all_required_context_fields() {
        let ctx = HintContext {
            puzzle_id: "vim_test".to_string(),
            puzzle_description: "Move cursor to end of line".to_string(),
            buffer_content: "foo bar baz".to_string(),
            cursor: serde_json::json!({"line": 1, "col": 5}),
            recent_commands: vec!["j".to_string(), "k".to_string(), "l".to_string()],
            attempt_count: 7,
            description_lang: "pt".to_string(),
        };
        let prompt = build_prompt(&ctx);

        assert!(
            prompt.contains("foo bar baz"),
            "buffer content missing from prompt"
        );
        assert!(
            prompt.contains("Move cursor to end of line"),
            "puzzle description missing from prompt"
        );
        assert!(prompt.contains('7'), "attempt count missing from prompt");
        assert!(
            prompt.contains("PT-BR"),
            "language direction missing from prompt"
        );
        assert!(prompt.contains('j'), "recent commands missing from prompt");
        assert!(prompt.contains("k"), "recent commands truncated at <5");
    }

    #[test]
    fn prompt_uses_english_when_lang_not_pt() {
        let ctx = HintContext {
            description_lang: "en".to_string(),
            ..make_ctx("vim_5")
        };
        let prompt = build_prompt(&ctx);
        assert!(
            prompt.contains("English"),
            "non-pt lang must request English hint"
        );
        assert!(!prompt.contains("PT-BR"));
    }
}
