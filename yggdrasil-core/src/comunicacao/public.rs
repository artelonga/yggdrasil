//! Salas públicas — duas salas read-only geradas a partir do léxico completo:
//! `public-yoruba` e `public-mbya`. Qualquer um vê sem login; para **sugerir**
//! (editar), o usuário faz fork (cópia pessoal) — ver rota `fork`.
//!
//! Os termos vêm de JSON baked-in (`<lingua>/lexicon.<code>.json`), gerados do
//! léxico (DB Arandu/Dooley p/ Mbyá; markdown curado p/ Iorubá). Layout em grid
//! alfabético — o cliente navega por pan/zoom/busca.

use std::path::Path;

use serde::Deserialize;

use super::room::{Element, Room};
use super::store::RoomStore;

pub const PUBLIC_YORUBA: &str = "public-yoruba";
pub const PUBLIC_MBYA: &str = "public-mbya";
/// Dono sintético das salas públicas — nunca é um `sub` de usuário real, então
/// nenhum usuário "é dono" (logo todas são read-only para todos).
pub const SYSTEM_OWNER: &str = "__system__";

/// Espaçamento (unidades de mundo) entre nós no grid de layout.
const SPACING: f64 = 150.0;

#[derive(Deserialize)]
struct LexEntry {
    word: String,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    gloss: Option<String>,
    #[serde(default)]
    pron: Option<String>,
}

fn load_entries(root: &Path, rel: &str) -> Vec<LexEntry> {
    match std::fs::read(root.join(rel)) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Constrói uma sala pública a partir de entradas de léxico, em grid alfabético.
fn build_room(id: &str, title: &str, lang: &str, mut entries: Vec<LexEntry>) -> Room {
    entries.sort_by(|a, b| a.word.to_lowercase().cmp(&b.word.to_lowercase()));
    let n = entries.len();
    let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
    let rows = n.div_ceil(cols);
    let x0 = -((cols.saturating_sub(1)) as f64) * SPACING / 2.0;
    let y0 = -((rows.saturating_sub(1)) as f64) * SPACING / 2.0;

    let mut elements = Vec::with_capacity(n);
    for (i, e) in entries.into_iter().enumerate() {
        let col = (i % cols) as f64;
        let row = (i / cols) as f64;
        let lang_code = if e.lang.is_empty() {
            lang.to_string()
        } else {
            e.lang
        };
        let mut el = Element::new(format!("e{i}"), e.word, lang_code)
            .at(x0 + col * SPACING, y0 + row * SPACING);
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
    fn build_room_layout_grid() {
        let entries: Vec<LexEntry> =
            serde_json::from_str(r#"[{"word":"b"},{"word":"a"},{"word":"c"},{"word":"d"}]"#)
                .unwrap();
        let room = build_room("public-x", "X", "yo", entries);
        assert_eq!(room.elements.len(), 4);
        assert_eq!(room.owner, SYSTEM_OWNER);
        assert!(room.published);
        // ordenado alfabeticamente: a vem primeiro
        assert_eq!(room.elements[0].word, "a");
        // ids únicos e estáveis
        assert_eq!(room.elements[0].id, "e0");
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
