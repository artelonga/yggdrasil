//! Persistência de [`UniverseInstance`] em disco (YG-75/YG-77).
//!
//! Disco é a fonte da verdade (princípio do `co`): cada instância é um
//! `instance.json` escrito atomicamente (temp + rename), e anexos binários ficam
//! content-addressed por SHA-256 sob `_blobs/<shard>/<hash>` — dedupe automático.
//!
//! Listagem é feita por varredura de diretório. Um índice SQLite para listagem
//! O(1) por owner/publish é uma otimização futura (não necessária no MVP).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::schema::UniverseInstance;

/// Falhas de I/O / serialização do store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("instância '{0}' não encontrada")]
    NotFound(String),
    #[error("blob '{0}' não encontrado")]
    BlobNotFound(String),
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro de serialização: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Store em disco para instâncias e seus anexos.
pub struct InstanceStore {
    root: PathBuf,
}

impl InstanceStore {
    /// Abre (criando se preciso) o store enraizado em `root`.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join("_blobs"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn instance_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn instance_path(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("instance.json")
    }

    /// Grava a instância atomicamente (temp + rename). Idempotente.
    pub fn save(&self, inst: &UniverseInstance) -> Result<(), StoreError> {
        let dir = self.instance_dir(&inst.id);
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(inst)?;
        let final_path = self.instance_path(&inst.id);
        let tmp_path = dir.join("instance.json.tmp");
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &final_path)?;
        Ok(())
    }

    /// Carrega a instância pelo id.
    pub fn load(&self, id: &str) -> Result<UniverseInstance, StoreError> {
        let path = self.instance_path(id);
        let bytes = std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(id.to_string())
            } else {
                StoreError::Io(e)
            }
        })?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Apaga a instância e seu diretório. Anexos em `_blobs` (compartilhados) não
    /// são removidos aqui — GC de blobs órfãos é separado.
    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let dir = self.instance_dir(id);
        if !dir.exists() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    pub fn exists(&self, id: &str) -> bool {
        self.instance_path(id).exists()
    }

    /// Varre o disco e devolve todas as instâncias (ignora as ilegíveis).
    pub fn list_all(&self) -> Result<Vec<UniverseInstance>, StoreError> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == "_blobs" {
                continue;
            }
            if let Ok(inst) = self.load(name) {
                out.push(inst);
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(out)
    }

    /// Instâncias de um dono específico.
    pub fn list_owner(&self, owner: &str) -> Result<Vec<UniverseInstance>, StoreError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|i| i.owner == owner)
            .collect())
    }

    /// Instâncias publicadas (feed público).
    pub fn list_published(&self) -> Result<Vec<UniverseInstance>, StoreError> {
        Ok(self
            .list_all()?
            .into_iter()
            .filter(|i| i.published)
            .collect())
    }

    // ─── Blobs content-addressed ────────────────────────────────────────────

    fn blob_path(&self, hash: &str) -> PathBuf {
        let shard = &hash[..2.min(hash.len())];
        self.root.join("_blobs").join(shard).join(hash)
    }

    /// SHA-256 hex de um buffer.
    pub fn hash_bytes(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Grava bytes content-addressed; devolve `(hash, size)`. Dedupe: se o blob
    /// já existe, não reescreve.
    pub fn write_blob(&self, bytes: &[u8]) -> Result<(String, u64), StoreError> {
        let hash = Self::hash_bytes(bytes);
        let path = self.blob_path(&hash);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, &path)?;
        }
        Ok((hash, bytes.len() as u64))
    }

    /// Lê os bytes de um blob pelo hash.
    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>, StoreError> {
        let path = self.blob_path(hash);
        std::fs::read(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::BlobNotFound(hash.to_string())
            } else {
                StoreError::Io(e)
            }
        })
    }

    pub fn blob_exists(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (InstanceStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        (InstanceStore::new(dir.path()).unwrap(), dir)
    }

    #[test]
    fn save_load_round_trip() {
        let (s, _d) = store();
        let inst = UniverseInstance::empty("a1", "owner", "Title");
        s.save(&inst).unwrap();
        let loaded = s.load("a1").unwrap();
        assert_eq!(inst, loaded);
    }

    #[test]
    fn load_inexistente_erro_notfound() {
        let (s, _d) = store();
        assert!(matches!(s.load("nope"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn list_owner_filtra() {
        let (s, _d) = store();
        s.save(&UniverseInstance::empty("a", "alice", "A")).unwrap();
        s.save(&UniverseInstance::empty("b", "bob", "B")).unwrap();
        let alice = s.list_owner("alice").unwrap();
        assert_eq!(alice.len(), 1);
        assert_eq!(alice[0].id, "a");
    }

    #[test]
    fn list_published_filtra() {
        let (s, _d) = store();
        let mut pub_inst = UniverseInstance::empty("p", "o", "P");
        pub_inst.published = true;
        s.save(&pub_inst).unwrap();
        s.save(&UniverseInstance::empty("q", "o", "Q")).unwrap();
        assert_eq!(s.list_published().unwrap().len(), 1);
    }

    #[test]
    fn delete_remove() {
        let (s, _d) = store();
        s.save(&UniverseInstance::empty("z", "o", "Z")).unwrap();
        assert!(s.exists("z"));
        s.delete("z").unwrap();
        assert!(!s.exists("z"));
        assert!(matches!(s.delete("z"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn blob_write_dedupe_read() {
        let (s, _d) = store();
        let data = b"<svg>corpo</svg>";
        let (h1, n1) = s.write_blob(data).unwrap();
        let (h2, _n2) = s.write_blob(data).unwrap();
        assert_eq!(h1, h2, "mesmo conteúdo → mesmo hash");
        assert_eq!(n1, data.len() as u64);
        assert!(s.blob_exists(&h1));
        assert_eq!(s.read_blob(&h1).unwrap(), data);
    }

    #[test]
    fn blob_inexistente_erro() {
        let (s, _d) = store();
        assert!(matches!(
            s.read_blob(&"f".repeat(64)),
            Err(StoreError::BlobNotFound(_))
        ));
    }
}
