//! Notas em Markdown ligadas por wikilinks — o "jardim" do editor de instâncias
//! (Fase 0 do bridge com o `co`).
//!
//! **Canônico = Markdown em disco**: uma `notes/<slug>.md` por nota, sob o
//! diretório da instância (`<store root>/<id>/notes/`). O grafo da instância
//! (`instance.json`) permanece projeção de *layout*: um [`super::schema::Block`]
//! referencia sua nota por `props.note_slug`. Os dois lados são disjuntos —
//! o `co` edita corpo/links (Markdown), o Yggdrasil mantém posições (JSON) —
//! então não há merge: é o que torna a nota promovível a sub-universo sem
//! re-plumbing (só mover o arquivo).
//!
//! Wikilinks `[[slug]]` / `[[slug|alias]]` no corpo são parseados com a mesma
//! semântica do `co` (porte de `co/core/src/wikilink.rs`), resolvidos a slugs via
//! [`slugify`], e invertidos em backlinks. Escrita atômica (temp + rename),
//! espelhando [`super::store::InstanceStore`].

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::comunicacao::lexicon::slugify;

/// Falhas de I/O do store de notas.
#[derive(Debug, thiserror::Error)]
pub enum NoteError {
    #[error("nota '{0}' não encontrada")]
    NotFound(String),
    #[error("slug de nota inválido")]
    InvalidSlug,
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Uma nota: Markdown canônico + metadados derivados do frontmatter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub slug: String,
    pub title: String,
    /// Corpo Markdown verbatim (sem o frontmatter).
    pub body: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    /// Wikilinks de saída, na ordem do corpo (alvo já resolvido a slug).
    #[serde(default)]
    pub links: Vec<NoteLink>,
}

/// Um wikilink de saída.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteLink {
    /// Slug-alvo resolvido (via [`slugify`] do target do `[[...]]`).
    pub target: String,
    /// Label do alias (`[[alvo|label]]`), se houver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Uma referência de entrada (backlink): quem aponta para esta nota.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backlink {
    /// Slug da nota de origem.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Store em disco das notas de uma instância.
pub struct NoteStore {
    dir: PathBuf,
}

impl NoteStore {
    /// Store enraizado em `<store_root>/<instance_id>/notes/`.
    pub fn for_instance(store_root: &Path, instance_id: &str) -> Self {
        Self {
            dir: store_root.join(instance_id).join("notes"),
        }
    }

    fn note_path(&self, slug: &str) -> PathBuf {
        self.dir.join(format!("{slug}.md"))
    }

    /// Cria/atualiza uma nota. `slug` é sempre normalizado por [`note_slug`] (defesa
    /// contra path traversal). Preserva `created` se a nota já existir.
    pub fn save(&self, slug: &str, title: &str, body: &str) -> Result<Note, NoteError> {
        let slug = note_slug(slug).ok_or(NoteError::InvalidSlug)?;
        std::fs::create_dir_all(&self.dir)?;
        let now = Utc::now();
        let created = self.load(&slug).map(|n| n.created).unwrap_or(now);
        let md = render_note_markdown(&slug, title, body, created, now);
        let path = self.note_path(&slug);
        let tmp = self.dir.join(format!("{slug}.md.tmp"));
        std::fs::write(&tmp, md)?;
        std::fs::rename(&tmp, &path)?;
        Ok(Note {
            slug,
            title: title.to_string(),
            body: body.to_string(),
            created,
            updated: now,
            links: links_from_body(body),
        })
    }

    /// Lê uma nota pelo slug.
    pub fn load(&self, slug: &str) -> Result<Note, NoteError> {
        let slug = note_slug(slug).ok_or_else(|| NoteError::NotFound(slug.to_string()))?;
        let path = self.note_path(&slug);
        let raw = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                NoteError::NotFound(slug.clone())
            } else {
                NoteError::Io(e)
            }
        })?;
        Ok(parse_note(&slug, &raw))
    }

    /// Lista todas as notas (mais recentes primeiro). Diretório ausente = vazio.
    pub fn list(&self) -> Result<Vec<Note>, NoteError> {
        let mut out = Vec::new();
        let rd = match std::fs::read_dir(&self.dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(NoteError::Io(e)),
        };
        for entry in rd {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Ok(raw) = std::fs::read_to_string(&path) {
                out.push(parse_note(stem, &raw));
            }
        }
        out.sort_by_key(|n| Reverse(n.updated));
        Ok(out)
    }

    /// Remove uma nota.
    pub fn delete(&self, slug: &str) -> Result<(), NoteError> {
        let slug = note_slug(slug).ok_or_else(|| NoteError::NotFound(slug.to_string()))?;
        let path = self.note_path(&slug);
        std::fs::remove_file(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                NoteError::NotFound(slug)
            } else {
                NoteError::Io(e)
            }
        })
    }
}

// ─── Rascunhos (draft-as-branch, YG-125) ─────────────────────────────────────

/// Um rascunho de nota: corpo cru, por usuário, fora do caminho federado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub slug: String,
    pub markdown: String,
    pub updated: DateTime<Utc>,
}

/// Store de rascunhos por usuário: `<store_root>/<id>/drafts/<user-slug>/<slug>.md`.
///
/// Deliberadamente FORA de `notes/`: o producer do bridge só publica notas
/// canônicas, então um rascunho **nunca federa** — privacidade por construção.
/// O corpo é gravado cru (sem frontmatter); `updated` vem do mtime do arquivo.
pub struct DraftStore {
    dir: PathBuf,
}

impl DraftStore {
    pub fn for_user(store_root: &Path, instance_id: &str, user: &str) -> Self {
        let user_slug = {
            let s = slugify(user);
            if s.is_empty() { "anon".to_string() } else { s }
        };
        Self {
            dir: store_root.join(instance_id).join("drafts").join(user_slug),
        }
    }

    fn draft_path(&self, slug: &str) -> Result<PathBuf, NoteError> {
        let slug = note_slug(slug).ok_or(NoteError::InvalidSlug)?;
        Ok(self.dir.join(format!("{slug}.md")))
    }

    /// Grava/substitui o rascunho (escrita atômica: temp + rename).
    pub fn save(&self, slug: &str, markdown: &str) -> Result<Draft, NoteError> {
        let path = self.draft_path(slug)?;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("md.tmp");
        std::fs::write(&tmp, markdown)?;
        std::fs::rename(&tmp, &path)?;
        Ok(Draft {
            slug: note_slug(slug).unwrap_or_default(),
            markdown: markdown.to_string(),
            updated: Utc::now(),
        })
    }

    /// Lê o rascunho; `updated` = mtime do arquivo.
    pub fn load(&self, slug: &str) -> Result<Draft, NoteError> {
        let path = self.draft_path(slug)?;
        let markdown = std::fs::read_to_string(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                NoteError::NotFound(slug.to_string())
            } else {
                NoteError::Io(e)
            }
        })?;
        let updated = std::fs::metadata(&path)?
            .modified()
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());
        Ok(Draft {
            slug: note_slug(slug).unwrap_or_default(),
            markdown,
            updated,
        })
    }

    /// Descarta o rascunho.
    pub fn delete(&self, slug: &str) -> Result<(), NoteError> {
        let path = self.draft_path(slug)?;
        std::fs::remove_file(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                NoteError::NotFound(slug.to_string())
            } else {
                NoteError::Io(e)
            }
        })
    }
}

// ─── Wikilinks + grafo ────────────────────────────────────────────────────────

/// Extrai wikilinks `[[target]]` / `[[target|alias]]` de um texto, preservando o
/// label. Porte de `co/core/src/wikilink.rs::extract_wikilinks_with_labels` para
/// manter a semântica idêntica à do `co`.
pub fn extract_wikilinks(text: &str) -> Vec<(String, Option<String>)> {
    let mut results = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            let inner = &rest[..end];
            let (raw_target, label) = if let Some(pipe) = inner.find('|') {
                let label = inner[pipe + 1..].trim();
                (
                    &inner[..pipe],
                    if label.is_empty() {
                        None
                    } else {
                        Some(label.to_string())
                    },
                )
            } else {
                (inner, None)
            };
            let target = raw_target.trim();
            if !target.is_empty() {
                results.push((target.to_string(), label));
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    results
}

/// Normaliza um slug de nota. Devolve `None` para entradas sem nenhum caractere
/// alfanumérico (ex. `"..."`, `"/"`) — caso em que [`slugify`] cairia no fallback
/// `"termo"` do léxico e colidiria lixo distinto no mesmo arquivo. Também é a
/// defesa contra path traversal (`../` vira `-`).
fn note_slug(raw: &str) -> Option<String> {
    if raw.chars().any(|c| c.is_alphanumeric()) {
        Some(slugify(raw))
    } else {
        None
    }
}

/// Resolve os wikilinks de um corpo a [`NoteLink`]s (alvo → slug). Aceita
/// `[[Minha Nota]]`, `[[minha-nota]]` e `[[notes/minha-nota.md]]` — todos
/// dobram para o mesmo slug.
fn links_from_body(body: &str) -> Vec<NoteLink> {
    extract_wikilinks(body)
        .into_iter()
        .map(|(raw, label)| NoteLink {
            target: slugify(&raw),
            label,
        })
        .filter(|l| !l.target.is_empty())
        .collect()
}

/// Backlinks: para cada slug-alvo, quem o referencia.
pub fn backlinks(notes: &[Note]) -> BTreeMap<String, Vec<Backlink>> {
    let mut map: BTreeMap<String, Vec<Backlink>> = BTreeMap::new();
    for n in notes {
        for l in &n.links {
            map.entry(l.target.clone()).or_default().push(Backlink {
                source: n.slug.clone(),
                label: l.label.clone(),
            });
        }
    }
    map
}

/// Grafo de adjacência `slug → [slug-alvo]`, restrito a alvos que existem (links
/// pendentes não viram arestas).
pub fn graph(notes: &[Note]) -> BTreeMap<String, Vec<String>> {
    let existing: BTreeSet<&str> = notes.iter().map(|n| n.slug.as_str()).collect();
    let mut adj = BTreeMap::new();
    for n in notes {
        let targets: Vec<String> = n
            .links
            .iter()
            .map(|l| l.target.clone())
            .filter(|t| existing.contains(t.as_str()))
            .collect();
        adj.insert(n.slug.clone(), targets);
    }
    adj
}

// ─── (De)serialização Markdown ────────────────────────────────────────────────

/// Renderiza o arquivo Markdown canônico (frontmatter + corpo verbatim). Mantém o
/// corpo intacto (sem injetar `# título`) para um round-trip limpo; o título mora
/// no frontmatter, como nas Entries do `co`.
fn render_note_markdown(
    slug: &str,
    title: &str,
    body: &str,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
) -> String {
    let mut s = String::from("---\n");
    s.push_str("type: nota\n");
    s.push_str(&format!("slug: {}\n", yaml_scalar(slug)));
    s.push_str(&format!("title: {}\n", yaml_scalar(title)));
    s.push_str(&format!(
        "created: {}\n",
        yaml_scalar(&created.to_rfc3339())
    ));
    s.push_str(&format!(
        "updated: {}\n",
        yaml_scalar(&updated.to_rfc3339())
    ));
    // links de saída como wikilinks — mesma convenção do léxico (`parts:`), para
    // que o resolver de FK tipado do `co` os reconheça na ingestão (Fase 2).
    let links = links_from_body(body);
    if !links.is_empty() {
        s.push_str("links:\n");
        for l in &links {
            s.push_str(&format!(
                "- {}\n",
                yaml_scalar(&format!("[[{}.md]]", l.target))
            ));
        }
    }
    s.push_str("contributed_via: yggdrasil-instance\n");
    s.push_str("---\n\n");
    s.push_str(body);
    if !body.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Lê título/datas do frontmatter e o corpo verbatim. Tolerante: arquivo sem
/// frontmatter vira nota só-corpo com defaults.
fn parse_note(slug: &str, raw: &str) -> Note {
    let (fm, body) = split_frontmatter(raw);
    let title = fm_get(&fm, "title").unwrap_or_else(|| slug.to_string());
    let created = fm_get(&fm, "created")
        .and_then(|s| parse_dt(&s))
        .unwrap_or_else(Utc::now);
    let updated = fm_get(&fm, "updated")
        .and_then(|s| parse_dt(&s))
        .unwrap_or(created);
    Note {
        slug: slug.to_string(),
        title,
        links: links_from_body(&body),
        body,
        created,
        updated,
    }
}

/// Separa `---\n<fm>\n---\n\n<body>`. Devolve `(fm, body)`; body preserva bytes.
fn split_frontmatter(raw: &str) -> (String, String) {
    if let Some(rest) = raw.strip_prefix("---\n")
        && let Some(pos) = rest.find("\n---\n")
    {
        let fm = rest[..pos].to_string();
        let after = &rest[pos + 5..]; // pula "\n---\n"
        let body = after.strip_prefix('\n').unwrap_or(after); // linha em branco separadora
        return (fm, body.to_string());
    }
    (String::new(), raw.to_string())
}

fn fm_get(fm: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}: ");
    fm.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|v| unquote_yaml(v.trim())))
}

/// Inverte [`yaml_scalar`]: tira aspas simples e desfaz o escape `''`.
fn unquote_yaml(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        t[1..t.len() - 1].replace("''", "'")
    } else {
        t.to_string()
    }
}

/// Escalar YAML seguro: aspas simples, com `'` duplicado (espelha
/// `comunicacao::lexicon::yaml_scalar`).
fn yaml_scalar(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn parse_dt(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (NoteStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let s = NoteStore::for_instance(dir.path(), "inst-1");
        (s, dir)
    }

    #[test]
    fn save_load_round_trip() {
        let (s, _d) = store();
        let saved = s
            .save("Minha Nota", "Minha Nota", "Corpo com [[outra-nota]].")
            .unwrap();
        assert_eq!(saved.slug, "minha-nota");
        let loaded = s.load("minha-nota").unwrap();
        assert_eq!(loaded.title, "Minha Nota");
        assert_eq!(loaded.body, "Corpo com [[outra-nota]].\n");
        assert_eq!(
            loaded.links,
            vec![NoteLink {
                target: "outra-nota".into(),
                label: None
            }]
        );
        assert_eq!(loaded.created, saved.created);
    }

    #[test]
    fn save_preserva_created_no_update() {
        let (s, _d) = store();
        let a = s.save("n", "N", "v1").unwrap();
        let b = s.save("n", "N", "v2 [[x]]").unwrap();
        assert_eq!(a.created, b.created, "created preservado");
        assert!(b.updated >= a.updated);
        assert_eq!(s.load("n").unwrap().body, "v2 [[x]]\n");
    }

    #[test]
    fn slug_sanitizado_contra_traversal() {
        let (s, d) = store();
        let n = s.save("../../etc/passwd", "x", "y").unwrap();
        assert_eq!(n.slug, "etc-passwd");
        // arquivo ficou dentro do dir de notas
        assert!(d.path().join("inst-1/notes/etc-passwd.md").exists());
    }

    #[test]
    fn slug_vazio_erro() {
        let (s, _d) = store();
        assert!(matches!(
            s.save("...", "x", "y"),
            Err(NoteError::InvalidSlug)
        ));
    }

    #[test]
    fn list_vazio_sem_dir() {
        let (s, _d) = store();
        assert!(s.list().unwrap().is_empty());
    }

    #[test]
    fn load_inexistente_notfound() {
        let (s, _d) = store();
        assert!(matches!(s.load("nope"), Err(NoteError::NotFound(_))));
    }

    #[test]
    fn delete_remove() {
        let (s, _d) = store();
        s.save("n", "N", "x").unwrap();
        s.delete("n").unwrap();
        assert!(matches!(s.load("n"), Err(NoteError::NotFound(_))));
        assert!(matches!(s.delete("n"), Err(NoteError::NotFound(_))));
    }

    #[test]
    fn wikilinks_com_alias_e_path() {
        let links = links_from_body("ver [[Outra Nota|alias]] e [[notes/terceira.md]]");
        assert_eq!(links.len(), 2);
        assert_eq!(
            links[0],
            NoteLink {
                target: "outra-nota".into(),
                label: Some("alias".into())
            }
        );
        assert_eq!(
            links[1],
            NoteLink {
                target: "notes-terceira-md".into(),
                label: None
            }
        );
    }

    // ── DraftStore (YG-125) ───────────────────────────────────────────────

    #[test]
    fn draft_round_trip_e_descartar() {
        let dir = tempdir().unwrap();
        let d = DraftStore::for_user(dir.path(), "inst-1", "yuri@artelonga.com.br");
        d.save("minha-nota", "# rascunho\ncorpo").unwrap();
        let got = d.load("minha-nota").unwrap();
        assert_eq!(got.markdown, "# rascunho\ncorpo");
        assert_eq!(got.slug, "minha-nota");
        d.delete("minha-nota").unwrap();
        assert!(matches!(d.load("minha-nota"), Err(NoteError::NotFound(_))));
        assert!(matches!(
            d.delete("minha-nota"),
            Err(NoteError::NotFound(_))
        ));
    }

    #[test]
    fn draft_fica_fora_de_notes_e_isolado_por_usuario() {
        let dir = tempdir().unwrap();
        let a = DraftStore::for_user(dir.path(), "i", "ana@x.com");
        let b = DraftStore::for_user(dir.path(), "i", "bia@x.com");
        a.save("n", "de ana").unwrap();
        assert!(matches!(b.load("n"), Err(NoteError::NotFound(_))));
        // nada vaza para notes/ (o caminho que o producer federa)
        assert!(!dir.path().join("i/notes").exists());
        assert!(dir.path().join("i/drafts").exists());
    }

    #[test]
    fn draft_slug_invalido_rejeitado() {
        let dir = tempdir().unwrap();
        let d = DraftStore::for_user(dir.path(), "i", "u");
        // o slug é normalizado por note_slug/slugify — nunca escapa do dir
        d.save("../escape", "x").unwrap();
        assert!(dir.path().join("i/drafts/u/escape.md").exists());
        assert!(!dir.path().parent().unwrap().join("escape.md").exists());
        assert!(matches!(d.save("///", "x"), Err(NoteError::InvalidSlug)));
    }

    #[test]
    fn backlinks_invertem() {
        let (s, _d) = store();
        s.save("a", "A", "liga [[b]]").unwrap();
        s.save("b", "B", "sem links").unwrap();
        let notes = s.list().unwrap();
        let back = backlinks(&notes);
        assert_eq!(back.get("b").unwrap()[0].source, "a");
        assert!(back.get("a").is_none());
    }

    #[test]
    fn graph_filtra_alvos_inexistentes() {
        let (s, _d) = store();
        s.save("a", "A", "[[b]] e [[fantasma]]").unwrap();
        s.save("b", "B", "").unwrap();
        let notes = s.list().unwrap();
        let g = graph(&notes);
        assert_eq!(g.get("a").unwrap(), &vec!["b".to_string()]);
        assert_eq!(g.get("b").unwrap().len(), 0);
    }

    #[test]
    fn frontmatter_round_trip_com_aspas() {
        let (s, _d) = store();
        // título com apóstrofo exercita o escape '' do YAML
        let n = s.save("nhe-e", "Nhe'ẽ", "fala").unwrap();
        assert_eq!(n.title, "Nhe'ẽ");
        assert_eq!(s.load("nhe-e").unwrap().title, "Nhe'ẽ");
    }
}
