//! Salas públicas — duas salas read-only geradas a partir do léxico completo:
//! `public-yoruba` e `public-mbya`. Qualquer um vê sem login; para **sugerir**
//! (editar), o usuário faz fork (cópia pessoal) — ver rota `fork`.
//!
//! Os termos vêm de JSON baked-in (`<lingua>/lexicon.<code>.json`), gerados do
//! léxico (DB Arandu/Dooley p/ Mbyá; markdown curado p/ Iorubá). Layout em grid
//! alfabético — o cliente navega por pan/zoom/busca.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::room::{Element, Room};
use super::store::RoomStore;

pub const PUBLIC_YORUBA: &str = "public-yoruba";
pub const PUBLIC_MBYA: &str = "public-mbya";
/// Dono sintético das salas públicas — nunca é um `sub` de usuário real, então
/// nenhum usuário "é dono" (logo todas são read-only para todos).
pub const SYSTEM_OWNER: &str = "__system__";

/// Quantos termos a sala pública mostra de início (top-N por popularidade).
/// Os demais (~milhares) chegam sob demanda via [`lexicon_slice`].
pub const DEFAULT_TOP_N: usize = 100;
/// Constante de densidade da espiral (unidades de mundo). Distância radial do
/// rank `i` ≈ `SPIRAL_C * sqrt(i)`.
pub const SPIRAL_C: f64 = 40.0;

#[derive(Deserialize)]
struct LexEntry {
    word: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    gloss: Option<String>,
    #[serde(default)]
    pron: Option<String>,
    /// Popularidade (nº de exemplos no corpus). O JSON já vem ordenado por isto.
    #[serde(default)]
    pop: i64,
    /// Decomposição morfológica (partículas) — semeada das NOTAS de Cadogan no
    /// estudo do Ayvu Rapyta, ex.: `apy 'extremidade' + yta 'sustento'`.
    #[serde(default)]
    decomp: Option<String>,
}

fn load_entries(root: &Path, rel: &str) -> Vec<LexEntry> {
    match std::fs::read(root.join(rel)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Posição de mundo do termo de rank `i` — espiral de phyllotaxis (girassol):
/// rank 0 (mais popular) ao centro, termos menos populares espiralando para
/// fora pelo ângulo áureo. Layout orgânico ("galáxia"), estável e por rank.
fn node_pos(i: usize) -> (f64, f64) {
    let golden = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt()); // ~137.5°
    let r = SPIRAL_C * (i as f64 + 0.5).sqrt();
    let a = i as f64 * golden;
    (r * a.cos(), r * a.sin())
}

/// Lê o JSON de corpus baked (`corpus/<slug>.json`, gerado por
/// `mbya/scripts/corpus-to-json.py`) — a fonte da superfície de exploração do
/// Ayvu Rapyta (capítulos → versos Mbyá ⟷ Español + glosas/partículas + NOTAS).
/// `None` se ausente. `slug` é sanitizado (sem travessia de diretório).
pub fn corpus_json(root: &Path, slug: &str) -> Option<String> {
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    std::fs::read_to_string(root.join("corpus").join(format!("{slug}.json"))).ok()
}

/// Arquivo de léxico baked por língua.
pub fn lang_file(lang: &str) -> Option<&'static str> {
    match lang {
        "gn-mbya" | "gn" => Some("guarani-mbya/lexicon.mbya.json"),
        "yo" => Some("yoruba/lexicon.yo.json"),
        _ => None,
    }
}

/// Sala pública = top-N do léxico (já ordenado por popularidade no JSON),
/// posicionado por rank. O resto chega via [`lexicon_slice`] ("carregar mais").
fn build_room(id: &str, title: &str, lang: &str, entries: Vec<LexEntry>) -> Room {
    let mut elements = Vec::new();
    for (i, e) in entries.into_iter().take(DEFAULT_TOP_N).enumerate() {
        let (x, y) = node_pos(i);
        let lang_code = if e.lang.is_empty() {
            lang.to_string()
        } else {
            e.lang
        };
        let mut el = Element::new(format!("e{i}"), e.word, lang_code).at(x, y);
        el.gloss = e.gloss;
        el.pronunciation = e.pron;
        elements.push(el);
    }
    let mut room = Room::empty(id, SYSTEM_OWNER, title, lang);
    room.template = "publico".to_string();
    room.published = true;
    room.elements = elements;
    room
}

/// As duas salas públicas, lidas do léxico enraizado em `root` (= COMUNICACAO_DIR).
pub fn public_rooms(root: &Path) -> Vec<Room> {
    vec![
        build_room(
            PUBLIC_YORUBA,
            "Léxico Iorubá (público)",
            "yo",
            load_entries(root, "yoruba/lexicon.yo.json"),
        ),
        build_room(
            PUBLIC_MBYA,
            "Léxico Mbyá Guaraní (público)",
            "gn-mbya",
            load_entries(root, "guarani-mbya/lexicon.mbya.json"),
        ),
    ]
}

/// Uma entrada de "carregar mais" — já com posição de mundo (pelo rank global).
#[derive(Serialize)]
pub struct SliceEntry {
    pub index: usize,
    pub word: String,
    pub lang: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pron: Option<String>,
    pub pop: i64,
    /// Decomposição morfológica em partículas (estudo Ayvu Rapyta / NOTAS Cadogan).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decomp: Option<String>,
    pub x: f64,
    pub y: f64,
}

/// Página do léxico ordenado por popularidade. `limit == 0` devolve só o `total`
/// (útil para o cliente saber quantos termos existem ao todo).
#[derive(Serialize)]
pub struct LexSlice {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub lang: String,
    pub entries: Vec<SliceEntry>,
}

/// Lê o léxico baked e devolve a fatia `[offset, offset+limit)` por popularidade.
pub fn lexicon_slice(root: &Path, lang: &str, offset: usize, limit: usize) -> LexSlice {
    let all = match lang_file(lang) {
        Some(rel) => load_entries(root, rel),
        None => Vec::new(),
    };
    let total = all.len();
    let entries = all
        .into_iter()
        .enumerate()
        .skip(offset)
        .take(limit)
        .map(|(i, e)| {
            let (x, y) = node_pos(i);
            let lang_code = if e.lang.is_empty() {
                lang.to_string()
            } else {
                e.lang
            };
            SliceEntry {
                index: i,
                word: e.word,
                lang: lang_code,
                gloss: e.gloss,
                pron: e.pron,
                pop: e.pop,
                decomp: e.decomp,
                x,
                y,
            }
        })
        .collect();
    LexSlice {
        total,
        offset,
        limit,
        lang: lang.to_string(),
        entries,
    }
}

/// `true` se o id é uma sala pública (read-only, fork-only).
pub fn is_public_id(id: &str) -> bool {
    id == PUBLIC_YORUBA || id == PUBLIC_MBYA
}

/// (Re)gera as salas públicas no disco. Idempotente; pula as que vierem vazias
/// (JSON ausente) para não sobrescrever uma sala já populada com uma vazia.
pub fn ensure_public_rooms(store: &RoomStore, root: &Path) {
    for room in public_rooms(root) {
        if room.elements.is_empty() {
            continue;
        }
        if let Err(e) = store.save(&room) {
            tracing_save_warn(&room.id, &e);
        }
    }
}

fn tracing_save_warn(id: &str, e: &super::store::StoreError) {
    // `yggdrasil-core` não depende de `tracing`; eprintln é suficiente p/ boot.
    eprintln!("comunicacao: falha ao gerar sala pública {id}: {e}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_lex(root: &Path, rel: &str, json: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, json).unwrap();
    }

    #[test]
    fn build_room_preserva_ordem_de_popularidade() {
        // o JSON já vem ordenado por popularidade; build_room NÃO reordena
        let entries: Vec<LexEntry> =
            serde_json::from_str(r#"[{"word":"b","pop":9},{"word":"a","pop":2}]"#).unwrap();
        let room = build_room("public-x", "X", "yo", entries);
        assert_eq!(room.elements.len(), 2);
        assert_eq!(room.owner, SYSTEM_OWNER);
        assert!(room.published);
        assert_eq!(room.elements[0].word, "b"); // rank 0 = mais popular, não alfabético
        assert_eq!(room.elements[0].id, "e0");
    }

    #[test]
    fn build_room_limita_ao_top_n() {
        let entries: Vec<LexEntry> = (0..DEFAULT_TOP_N + 50)
            .map(|i| serde_json::from_str(&format!(r#"{{"word":"w{i}"}}"#)).unwrap())
            .collect();
        let room = build_room("public-x", "X", "yo", entries);
        assert_eq!(room.elements.len(), DEFAULT_TOP_N);
    }

    #[test]
    fn lexicon_slice_pagina_por_rank() {
        let dir = tempdir().unwrap();
        let items: Vec<String> = (0..250)
            .map(|i| format!(r#"{{"word":"w{i}","pop":{}}}"#, 250 - i))
            .collect();
        write_lex(
            dir.path(),
            "guarani-mbya/lexicon.mbya.json",
            &format!("[{}]", items.join(",")),
        );
        let s = super::lexicon_slice(dir.path(), "gn-mbya", 100, 100);
        assert_eq!(s.total, 250);
        assert_eq!(s.entries.len(), 100);
        assert_eq!(s.entries[0].index, 100);
        assert_eq!(s.entries[0].lang, "gn-mbya");
        // limit 0 → só total
        let only_total = super::lexicon_slice(dir.path(), "gn-mbya", 0, 0);
        assert_eq!(only_total.total, 250);
        assert!(only_total.entries.is_empty());
        // língua sem léxico → vazio
        assert_eq!(super::lexicon_slice(dir.path(), "klingon", 0, 50).total, 0);
    }

    #[test]
    fn public_rooms_lidas_do_disco() {
        let dir = tempdir().unwrap();
        write_lex(
            dir.path(),
            "yoruba/lexicon.yo.json",
            r#"[{"word":"àṣẹ","lang":"yo","gloss":"força"}]"#,
        );
        write_lex(
            dir.path(),
            "guarani-mbya/lexicon.mbya.json",
            r#"[{"word":"ayvu","gloss":"fala"},{"word":"teko"}]"#,
        );
        let rooms = public_rooms(dir.path());
        let yo = rooms.iter().find(|r| r.id == PUBLIC_YORUBA).unwrap();
        let mbya = rooms.iter().find(|r| r.id == PUBLIC_MBYA).unwrap();
        assert_eq!(yo.elements.len(), 1);
        assert_eq!(yo.elements[0].word, "àṣẹ");
        assert_eq!(mbya.elements.len(), 2);
        assert!(mbya.elements.iter().all(|e| e.lang == "gn-mbya"));
    }

    #[test]
    fn ensure_pula_vazias() {
        let dir = tempdir().unwrap();
        let store = RoomStore::new(dir.path().join("rooms")).unwrap();
        // sem JSON → salas vazias → não devem ser gravadas
        ensure_public_rooms(&store, dir.path());
        assert!(!store.exists(PUBLIC_YORUBA));
        assert!(!store.exists(PUBLIC_MBYA));
    }

    #[test]
    fn ensure_grava_quando_ha_lexico() {
        let dir = tempdir().unwrap();
        write_lex(dir.path(), "yoruba/lexicon.yo.json", r#"[{"word":"àṣẹ"}]"#);
        let store = RoomStore::new(dir.path().join("rooms")).unwrap();
        ensure_public_rooms(&store, dir.path());
        assert!(store.exists(PUBLIC_YORUBA));
        let r = store.load(PUBLIC_YORUBA).unwrap();
        assert!(r.published);
        assert_eq!(r.owner, SYSTEM_OWNER);
    }

    #[test]
    fn is_public_id_reconhece() {
        assert!(is_public_id(PUBLIC_MBYA));
        assert!(is_public_id(PUBLIC_YORUBA));
        assert!(!is_public_id("abc123"));
    }
}
