//! Ponte sala → léxico: lê e escreve entradas Markdown no repositório
//! `comunicacao` (mesmo formato de `yoruba/terms/*.md`,
//! `guarani-mbya/terms/*.md`).
//!
//! Quando um usuário **publica** um elemento da sua sala:
//! - se a palavra já existe no léxico compartilhado → apenas **liga** a ela
//!   ([`LexiconLink::Linked`]); não duplica.
//! - se não existe → cria uma entrada nova sob
//!   `<lingua>/terms/_users/<usuario>/<slug>.md`, atribuída ao usuário
//!   ([`LexiconLink::Contributed`]).
//!
//! O disco do repo `comunicacao` é a fonte da verdade — o servidor só
//! acrescenta; nunca sobrescreve entradas curadas do léxico compartilhado.

use std::path::{Path, PathBuf};

use chrono::Utc;

use super::room::{Element, LexiconLink};

#[derive(Debug, thiserror::Error)]
pub enum LexiconError {
    #[error("língua sem plano de léxico: '{0}'")]
    UnsupportedLang(String),
    #[error("palavra vazia")]
    EmptyWord,
    #[error("entrada de léxico não encontrada: '{0}'")]
    NotFound(String),
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Escopo onde uma entrada de léxico vive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexiconScope {
    /// Léxico compartilhado, curado (`<lingua>/terms/<slug>.md`).
    Shared,
    /// Contribuição de usuário (`<lingua>/terms/_users/<user>/<slug>.md`).
    User,
}

/// Resultado de promover um stub ao léxico curado (YG-101).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promotion {
    /// Caminho (relativo ao repo) do stub de usuário arquivado.
    pub from_rel: String,
    /// Caminho (relativo) do termo curado resultante (`<lang>/terms/<slug>.md`).
    pub to_rel: String,
    /// Slug canônico do termo.
    pub slug: String,
    /// `true` se este promote criou o termo curado (`false` = já existia).
    pub created: bool,
    /// `true` se o stub de usuário foi removido (arquivado).
    pub archived: bool,
}

/// Resultado de uma publicação no léxico.
#[derive(Debug, Clone)]
pub struct Contribution {
    /// Caminho relativo ao repo `comunicacao` (sempre com `/`).
    pub relative_path: String,
    /// `true` se um arquivo novo foi criado; `false` se já existia (ligado).
    pub created: bool,
    pub scope: LexiconScope,
    pub link: LexiconLink,
}

/// Store sobre um checkout do repositório `comunicacao`.
pub struct LexiconStore {
    root: PathBuf,
}

impl LexiconStore {
    /// Enraíza no diretório do repo `comunicacao`. Não cria o diretório —
    /// ele deve ser um checkout existente (a publicação cria só subpastas
    /// `terms/_users/...`).
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Pasta-plano da língua dentro do repo. `None` se a língua não tem plano.
    pub fn lang_dir(code: &str) -> Option<&'static str> {
        match code {
            "gn-mbya" | "gn" => Some("guarani-mbya"),
            "yo" => Some("yoruba"),
            "pt" | "pt-br" => Some("portuguese"),
            "es" => Some("spanish"),
            _ => None,
        }
    }

    fn dir_for(&self, lang: &str) -> Result<&'static str, LexiconError> {
        Self::lang_dir(lang).ok_or_else(|| LexiconError::UnsupportedLang(lang.to_string()))
    }

    /// Procura uma entrada existente (compartilhada tem prioridade sobre user).
    /// Não recebe `user` → busca só o léxico compartilhado.
    pub fn lookup_shared(&self, lang: &str, word: &str) -> Result<Option<String>, LexiconError> {
        let dir = self.dir_for(lang)?;
        let slug = slugify(word);
        let rel = format!("{dir}/terms/{slug}.md");
        if self.root.join(&rel).is_file() {
            Ok(Some(rel))
        } else {
            Ok(None)
        }
    }

    /// Publica um elemento no léxico. Idempotente: republicar liga ao arquivo
    /// já criado sem reescrever. `parts` são as palavras-parente (composição
    /// fractal/etimológica) — viram `parts:` no frontmatter como wikilinks.
    pub fn contribute(
        &self,
        user: &str,
        el: &Element,
        parts: &[String],
    ) -> Result<Contribution, LexiconError> {
        if el.word.trim().is_empty() {
            return Err(LexiconError::EmptyWord);
        }
        let dir = self.dir_for(&el.lang)?;
        let slug = slugify(&el.word);

        // 1) já está no léxico compartilhado? → apenas liga.
        if let Some(shared_rel) = self.lookup_shared(&el.lang, &el.word)? {
            return Ok(Contribution {
                relative_path: shared_rel.clone(),
                created: false,
                scope: LexiconScope::Shared,
                link: LexiconLink::Linked { path: shared_rel },
            });
        }

        // 2) cria/atualiza a entrada de usuário.
        let user_slug = slugify(user);
        let rel = format!("{dir}/terms/_users/{user_slug}/{slug}.md");
        let abs = self.root.join(&rel);
        let already = abs.is_file();
        if !already {
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let md = render_term_markdown(user, el, parts);
            // escrita atômica: temp + rename no mesmo diretório
            let tmp = abs.with_extension("md.tmp");
            std::fs::write(&tmp, md)?;
            std::fs::rename(&tmp, &abs)?;
        }
        Ok(Contribution {
            relative_path: rel.clone(),
            created: !already,
            scope: LexiconScope::User,
            link: LexiconLink::Contributed { path: rel },
        })
    }

    /// Promove uma contribuição de usuário (`_users/<u>/<slug>.md`, `seed_status:
    /// stub`) ao léxico **curado** (`terms/<slug>.md`, `reviewed`) — YG-101. Lê o
    /// stub, vira o frontmatter (`stub → reviewed` + `reviewed_by`/`reviewed_at`),
    /// grava o termo curado atomicamente e **arquiva** (remove) o stub; republicar
    /// depois religa via [`lookup_shared`]. Idempotente: se o termo curado já
    /// existe, só garante a remoção do stub. `LexiconError::NotFound` se não há
    /// stub nem termo curado.
    pub fn promote(
        &self,
        lang: &str,
        user: &str,
        slug: &str,
        curator: &str,
    ) -> Result<Promotion, LexiconError> {
        let dir = self.dir_for(lang)?;
        let slug = slugify(slug);
        let user_slug = slugify(user);
        let from_rel = format!("{dir}/terms/_users/{user_slug}/{slug}.md");
        let to_rel = format!("{dir}/terms/{slug}.md");
        let from_abs = self.root.join(&from_rel);
        let to_abs = self.root.join(&to_rel);

        let already_curated = to_abs.is_file();
        if !already_curated {
            // precisa do stub p/ promover.
            let raw = std::fs::read_to_string(&from_abs).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    LexiconError::NotFound(from_rel.clone())
                } else {
                    LexiconError::Io(e)
                }
            })?;
            let reviewed = mark_reviewed(&raw, curator);
            if let Some(parent) = to_abs.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = to_abs.with_extension("md.tmp");
            std::fs::write(&tmp, reviewed)?;
            std::fs::rename(&tmp, &to_abs)?;
        } else if !from_abs.is_file() {
            // já curado e sem stub: nada a fazer (idempotente).
            return Ok(Promotion {
                from_rel,
                to_rel,
                slug,
                created: false,
                archived: false,
            });
        }
        let archived = std::fs::remove_file(&from_abs).is_ok();
        Ok(Promotion {
            from_rel,
            to_rel,
            slug,
            created: !already_curated,
            archived,
        })
    }
}

/// Vira o frontmatter de um termo de `seed_status: stub` p/ `reviewed`, anexando
/// `reviewed_by`/`reviewed_at`. Se não houver linha `seed_status: stub`
/// (inesperado), injeta o carimbo logo após o `---` de abertura.
fn mark_reviewed(raw: &str, curator: &str) -> String {
    let now = Utc::now().to_rfc3339();
    // carimbo da linha de frontmatter (terminada em `\n` — não casa a menção
    // inline na prosa, que vem entre crases).
    let stamp = format!(
        "seed_status: reviewed\nreviewed_by: {}\nreviewed_at: {}\n",
        yaml_scalar(curator),
        yaml_scalar(&now)
    );
    let mut out = if raw.contains("seed_status: stub\n") {
        raw.replacen("seed_status: stub\n", &stamp, 1)
    } else {
        raw.replacen("---\n", &format!("---\n{stamp}"), 1)
    };
    // atualiza a nota de stub na prosa (gerada por `render_term_markdown`) p/ não
    // reaparecer como pendente após a curadoria.
    out = out.replace(
        "`seed_status: stub` — pendente de revisão por falante/curador.",
        "`seed_status: reviewed` — curado.",
    );
    out
}

/// Renderiza o Markdown de uma entrada `term` no formato do repo `comunicacao`.
fn render_term_markdown(user: &str, el: &Element, parts: &[String]) -> String {
    let mut fm = String::from("---\n");
    fm.push_str("type: term\n");
    fm.push_str(&format!("word: {}\n", yaml_scalar(&el.word)));
    fm.push_str(&format!("language_code: {}\n", yaml_scalar(&el.lang)));
    if let Some(p) = &el.pronunciation {
        fm.push_str(&format!("pronunciation: {}\n", yaml_scalar(p)));
    }
    if let Some(c) = &el.concept {
        fm.push_str(&format!("concept: {}\n", yaml_scalar(c)));
    }
    // parts: morfemas/termos-parente (composição fractal) como wikilinks
    if !parts.is_empty() {
        fm.push_str("parts:\n");
        for p in parts {
            fm.push_str(&format!(
                "- {}\n",
                yaml_scalar(&format!("[[terms/{}.md]]", slugify(p)))
            ));
        }
    }
    fm.push_str("seed_status: stub\n");
    fm.push_str(&format!("author: {}\n", yaml_scalar(user)));
    fm.push_str("contributed_via: yggdrasil-comunicacao\n");
    fm.push_str(&format!(
        "created: {}\n",
        yaml_scalar(&Utc::now().to_rfc3339())
    ));
    if !el.refs.is_empty() {
        fm.push_str("references:\n");
        for r in &el.refs {
            fm.push_str(&format!(
                "- {{ url: {}, source: {} }}\n",
                yaml_scalar(&r.url),
                yaml_scalar(&r.source)
            ));
        }
    }
    fm.push_str("tags:\n- term\n- user-contributed\n");
    fm.push_str("---\n\n");

    let mut body = String::new();
    body.push_str(&format!("# {}\n\n", el.word));
    match &el.gloss {
        Some(g) if !g.trim().is_empty() => body.push_str(&format!("{g}\n\n")),
        _ => body.push_str("_(sem definição ainda — pendente de revisão)_\n\n"),
    }
    if let Some(c) = &el.concept {
        body.push_str(&format!("Conceito âncora: `{c}`\n\n"));
    }
    if !el.refs.is_empty() {
        body.push_str("## Referências\n\n");
        for r in &el.refs {
            body.push_str(&format!("- [{}]({})\n", r.source, r.url));
        }
        body.push('\n');
    }
    body.push_str(&format!(
        "> Contribuído por `{user}` via Yggdrasil (sala de comunicação). \
         `seed_status: stub` — pendente de revisão por falante/curador.\n"
    ));

    format!("{fm}{body}")
}

/// Escreve um escalar YAML seguro: aspas simples, com `'` duplicado.
fn yaml_scalar(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Fold de diacríticos + tom (yoruba/mbya/pt) para um caractere ASCII base.
/// `None` = descartar (marcas combinantes U+0300..U+036F).
fn fold_char(c: char) -> Option<char> {
    if ('\u{300}'..='\u{36F}').contains(&c) {
        return None;
    }
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ẹ' | 'ɛ' | 'ẽ' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'ĩ' => 'i',
        'ñ' | 'ŋ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ọ' | 'ɔ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ũ' => 'u',
        'ṣ' => 's',
        'ý' | 'ÿ' | 'ỹ' => 'y',
        other => other,
    })
}

/// Transforma uma palavra em slug de arquivo: minúsculas, sem diacríticos,
/// separadores `-`. `iemanjá → iemanja`, `nhe'ẽ → nhe-e`, `ara pyau → ara-pyau`.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true; // evita `-` inicial
    for ch in s.chars() {
        // Minúsculas PRIMEIRO: assim letras acentuadas maiúsculas (`Ò`, `È`)
        // viram `ò`/`è` e então são dobradas pela tabela (que é lowercase).
        for lc in ch.to_lowercase() {
            let Some(folded) = fold_char(lc) else {
                continue;
            };
            if folded.is_ascii_alphanumeric() {
                out.push(folded);
                prev_dash = false;
            } else if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("termo");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn seeded_repo() -> (LexiconStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        // simula o léxico compartilhado: yoruba/terms/iemanja.md
        let shared = dir.path().join("yoruba/terms");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("iemanja.md"), "---\nword: iemanjá\n---\n").unwrap();
        (LexiconStore::new(dir.path()), dir)
    }

    #[test]
    fn slugify_remove_diacriticos_e_tom() {
        assert_eq!(slugify("iemanjá"), "iemanja");
        assert_eq!(slugify("àṣẹ"), "ase");
        assert_eq!(slugify("ọ̀rọ̀"), "oro");
        assert_eq!(slugify("nhe'ẽ"), "nhe-e");
        assert_eq!(slugify("ara pyau"), "ara-pyau");
        assert_eq!(slugify("Nhandereko"), "nhandereko");
        assert_eq!(slugify("  --  "), "termo");
    }

    #[test]
    fn slugify_dobra_maiusculas_acentuadas() {
        // Letras acentuadas MAIÚSCULAS (início de palavra) precisam ser dobradas
        // tanto quanto as minúsculas — regressão do bug "Òrúnmìlà → runmila".
        assert_eq!(slugify("Òrúnmìlà"), "orunmila");
        assert_eq!(slugify("Èṣù"), "esu");
        assert_eq!(slugify("Egbé Òrun"), "egbe-orun");
        assert_eq!(slugify("Olódùmarè"), "olodumare");
        assert_eq!(slugify("Ñamandu"), "namandu");
        assert_eq!(slugify("Yemọja"), "yemoja");
        assert_eq!(slugify("Ifá"), "ifa");
    }

    #[test]
    fn lang_dir_mapeia_codigos() {
        assert_eq!(LexiconStore::lang_dir("yo"), Some("yoruba"));
        assert_eq!(LexiconStore::lang_dir("gn-mbya"), Some("guarani-mbya"));
        assert_eq!(LexiconStore::lang_dir("pt-br"), Some("portuguese"));
        assert_eq!(LexiconStore::lang_dir("es"), Some("spanish"));
        assert_eq!(LexiconStore::lang_dir("klingon"), None);
    }

    #[test]
    fn contribute_lingua_sem_plano_falha() {
        let (store, _d) = seeded_repo();
        let el = Element::new("e1", "Qapla'", "klingon");
        assert!(matches!(
            store.contribute("alice", &el, &[]),
            Err(LexiconError::UnsupportedLang(_))
        ));
    }

    #[test]
    fn contribute_palavra_existente_liga_sem_duplicar() {
        let (store, _d) = seeded_repo();
        let el = Element::new("e1", "iemanjá", "yo");
        let c = store.contribute("alice", &el, &[]).unwrap();
        assert!(!c.created, "não deve criar — já existe no compartilhado");
        assert_eq!(c.scope, LexiconScope::Shared);
        assert_eq!(c.relative_path, "yoruba/terms/iemanja.md");
        assert!(matches!(c.link, LexiconLink::Linked { .. }));
        // nenhum arquivo de usuário foi criado
        assert!(!store.root().join("yoruba/terms/_users").exists());
    }

    #[test]
    fn contribute_palavra_nova_cria_entrada_de_usuario() {
        let (store, _d) = seeded_repo();
        let el = Element::new("e1", "àṣẹ", "yo")
            .with_gloss("força vital; 'assim seja'")
            .with_pronunciation("[à.ʃɛ]")
            .with_concept("co://concepts/word.md");
        let c = store.contribute("yuri@x.com", &el, &[]).unwrap();
        assert!(c.created);
        assert_eq!(c.scope, LexiconScope::User);
        assert_eq!(c.relative_path, "yoruba/terms/_users/yuri-x-com/ase.md");
        let written = std::fs::read_to_string(store.root().join(&c.relative_path)).unwrap();
        assert!(
            written.contains("word: 'àṣẹ'"),
            "frontmatter word\n{written}"
        );
        assert!(written.contains("language_code: 'yo'"));
        assert!(written.contains("author: 'yuri@x.com'"));
        assert!(written.contains("contributed_via: yggdrasil-comunicacao"));
        assert!(written.contains("força vital"));
        assert!(written.contains("# àṣẹ"));
    }

    #[test]
    fn contribute_idempotente() {
        let (store, _d) = seeded_repo();
        let el = Element::new("e1", "tekoa", "gn-mbya").with_gloss("aldeia");
        let first = store.contribute("alice", &el, &[]).unwrap();
        assert!(first.created);
        let second = store.contribute("alice", &el, &[]).unwrap();
        assert!(!second.created, "republicar não recria");
        assert_eq!(first.relative_path, second.relative_path);
        assert_eq!(
            first.relative_path,
            "guarani-mbya/terms/_users/alice/tekoa.md"
        );
    }

    #[test]
    fn contribute_com_parts_escreve_wikilinks() {
        let (store, _d) = seeded_repo();
        // kuarahy ← kuaa + ra + hy (composição/etimologia fractal)
        let el = Element::new("e1", "kuarahy", "gn-mbya").with_gloss("sol");
        let parts = vec!["kuaa".to_string(), "ra".to_string(), "hy".to_string()];
        let c = store.contribute("alice", &el, &parts).unwrap();
        let md = std::fs::read_to_string(store.root().join(&c.relative_path)).unwrap();
        assert!(md.contains("parts:"), "frontmatter deve ter parts\n{md}");
        assert!(md.contains("[[terms/kuaa.md]]"));
        assert!(md.contains("[[terms/ra.md]]"));
        assert!(md.contains("[[terms/hy.md]]"));
    }

    // ── YG-101: curadoria (promote stub → reviewed) ───────────────────────

    #[test]
    fn promote_move_stub_para_curado_e_vira_reviewed() {
        let (store, _dir) = seeded_repo();
        let el = Element::new("e1", "tekoa", "gn-mbya").with_gloss("aldeia");
        let c = store.contribute("alice", &el, &[]).unwrap();
        assert_eq!(c.scope, LexiconScope::User);
        let stub_abs = store.root().join(&c.relative_path);
        assert!(stub_abs.is_file(), "stub criado");
        assert!(
            std::fs::read_to_string(&stub_abs)
                .unwrap()
                .contains("seed_status: stub")
        );

        let p = store
            .promote("gn-mbya", "alice", "tekoa", "yuri@curador")
            .unwrap();
        assert_eq!(p.to_rel, "guarani-mbya/terms/tekoa.md");
        assert_eq!(p.from_rel, "guarani-mbya/terms/_users/alice/tekoa.md");
        assert!(p.created, "termo curado criado");
        assert!(p.archived, "stub removido");

        // curado existe, com seed_status reviewed + atribuição
        let curado = std::fs::read_to_string(store.root().join(&p.to_rel)).unwrap();
        assert!(curado.contains("seed_status: reviewed"));
        assert!(!curado.contains("seed_status: stub"));
        assert!(curado.contains("reviewed_by: 'yuri@curador'"));
        assert!(curado.contains("reviewed_at:"));
        // stub arquivado (removido)
        assert!(!stub_abs.is_file(), "stub arquivado");
    }

    #[test]
    fn promote_idempotente_se_ja_curado() {
        let (store, _dir) = seeded_repo();
        let el = Element::new("e1", "ara", "gn-mbya").with_gloss("tempo");
        store.contribute("alice", &el, &[]).unwrap();
        let first = store.promote("gn-mbya", "alice", "ara", "c").unwrap();
        assert!(first.created);
        // segundo promote: stub já não existe, termo curado já existe → no-op.
        let second = store.promote("gn-mbya", "alice", "ara", "c").unwrap();
        assert!(!second.created, "não recria o curado");
        assert!(!second.archived, "nada a arquivar");
        assert!(store.root().join(&second.to_rel).is_file());
    }

    #[test]
    fn promote_sem_stub_nem_curado_e_notfound() {
        let (store, _dir) = seeded_repo();
        let err = store
            .promote("gn-mbya", "alice", "inexistente", "c")
            .unwrap_err();
        assert!(matches!(err, LexiconError::NotFound(_)));
    }

    #[test]
    fn promote_lingua_sem_plano_falha() {
        let (store, _dir) = seeded_repo();
        assert!(matches!(
            store.promote("klingon", "alice", "qapla", "c"),
            Err(LexiconError::UnsupportedLang(_))
        ));
    }
}
