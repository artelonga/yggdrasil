//! Universo **centralizado** — topologia de sentido cross-linguística (YG-175).
//!
//! Não é uma nova ilha: é o **tecido conectivo entre packs**. Cada termo de
//! qualquer [`super::pack::LexiconPack`] (idioma, música, domínio) é um **nó**;
//! arestas ligam nós **através de linguagens** (um *odù* yorubá a um termo mbyá).
//! As arestas nascem por **exploração** (co-visitação → `weight++`) e podem ser
//! **promovidas** a relação nomeada pelo usuário; referências (frase, etimologia,
//! áudio) anexam-se a um nó **por link**, nunca embutidas.
//!
//! Este módulo é **puro** (só identidade/canonicalização de nós + enums); a
//! persistência SQLite e a resolução contra os packs vivem no `yggdrasil-web`
//! (`topologia.rs` + `api/topologia.rs`), mesmo princípio de camadas do
//! `comunicacao`.

use serde::{Deserialize, Serialize};

use super::lexicon::slugify;

/// Identidade estável de um nó: `"{pack_id}:{term_slug}"`. O slug dobra
/// diacríticos/tom ([`slugify`]) — `guarani-mbya:ñe'ẽ` → `guarani-mbya:ne-e` —
/// para que o mesmo termo escrito de formas diferentes case num só nó.
pub fn node_id(pack_id: &str, term: &str) -> String {
    format!("{}:{}", pack_id, slugify(term))
}

/// Quebra um `NodeId` em `(pack_id, slug)`. `None` se faltar o `:` ou se algum
/// lado vier vazio. Divide no **primeiro** `:` (pack ids não têm `:`; slugs
/// nunca têm, pois `slugify` só emite `[a-z0-9-]`).
pub fn split_node_id(id: &str) -> Option<(&str, &str)> {
    let (pack, slug) = id.split_once(':')?;
    if pack.is_empty() || slug.is_empty() {
        return None;
    }
    Some((pack, slug))
}

/// Par **canônico** de uma aresta não-direcionada: ordena `(a, b)` para que
/// `(a,b)` e `(b,a)` apontem para a MESMA linha — base da idempotência do
/// `weight++`. Loops (`a == b`) são rejeitados pelo store, não aqui.
pub fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// De onde veio a aresta. `Explore` = materializada por co-visitação
/// (telemetria, YG-145); `User` = promovida/nomeada explicitamente; `Semantic` =
/// sugerida por similaridade de sentido por contexto (YG-176, overlay computado).
/// Uma aresta `Explore` sobe para `User` ao ser promovida; nunca regride.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeSource {
    Explore,
    User,
    Semantic,
}

impl EdgeSource {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeSource::Explore => "explore",
            EdgeSource::User => "user",
            EdgeSource::Semantic => "semantic",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "explore" => Some(EdgeSource::Explore),
            "user" => Some(EdgeSource::User),
            "semantic" => Some(EdgeSource::Semantic),
            _ => None,
        }
    }
}

/// Tipo de referência anexada a um nó — **sempre um link** (`href`), nunca o
/// conteúdo copiado. `href` aponta pro CO (term_path/markdown), pra um áudio
/// (params de síntese do pack) ou pra uma URL externa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Sentence,
    Etymology,
    Audio,
    Link,
}

impl RefKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RefKind::Sentence => "sentence",
            RefKind::Etymology => "etymology",
            RefKind::Audio => "audio",
            RefKind::Link => "link",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sentence" => Some(RefKind::Sentence),
            "etymology" => Some(RefKind::Etymology),
            "audio" => Some(RefKind::Audio),
            "link" => Some(RefKind::Link),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_dobra_diacriticos_e_tom() {
        // ñe'ẽ → ne-e (ñ→n, apóstrofo some, ẽ→e); pack id preservado.
        assert_eq!(node_id("guarani-mbya", "ñe'ẽ"), "guarani-mbya:ne-e");
        assert_eq!(node_id("musica", "C4"), "musica:c4");
    }

    #[test]
    fn split_node_id_no_primeiro_dois_pontos() {
        assert_eq!(
            split_node_id("guarani-mbya:ne-e"),
            Some(("guarani-mbya", "ne-e"))
        );
        assert_eq!(split_node_id("semdoispontos"), None);
        assert_eq!(split_node_id(":slug"), None);
        assert_eq!(split_node_id("pack:"), None);
    }

    #[test]
    fn canonical_pair_e_comutativo() {
        let ab = canonical_pair("guarani-mbya:ne-e", "musica:c4");
        let ba = canonical_pair("musica:c4", "guarani-mbya:ne-e");
        assert_eq!(ab, ba);
        // ordenado: "guarani-..." < "musica..." lexicograficamente
        assert_eq!(ab.0, "guarani-mbya:ne-e");
        assert_eq!(ab.1, "musica:c4");
    }

    #[test]
    fn edge_source_round_trip() {
        for s in [EdgeSource::Explore, EdgeSource::User, EdgeSource::Semantic] {
            assert_eq!(EdgeSource::parse(s.as_str()), Some(s));
        }
        assert_eq!(EdgeSource::parse("nope"), None);
    }

    #[test]
    fn ref_kind_round_trip() {
        for k in [
            RefKind::Sentence,
            RefKind::Etymology,
            RefKind::Audio,
            RefKind::Link,
        ] {
            assert_eq!(RefKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(RefKind::parse("nope"), None);
    }
}
