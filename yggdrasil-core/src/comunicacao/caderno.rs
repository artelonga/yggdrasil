//! Caderno do Ayvu Rapyta — persistência **por usuário** de favoritos, notas e
//! progresso (YG-112).
//!
//! Espelha o padrão de store per-user da fila de revisão
//! ([`crate::comunicacao::store::RoomStore::load_review`]): o disco é a fonte da
//! verdade, escrita atômica (temp + rename), um arquivo por usuário em
//! `<root>/_caderno/<user-slug>.json`. Antes da YG-112 o Caderno vivia 100% no
//! `localStorage` do navegador (`db.fav`/`db.notes`/`db.progress`); aqui ele vira
//! durável e cross-device, com migração idempotente do blob local no primeiro
//! login.
//!
//! As **notas** do Caderno têm uma segunda vida: além de guardadas aqui (para a
//! UI recuperar rápido), são gravadas via `NoteStore` sob a instância canônica do
//! Ayvu (YG-114) — assim o producer staged (YG-93/97) as federa de graça. Este
//! módulo só cuida da persistência por usuário; a ponte com o `NoteStore`/
//! `emit_note` mora na camada web (`comunicacao_routes`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::lexicon::slugify;
use super::store::StoreError;

/// Uma nota do Caderno — prosa livre do usuário ancorada num verso do corpus.
/// A `key` é a âncora estável (tipicamente `verse_slug(capítulo, verso)`, YG-114);
/// o `markdown` é o corpo editável.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CadernoNote {
    /// Âncora estável do verso (slug). Igual à chave no mapa `notes`.
    pub key: String,
    /// Título da nota (default = `key` quando vazio).
    #[serde(default)]
    pub title: String,
    /// Corpo Markdown da anotação.
    #[serde(default)]
    pub markdown: String,
    /// Última atualização (RFC3339), avançada a cada escrita.
    pub updated_at: DateTime<Utc>,
}

/// O Caderno de um usuário: favoritos (`★`), notas (`✎`) e progresso de leitura.
///
/// Serializado como um único JSON por usuário. Os mapas são `BTreeMap` para
/// ordem determinística no disco (diffs estáveis) e idempotência da migração.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caderno {
    /// Versos favoritados, por âncora (`verse_slug`). Valor = quando favoritou.
    #[serde(default)]
    pub fav: BTreeMap<String, DateTime<Utc>>,
    /// Notas do usuário, por âncora (`verse_slug`).
    #[serde(default)]
    pub notes: BTreeMap<String, CadernoNote>,
    /// Progresso de leitura por capítulo/seção: âncora → último verso lido.
    #[serde(default)]
    pub progress: BTreeMap<String, String>,
}

impl Caderno {
    /// Marca um verso como favorito (idempotente: preserva o timestamp original).
    /// Devolve `true` se favoritou agora (não estava antes).
    pub fn add_fav(&mut self, key: &str, when: DateTime<Utc>) -> bool {
        if self.fav.contains_key(key) {
            return false;
        }
        self.fav.insert(key.to_string(), when);
        true
    }

    /// Remove um favorito. Devolve `true` se havia algo a remover.
    pub fn remove_fav(&mut self, key: &str) -> bool {
        self.fav.remove(key).is_some()
    }

    /// Cria/atualiza uma nota. Devolve `false` se a nota já existia (update) e
    /// `true` se foi criada agora — espelha o `created vs updated` do `NoteStore`.
    pub fn upsert_note(
        &mut self,
        key: &str,
        title: &str,
        markdown: &str,
        when: DateTime<Utc>,
    ) -> bool {
        let title = if title.trim().is_empty() {
            key.to_string()
        } else {
            title.to_string()
        };
        let existed = self.notes.contains_key(key);
        self.notes.insert(
            key.to_string(),
            CadernoNote {
                key: key.to_string(),
                title,
                markdown: markdown.to_string(),
                updated_at: when,
            },
        );
        !existed
    }

    /// Remove uma nota. Devolve `true` se havia uma nota a remover.
    pub fn remove_note(&mut self, key: &str) -> bool {
        self.notes.remove(key).is_some()
    }

    /// Registra o progresso de leitura de uma âncora.
    pub fn set_progress(&mut self, key: &str, verse: &str) {
        self.progress.insert(key.to_string(), verse.to_string());
    }

    /// Funde outro Caderno **dentro** deste (upsert sem perda) — o coração da
    /// migração idempotente do `localStorage`. Regras de fusão:
    /// - favoritos: união; mantém o timestamp mais **antigo** (primeiro registro);
    /// - notas: a versão mais **recente** (`updated_at`) vence;
    /// - progresso: o `other` (geralmente o blob local mais novo) sobrescreve.
    ///
    /// Idempotente: fundir o mesmo blob duas vezes dá o mesmo resultado.
    pub fn merge_from(&mut self, other: Caderno) {
        for (k, when) in other.fav {
            self.fav
                .entry(k)
                .and_modify(|cur| {
                    if when < *cur {
                        *cur = when;
                    }
                })
                .or_insert(when);
        }
        for (k, note) in other.notes {
            match self.notes.get(&k) {
                Some(existing) if existing.updated_at >= note.updated_at => {}
                _ => {
                    self.notes.insert(k, note);
                }
            }
        }
        for (k, v) in other.progress {
            self.progress.insert(k, v);
        }
    }
}

/// Store em disco do Caderno por usuário: `<root>/_caderno/<user-slug>.json`.
/// Reusa a mesma raiz do [`RoomStore`](super::store::RoomStore) (salas + revisão)
/// — o subdiretório `_caderno` não colide com `_review` nem com salas (cujos
/// diretórios a varredura de `list_all` já filtra por nome).
pub struct CadernoStore {
    root: PathBuf,
}

impl CadernoStore {
    /// Abre (criando se preciso) o store enraizado em `<root>/_caderno/`.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("_caderno"))?;
        Ok(Self { root })
    }

    fn caderno_path(&self, user: &str) -> PathBuf {
        self.root
            .join("_caderno")
            .join(format!("{}.json", slugify(user)))
    }

    /// Carrega o Caderno do usuário (vazio se ainda não existe).
    pub fn load(&self, user: &str) -> Result<Caderno, StoreError> {
        let path = self.caderno_path(user);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Caderno::default()),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    /// Grava o Caderno do usuário atomicamente (temp + rename).
    pub fn save(&self, user: &str, caderno: &Caderno) -> Result<(), StoreError> {
        let dir = self.root.join("_caderno");
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(caderno)?;
        let final_path = self.caderno_path(user);
        let tmp = final_path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &final_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (CadernoStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        (CadernoStore::new(dir.path()).unwrap(), dir)
    }

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn vazio_quando_nao_existe() {
        let (s, _d) = store();
        let c = s.load("alice").unwrap();
        assert!(c.fav.is_empty());
        assert!(c.notes.is_empty());
        assert!(c.progress.is_empty());
    }

    #[test]
    fn round_trip_persiste() {
        let (s, _d) = store();
        let mut c = Caderno::default();
        c.add_fav("cap-1-v-2", ts("2026-06-08T00:00:00Z"));
        c.upsert_note("cap-1-v-2", "Nota", "corpo", ts("2026-06-08T00:00:00Z"));
        c.set_progress("cap-1", "cap-1-v-2");
        s.save("alice", &c).unwrap();

        let loaded = s.load("alice").unwrap();
        assert_eq!(loaded, c);
        assert_eq!(loaded.progress["cap-1"], "cap-1-v-2");
    }

    #[test]
    fn add_fav_idempotente_preserva_timestamp() {
        let mut c = Caderno::default();
        assert!(c.add_fav("k", ts("2026-06-08T00:00:00Z")));
        assert!(!c.add_fav("k", ts("2026-06-09T00:00:00Z")), "já existia");
        assert_eq!(c.fav["k"], ts("2026-06-08T00:00:00Z"), "mantém o original");
        assert!(c.remove_fav("k"));
        assert!(!c.remove_fav("k"));
    }

    #[test]
    fn upsert_note_distingue_created_de_updated() {
        let mut c = Caderno::default();
        assert!(c.upsert_note("k", "T", "v1", ts("2026-06-08T00:00:00Z")));
        assert!(!c.upsert_note("k", "T", "v2", ts("2026-06-09T00:00:00Z")));
        assert_eq!(c.notes["k"].markdown, "v2");
    }

    #[test]
    fn note_titulo_vazio_usa_a_chave() {
        let mut c = Caderno::default();
        c.upsert_note("cap-1-v-2", "   ", "corpo", ts("2026-06-08T00:00:00Z"));
        assert_eq!(c.notes["cap-1-v-2"].title, "cap-1-v-2");
    }

    #[test]
    fn merge_sem_perda_e_idempotente() {
        let mut server = Caderno::default();
        server.add_fav("a", ts("2026-06-01T00:00:00Z"));
        server.upsert_note("n", "Server", "server-body", ts("2026-06-05T00:00:00Z"));
        server.set_progress("cap-1", "v-3");

        let mut local = Caderno::default();
        // favorito mais antigo no local → deve vencer (timestamp mínimo)
        local.add_fav("a", ts("2026-05-20T00:00:00Z"));
        local.add_fav("b", ts("2026-06-02T00:00:00Z"));
        // nota mais nova no local → deve sobrescrever
        local.upsert_note("n", "Local", "local-body", ts("2026-06-07T00:00:00Z"));
        local.set_progress("cap-1", "v-5");

        let snapshot = local.clone();
        server.merge_from(local);

        assert_eq!(
            server.fav["a"],
            ts("2026-05-20T00:00:00Z"),
            "fav mais antigo vence"
        );
        assert!(server.fav.contains_key("b"));
        assert_eq!(
            server.notes["n"].markdown, "local-body",
            "nota mais nova vence"
        );
        assert_eq!(
            server.progress["cap-1"], "v-5",
            "progresso local sobrescreve"
        );

        // idempotência: re-fundir o mesmo blob não muda nada
        let before = server.clone();
        server.merge_from(snapshot);
        assert_eq!(server, before, "merge é idempotente");
    }

    #[test]
    fn merge_nao_regride_nota_mais_nova_do_servidor() {
        let mut server = Caderno::default();
        server.upsert_note("n", "Server", "novo", ts("2026-06-09T00:00:00Z"));
        let mut local = Caderno::default();
        local.upsert_note("n", "Local", "velho", ts("2026-06-01T00:00:00Z"));
        server.merge_from(local);
        assert_eq!(
            server.notes["n"].markdown, "novo",
            "servidor mais novo não regride"
        );
    }

    #[test]
    fn user_slug_isola_cadernos() {
        let (s, _d) = store();
        let mut a = Caderno::default();
        a.add_fav("x", ts("2026-06-08T00:00:00Z"));
        s.save("alice@test.com", &a).unwrap();
        // outro usuário tem Caderno vazio
        assert!(s.load("bob@test.com").unwrap().fav.is_empty());
        assert_eq!(s.load("alice@test.com").unwrap().fav.len(), 1);
    }
}
