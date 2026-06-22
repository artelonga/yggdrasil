//! YG-146: POST /api/v1/npc — NPC com Ollama (env-gated) + fallback determinístico.
//!
//! Sem `OLLAMA_URL`, responde por keyword-match sobre `tutorial.md` embutido.
//! Com `OLLAMA_URL`, tenta `POST {url}/api/chat` (OpenAI-compat Ollama) com
//! `tutorial.md` como system prompt + `universo` como contexto; cai no
//! determinístico em qualquer falha (timeout 15s, nenhum retry).

use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const TUTORIAL_MD: &str = include_str!("../../static/universos/mundo/tutorial.md");

#[derive(Clone)]
pub struct NpcState {
    /// `OLLAMA_URL` — ex.: `http://localhost:11434`. Ausente ⇒ só determinístico.
    ollama_url: Option<String>,
    /// `OLLAMA_MODEL` — padrão `qwen2.5-coder:14b` (coincide com o local-inference).
    model: String,
    client: reqwest::Client,
}

impl NpcState {
    pub fn from_env() -> Self {
        Self {
            ollama_url: std::env::var("OLLAMA_URL").ok(),
            model: std::env::var("OLLAMA_MODEL")
                .unwrap_or_else(|_| "qwen2.5-coder:14b".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client para NPC"),
        }
    }
}

#[derive(Deserialize)]
pub struct NpcRequest {
    pub q: String,
    #[serde(default)]
    pub universo: String,
}

#[derive(Serialize)]
pub struct NpcResponse {
    pub answer: String,
    pub source: &'static str,
}

pub async fn post_npc(
    State(state): State<Arc<NpcState>>,
    Json(req): Json<NpcRequest>,
) -> impl IntoResponse {
    let q = req.q.trim().to_string();
    if q.is_empty() {
        return Json(NpcResponse {
            answer: "Pode me perguntar algo?".into(),
            source: "deterministic",
        });
    }

    if let Some(base) = &state.ollama_url {
        let system = format!(
            "Você é o Guia — NPC tutorial de um universo 2D navegável (Mundo). \
             Responda em português, de forma breve e amigável. \
             Contexto: universo `{}`.\n\n{TUTORIAL_MD}",
            req.universo
        );
        let url = format!("{base}/api/chat");
        let body = serde_json::json!({
            "model": state.model,
            "stream": false,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user",   "content": q },
            ],
        });
        if let Ok(resp) = state.client.post(&url).json(&body).send().await
            && resp.status().is_success()
            && let Ok(v) = resp.json::<serde_json::Value>().await
            && let Some(ans) = v["message"]["content"].as_str()
        {
            return Json(NpcResponse {
                answer: ans.to_string(),
                source: "llm",
            });
        }
        tracing::debug!("npc: ollama indisponível ou resposta inesperada — usando fallback");
    }

    Json(NpcResponse {
        answer: deterministic(&q),
        source: "deterministic",
    })
}

/// Keyword-match sobre `tutorial.md`: escolhe o tópico com maior sobreposição.
fn deterministic(q: &str) -> String {
    let q_norm = normalize(q);
    let qwords: Vec<&str> = q_norm
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .collect();

    let mut best_score = 0usize;
    let mut best_body = "";
    for (title, body) in parse_tutorial() {
        let hay = normalize(&format!("{title} {body}"));
        let score = qwords.iter().filter(|w| hay.contains(**w)).count();
        if score > best_score {
            best_score = score;
            best_body = body;
        }
    }

    if best_score > 0 {
        best_body.to_string()
    } else {
        "Não sei isso ainda — escolha um tópico no menu. \
         Com o LLM local (Ollama) ligado, respondo perguntas livres."
            .to_string()
    }
}

/// Transforma em ASCII minúsculo (strip de acentos simples).
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ã' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

/// Parseia `## Título\nbody\n## ...` do tutorial.md embutido.
/// Retorna `Vec<(&'static str, &'static str)>` usando slices do `TUTORIAL_MD`.
fn parse_tutorial() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    let mut title_start = 0usize;
    let mut title_end = 0usize;
    let mut in_topic = false;
    let mut body_start = 0usize;

    let bytes = TUTORIAL_MD.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // find end of line
        let line_start = i;
        while i < bytes.len() && bytes[i] != b'\n' {
            i += 1;
        }
        let line_end = i;
        if i < bytes.len() {
            i += 1; // skip \n
        }
        let line = &TUTORIAL_MD[line_start..line_end];
        if let Some(rest) = line.strip_prefix("## ") {
            if in_topic {
                let body = TUTORIAL_MD[body_start..line_start].trim();
                out.push((&TUTORIAL_MD[title_start..title_end], body));
            }
            let rest_start = line_start + 3;
            title_start = rest_start;
            title_end = rest_start + rest.len();
            body_start = i; // after the \n of the ## line
            in_topic = true;
        }
    }
    if in_topic {
        let body = TUTORIAL_MD[body_start..].trim();
        out.push((&TUTORIAL_MD[title_start..title_end], body));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tutorial_returns_topics() {
        let topics = parse_tutorial();
        assert!(!topics.is_empty(), "deve ter tópicos do tutorial.md");
        assert!(
            topics.iter().any(|(t, _)| t.contains("Como ando")),
            "deve ter tópico de movimento"
        );
    }

    #[test]
    fn deterministic_responde_a_movimento() {
        let r = deterministic("como ando pela sala");
        assert!(r.contains("WASD") || r.contains("andar") || r.contains("setas"));
    }

    #[test]
    fn deterministic_fallback_sem_match() {
        let r = deterministic("xyz qwerty");
        assert!(!r.is_empty());
        assert!(r.contains("tópico") || r.contains("Ollama") || r.contains("LLM"));
    }

    #[test]
    fn normalize_strips_accents() {
        assert_eq!(normalize("áéíóú"), "aeiou");
        assert_eq!(normalize("ção"), "cao");
    }
}
