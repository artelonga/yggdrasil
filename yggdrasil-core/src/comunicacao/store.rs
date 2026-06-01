//! Persistência em disco das salas e das filas de revisão.
//!
//! Mesmo princípio do [`crate::instance::store`]: disco é a fonte da verdade,
//! escrita atômica (temp + rename). Cada sala é `<root>/<id>/room.json`; cada
//! fila de revisão é `<root>/_review/<user-slug>.json`.

use std::path::{Path, PathBuf};

use chrono::Utc;

use super::lexicon::slugify;
use super::review::ReviewQueue;
use super::room::Room;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sala '{0}' não encontrada")]
    NotFound(String),
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro de serialização: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Store de salas + revisões.
pub struct RoomStore {
    root: PathBuf,
}

impl RoomStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join("_review"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn room_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn room_path(&self, id: &str) -> PathBuf {
        self.room_dir(id).join("room.json")
    }

    // ─── Salas ────────────────────────────────────────────────────────────────

    /// Grava a sala atomicamente, avançando `updated_at`.
    pub fn save(&self, room: &Room) -> Result<(), StoreError> {
        let mut room = room.clone();
        room.updated_at = Utc::now();
        let dir = self.room_dir(&room.id);
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(&room)?;
        let final_path = self.room_path(&room.id);
        let tmp = dir.join("room.json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &final_path)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Room, StoreError> {
        let path = self.room_path(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(id.to_string())
            } else {
                StoreError::Io(e)
            }
        })?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn exists(&self, id: &str) -> bool {
        self.room_path(id).exists()
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let dir = self.room_dir(id);
        if !dir.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    /// Todas as salas (ignora ilegíveis e diretórios de serviço).
    pub fn list_all(&self) -> Result<Vec<Room>, StoreError> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == "_review" {
                continue;
            }
            if let Ok(room) = self.load(name) {
                out.push(room);
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }

    pub fn list_owner(&self, owner: &str) -> Result<Vec<Room>, StoreError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|r| r.owner == owner)
            .collect())
    }

    pub fn list_published(&self) -> Result<Vec<Room>, StoreError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|r| r.published)
            .collect())
    }

    // ─── Filas de revisão (por usuário) ────────────────────────────────────────

    fn review_path(&self, user: &str) -> PathBuf {
        self.root
            .join("_review")
            .join(format!("{}.json", slugify(user)))
    }

    /// Carrega a fila do usuário (vazia se ainda não existe).
    pub fn load_review(&self, user: &str) -> Result<ReviewQueue, StoreError> {
        let path = self.review_path(user);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ReviewQueue::default()),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    pub fn save_review(&self, user: &str, queue: &ReviewQueue) -> Result<(), StoreError> {
        let dir = self.root.join("_review");
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(queue)?;
        let final_path = self.review_path(user);
        let tmp = final_path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &final_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comunicacao::review::ReviewItem;
    use tempfile::tempdir;

    fn store() -> (RoomStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        (RoomStore::new(dir.path()).unwrap(), dir)
    }

    #[test]
    fn save_load_round_trip() {
        let (s, _d) = store();
        let r = Room::empty("r1", "alice", "Sala", "yo");
        s.save(&r).unwrap();
        let loaded = s.load("r1").unwrap();
        assert_eq!(loaded.id, "r1");
        assert_eq!(loaded.owner, "alice");
    }

    #[test]
    fn list_owner_filtra() {
        let (s, _d) = store();
        s.save(&Room::empty("a", "alice", "A", "yo")).unwrap();
        s.save(&Room::empty("b", "bob", "B", "gn-mbya")).unwrap();
        let alice = s.list_owner("alice").unwrap();
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].id, "a");
    }

    #[test]
    fn review_dir_nao_aparece_como_sala() {
        let (s, _d) = store();
        s.save(&Room::empty("a", "alice", "A", "yo")).unwrap();
        let mut q = ReviewQueue::default();
        q.upsert(ReviewItem::new(
            "yoruba/terms/ase.md",
            "àṣẹ",
            "yo",
            None,
            Utc::now(),
        ));
        s.save_review("alice", &q).unwrap();
        // a varredura de salas não pode confundir _review com uma sala
        assert_eq!(s.list_all().unwrap().len(), 1);
    }

    #[test]
    fn review_queue_persiste() {
        let (s, _d) = store();
        assert!(s.load_review("alice").unwrap().items.is_empty());
        let mut q = ReviewQueue::default();
        q.upsert(ReviewItem::new("p", "w", "yo", None, Utc::now()));
        s.save_review("alice", &q).unwrap();
        let loaded = s.load_review("alice").unwrap();
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].term_path, "p");
    }

    #[test]
    fn delete_remove() {
        let (s, _d) = store();
        s.save(&Room::empty("z", "o", "Z", "yo")).unwrap();
        assert!(s.exists("z"));
        s.delete("z").unwrap();
        assert!(!s.exists("z"));
        assert!(matches!(s.delete("z"), Err(StoreError::NotFound(_))));
    }
}
