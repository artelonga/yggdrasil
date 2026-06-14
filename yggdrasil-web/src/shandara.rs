//! Reader do SRD de Shandara (YG-144) — MVP.
//!
//! O conteúdo (CC-BY-SA) vive em `universes/universe-shandara/content/` e é
//! **embutido no binário** via [`include_dir`] — deploy-safe (sem caminho em
//! runtime, mesmo princípio do `REGISTRY.yaml` por `include_str!`). Servimos
//! uma árvore (seções → docs) + o Markdown cru de cada doc; o cliente renderiza.

use include_dir::{Dir, include_dir};
use serde::Serialize;

static SRD: Dir = include_dir!("$CARGO_MANIFEST_DIR/../universes/universe-shandara/content");

#[derive(Serialize)]
pub struct DocRef {
    /// Caminho relativo sem `.md` (ex.: `mundo/grande-guerra`). É o id na URL.
    pub path: String,
    pub title: String,
    pub section: String,
}

/// Rótulo humano de uma seção (diretório). Capitaliza e acentua os conhecidos.
fn section_label(dir: &str) -> String {
    match dir {
        "" => "Visão geral".into(),
        "mundo" => "Mundo".into(),
        "povos" => "Povos".into(),
        "regras" => "Regras".into(),
        "bestiario" => "Bestiário".into(),
        "aventuras" => "Aventuras".into(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
    }
}

/// Título do doc = primeira linha `# ...`; fallback = nome do arquivo.
fn title_of(content: &str, fallback: &str) -> String {
    content
        .lines()
        .find_map(|l| l.strip_prefix("# ").map(|t| t.trim().to_string()))
        .unwrap_or_else(|| fallback.to_string())
}

fn walk(dir: &Dir, out: &mut Vec<DocRef>) {
    for f in dir.files() {
        let p = f.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if stem.eq_ignore_ascii_case("LICENSE-CONTENT") {
            continue;
        }
        let rel = p.with_extension("");
        let path = rel.to_string_lossy().replace('\\', "/");
        let section = p
            .parent()
            .and_then(|d| d.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        let content = f.contents_utf8().unwrap_or("");
        out.push(DocRef {
            title: title_of(content, stem),
            section: section_label(&section),
            path,
        });
    }
    for sub in dir.dirs() {
        walk(sub, out);
    }
}

/// Árvore plana de docs (a UI agrupa por `section`). `index` primeiro; `_index`
/// de seção logo após o próprio doc raiz. Ordenação estável por path.
pub fn tree() -> Vec<DocRef> {
    let mut out = Vec::new();
    walk(&SRD, &mut out);
    out.sort_by(|a, b| {
        // index.md no topo absoluto
        let rank = |d: &DocRef| if d.path == "index" { 0 } else { 1 };
        rank(a).cmp(&rank(b)).then(a.path.cmp(&b.path))
    });
    out
}

/// Markdown cru de um doc por `path` (sem `.md`). `None` se não existe.
/// `path` é sanitizado (sem travessia de diretório).
pub fn doc(path: &str) -> Option<String> {
    if path.is_empty()
        || path.contains("..")
        || !path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/'))
    {
        return None;
    }
    SRD.get_file(format!("{path}.md"))
        .and_then(|f| f.contents_utf8())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_tem_index_e_secoes_conhecidas() {
        let t = tree();
        assert!(!t.is_empty(), "SRD embutido não pode estar vazio");
        assert_eq!(t[0].path, "index", "index.md vem primeiro");
        // seções esperadas presentes
        let secs: std::collections::BTreeSet<&str> = t.iter().map(|d| d.section.as_str()).collect();
        assert!(secs.contains("Mundo"));
        assert!(secs.contains("Povos"));
        assert!(secs.contains("Regras"));
        // título extraído do `# `
        let gg = t.iter().find(|d| d.path == "mundo/grande-guerra").unwrap();
        assert_eq!(gg.title, "A Grande Guerra");
        // LICENSE excluída
        assert!(!t.iter().any(|d| d.path.to_lowercase().contains("license")));
    }

    #[test]
    fn doc_serve_markdown_e_rejeita_traversal() {
        assert!(doc("index").unwrap().contains("System Reference Document"));
        assert!(
            doc("mundo/grande-guerra")
                .unwrap()
                .contains("Grande Guerra")
        );
        assert!(doc("../../etc/passwd").is_none());
        assert!(doc("nao-existe").is_none());
    }
}
