//! Perfil universal de usuário (YG-137) — UM perfil por usuário, sobre o qual
//! cada universo **compõe** (composição sobre herança, como nota↔tarefa).
//!
//! Canônico = um JSON por usuário em `<root>/<user-slug>.json`. O perfil base
//! (nome, bio, avatar) é universal; o mapa `universes` guarda o perfil que o
//! usuário criou *dentro* de cada universo — cada universo lê/escreve seu
//! sub-objeto livre (`props`) sem um tipo novo. Escrita atômica (temp+rename).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Normalização de usuário → nome de arquivo, SEM o fallback "termo" do
// `slugify` de domínio (que colidiria identidades). Minúsculas, [a-z0-9] e
// `_`/`@`/`.`→`-`; vazio = inválido.
fn user_slug(user: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in user.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("usuário inválido")]
    InvalidUser,
    #[error("erro de I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// O perfil que um usuário tem DENTRO de um universo — composição livre.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UniverseProfile {
    pub slug: String,
    pub joined_at: DateTime<Utc>,
    /// Props que cada universo compõe por cima (papel, apelido local, etc.).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, serde_json::Value>,
}

/// Perfil universal — base + composição por universo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// Dono (sub do JWT — e-mail/id). Imutável.
    pub user: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub bio: String,
    /// Avatar como emoji (sem upload por ora).
    #[serde(default)]
    pub avatar: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Perfis criados dentro de universos (slug → composição).
    #[serde(default)]
    pub universes: BTreeMap<String, UniverseProfile>,
}

impl Profile {
    fn new(user: &str, now: DateTime<Utc>) -> Self {
        Self {
            user: user.to_string(),
            display_name: user.split('@').next().unwrap_or(user).to_string(),
            bio: String::new(),
            avatar: "✦".to_string(),
            created_at: now,
            updated_at: now,
            universes: BTreeMap::new(),
        }
    }
}

/// Store de perfis enraizado num diretório (`<root>/<user-slug>.json`).
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            dir: root.as_ref().to_path_buf(),
        }
    }

    fn path(&self, user: &str) -> Result<PathBuf, ProfileError> {
        let s = user_slug(user);
        if s.is_empty() {
            return Err(ProfileError::InvalidUser);
        }
        Ok(self.dir.join(format!("{s}.json")))
    }

    /// Lê o perfil; cria um vazio (não persistido) se ainda não existe — assim
    /// `GET /profile` sempre devolve algo coerente no primeiro acesso.
    pub fn load_or_new(&self, user: &str) -> Result<Profile, ProfileError> {
        let path = self.path(user)?;
        match std::fs::read(&path) {
            Ok(bytes) => {
                Ok(serde_json::from_slice(&bytes)
                    .unwrap_or_else(|_| Profile::new(user, Utc::now())))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Profile::new(user, Utc::now()))
            }
            Err(e) => Err(ProfileError::Io(e)),
        }
    }

    fn save(&self, p: &Profile) -> Result<(), ProfileError> {
        let path = self.path(&p.user)?;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(p).unwrap_or_default())?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Atualiza a base (nome/bio/avatar); `None` preserva o campo atual.
    pub fn update_base(
        &self,
        user: &str,
        display_name: Option<&str>,
        bio: Option<&str>,
        avatar: Option<&str>,
    ) -> Result<Profile, ProfileError> {
        let mut p = self.load_or_new(user)?;
        if let Some(v) = display_name {
            p.display_name = v.to_string();
        }
        if let Some(v) = bio {
            p.bio = v.to_string();
        }
        if let Some(v) = avatar {
            p.avatar = v.to_string();
        }
        p.updated_at = Utc::now();
        self.save(&p)?;
        Ok(p)
    }

    /// "Criar perfil neste universo" (a CTA do catálogo) — idempotente:
    /// preserva `joined_at` se já existir; mescla `props`. Devolve o perfil.
    pub fn join_universe(
        &self,
        user: &str,
        slug: &str,
        props: BTreeMap<String, serde_json::Value>,
    ) -> Result<Profile, ProfileError> {
        let mut p = self.load_or_new(user)?;
        let now = Utc::now();
        let up = p
            .universes
            .entry(slug.to_string())
            .or_insert_with(|| UniverseProfile {
                slug: slug.to_string(),
                joined_at: now,
                props: BTreeMap::new(),
            });
        for (k, v) in props {
            up.props.insert(k, v);
        }
        p.updated_at = now;
        self.save(&p)?;
        Ok(p)
    }

    /// Sai de um universo (remove o sub-perfil). `true` se havia.
    pub fn leave_universe(&self, user: &str, slug: &str) -> Result<bool, ProfileError> {
        let mut p = self.load_or_new(user)?;
        let had = p.universes.remove(slug).is_some();
        if had {
            p.updated_at = Utc::now();
            self.save(&p)?;
        }
        Ok(had)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (ProfileStore, tempfile::TempDir) {
        let d = tempdir().unwrap();
        (ProfileStore::new(d.path()), d)
    }

    #[test]
    fn perfil_base_default_e_update_preserva() {
        let (s, _d) = store();
        let p = s.load_or_new("yuri@x.com").unwrap();
        assert_eq!(p.display_name, "yuri"); // derivado do e-mail
        assert_eq!(p.avatar, "✦");
        let u = s
            .update_base("yuri@x.com", Some("Yuri"), None, Some("🦊"))
            .unwrap();
        assert_eq!(u.display_name, "Yuri");
        assert_eq!(u.avatar, "🦊");
        // None preserva
        let u2 = s
            .update_base("yuri@x.com", None, Some("bio nova"), None)
            .unwrap();
        assert_eq!(u2.display_name, "Yuri");
        assert_eq!(u2.bio, "bio nova");
        assert_eq!(u2.avatar, "🦊");
    }

    #[test]
    fn join_universe_idempotente_e_compoe_props() {
        let (s, _d) = store();
        let mut props = BTreeMap::new();
        props.insert("papel".into(), serde_json::json!("explorador"));
        let p = s.join_universe("ana@x.com", "shandara", props).unwrap();
        assert!(p.universes.contains_key("shandara"));
        let joined0 = p.universes["shandara"].joined_at;
        assert_eq!(
            p.universes["shandara"].props["papel"],
            serde_json::json!("explorador")
        );
        // re-join preserva joined_at e mescla
        let mut props2 = BTreeMap::new();
        props2.insert("nivel".into(), serde_json::json!(3));
        let p2 = s.join_universe("ana@x.com", "shandara", props2).unwrap();
        assert_eq!(
            p2.universes["shandara"].joined_at, joined0,
            "joined_at preservado"
        );
        assert_eq!(
            p2.universes["shandara"].props["papel"],
            serde_json::json!("explorador")
        );
        assert_eq!(
            p2.universes["shandara"].props["nivel"],
            serde_json::json!(3)
        );
        // leave remove
        assert!(s.leave_universe("ana@x.com", "shandara").unwrap());
        assert!(
            !s.load_or_new("ana@x.com")
                .unwrap()
                .universes
                .contains_key("shandara")
        );
    }

    #[test]
    fn usuario_invalido_rejeitado() {
        let (s, _d) = store();
        assert!(matches!(s.path("///"), Err(ProfileError::InvalidUser)));
    }
}
