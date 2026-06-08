//! Shandara — universo "content reader" (não tick-based).
//!
//! Implementa a ABI v1 do `universe-sdk` com semântica de navegação de
//! conteúdo: cada seção do SRD é um arquivo markdown embedado via `include_str!`
//! em compile-time. Não há simulação por tick — `tick` apenas navega/retorna a
//! seção pedida.
//!
//! - `create(params)` — params opcional `{ "section": "mundo/forcas-primordiais" }`
//!   abre uma sessão de leitura apontando para essa seção (default: `index`).
//! - `tick(input)` — input `{ "action": "navigate", "to": "regras/atributos" }`
//!   retorna `{ "section": <slug>, "markdown": <conteúdo>, "exists": <bool> }`.
//! - `manifest()` — `capabilities: ["content", "rpg", "srd"]`.

use serde::{Deserialize, Serialize};
use universe_sdk::{Universe, UniverseManifest, universe_exports};

// ─── Seções do SRD embedadas (compile-time) ──────────────────────────────────

/// Tabela (slug → markdown) das seções disponíveis nesta versão.
const SECTIONS: &[(&str, &str)] = &[
    ("index", include_str!("../content/index.md")),
    (
        "mundo/forcas-primordiais",
        include_str!("../content/mundo/forcas-primordiais.md"),
    ),
    (
        "mundo/grande-guerra",
        include_str!("../content/mundo/grande-guerra.md"),
    ),
    (
        "povos/verdejantes",
        include_str!("../content/povos/verdejantes.md"),
    ),
    (
        "povos/transmutos",
        include_str!("../content/povos/transmutos.md"),
    ),
    (
        "regras/atributos",
        include_str!("../content/regras/atributos.md"),
    ),
    (
        "regras/criacao-personagem",
        include_str!("../content/regras/criacao-personagem.md"),
    ),
];

fn lookup(section: &str) -> Option<&'static str> {
    SECTIONS
        .iter()
        .find(|(s, _)| *s == section)
        .map(|(_, md)| *md)
}

// ─── Tipos de I/O ─────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct CreateParams {
    section: Option<String>,
}

#[derive(Deserialize)]
struct Input {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

#[derive(Serialize)]
struct ReadState {
    section: String,
    markdown: String,
    exists: bool,
    /// Slugs de todas as seções disponíveis — facilita navegação no cliente.
    sections: Vec<&'static str>,
}

// ─── Sessão de leitura ─────────────────────────────────────────────────────────

pub struct ShandaraReader {
    section: String,
}

impl ShandaraReader {
    fn render(&self) -> String {
        let md = lookup(&self.section);
        let state = ReadState {
            section: self.section.clone(),
            markdown: md.unwrap_or_default().to_string(),
            exists: md.is_some(),
            sections: SECTIONS.iter().map(|(s, _)| *s).collect(),
        };
        serde_json::to_string(&state).unwrap_or_default()
    }
}

impl Universe for ShandaraReader {
    fn create(params: &str) -> Self {
        let parsed: CreateParams = serde_json::from_str(params).unwrap_or_default();
        let section = parsed.section.unwrap_or_else(|| "index".to_string());
        ShandaraReader { section }
    }

    fn tick(&mut self, input: &str) -> String {
        let parsed: Input = match serde_json::from_str(input) {
            Ok(i) => i,
            Err(_) => Input {
                action: None,
                to: None,
            },
        };
        // Única ação suportada: navegar para outra seção.
        if parsed.action.as_deref() == Some("navigate")
            && let Some(to) = parsed.to
        {
            self.section = to;
        }
        self.render()
    }

    fn manifest() -> UniverseManifest {
        UniverseManifest {
            name: "Shandara".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            api_version: 1,
            max_players: 1,
            capabilities: vec!["content".to_string(), "rpg".to_string(), "srd".to_string()],
        }
    }
}

universe_exports!(ShandaraReader);

// ─── Testes (native — exercitam a lógica de leitura, não a ABI WASM) ──────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_default_abre_index() {
        let r = ShandaraReader::create("{}");
        assert_eq!(r.section, "index");
        let v: serde_json::Value = serde_json::from_str(&r.render()).unwrap();
        assert_eq!(v["section"], "index");
        assert_eq!(v["exists"], true);
        assert!(v["markdown"].as_str().unwrap().contains("Shandara"));
    }

    #[test]
    fn create_com_section_param() {
        let r = ShandaraReader::create(r#"{"section":"mundo/forcas-primordiais"}"#);
        assert_eq!(r.section, "mundo/forcas-primordiais");
        let v: serde_json::Value = serde_json::from_str(&r.render()).unwrap();
        assert_eq!(v["exists"], true);
    }

    #[test]
    fn tick_navigate_muda_secao_e_retorna_markdown() {
        let mut r = ShandaraReader::create("{}");
        let out = r.tick(r#"{"action":"navigate","to":"regras/atributos"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["section"], "regras/atributos");
        assert_eq!(v["exists"], true);
        assert!(v["markdown"].as_str().unwrap().contains("Atributos"));
    }

    #[test]
    fn tick_secao_inexistente_exists_false() {
        let mut r = ShandaraReader::create("{}");
        let out = r.tick(r#"{"action":"navigate","to":"nao/existe"}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["exists"], false);
        assert_eq!(v["markdown"], "");
    }

    #[test]
    fn manifest_tem_capabilities_de_srd() {
        let m = ShandaraReader::manifest();
        assert_eq!(m.name, "Shandara");
        assert_eq!(m.api_version, 1);
        let caps: Vec<&str> = m.capabilities.iter().map(|s| s.as_str()).collect();
        for c in ["content", "rpg", "srd"] {
            assert!(caps.contains(&c), "capability '{c}' ausente");
        }
    }

    #[test]
    fn todas_as_secoes_listadas_existem() {
        for (slug, _) in SECTIONS {
            assert!(lookup(slug).is_some(), "seção '{slug}' não resolve");
        }
    }
}
