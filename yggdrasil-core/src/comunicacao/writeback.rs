//! Write-back de contribuições de léxico: leva os arquivos `_users/*.md` que
//! [`super::lexicon::LexiconStore::contribute`] gravou em disco para o **git**
//! do checkout `comunicacao` — `git add`/commit (e opcionalmente push).
//!
//! Motivação (YG-100): hoje o servidor só **escreve o arquivo** no checkout.
//! Num redeploy/restart o volume efêmero some e a contribuição se perde. Este
//! motor versiona as contribuições para que persistam no repo curável.
//!
//! Princípios:
//! - **Sem dependência de lib git** — usa só [`std::process::Command`] sobre o
//!   `git` do ambiente (mesmo binário que o operador usa no Fly).
//! - **Env-gated**: só roda se [`WritebackConfig::enabled`] (ligado por
//!   `YGGDRASIL_COMUNICACAO_WRITEBACK`). Desligado = no-op silencioso.
//! - **Funil de erro único**: [`WritebackOutcome`] nunca entra em panic; toda
//!   falha (git ausente, dir não-git, conflito) vira `Err` reportável e o
//!   request/job segue.
//! - **Idempotente**: faz stage só dos caminhos `_users/` da contribuição e
//!   confere `git diff --cached --quiet` antes de commitar — sem mudança no
//!   índice, não cria commit vazio.
//! - **Identidade local**: commita com `-c user.name -c user.email` (não
//!   depende de config global do container).
//! - **Push best-effort**: se `push` está ligado e falha, não falha o commit.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Configuração do motor de write-back. Construída de env em produção
/// ([`WritebackConfig::from_env`]) ou direta nos testes
/// ([`WritebackConfig::for_testing`]).
#[derive(Debug, Clone)]
pub struct WritebackConfig {
    /// Liga/desliga o motor inteiro. Desligado → todo `run()` é no-op `Ok`.
    pub enabled: bool,
    /// Raiz do checkout `comunicacao` (o repo git).
    pub repo_dir: PathBuf,
    /// `user.name` usado no commit local (não toca config global).
    pub author_name: String,
    /// `user.email` usado no commit local.
    pub author_email: String,
    /// Se `true`, tenta `git push` best-effort após commitar.
    pub push: bool,
    /// Remote para o push (default `origin`).
    pub remote: String,
}

impl WritebackConfig {
    /// Lê a configuração do ambiente. O gate é
    /// `YGGDRASIL_COMUNICACAO_WRITEBACK` (`1`/`true`/`on`/`yes`). `repo_dir`
    /// reusa `COMUNICACAO_DIR` (mesmo checkout do léxico).
    pub fn from_env() -> Self {
        let enabled = std::env::var("YGGDRASIL_COMUNICACAO_WRITEBACK")
            .map(|v| is_truthy(&v))
            .unwrap_or(false);
        let repo_dir = std::env::var("COMUNICACAO_DIR").unwrap_or_else(|_| "../comunicacao".into());
        let author_name = std::env::var("YGGDRASIL_COMUNICACAO_GIT_NAME")
            .unwrap_or_else(|_| "Yggdrasil Comunicação".into());
        let author_email = std::env::var("YGGDRASIL_COMUNICACAO_GIT_EMAIL")
            .unwrap_or_else(|_| "comunicacao@yggdrasil".into());
        let push = std::env::var("YGGDRASIL_COMUNICACAO_WRITEBACK_PUSH")
            .map(|v| is_truthy(&v))
            .unwrap_or(false);
        let remote =
            std::env::var("YGGDRASIL_COMUNICACAO_GIT_REMOTE").unwrap_or_else(|_| "origin".into());
        Self {
            enabled,
            repo_dir: PathBuf::from(repo_dir),
            author_name,
            author_email,
            push,
            remote,
        }
    }

    /// Config explícita para testes: motor ligado, identidade fixa, sem push.
    pub fn for_testing(repo_dir: impl AsRef<Path>) -> Self {
        Self {
            enabled: true,
            repo_dir: repo_dir.as_ref().to_path_buf(),
            author_name: "Test Bot".into(),
            author_email: "test@yggdrasil".into(),
            push: false,
            remote: "origin".into(),
        }
    }
}

fn is_truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

/// Erro de write-back. Sempre carregado de contexto (nunca panic).
#[derive(Debug, thiserror::Error)]
pub enum WritebackError {
    #[error("git não disponível ou diretório não é repo git: {0}")]
    NotAGitRepo(String),
    #[error("falha ao executar git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {cmd} falhou (status {status}): {stderr}")]
    Git {
        cmd: String,
        status: String,
        stderr: String,
    },
}

/// Desfecho de uma rodada de write-back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WritebackOutcome {
    /// Motor desligado pelo gate — nada feito.
    Disabled,
    /// Nenhum caminho mudou no índice — sem commit (idempotente).
    NothingToCommit,
    /// Commit criado. `pushed` = se o push best-effort teve sucesso.
    Committed { pushed: bool },
}

/// Motor de write-back. Barato de clonar (só a config).
#[derive(Debug, Clone)]
pub struct Writeback {
    config: WritebackConfig,
}

impl Writeback {
    pub fn new(config: WritebackConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &WritebackConfig {
        &self.config
    }

    /// Funil de erro único: roda o write-back de UMA contribuição.
    ///
    /// `paths` são caminhos **relativos ao repo** (ex.:
    /// `"yoruba/terms/_users/alice/ase.md"`). Por segurança, só caminhos sob
    /// `_users/` entram no stage — qualquer outro é ignorado, garantindo que o
    /// motor nunca commite arquivos fora da área de contribuição.
    ///
    /// `message` é a mensagem de commit (conventional). Nunca entra em panic:
    /// toda falha vira [`WritebackError`].
    pub fn run(&self, paths: &[String], message: &str) -> Result<WritebackOutcome, WritebackError> {
        if !self.config.enabled {
            return Ok(WritebackOutcome::Disabled);
        }
        // só caminhos de contribuição (`_users/`).
        let staged: Vec<&String> = paths.iter().filter(|p| is_user_path(p)).collect();
        if staged.is_empty() {
            return Ok(WritebackOutcome::NothingToCommit);
        }

        self.ensure_git_repo()?;

        // stage só os caminhos de contribuição.
        let mut add_args: Vec<&str> = vec!["add", "--"];
        add_args.extend(staged.iter().map(|s| s.as_str()));
        self.git(&add_args)?;

        // idempotência: nada no índice → sem commit vazio.
        if self.index_clean()? {
            return Ok(WritebackOutcome::NothingToCommit);
        }

        // commit com identidade LOCAL (não depende de config global).
        self.git(&[
            "-c",
            &format!("user.name={}", self.config.author_name),
            "-c",
            &format!("user.email={}", self.config.author_email),
            "commit",
            "-m",
            message,
            "--",
        ])?;

        // push best-effort: falha não derruba o commit já feito.
        let pushed = if self.config.push {
            self.git(&["push", &self.config.remote, "HEAD"]).is_ok()
        } else {
            false
        };

        Ok(WritebackOutcome::Committed { pushed })
    }

    /// Confere que `repo_dir` é um worktree git. Tolera dir não-git devolvendo
    /// [`WritebackError::NotAGitRepo`] em vez de entrar em panic.
    fn ensure_git_repo(&self) -> Result<(), WritebackError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.config.repo_dir)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .map_err(WritebackError::Spawn)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(WritebackError::NotAGitRepo(
                self.config.repo_dir.display().to_string(),
            ))
        }
    }

    /// Como [`run`], mas para a **curadoria** (YG-101): versiona caminhos curados
    /// (`<lang>/terms/<slug>.md`) **além** dos de `_users/`, num único commit. É
    /// curador-autorizado — não passa pelo filtro `is_user_path` (que protege só
    /// o write-back automático de contribuições). Mesma semântica de
    /// idempotência/push/identidade-local do [`run`].
    pub fn commit_paths(
        &self,
        paths: &[String],
        message: &str,
    ) -> Result<WritebackOutcome, WritebackError> {
        if !self.config.enabled {
            return Ok(WritebackOutcome::Disabled);
        }
        if paths.is_empty() {
            return Ok(WritebackOutcome::NothingToCommit);
        }
        self.ensure_git_repo()?;
        let mut add_args: Vec<&str> = vec!["add", "--"];
        add_args.extend(paths.iter().map(|s| s.as_str()));
        self.git(&add_args)?;
        if self.index_clean()? {
            return Ok(WritebackOutcome::NothingToCommit);
        }
        self.git(&[
            "-c",
            &format!("user.name={}", self.config.author_name),
            "-c",
            &format!("user.email={}", self.config.author_email),
            "commit",
            "-m",
            message,
            "--",
        ])?;
        let pushed = if self.config.push {
            self.git(&["push", &self.config.remote, "HEAD"]).is_ok()
        } else {
            false
        };
        Ok(WritebackOutcome::Committed { pushed })
    }

    /// `true` se o índice está limpo (nada staged) → não há o que commitar.
    fn index_clean(&self) -> Result<bool, WritebackError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.config.repo_dir)
            .args(["diff", "--cached", "--quiet"])
            .output()
            .map_err(WritebackError::Spawn)?;
        // exit 0 → sem diff (limpo); exit 1 → há diff staged.
        Ok(out.status.success())
    }

    /// Roda um subcomando git no `repo_dir`. Erro não-zero vira
    /// [`WritebackError::Git`] com stderr capturado.
    fn git(&self, args: &[&str]) -> Result<(), WritebackError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.config.repo_dir)
            .args(args)
            .output()
            .map_err(WritebackError::Spawn)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(WritebackError::Git {
                cmd: args.first().map(|s| s.to_string()).unwrap_or_default(),
                status: out.status.to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            })
        }
    }
}

/// `true` se o caminho relativo é uma contribuição de usuário (`_users/`).
fn is_user_path(path: &str) -> bool {
    path.split('/').any(|seg| seg == "_users")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Cria um repo git inicializado num tempdir, com identidade e commit base.
    fn git_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?} falhou");
        };
        run(&["init", "-q"]);
        run(&["config", "user.name", "Seed"]);
        run(&["config", "user.email", "seed@test"]);
        // commit base p/ HEAD existir
        fs::write(dir.path().join("README.md"), "seed\n").unwrap();
        run(&["add", "README.md"]);
        run(&["commit", "-q", "-m", "seed"]);
        dir
    }

    fn write_contrib(root: &Path, rel: &str) {
        let abs = root.join(rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, "---\nword: teste\n---\n").unwrap();
    }

    fn head_count(root: &Path) -> usize {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    }

    #[test]
    fn nova_contribuicao_gera_um_commit() {
        let dir = git_repo();
        let rel = "yoruba/terms/_users/alice/ase.md";
        write_contrib(dir.path(), rel);
        let wb = Writeback::new(WritebackConfig::for_testing(dir.path()));
        let before = head_count(dir.path());
        let outcome = wb
            .run(&[rel.to_string()], "feat(comunicacao): nova contribuição")
            .unwrap();
        assert_eq!(outcome, WritebackOutcome::Committed { pushed: false });
        assert_eq!(head_count(dir.path()), before + 1, "deve criar 1 commit");
    }

    #[test]
    fn sem_mudanca_nao_commita() {
        let dir = git_repo();
        let rel = "yoruba/terms/_users/alice/ase.md";
        write_contrib(dir.path(), rel);
        let wb = Writeback::new(WritebackConfig::for_testing(dir.path()));
        wb.run(&[rel.to_string()], "feat: 1").unwrap();
        let after_first = head_count(dir.path());
        // segunda rodada sem alterar o arquivo → idempotente, sem commit novo.
        let outcome = wb.run(&[rel.to_string()], "feat: 2").unwrap();
        assert_eq!(outcome, WritebackOutcome::NothingToCommit);
        assert_eq!(head_count(dir.path()), after_first, "sem commit vazio");
    }

    #[test]
    fn caminho_fora_de_users_fica_sem_track() {
        let dir = git_repo();
        // arquivo fora de `_users/` é gravado mas NÃO deve ser stageado/commitado.
        let rel = "yoruba/terms/curado.md";
        write_contrib(dir.path(), rel);
        let wb = Writeback::new(WritebackConfig::for_testing(dir.path()));
        let outcome = wb.run(&[rel.to_string()], "feat: nope").unwrap();
        assert_eq!(outcome, WritebackOutcome::NothingToCommit);
        // o arquivo continua untracked no repo.
        let status = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["status", "--porcelain", "--", rel])
            .output()
            .unwrap();
        let s = String::from_utf8_lossy(&status.stdout);
        assert!(s.contains("??"), "deve estar untracked: {s:?}");
    }

    #[test]
    fn dir_nao_git_e_tolerado() {
        // tempdir sem `git init` → run devolve Err, nunca panic.
        let dir = tempfile::tempdir().unwrap();
        let rel = "yoruba/terms/_users/alice/ase.md";
        write_contrib(dir.path(), rel);
        let wb = Writeback::new(WritebackConfig::for_testing(dir.path()));
        let res = wb.run(&[rel.to_string()], "feat: x");
        assert!(matches!(res, Err(WritebackError::NotAGitRepo(_))));
    }

    #[test]
    fn gate_desligado_e_noop() {
        let dir = git_repo();
        let rel = "yoruba/terms/_users/alice/ase.md";
        write_contrib(dir.path(), rel);
        let mut cfg = WritebackConfig::for_testing(dir.path());
        cfg.enabled = false;
        let wb = Writeback::new(cfg);
        let before = head_count(dir.path());
        let outcome = wb.run(&[rel.to_string()], "feat: x").unwrap();
        assert_eq!(outcome, WritebackOutcome::Disabled);
        assert_eq!(head_count(dir.path()), before, "nada commitado");
    }

    #[test]
    fn is_user_path_detecta_segmento() {
        assert!(is_user_path("yoruba/terms/_users/alice/ase.md"));
        assert!(is_user_path("_users/x.md"));
        assert!(!is_user_path("yoruba/terms/curado.md"));
        assert!(!is_user_path("_userss/x.md"));
    }
}
