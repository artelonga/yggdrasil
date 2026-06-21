//! Projeção CO-universe → LexiconPack (YG-169).
//!
//! Varre `<plano>/terms/**/*.md` do repo `comunicacao` (somente leitura) e emite
//! um [`LexiconPack`] com `kind=Language`, `notation=ipa` e `audio: Speech` por
//! palavra. Memory-light: projetado sob demanda, sem residência pesada; cap em
//! [`MAX_RESIDENT_ENTRIES`].

use std::collections::HashMap;
use std::path::Path;

use super::lexicon::slugify;
use super::pack::{
    AbstractPos, EntropyStats, EntryAudio, LexiconPack, MAX_RESIDENT_ENTRIES, PackEntry, PackKind,
    Relation,
};
use super::room::Reference;

/// `true` se o plano CO é conhecido (tem diretório de léxico mapeado).
pub fn is_known_plano(plano: &str) -> bool {
    matches!(plano, "yoruba" | "guarani-mbya" | "portuguese" | "spanish")
}

fn bcp47(plano: &str) -> &'static str {
    match plano {
        "yoruba" => "yo",
        "guarani-mbya" => "gn-mbya",
        "portuguese" => "pt",
        "spanish" => "es",
        _ => "und",
    }
}

fn plano_title(plano: &str) -> String {
    match plano {
        "yoruba" => "Iorubá — léxico CO".to_string(),
        "guarani-mbya" => "Guaraní Mbyá — léxico CO".to_string(),
        "portuguese" => "Português — léxico CO".to_string(),
        "spanish" => "Espanhol — léxico CO".to_string(),
        _ => format!("{plano} — léxico CO"),
    }
}

fn plano_theme(plano: &str) -> &'static str {
    match plano {
        "yoruba" => "savana",
        "guarani-mbya" => "floresta",
        "portuguese" => "oceano",
        _ => "neutro",
    }
}

struct ParsedTerm {
    word: String,
    ipa: Option<String>,
    gloss: Option<String>,
    parts: Vec<String>,
    refs: Vec<(String, String)>,
}

fn strip_yaml_scalar(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 {
        if s.starts_with('\'') && s.ends_with('\'') {
            return s[1..s.len() - 1].replace("''", "'");
        }
        if s.starts_with('"') && s.ends_with('"') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Extrai o slug de um wikilink `[[terms/<slug>.md]]` (com ou sem aspas YAML).
fn wikilink_slug(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let brackets = if raw.len() >= 2
        && ((raw.starts_with('\'') && raw.ends_with('\''))
            || (raw.starts_with('"') && raw.ends_with('"')))
    {
        raw[1..raw.len() - 1].trim()
    } else {
        raw
    };
    let inner = brackets.strip_prefix("[[")?.strip_suffix("]]")?;
    let slug = inner.strip_prefix("terms/")?.strip_suffix(".md")?;
    if slug.is_empty() {
        None
    } else {
        Some(slug.to_string())
    }
}

/// Primeiro parágrafo do corpo Markdown após o `# heading`.
fn body_gloss(body: &str) -> Option<String> {
    let mut past_heading = false;
    let mut lines: Vec<&str> = Vec::new();
    for line in body.lines() {
        if line.starts_with('#') {
            if !lines.is_empty() {
                break;
            }
            past_heading = true;
            continue;
        }
        if !past_heading {
            continue;
        }
        if line.trim().is_empty() {
            if !lines.is_empty() {
                break;
            }
        } else {
            lines.push(line.trim());
        }
    }
    if lines.is_empty() {
        return None;
    }
    let s = lines.join(" ");
    // ignora placeholder gerado automaticamente
    if s.starts_with("_(sem definição") {
        None
    } else {
        Some(s)
    }
}

/// Parseia `- { url: '...', source: '...' }` em `(url, source)`.
fn parse_inline_ref(s: &str) -> Option<(String, String)> {
    let s = s.trim().strip_prefix('{')?.strip_suffix('}')?.trim();
    let mut url = String::new();
    let mut source = String::new();
    if let Some(after_url) = s.find("url: ") {
        let v = &s[after_url + 5..];
        let end = v.find(", source:").unwrap_or(v.len());
        url = strip_yaml_scalar(v[..end].trim());
    }
    if let Some(after_src) = s.find("source: ") {
        let v = &s[after_src + 8..];
        let end = v.find(", ").unwrap_or(v.len());
        source = strip_yaml_scalar(v[..end].trim());
    }
    if url.is_empty() && source.is_empty() {
        None
    } else {
        Some((url, source))
    }
}

/// Parseia um arquivo `.md` de termo e extrai os campos relevantes.
/// Retorna `None` se não há frontmatter ou campo `word:`.
fn parse_term_md(content: &str) -> Option<ParsedTerm> {
    let rest = content.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let fm = &rest[..end];
    let body = &rest[end + 5..];

    let mut word: Option<String> = None;
    let mut ipa: Option<String> = None;
    let mut parts: Vec<String> = Vec::new();
    let mut refs: Vec<(String, String)> = Vec::new();

    #[derive(PartialEq)]
    enum Sec {
        None,
        Parts,
        Refs,
    }
    let mut sec = Sec::None;

    for line in fm.lines() {
        if let Some(v) = line.strip_prefix("word: ") {
            word = Some(strip_yaml_scalar(v));
            sec = Sec::None;
        } else if let Some(v) = line.strip_prefix("pronunciation: ") {
            ipa = Some(strip_yaml_scalar(v));
            sec = Sec::None;
        } else if line == "parts:" {
            sec = Sec::Parts;
        } else if line == "references:" {
            sec = Sec::Refs;
        } else if let Some(rest) = line.strip_prefix("- ") {
            match sec {
                Sec::Parts => {
                    if let Some(slug) = wikilink_slug(rest) {
                        parts.push(slug);
                    }
                }
                Sec::Refs => {
                    if let Some(pair) = parse_inline_ref(rest) {
                        refs.push(pair);
                    }
                }
                Sec::None => {}
            }
        } else if !line.is_empty() && !line.starts_with(' ') && !line.starts_with('\t') {
            // nova chave raiz → reset de seção de lista
            sec = Sec::None;
        }
    }

    Some(ParsedTerm {
        word: word?,
        ipa,
        gloss: body_gloss(body),
        parts,
        refs,
    })
}

fn collect_md(dir: &Path, out: &mut Vec<ParsedTerm>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_md(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("md")
            && let Ok(content) = std::fs::read_to_string(&p)
            && let Some(term) = parse_term_md(&content)
        {
            out.push(term);
        }
    }
}

/// Projeta o plano CO `plano` em [`LexiconPack`] — somente leitura, memory-light.
///
/// Varre `<root>/<plano>/terms/**/*.md` (incluindo `_users/`), mapeia frontmatter
/// → [`PackEntry`] com `audio: Speech`, `relations` de wikilinks/parts e `pos` em
/// grade determinística (ordem alfabética). Cap em [`MAX_RESIDENT_ENTRIES`].
pub fn pack_for_language(plano: &str, root: &Path) -> LexiconPack {
    let lang = bcp47(plano);
    let mut terms: Vec<ParsedTerm> = Vec::new();
    collect_md(&root.join(plano).join("terms"), &mut terms);

    // Ordenação determinística: lexicográfica Unicode pela palavra
    terms.sort_by(|a, b| a.word.cmp(&b.word));
    terms.truncate(MAX_RESIDENT_ENTRIES);

    // Mapa slug → word para resolver relações entre termos do mesmo pack
    let slug_map: HashMap<String, String> = terms
        .iter()
        .map(|t| (slugify(&t.word), t.word.clone()))
        .collect();

    // Grade: cols = ceil(sqrt(n)) — layout estável e reprodutível
    let n = terms.len();
    let cols = if n == 0 {
        1
    } else {
        (n as f64).sqrt().ceil() as usize
    }
    .max(1);

    let entries: Vec<PackEntry> = terms
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let mut e = PackEntry::new(
                t.word.clone(),
                AbstractPos::new((i % cols) as f64, (i / cols) as f64, 0.0),
            )
            .with_role("word")
            .with_audio(EntryAudio::Speech {
                text: t.word.clone(),
                ipa: t.ipa,
                lang: lang.to_string(),
            });

            if let Some(g) = t.gloss {
                e.gloss = Some(g);
            }

            for slug in &t.parts {
                if let Some(target) = slug_map.get(slug.as_str()) {
                    e.relations.push(Relation::new(target, "parte"));
                }
            }

            for (url, source) in t.refs {
                e.refs.push(Reference {
                    url,
                    source,
                    kind: None,
                });
            }

            e
        })
        .collect();

    LexiconPack {
        id: format!("lang:{plano}"),
        kind: PackKind::Language,
        notation: "ipa".to_string(),
        title: plano_title(plano),
        theme: plano_theme(plano).to_string(),
        entropy_stats: Some(EntropyStats::uniform(entries.len())),
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn mk(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn seed_fixture(root: &Path) {
        mk(
            root,
            "yoruba/terms/ase.md",
            "---\ntype: term\nword: 'àṣẹ'\nlanguage_code: 'yo'\n\
             pronunciation: '[à.ʃɛ]'\nseed_status: reviewed\n---\n\n\
             # àṣẹ\n\nforça vital; 'assim seja'\n",
        );
        mk(
            root,
            "yoruba/terms/ile.md",
            "---\ntype: term\nword: 'ilẹ̀'\nlanguage_code: 'yo'\n\
             pronunciation: '[ì.lɛ̀]'\nparts:\n- '[[terms/ase.md]]'\n\
             references:\n- { url: 'https://example.org', source: 'Ref A' }\n\
             seed_status: reviewed\n---\n\n# ilẹ̀\n\nterra, chão, casa\n",
        );
        mk(
            root,
            "yoruba/terms/_users/alice/ina.md",
            "---\ntype: term\nword: 'iná'\nlanguage_code: 'yo'\n\
             seed_status: stub\nauthor: 'alice'\n\
             contributed_via: yggdrasil-comunicacao\ncreated: '2026-06-21T00:00:00Z'\n\
             ---\n\n# iná\n\nfogo, chama\n",
        );
    }

    #[test]
    fn fixture_tres_termos() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let pack = pack_for_language("yoruba", dir.path());
        assert_eq!(pack.kind, PackKind::Language);
        assert_eq!(pack.notation, "ipa");
        assert_eq!(pack.id, "lang:yoruba");
        assert_eq!(pack.entries.len(), 3);
    }

    #[test]
    fn gloss_e_ipa_extraidos() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let pack = pack_for_language("yoruba", dir.path());
        let ase = pack.entry("àṣẹ").expect("àṣẹ ausente");
        assert_eq!(ase.gloss.as_deref(), Some("força vital; 'assim seja'"));
        match ase.audio.as_ref().unwrap() {
            EntryAudio::Speech { ipa, lang, text } => {
                assert_eq!(ipa.as_deref(), Some("[à.ʃɛ]"));
                assert_eq!(lang, "yo");
                assert_eq!(text, "àṣẹ");
            }
            other => panic!("esperava Speech, achei {other:?}"),
        }
    }

    #[test]
    fn relations_de_wikilinks() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let pack = pack_for_language("yoruba", dir.path());
        let ile = pack.entry("ilẹ̀").expect("ilẹ̀ ausente");
        assert!(
            ile.relations.iter().any(|r| r.to == "àṣẹ"),
            "relação wikilink ilẹ̀→àṣẹ ausente; relações: {:?}",
            ile.relations
        );
    }

    #[test]
    fn refs_bibliograficas_presentes() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let pack = pack_for_language("yoruba", dir.path());
        let ile = pack.entry("ilẹ̀").expect("ilẹ̀ ausente");
        assert!(
            ile.refs.iter().any(|r| r.source == "Ref A"),
            "ref bibliográfica ausente"
        );
    }

    #[test]
    fn inclui_contribuicoes_de_usuario() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let pack = pack_for_language("yoruba", dir.path());
        assert!(
            pack.entries.iter().any(|e| e.term == "iná"),
            "iná (_users/) deve estar no pack"
        );
    }

    #[test]
    fn pos_deterministica_e_ordem_alfabetica() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let p1 = pack_for_language("yoruba", dir.path());
        let p2 = pack_for_language("yoruba", dir.path());
        for (e1, e2) in p1.entries.iter().zip(p2.entries.iter()) {
            assert_eq!(e1.term, e2.term, "ordem inconsistente entre projeções");
            assert_eq!(e1.pos, e2.pos, "posição inconsistente entre projeções");
        }
        let words: Vec<&str> = p1.entries.iter().map(|e| e.term.as_str()).collect();
        let mut sorted = words.clone();
        sorted.sort();
        assert_eq!(words, sorted, "entradas devem estar em ordem alfabética");
    }

    #[test]
    fn entropy_stats_preenchido() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let pack = pack_for_language("yoruba", dir.path());
        let stats = pack.entropy_stats.expect("entropy_stats ausente");
        assert_eq!(stats.symbols, 3);
        assert!((stats.bits_per_symbol - 3f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn somente_leitura_nao_muta_markdown() {
        let dir = tempdir().unwrap();
        seed_fixture(dir.path());
        let path = dir.path().join("yoruba/terms/ase.md");
        let before = std::fs::read_to_string(&path).unwrap();
        let _ = pack_for_language("yoruba", dir.path());
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            before, after,
            "pack_for_language não deve modificar o markdown"
        );
    }

    #[test]
    fn cap_max_resident_entries() {
        let dir = tempdir().unwrap();
        let terms_dir = dir.path().join("yoruba/terms");
        std::fs::create_dir_all(&terms_dir).unwrap();
        for i in 0..MAX_RESIDENT_ENTRIES + 10 {
            std::fs::write(
                terms_dir.join(format!("w{i:04}.md")),
                format!(
                    "---\ntype: term\nword: 'w{i:04}'\nlanguage_code: 'yo'\n---\n\n# w{i:04}\n\ngloss {i}\n"
                ),
            )
            .unwrap();
        }
        let pack = pack_for_language("yoruba", dir.path());
        assert_eq!(pack.entries.len(), MAX_RESIDENT_ENTRIES);
    }

    #[test]
    fn plano_sem_termos_retorna_pack_vazio() {
        let dir = tempdir().unwrap();
        let pack = pack_for_language("yoruba", dir.path());
        assert!(pack.entries.is_empty());
        assert_eq!(pack.entropy_stats.unwrap().symbols, 0);
    }

    #[test]
    fn is_known_plano_reconhece_linguas() {
        assert!(is_known_plano("yoruba"));
        assert!(is_known_plano("guarani-mbya"));
        assert!(is_known_plano("portuguese"));
        assert!(is_known_plano("spanish"));
        assert!(!is_known_plano("klingon"));
        assert!(!is_known_plano(""));
    }
}
