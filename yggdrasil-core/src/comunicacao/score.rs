//! Camada Shannon — ledger de bits por usuário (YG-168).
//!
//! **Mecânica**: aprender uma linguagem *rende* bits proporcionais à sua
//! entropia (Shannon) e bits *compram* pistas (revelar glosa oculta).
//!
//! - **Descoberta** (1ª vez): credita `⌊bits_per_symbol⌋` (mín 1) — idempotente.
//! - **Identificação correta** (quiz): bônus `⌊2 × bits_per_symbol / 2^tentativas⌋`;
//!   tentativas contam tanto erros quanto acertos (anti-brute-force).
//! - **Revelar**: debita `⌈bits_per_symbol⌉`; saldo nunca vai abaixo de zero.
//!
//! `bits_per_symbol` vem de `entropy_stats` quando presente; senão `log2(n)` onde
//! `n` é o número de entradas do pacote (alfabeto uniforme — [`EntropyStats::uniform`]).
//!
//! Persistência: `<root>/_score/<user-slug>.json`, mesma política atômica das salas.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::lexicon::slugify;
use super::pack::{EntropyStats, LexiconPack};
use super::store::StoreError;

/// Calcula `bits_per_symbol` de um pacote: de `entropy_stats` quando presente,
/// senão entropia uniforme sobre as entradas do pacote.
pub fn pack_bits_per_symbol(pack: &LexiconPack) -> f64 {
    if let Some(stats) = &pack.entropy_stats {
        stats.bits_per_symbol
    } else {
        EntropyStats::uniform(pack.entries.len()).bits_per_symbol
    }
}

/// Ledger de bits por usuário.
///
/// Todas as chaves de entrada são `"{pack_id}/{term}"` — estável e único.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BitsLedger {
    /// Saldo atual de bits (nunca negativo).
    pub total_bits: f64,
    /// Entradas já descobertas (idempotente — 2ª descoberta não credita).
    #[serde(default)]
    pub discovered: BTreeSet<String>,
    /// Total de tentativas de identificação por entrada (acertos + erros).
    #[serde(default)]
    pub identified: BTreeMap<String, u32>,
}

impl BitsLedger {
    fn entry_key(pack: &str, term: &str) -> String {
        format!("{pack}/{term}")
    }

    /// Credita bits pela **primeira** descoberta de `term` no `pack`.
    /// Retorna os bits creditados (0 se já descoberto).
    pub fn discover(&mut self, pack: &str, term: &str, bits_per_symbol: f64) -> f64 {
        let key = Self::entry_key(pack, term);
        if self.discovered.contains(&key) {
            return 0.0;
        }
        let credit = bits_per_symbol.floor().max(1.0);
        self.total_bits += credit;
        self.discovered.insert(key);
        credit
    }

    /// Valida uma tentativa de identificação no servidor.
    ///
    /// `answer` é o que o usuário escolheu; `correct_term` é o gabarito (do
    /// pack, nunca do cliente). Incrementa o contador de tentativas para ambos
    /// — acertos E erros —, depois credita bônus decrescente se correto.
    ///
    /// Retorna `(correto, bits_creditados)`.
    pub fn grade_attempt(
        &mut self,
        pack: &str,
        term: &str,
        answer: &str,
        correct_term: &str,
        bits_per_symbol: f64,
    ) -> (bool, f64) {
        let key = Self::entry_key(pack, term);
        let attempts = *self.identified.get(&key).unwrap_or(&0);
        *self.identified.entry(key).or_insert(0) += 1;

        let correct = answer.trim().to_lowercase() == correct_term.trim().to_lowercase();
        if !correct {
            return (false, 0.0);
        }
        // Bônus decrescente: 2×bps na 1ª tentativa, divide por 2 a cada erro.
        let divisor = (1u64 << attempts.min(62)) as f64;
        let raw = 2.0 * bits_per_symbol / divisor;
        let bonus = if attempts == 0 {
            raw.floor().max(1.0)
        } else {
            raw.floor().max(0.0)
        };
        self.total_bits += bonus;
        (true, bonus)
    }

    /// Debita `⌈bits_per_symbol⌉` bits para revelar uma glosa oculta.
    ///
    /// Retorna `Ok(custo)` quando o saldo é suficiente, ou `Err(custo)` quando
    /// insuficiente (saldo permanece inalterado — nunca vai negativo).
    pub fn reveal(&mut self, pack: &str, term: &str, bits_per_symbol: f64) -> Result<f64, f64> {
        let _ = (pack, term); // chave usada p/ idempotência futura se necessário
        let cost = bits_per_symbol.ceil().max(1.0);
        if self.total_bits < cost {
            return Err(cost);
        }
        self.total_bits -= cost;
        Ok(cost)
    }
}

/// Store em disco do ledger por usuário: `<root>/_score/<user-slug>.json`.
/// Mesmo padrão de [`CadernoStore`](super::caderno::CadernoStore).
pub struct BitsLedgerStore {
    root: PathBuf,
}

impl BitsLedgerStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("_score"))?;
        Ok(Self { root })
    }

    fn score_path(&self, user: &str) -> PathBuf {
        self.root
            .join("_score")
            .join(format!("{}.json", slugify(user)))
    }

    /// Carrega o ledger do usuário (vazio se ainda não existe).
    pub fn load(&self, user: &str) -> Result<BitsLedger, StoreError> {
        let path = self.score_path(user);
        match std::fs::read(&path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BitsLedger::default()),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    /// Grava o ledger atomicamente (temp + rename).
    pub fn save(&self, user: &str, ledger: &BitsLedger) -> Result<(), StoreError> {
        let dir = self.root.join("_score");
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(ledger)?;
        let final_path = self.score_path(user);
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

    // ── BitsLedger unit ──────────────────────────────────────────────────────

    #[test]
    fn descoberta_credita_floor_bps_min1() {
        let mut l = BitsLedger::default();
        let credited = l.discover("musica", "A4", 3.58);
        assert!((credited - 3.0).abs() < 1e-9, "floor(3.58)=3");
        assert!((l.total_bits - 3.0).abs() < 1e-9);
    }

    #[test]
    fn descoberta_min1_quando_bps_zero() {
        let mut l = BitsLedger::default();
        let c = l.discover("pack", "term", 0.0);
        assert_eq!(c, 1.0, "mínimo de 1 bit mesmo com bps=0");
    }

    #[test]
    fn descoberta_idempotente() {
        let mut l = BitsLedger::default();
        l.discover("musica", "A4", 3.58);
        let second = l.discover("musica", "A4", 3.58);
        assert_eq!(second, 0.0, "segunda descoberta não credita");
        assert!((l.total_bits - 3.0).abs() < 1e-9, "saldo não muda");
    }

    #[test]
    fn identificacao_correta_credita_bonus_decrescente() {
        let mut l = BitsLedger::default();
        // 1ª tentativa certa: floor(2 * 3.58 / 1) = floor(7.16) = 7, mín 1 → 7
        let (ok, b) = l.grade_attempt("p", "t", "t", "t", 3.58);
        assert!(ok);
        assert!((b - 7.0).abs() < 1e-9, "1ª tentativa: {b}");
        // 2ª tentativa (2ª vez que gradua = 1 tentativa acumulada → divisor=2): floor(7.16/2)=3
        let (ok2, b2) = l.grade_attempt("p", "t", "t", "t", 3.58);
        assert!(ok2);
        assert!((b2 - 3.0).abs() < 1e-9, "2ª tentativa: {b2}");
    }

    #[test]
    fn identificacao_errada_nao_credita_mas_conta_tentativa() {
        let mut l = BitsLedger::default();
        let (ok, b) = l.grade_attempt("p", "t", "errado", "t", 3.58);
        assert!(!ok);
        assert_eq!(b, 0.0);
        // após 1 erro a tentativa certa tem divisor=2
        let (ok2, b2) = l.grade_attempt("p", "t", "t", "t", 3.58);
        assert!(ok2);
        assert!((b2 - 3.0).abs() < 1e-9, "após 1 erro: {b2}");
    }

    #[test]
    fn reveal_debita_ceil_bps() {
        let mut l = BitsLedger::default();
        l.total_bits = 10.0;
        let r = l.reveal("musica", "A4", 3.58);
        assert!(r.is_ok());
        let cost = r.unwrap();
        assert!((cost - 4.0).abs() < 1e-9, "ceil(3.58)=4");
        assert!((l.total_bits - 6.0).abs() < 1e-9);
    }

    #[test]
    fn reveal_saldo_insuficiente_nao_debita_e_retorna_custo() {
        let mut l = BitsLedger::default();
        l.total_bits = 1.0;
        let r = l.reveal("musica", "A4", 3.58);
        assert!(r.is_err());
        let cost = r.unwrap_err();
        assert!((cost - 4.0).abs() < 1e-9);
        assert!((l.total_bits - 1.0).abs() < 1e-9, "saldo inalterado");
    }

    #[test]
    fn reveal_min1_quando_bps_zero() {
        let mut l = BitsLedger::default();
        l.total_bits = 5.0;
        let r = l.reveal("pack", "term", 0.0);
        assert!(r.is_ok());
        assert!((r.unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn saldo_nunca_negativo() {
        let mut l = BitsLedger::default();
        // descoberta credita 1 bit (mín), reveal tenta debitar 4 → bloqueado
        l.discover("p", "t", 0.0);
        assert_eq!(l.total_bits, 1.0);
        let r = l.reveal("p", "t", 3.58);
        assert!(r.is_err(), "saldo insuficiente deve ser Err");
        assert_eq!(l.total_bits, 1.0, "saldo permanece positivo");
    }

    // ── BitsLedgerStore ──────────────────────────────────────────────────────

    #[test]
    fn store_vazio_quando_novo_usuario() {
        let dir = tempdir().unwrap();
        let store = BitsLedgerStore::new(dir.path()).unwrap();
        let l = store.load("alice").unwrap();
        assert_eq!(l.total_bits, 0.0);
        assert!(l.discovered.is_empty());
    }

    #[test]
    fn store_round_trip() {
        let dir = tempdir().unwrap();
        let store = BitsLedgerStore::new(dir.path()).unwrap();
        let mut l = BitsLedger::default();
        l.discover("musica", "A4", 3.58);
        l.total_bits = 10.0;
        store.save("alice", &l).unwrap();
        let loaded = store.load("alice").unwrap();
        assert!((loaded.total_bits - 10.0).abs() < 1e-9);
        assert!(loaded.discovered.contains("musica/A4"));
    }

    #[test]
    fn store_isola_usuarios() {
        let dir = tempdir().unwrap();
        let store = BitsLedgerStore::new(dir.path()).unwrap();
        let mut l = BitsLedger::default();
        l.total_bits = 99.0;
        store.save("alice@test.com", &l).unwrap();
        let bob = store.load("bob@test.com").unwrap();
        assert_eq!(bob.total_bits, 0.0, "bob começa zerado");
    }

    // ── pack_bits_per_symbol ─────────────────────────────────────────────────

    #[test]
    fn bps_de_pack_com_entropy_stats() {
        let pack = super::super::pack::music_pack();
        let bps = pack_bits_per_symbol(&pack);
        // music_pack usa EntropyStats::uniform(12) → log2(12)
        assert!((bps - 12f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn bps_de_pack_sem_entropy_stats_usa_log2_n() {
        let pack = super::super::pack::language_pack();
        assert!(pack.entropy_stats.is_none());
        let n = pack.entries.len() as f64;
        let bps = pack_bits_per_symbol(&pack);
        assert!((bps - n.log2()).abs() < 1e-9);
    }
}
