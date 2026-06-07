//! Apply inbound: aplica eventos vindos do CO ao `NoteStore` (YG-97, Fase P-B do
//! ADR `docs/architecture/event-driven-sync.md`).
//!
//! É a metade **de volta** do round-trip: o YG-93/YG-103 *emite* (`entry.*` →
//! hub); aqui o Yggdrasil *recebe* `entry.{created,updated,deleted}` do CO e
//! grava na nota correspondente — tornando as notas **editáveis no CO**, não só
//! read-only.
//!
//! **Path instance-qualified.** O alvo vem do `path` do envelope
//! `instances/<id>/notes/<slug>.md` (YG-97) — [`parse_note_path`] resolve o
//! `(instance, slug)` p/ achar o `NoteStore`. Só `universe_key=yggdrasil` é
//! aplicado aqui; termos de comunicação (write-back CO→`comunicacao`) são fora de
//! escopo (análogo ao "Não no escopo" da YG-103).
//!
//! **Loop-guard.** Um evento cujo `origin_deployment` é este deployment e
//! `hop_count > 0` (echo da **nossa** própria escrita que voltou do hub) é
//! descartado ([`FederatedEvent::is_own_echo`]) — sem isso o apply re-emitiria e
//! laçaria.
//!
//! **Action tree (UPSERT, file-granular).** [`UpsertAction`] são os verbos do
//! ADR. [`decide_default`] é o **auto-resolve** headless (o `sugestao` do ADR):
//! Deleted→delete, ausente→cria, sha igual→skip, mudou→update (remoto aplica). O
//! **modal do CO-385** sobrepõe com `keep-both`/`replace`/`skip` quando um humano
//! reconcilia dois dispositivos divergentes — [`apply_with_action`] executa
//! qualquer verbo, incl. `keep-both` (grava `<slug>-<n>.md`, ambos retidos).
//!
//! **E2E pende CO-385** (o modal/executor que envia o verbo de conflito). Aqui:
//! build + testes de unidade do parse, do auto-resolve e de cada verbo (CRUD em
//! disco via `tempfile`), sem depender de um CO vivo.

use std::path::Path;

use yggdrasil_core::instance::{NoteStore, UniverseInstance};

use crate::co_bridge_producer::{FederatedEvent, NoteKind};

/// Verbo do action tree (UPSERT) a aplicar a uma nota — file-granular (ADR §
/// "UPSERT tree"). `skip`/`update`/`upsert`/`replace`/`keep-both`/`delete`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertAction {
    /// sha256 idêntico → não transfere (a "scaling win" do ADR).
    Skip,
    /// Mudou, sem conflito → aplica (overwrite limpo).
    Update,
    /// Mudou **ou** ausente localmente → cria-se-ausente, senão aplica.
    Upsert,
    /// Força: o remoto é autoritativo → overwrite, descarta o local.
    Replace,
    /// Conflito real → mantém ambos: grava `<slug>-<n>.md` ao lado do local.
    KeepBoth,
    /// Remoto `Deleted` → remove a nota local (tombstone).
    Delete,
}

/// Resultado de aplicar um evento inbound. Carrega o `slug` afetado p/ log/teste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Applied {
    /// Echo da nossa própria escrita (`hop_count > 0`) — descartado (loop-guard).
    EchoIgnored,
    /// Fora de escopo (universe ≠ yggdrasil, ou path não-nota) — ignorado.
    OutOfScope(String),
    /// Path malformado p/ uma nota instance-qualified.
    InvalidPath(String),
    /// Nota criada (não existia).
    Created(String),
    /// Nota atualizada (já existia).
    Updated(String),
    /// sha igual — nada a fazer.
    Skipped(String),
    /// Conflito resolvido por keep-both: `local` mantido, `copy` criado.
    KeptBoth { local: String, copy: String },
    /// Nota removida.
    Deleted(String),
    /// Erro de IO/slug ao aplicar.
    Failed(String),
}

/// Resolve `instances/<id>/notes/<slug>.md` → `(instance, slug)`. `None` p/
/// qualquer outra forma (defesa: o slug/inst não pode conter `/` nem `..`).
pub fn parse_note_path(path: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["instances", inst, "notes", file]
            if !inst.is_empty()
                && *inst != ".."
                && file.ends_with(".md")
                && !file.starts_with("..") =>
        {
            let slug = file.strip_suffix(".md")?;
            if slug.is_empty() {
                None
            } else {
                Some(((*inst).to_string(), slug.to_string()))
            }
        }
        _ => None,
    }
}

/// Auto-resolve headless (o `sugestao` default do ADR): decide o verbo sem um
/// humano no loop. `local` é o corpo da nota em disco (`None` = ausente). A
/// "scaling win" (skip por sha igual do ADR) aqui é comparação direta de corpo —
/// o conteúdo já está em memória, não há ganho em hashear.
pub fn decide_default(local: Option<&str>, kind: NoteKind, incoming_body: &str) -> UpsertAction {
    match kind {
        NoteKind::Deleted => UpsertAction::Delete,
        NoteKind::Created | NoteKind::Updated => match local {
            None => UpsertAction::Upsert,
            // `trim_end`: o round-trip do `NoteStore` (frontmatter markdown)
            // adiciona um `\n` final ao corpo; diferença só de whitespace final
            // não é uma mudança real → skip (evita re-escrita/eco espúrios).
            Some(body) if body.trim_end() == incoming_body.trim_end() => UpsertAction::Skip,
            // Headless: o CO é o hub e o evento representa uma edição intencional
            // → aplica. O conflito interativo (keep-both) é decidido pelo CO-385.
            Some(_) => UpsertAction::Update,
        },
    }
}

/// Aplica um evento inbound com o **auto-resolve** ([`decide_default`]). É o
/// caminho headless/proativo (sem modal). Para o caminho interativo (CO-385
/// manda o verbo), use [`apply_with_action`].
pub fn apply_inbound(root: &Path, event: &FederatedEvent) -> Applied {
    if event.is_own_echo() {
        return Applied::EchoIgnored;
    }
    if event.universe_key != crate::co_bridge_producer::UNIVERSE_KEY {
        return Applied::OutOfScope(event.universe_key.clone());
    }
    let Some((instance, slug)) = parse_note_path(&event.path) else {
        return Applied::InvalidPath(event.path.clone());
    };
    let kind = note_kind_from_event(event);
    let store = NoteStore::for_instance(root, &instance);
    let local = store.load(&slug).ok().map(|n| n.body);
    let action = decide_default(local.as_deref(), kind, &event.payload.body);
    apply_action(root, &store, &instance, &slug, event, action)
}

/// Aplica um evento inbound com um **verbo explícito** — o caminho do CO-385
/// (modal de conflito → `conflito.resolver`). Valida escopo/path como o auto.
pub fn apply_with_action(root: &Path, event: &FederatedEvent, action: UpsertAction) -> Applied {
    if event.is_own_echo() {
        return Applied::EchoIgnored;
    }
    if event.universe_key != crate::co_bridge_producer::UNIVERSE_KEY {
        return Applied::OutOfScope(event.universe_key.clone());
    }
    let Some((instance, slug)) = parse_note_path(&event.path) else {
        return Applied::InvalidPath(event.path.clone());
    };
    let store = NoteStore::for_instance(root, &instance);
    apply_action(root, &store, &instance, &slug, event, action)
}

fn note_kind_from_event(event: &FederatedEvent) -> NoteKind {
    match event.event_type.as_str() {
        "entry.deleted" => NoteKind::Deleted,
        "entry.updated" => NoteKind::Updated,
        _ => NoteKind::Created,
    }
}

/// Garante que o diretório da instância exista antes de gravar uma nota (o
/// `NoteStore` cria `notes/`, mas a instância em si pode não existir ainda num
/// apply puro). Cria um shell vazio se ausente — idempotente.
fn ensure_instance(root: &Path, instance: &str) {
    if let Ok(store) = yggdrasil_core::instance::InstanceStore::new(root)
        && store.load(instance).is_err()
    {
        let shell = UniverseInstance::empty(instance, "co-bridge", "Importado do CO");
        let _ = store.save(&shell);
    }
}

fn apply_action(
    root: &Path,
    store: &NoteStore,
    instance: &str,
    slug: &str,
    event: &FederatedEvent,
    action: UpsertAction,
) -> Applied {
    let title = &event.payload.title;
    let body = &event.payload.body;
    match action {
        UpsertAction::Skip => Applied::Skipped(slug.to_string()),
        UpsertAction::Delete => match store.delete(slug) {
            Ok(()) => Applied::Deleted(slug.to_string()),
            Err(e) => match e {
                // deletar algo já ausente é convergente, não erro.
                yggdrasil_core::instance::NoteError::NotFound(_) => {
                    Applied::Skipped(slug.to_string())
                }
                other => Applied::Failed(other.to_string()),
            },
        },
        UpsertAction::Update | UpsertAction::Upsert | UpsertAction::Replace => {
            let existed = store.load(slug).is_ok();
            if !existed {
                ensure_instance(root, instance);
            }
            match store.save(slug, title, body) {
                Ok(_) if existed => Applied::Updated(slug.to_string()),
                Ok(_) => Applied::Created(slug.to_string()),
                Err(e) => Applied::Failed(e.to_string()),
            }
        }
        UpsertAction::KeepBoth => {
            // mantém o local; grava a versão remota como <slug>-<n> livre.
            let copy = next_free_copy(store, slug);
            ensure_instance(root, instance);
            match store.save(&copy, title, body) {
                Ok(n) => Applied::KeptBoth {
                    local: slug.to_string(),
                    copy: n.slug,
                },
                Err(e) => Applied::Failed(e.to_string()),
            }
        }
    }
}

/// Primeiro `<slug>-<n>` (n≥1) sem nota em disco — o destino do keep-both.
fn next_free_copy(store: &NoteStore, slug: &str) -> String {
    (1..)
        .map(|n| format!("{slug}-{n}"))
        .find(|cand| store.load(cand).is_err())
        .unwrap_or_else(|| format!("{slug}-conflito"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use yggdrasil_core::instance::InstanceStore;

    use crate::co_bridge_producer::{EntryPayload, ORIGIN_DEPLOYMENT};

    fn event(path: &str, kind: &str, title: &str, body: &str) -> FederatedEvent {
        FederatedEvent {
            event_id: 1,
            event_type: kind.into(),
            universe_key: "yggdrasil".into(),
            path: path.into(),
            payload: EntryPayload {
                title: title.into(),
                body: body.into(),
                updated_at: Some("2026-06-07T00:00:00+00:00".into()),
            },
            origin_deployment: "co.artelonga.com.br".into(),
            signed_by: "co".into(),
            hop_count: 1,
        }
    }

    fn store_with_instance(id: &str) -> (TempDir, Arc<InstanceStore>) {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(InstanceStore::new(dir.path()).unwrap());
        let inst = UniverseInstance::empty(id, "owner@test", "Inst");
        store.save(&inst).unwrap();
        (dir, store)
    }

    // ── parse_note_path ───────────────────────────────────────────────────

    #[test]
    fn parse_note_path_resolve_instance_e_slug() {
        assert_eq!(
            parse_note_path("instances/inst-1/notes/minha-nota.md"),
            Some(("inst-1".into(), "minha-nota".into()))
        );
    }

    #[test]
    fn parse_note_path_rejeita_formas_invalidas() {
        assert_eq!(parse_note_path("notes/x.md"), None);
        assert_eq!(parse_note_path("instances/i/notes/x.txt"), None);
        assert_eq!(parse_note_path("instances//notes/x.md"), None);
        assert_eq!(parse_note_path("instances/i/notes/.md"), None);
        assert_eq!(parse_note_path("yoruba/terms/ase.md"), None);
        assert_eq!(parse_note_path("instances/../notes/x.md"), None);
    }

    // ── loop-guard + escopo ───────────────────────────────────────────────

    #[test]
    fn echo_proprio_e_descartado() {
        let (dir, _store) = store_with_instance("i");
        let mut ev = event("instances/i/notes/n.md", "entry.updated", "T", "corpo");
        ev.origin_deployment = ORIGIN_DEPLOYMENT.into(); // veio de nós
        ev.hop_count = 2;
        assert_eq!(apply_inbound(dir.path(), &ev), Applied::EchoIgnored);
    }

    #[test]
    fn universe_fora_de_escopo_e_ignorado() {
        let (dir, _store) = store_with_instance("i");
        let mut ev = event("yoruba/terms/ase.md", "entry.updated", "T", "c");
        ev.universe_key = "comunicacao".into();
        assert!(matches!(
            apply_inbound(dir.path(), &ev),
            Applied::OutOfScope(_)
        ));
    }

    // ── auto-resolve (decide_default) ─────────────────────────────────────

    #[test]
    fn decide_ausente_e_upsert_igual_e_skip_mudou_e_update() {
        assert_eq!(
            decide_default(None, NoteKind::Created, "x"),
            UpsertAction::Upsert
        );
        assert_eq!(
            decide_default(Some("x"), NoteKind::Updated, "x"),
            UpsertAction::Skip
        );
        assert_eq!(
            decide_default(Some("x"), NoteKind::Updated, "y"),
            UpsertAction::Update
        );
        assert_eq!(
            decide_default(Some("x"), NoteKind::Deleted, ""),
            UpsertAction::Delete
        );
    }

    // ── apply_inbound: CRUD em disco ──────────────────────────────────────

    #[test]
    fn inbound_cria_nota_ausente() {
        let (dir, store) = store_with_instance("i");
        let ev = event(
            "instances/i/notes/nova.md",
            "entry.created",
            "Nova",
            "corpo [[outra]]",
        );
        assert_eq!(
            apply_inbound(dir.path(), &ev),
            Applied::Created("nova".into())
        );
        let n = NoteStore::for_instance(store.root(), "i")
            .load("nova")
            .unwrap();
        assert_eq!(n.body.trim_end(), "corpo [[outra]]");
    }

    #[test]
    fn inbound_atualiza_nota_existente() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "Velho", "antigo").unwrap();
        let ev = event(
            "instances/i/notes/n.md",
            "entry.updated",
            "Novo",
            "novo corpo",
        );
        assert_eq!(apply_inbound(dir.path(), &ev), Applied::Updated("n".into()));
        assert_eq!(ns.load("n").unwrap().body.trim_end(), "novo corpo");
    }

    #[test]
    fn inbound_sha_igual_e_skip() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "T", "mesmo corpo").unwrap();
        let ev = event(
            "instances/i/notes/n.md",
            "entry.updated",
            "T",
            "mesmo corpo",
        );
        assert_eq!(apply_inbound(dir.path(), &ev), Applied::Skipped("n".into()));
    }

    #[test]
    fn inbound_deleted_remove() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "T", "corpo").unwrap();
        let ev = event("instances/i/notes/n.md", "entry.deleted", "", "");
        assert_eq!(apply_inbound(dir.path(), &ev), Applied::Deleted("n".into()));
        assert!(ns.load("n").is_err());
    }

    #[test]
    fn inbound_deleted_de_ausente_e_skip_convergente() {
        let (dir, _store) = store_with_instance("i");
        let ev = event("instances/i/notes/fantasma.md", "entry.deleted", "", "");
        assert_eq!(
            apply_inbound(dir.path(), &ev),
            Applied::Skipped("fantasma".into())
        );
    }

    #[test]
    fn inbound_cria_instancia_shell_se_ausente() {
        // instância 'novo' não existe; o apply deve criar um shell e a nota.
        let dir = TempDir::new().unwrap();
        let ev = event("instances/novo/notes/n.md", "entry.created", "T", "corpo");
        assert_eq!(apply_inbound(dir.path(), &ev), Applied::Created("n".into()));
        let store = InstanceStore::new(dir.path()).unwrap();
        assert!(store.load("novo").is_ok(), "shell de instância criado");
    }

    // ── action tree explícito (CO-385) ────────────────────────────────────

    #[test]
    fn keep_both_grava_copia_lado_a_lado() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "Meu", "minha versão").unwrap();
        let ev = event(
            "instances/i/notes/n.md",
            "entry.updated",
            "Deles",
            "versão do CO",
        );
        let out = apply_with_action(dir.path(), &ev, UpsertAction::KeepBoth);
        assert_eq!(
            out,
            Applied::KeptBoth {
                local: "n".into(),
                copy: "n-1".into()
            }
        );
        // ambos retidos, com corpos distintos
        assert_eq!(ns.load("n").unwrap().body.trim_end(), "minha versão");
        assert_eq!(ns.load("n-1").unwrap().body.trim_end(), "versão do CO");
    }

    #[test]
    fn keep_both_incrementa_sufixo_livre() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "T", "a").unwrap();
        ns.save("n-1", "T", "b").unwrap(); // já ocupado
        let ev = event("instances/i/notes/n.md", "entry.updated", "T", "c");
        let out = apply_with_action(dir.path(), &ev, UpsertAction::KeepBoth);
        assert_eq!(
            out,
            Applied::KeptBoth {
                local: "n".into(),
                copy: "n-2".into()
            }
        );
    }

    #[test]
    fn replace_sobrescreve_local() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "Meu", "minha versão").unwrap();
        let ev = event(
            "instances/i/notes/n.md",
            "entry.updated",
            "CO",
            "remoto vence",
        );
        let out = apply_with_action(dir.path(), &ev, UpsertAction::Replace);
        assert_eq!(out, Applied::Updated("n".into()));
        assert_eq!(ns.load("n").unwrap().body.trim_end(), "remoto vence");
    }

    #[test]
    fn skip_explicito_nao_toca_em_disco() {
        let (dir, store) = store_with_instance("i");
        let ns = NoteStore::for_instance(store.root(), "i");
        ns.save("n", "T", "intacto").unwrap();
        let ev = event("instances/i/notes/n.md", "entry.updated", "T", "ignorado");
        assert_eq!(
            apply_with_action(dir.path(), &ev, UpsertAction::Skip),
            Applied::Skipped("n".into())
        );
        assert_eq!(ns.load("n").unwrap().body.trim_end(), "intacto");
    }
}
