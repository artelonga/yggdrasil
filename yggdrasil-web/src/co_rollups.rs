//! Push de rollups diários ao hub de analytics do CO (YG-127 Fase 1).
//!
//! O CO expõe `POST /api/v1/analytics/public/rollups` (ingest idempotente por
//! `(universe, day)`, bearer `CO_ROLLUP_TOKEN`) — é como produtores externos
//! aparecem no dashboard público que a ArteLonga já usa. Aqui empurramos os
//! agregados de TELEMETRIA DE JOGO (sessões/conclusões por universo, do
//! `TelemetriaDb`), que o tracker client-side não vê.
//!
//! Gate por env (mesmo padrão do bridge): sem `YGG_CO_ROLLUP_TOKEN` → no-op.
//! Só agregados anônimos saem daqui — nenhum evento cru, nenhum identificador.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::telemetria::TelemetriaDb;

const PUSH_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1×/hora (upsert idempotente)

pub struct RollupConfig {
    pub url: String,
    pub token: String,
}

impl RollupConfig {
    /// Lê o gate de env. `None` = feature desligada (no-op).
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("YGG_CO_ROLLUP_TOKEN")
            .ok()?
            .trim()
            .to_string();
        if token.is_empty() {
            return None;
        }
        let url = std::env::var("YGG_CO_ROLLUP_URL").unwrap_or_else(|_| {
            "https://co.artelonga.com.br/api/v1/analytics/public/rollups".to_string()
        });
        Some(Self { url, token })
    }
}

/// Monta o payload do dia (UTC) a partir da telemetria de jogo. Função pura
/// sobre o snapshot do relatório — testável sem rede.
pub fn rollup_payload(
    telemetria: &TelemetriaDb,
    day_start_ms: i64,
    day: &str,
) -> serde_json::Value {
    let report = telemetria.get_analytics(day_start_ms, &HashMap::new());
    let total_sessoes: i64 = report.universos.iter().map(|u| u.sessions_today).sum();
    let total_conclusoes: i64 = report.universos.iter().map(|u| u.completions_today).sum();
    let por_universo: serde_json::Map<String, serde_json::Value> = report
        .universos
        .iter()
        .map(|u| (u.id.clone(), serde_json::json!(u.sessions_today)))
        .collect();
    serde_json::json!({
        "universe": "yggdrasil",
        "day": day,
        "metrics": {
            "game_sessions": total_sessoes,
            "game_completions": total_conclusoes,
            "universe_views": report.funnel_24h.universe_views,
        },
        "dims": { "sessions_by_universe": por_universo },
        "private": false,
    })
}

/// Sobe a task de fundo: a cada hora, upsert do rollup de HOJE no CO.
/// Best-effort: falha vira `warn` e tenta de novo no próximo tick.
pub fn spawn(telemetria: Arc<TelemetriaDb>) {
    let Some(config) = RollupConfig::from_env() else {
        tracing::info!("co-rollups desligado (YGG_CO_ROLLUP_TOKEN não configurado) — no-op");
        return;
    };
    tracing::info!(url = %config.url, "co-rollups ativo — agregados diários de jogo → CO");
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            let now = chrono::Utc::now();
            let day = now.format("%Y-%m-%d").to_string();
            let day_start_ms = now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|d| d.and_utc().timestamp_millis())
                .unwrap_or(0);
            let body = rollup_payload(&telemetria, day_start_ms, &day);
            let res = client
                .post(&config.url)
                .bearer_auth(&config.token)
                .header(
                    "user-agent",
                    concat!("yggdrasil-rollups/", env!("CARGO_PKG_VERSION")),
                )
                .json(&body)
                .send()
                .await;
            match res {
                Ok(r) if r.status().is_success() => {
                    tracing::debug!(%day, "co-rollups: upsert ok");
                }
                Ok(r) => tracing::warn!(status = %r.status(), "co-rollups: upsert recusado"),
                Err(e) => tracing::warn!(erro = %e, "co-rollups: falha de rede"),
            }
            tokio::time::sleep(PUSH_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_tem_shape_do_ingest_do_co() {
        let t = TelemetriaDb::in_memory().unwrap();
        t.session_create("s1", "snake", 1_000);
        t.session_complete("s1", "snake", 2_000, 1_000, 42);
        let p = rollup_payload(&t, 0, "2026-06-12");
        assert_eq!(p["universe"], "yggdrasil");
        assert_eq!(p["day"], "2026-06-12");
        assert_eq!(p["private"], false);
        assert!(p["metrics"]["game_sessions"].as_i64().unwrap() >= 1);
        assert_eq!(p["metrics"]["game_completions"], 1);
        assert!(p["dims"]["sessions_by_universe"].is_object());
        // só agregados anônimos: nenhum campo de evento cru/identificador
        assert!(p.get("events").is_none());
        assert!(p["dims"].get("visitors").is_none());
    }
}
