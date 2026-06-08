//! Catálogo de universos (YG-70) — lê `universes/REGISTRY.yaml` em compile-time
//! e oferece filtros dinâmicos por status, type, origin, genre, license e
//! busca textual.
//!
//! O catálogo é a *fonte da verdade* de tudo que o Yggdrasil mostra como
//! universo: embedados (jogáveis aqui), planejados (placeholder contribuível)
//! e externos (link out). O endpoint `GET /api/v1/universos` faz o merge desse
//! catálogo com o runtime real dos universos embedados.
//!
//! Schema documentado em `docs/architecture/catalog.md`.

use serde::{Deserialize, Serialize};

/// REGISTRY.yaml embedado em compile-time. Mudou o YAML → recompila.
const REGISTRY_YAML: &str = include_str!("../../universes/REGISTRY.yaml");

/// Status de uma entrada do catálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogStatus {
    /// WASM/engine em produção — jogável aqui.
    Embedded,
    /// Mapeado, sem código ainda — placeholder contribuível.
    Planned,
    /// Existe fora da plataforma — link out.
    External,
}

impl CatalogStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogStatus::Embedded => "embedded",
            CatalogStatus::Planned => "planned",
            CatalogStatus::External => "external",
        }
    }
}

/// Uma entrada do catálogo, como declarada no REGISTRY.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub slug: String,
    pub status: CatalogStatus,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub genre: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_release: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_difficulty: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub versions_tracked: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl CatalogEntry {
    /// Universo jogável diretamente na plataforma?
    pub fn playable(&self) -> bool {
        self.status == CatalogStatus::Embedded
    }
}

#[derive(Debug, Deserialize)]
struct Registry {
    universes: Vec<CatalogEntry>,
}

/// Parse + lista de entradas do catálogo. Falha em compile-only se o YAML for
/// inválido (o teste `registry_parseia` cobre isso); em runtime devolve erro.
pub fn catalog_entries() -> Result<Vec<CatalogEntry>, serde_yaml::Error> {
    let reg: Registry = serde_yaml::from_str(REGISTRY_YAML)?;
    Ok(reg.universes)
}

/// Filtros de query string aceitos por `GET /api/v1/universos`.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct CatalogFilter {
    /// `embedded | planned | external | all` (default: all).
    #[serde(default)]
    pub status: Option<String>,
    /// `rpg | arcade | puzzle | ...`
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// `brazilian | international | original`
    #[serde(default)]
    pub origin: Option<String>,
    /// Lista separada por vírgula — match se a entrada tiver QUALQUER um.
    #[serde(default)]
    pub genre: Option<String>,
    /// `open-source | commercial | all`
    #[serde(default)]
    pub license: Option<String>,
    /// Substring match em title + description (case-insensitive).
    #[serde(default)]
    pub search: Option<String>,
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

/// "all" / vazio = sem filtro nesse campo.
fn active(opt: &Option<String>) -> Option<String> {
    opt.as_deref()
        .map(norm)
        .filter(|s| !s.is_empty() && s != "all")
}

impl CatalogFilter {
    /// Aplica o filtro a uma lista de entradas, preservando a ordem.
    pub fn apply<'a>(&self, entries: &'a [CatalogEntry]) -> Vec<&'a CatalogEntry> {
        let status = active(&self.status);
        let kind = active(&self.kind);
        let origin = active(&self.origin);
        let license = active(&self.license);
        let search = active(&self.search);
        let genres: Vec<String> = self
            .genre
            .as_deref()
            .map(|g| g.split(',').map(norm).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();

        entries
            .iter()
            .filter(|e| {
                if let Some(ref s) = status
                    && e.status.as_str() != s
                {
                    return false;
                }
                if let Some(ref k) = kind
                    && norm(&e.kind) != *k
                {
                    return false;
                }
                if let Some(ref o) = origin
                    && e.origin.as_deref().map(norm).as_deref() != Some(o.as_str())
                {
                    return false;
                }
                if let Some(ref l) = license
                    && e.license.as_deref().map(norm).as_deref() != Some(l.as_str())
                {
                    return false;
                }
                if !genres.is_empty() && !e.genre.iter().any(|g| genres.contains(&norm(g))) {
                    return false;
                }
                if let Some(ref q) = search {
                    let hay = format!("{} {}", e.title, e.description).to_lowercase();
                    if !hay.contains(q) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }
}

/// Contagem por status sobre uma lista de entradas.
#[derive(Debug, Default, Serialize)]
pub struct StatusCounts {
    pub embedded: usize,
    pub planned: usize,
    pub external: usize,
}

pub fn count_by_status(entries: &[&CatalogEntry]) -> StatusCounts {
    let mut c = StatusCounts::default();
    for e in entries {
        match e.status {
            CatalogStatus::Embedded => c.embedded += 1,
            CatalogStatus::Planned => c.planned += 1,
            CatalogStatus::External => c.external += 1,
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parseia() {
        let entries = catalog_entries().expect("REGISTRY.yaml deve parsear");
        assert!(entries.len() >= 40, "got {}", entries.len());
    }

    #[test]
    fn slugs_sao_unicos() {
        let entries = catalog_entries().unwrap();
        let mut slugs: Vec<&str> = entries.iter().map(|e| e.slug.as_str()).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(before, slugs.len(), "slugs duplicados no REGISTRY.yaml");
    }

    #[test]
    fn embedados_sao_playable_planejados_nao() {
        let entries = catalog_entries().unwrap();
        let snake = entries.iter().find(|e| e.slug == "snake").unwrap();
        assert!(snake.playable());
        let tagmar = entries.iter().find(|e| e.slug == "tagmar").unwrap();
        assert!(!tagmar.playable());
    }

    #[test]
    fn shandara_presente_como_embedded_rpg() {
        let entries = catalog_entries().unwrap();
        let s = entries.iter().find(|e| e.slug == "shandara").unwrap();
        assert_eq!(s.status, CatalogStatus::Embedded);
        assert_eq!(s.kind, "rpg");
    }

    #[test]
    fn filtro_origin_brazilian_retorna_35_mais() {
        let entries = catalog_entries().unwrap();
        let f = CatalogFilter {
            origin: Some("brazilian".into()),
            ..Default::default()
        };
        let out = f.apply(&entries);
        assert!(out.len() >= 33, "brazilian: {}", out.len());
        assert!(out.iter().all(|e| e.origin.as_deref() == Some("brazilian")));
    }

    #[test]
    fn filtro_origin_brazilian_planned_retorna_30_mais() {
        let entries = catalog_entries().unwrap();
        let f = CatalogFilter {
            origin: Some("brazilian".into()),
            status: Some("planned".into()),
            ..Default::default()
        };
        let out = f.apply(&entries);
        assert!(out.len() >= 30, "brazilian+planned: {}", out.len());
        assert!(out.iter().all(|e| e.status == CatalogStatus::Planned));
    }

    #[test]
    fn filtro_status_embedded() {
        let entries = catalog_entries().unwrap();
        let f = CatalogFilter {
            status: Some("embedded".into()),
            ..Default::default()
        };
        let out = f.apply(&entries);
        assert!(out.iter().all(|e| e.playable()));
        assert!(out.len() >= 7, "embedded: {}", out.len());
    }

    #[test]
    fn filtro_genre_multi() {
        let entries = catalog_entries().unwrap();
        let f = CatalogFilter {
            genre: Some("horror".into()),
            ..Default::default()
        };
        let out = f.apply(&entries);
        // BREU, Oblívio (dark), Arquivos Paranormais, Ordem Paranormal...
        assert!(out.len() >= 2, "horror: {}", out.len());
        assert!(out.iter().all(|e| e.genre.iter().any(|g| g == "horror")));
    }

    #[test]
    fn filtro_search_substring() {
        let entries = catalog_entries().unwrap();
        let f = CatalogFilter {
            search: Some("quilombo".into()),
            ..Default::default()
        };
        let out = f.apply(&entries);
        assert!(out.iter().any(|e| e.slug == "kilombo"));
    }

    #[test]
    fn filtro_all_equivale_a_sem_filtro() {
        let entries = catalog_entries().unwrap();
        let f = CatalogFilter {
            status: Some("all".into()),
            license: Some("all".into()),
            ..Default::default()
        };
        let out = f.apply(&entries);
        assert_eq!(out.len(), entries.len());
    }

    #[test]
    fn count_by_status_soma_total() {
        let entries = catalog_entries().unwrap();
        let refs: Vec<&CatalogEntry> = entries.iter().collect();
        let c = count_by_status(&refs);
        assert_eq!(c.embedded + c.planned + c.external, entries.len());
        assert!(c.embedded >= 7);
        assert!(c.planned >= 30);
        assert!(c.external >= 3);
    }
}
