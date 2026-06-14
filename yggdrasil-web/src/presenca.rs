//! Presença/atividade ao vivo por universo (YG-145).
//!
//! O primitivo é **presença** (entrar/sair), não partida/fim. Registro in-memory
//! efêmero, alimentado pelo tracker client-side (`atividade()` em telemetria.js):
//!   - `entrada`/`pulso` marcam o `vid` como vivo no universo (janela ~60s);
//!   - `saida` remove;
//!   - `partida` (opcional, só jogos com resultado) entra no feed de atividade.
//!
//! Dois recortes:
//!   - **presença** (`universo → vid → último visto ms`) → "ativos agora";
//!   - **ring de atividade** por universo (anônimo: sem `vid`) → "últimas
//!     atividades" (entradas/saídas/partidas, com score/resultado quando há).
//!
//! Zero PII: o `vid` é um token opaco aleatório e nunca sai no feed público.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;

/// Um `vid` sem `entrada`/`pulso` há mais que isto deixa de contar como ativo.
const JANELA_MS: i64 = 60_000;
/// Tamanho do feed de atividade por universo.
const RING: usize = 30;

#[derive(Default)]
struct Inner {
    /// universo → (vid → último visto em ms)
    presenca: HashMap<String, HashMap<String, i64>>,
    /// universo → feed recente (mais novo no fim)
    atividade: HashMap<String, VecDeque<serde_json::Value>>,
}

#[derive(Clone, Default)]
pub struct Presenca {
    inner: Arc<Mutex<Inner>>,
}

impl Presenca {
    pub fn new() -> Self {
        Self::default()
    }

    /// Marca `vid` como vivo no `universo` (entrada/pulso).
    pub fn marcar(&self, universo: &str, vid: &str, agora_ms: i64) {
        if let Ok(mut g) = self.inner.lock() {
            g.presenca
                .entry(universo.to_string())
                .or_default()
                .insert(vid.to_string(), agora_ms);
        }
    }

    /// Remove `vid` do `universo` (saída explícita).
    pub fn sair(&self, universo: &str, vid: &str) {
        if let Ok(mut g) = self.inner.lock()
            && let Some(u) = g.presenca.get_mut(universo)
        {
            u.remove(vid);
        }
    }

    /// Empurra um evento anônimo (sem `vid`) no feed do universo.
    pub fn feed(&self, universo: &str, ev: serde_json::Value) {
        if let Ok(mut g) = self.inner.lock() {
            let ring = g.atividade.entry(universo.to_string()).or_default();
            if ring.len() >= RING {
                ring.pop_front();
            }
            ring.push_back(ev);
        }
    }

    /// Contagem de ativos por universo dentro da janela. Faz GC dos expirados.
    pub fn ativos_por_universo(&self, agora_ms: i64) -> HashMap<String, i64> {
        let mut out = HashMap::new();
        if let Ok(mut g) = self.inner.lock() {
            let corte = agora_ms - JANELA_MS;
            g.presenca.retain(|_, vids| {
                vids.retain(|_, ts| *ts >= corte);
                !vids.is_empty()
            });
            for (u, vids) in g.presenca.iter() {
                out.insert(u.clone(), vids.len() as i64);
            }
        }
        out
    }

    /// Ativos agora num único universo.
    pub fn ativos(&self, universo: &str, agora_ms: i64) -> i64 {
        *self
            .ativos_por_universo(agora_ms)
            .get(universo)
            .unwrap_or(&0)
    }

    /// Feed recente do universo (mais novo primeiro), até `limit`.
    pub fn recentes(&self, universo: &str, limit: usize) -> Vec<serde_json::Value> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| {
                g.atividade
                    .get(universo)
                    .map(|r| r.iter().rev().take(limit).cloned().collect::<Vec<_>>())
            })
            .unwrap_or_default()
    }
}

// ─── Estado compartilhado dos handlers ───────────────────────────────────────

#[derive(Clone)]
pub struct AtividadeState {
    pub presenca: Presenca,
    /// Pulso WS (YG-128): cada mudança de presença vira um frame ao vivo.
    pub pulso: crate::analytics_stream::Pulso,
}

// ─── Ingest: POST /api/v1/presenca ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct PresencaIn {
    pub universo: String,
    pub vid: String,
    /// "entrada" | "pulso" | "saida" | "partida"
    pub ev: String,
    #[serde(default)]
    pub score: Option<i64>,
    #[serde(default)]
    pub resultado: Option<String>,
    #[serde(default)]
    pub ativo_ms: Option<i64>,
}

fn limpa(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

/// `POST /api/v1/presenca` — ingest do tracker client-side. Público, anônimo,
/// best-effort (sempre 204). Atualiza presença e/ou o feed conforme o `ev`.
pub async fn ingest(
    State(state): State<AtividadeState>,
    Json(body): Json<PresencaIn>,
) -> impl IntoResponse {
    let universo = limpa(&body.universo, 64);
    let vid = limpa(&body.vid, 64);
    if universo.is_empty() || vid.is_empty() {
        return StatusCode::NO_CONTENT;
    }
    let agora = chrono::Utc::now().timestamp_millis();

    match body.ev.as_str() {
        "entrada" | "pulso" => state.presenca.marcar(&universo, &vid, agora),
        "saida" => state.presenca.sair(&universo, &vid),
        _ => {}
    }

    // Feed anônimo (sem vid): entrada/saida/partida — pulso não polui o feed.
    if matches!(body.ev.as_str(), "entrada" | "saida" | "partida") {
        let mut ev = serde_json::Map::new();
        ev.insert("ev".into(), serde_json::json!(limpa(&body.ev, 16)));
        ev.insert("ts".into(), serde_json::json!(agora));
        if let Some(s) = body.score {
            ev.insert("score".into(), serde_json::json!(s));
        }
        if let Some(r) = &body.resultado {
            ev.insert("resultado".into(), serde_json::json!(limpa(r, 24)));
        }
        if let Some(a) = body.ativo_ms {
            ev.insert("ativo_ms".into(), serde_json::json!(a));
        }
        state
            .presenca
            .feed(&universo, serde_json::Value::Object(ev));
    }

    // Frame ao vivo no pulso: contagem do universo afetado (sem vid).
    let ativos = state.presenca.ativos(&universo, agora);
    state.pulso.push(serde_json::json!({
        "ev": "presenca",
        "universo": universo,
        "ativos": ativos,
        "ts": agora,
    }));

    StatusCode::NO_CONTENT
}

// ─── GET /api/v1/atividade (lobby: contagem por universo) ─────────────────────

pub async fn por_universo(State(state): State<AtividadeState>) -> impl IntoResponse {
    let agora = chrono::Utc::now().timestamp_millis();
    let por = state.presenca.ativos_por_universo(agora);
    let total: i64 = por.values().sum();
    Json(serde_json::json!({ "por_universo": por, "total": total }))
}

// ─── GET /api/v1/universos/{id}/atividade (painel do universo) ────────────────

pub async fn do_universo(
    State(state): State<AtividadeState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let id = limpa(&id, 64);
    let agora = chrono::Utc::now().timestamp_millis();
    Json(serde_json::json!({
        "ativos_agora": state.presenca.ativos(&id, agora),
        "ultimas": state.presenca.recentes(&id, 20),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenca_conta_dentro_da_janela_e_expira() {
        let p = Presenca::new();
        let t0 = 1_000_000i64;
        p.marcar("snake", "vid-a", t0);
        p.marcar("snake", "vid-b", t0);
        assert_eq!(p.ativos("snake", t0), 2);
        // vid-a some depois da janela; vid-b renova
        let t1 = t0 + JANELA_MS + 1;
        p.marcar("snake", "vid-b", t1);
        assert_eq!(p.ativos("snake", t1), 1, "só o vid renovado conta");
    }

    #[test]
    fn saida_remove_na_hora() {
        let p = Presenca::new();
        let t = 5i64;
        p.marcar("poker", "v1", t);
        assert_eq!(p.ativos("poker", t), 1);
        p.sair("poker", "v1");
        assert_eq!(p.ativos("poker", t), 0);
    }

    #[test]
    fn feed_guarda_ate_o_limite_e_mais_novo_primeiro() {
        let p = Presenca::new();
        for i in 0..(RING + 5) {
            p.feed("snake", serde_json::json!({ "ev": "partida", "score": i }));
        }
        let r = p.recentes("snake", 100);
        assert_eq!(r.len(), RING);
        assert_eq!(r[0]["score"], (RING + 4) as i64, "mais novo primeiro");
    }

    #[test]
    fn ativos_por_universo_separa_universos() {
        let p = Presenca::new();
        let t = 10i64;
        p.marcar("snake", "a", t);
        p.marcar("tetris", "b", t);
        p.marcar("tetris", "c", t);
        let m = p.ativos_por_universo(t);
        assert_eq!(m.get("snake"), Some(&1));
        assert_eq!(m.get("tetris"), Some(&2));
    }
}
