//! Fila de revisão de vocabulário (SRS-lite, Leitner).
//!
//! Quando o usuário publica um termo, ele entra aqui para revisão espaçada.
//! Acertar dobra o intervalo; errar zera (revisar de novo hoje). O `term_path`
//! (caminho relativo no léxico) é a chave estável — o mesmo termo nunca
//! duplica na fila.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Teto de intervalo para não agendar revisões a anos de distância.
pub const MAX_INTERVAL_DAYS: u32 = 365;

/// Um item agendado para revisão.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewItem {
    /// Caminho relativo da entrada no léxico — chave estável.
    pub term_path: String,
    pub word: String,
    pub lang: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    pub added_at: DateTime<Utc>,
    pub due_at: DateTime<Utc>,
    /// Intervalo atual (dias). 0 = revisar hoje.
    pub interval_days: u32,
    /// Acertos consecutivos.
    pub reps: u32,
    /// Total de erros.
    pub lapses: u32,
}

impl ReviewItem {
    /// Item novo — vence imediatamente (primeira revisão hoje).
    pub fn new(
        term_path: impl Into<String>,
        word: impl Into<String>,
        lang: impl Into<String>,
        gloss: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            term_path: term_path.into(),
            word: word.into(),
            lang: lang.into(),
            gloss,
            added_at: now,
            due_at: now,
            interval_days: 0,
            reps: 0,
            lapses: 0,
        }
    }

    /// Aplica uma nota de revisão e reagenda.
    pub fn grade(&mut self, correct: bool, now: DateTime<Utc>) {
        if correct {
            self.reps += 1;
            self.interval_days = if self.interval_days == 0 {
                1
            } else {
                self.interval_days.saturating_mul(2).min(MAX_INTERVAL_DAYS)
            };
            self.due_at = now + Duration::days(self.interval_days as i64);
        } else {
            self.lapses += 1;
            self.reps = 0;
            self.interval_days = 0;
            self.due_at = now;
        }
    }

    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        self.due_at <= now
    }
}

/// Fila de revisão de um usuário.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReviewQueue {
    #[serde(default)]
    pub items: Vec<ReviewItem>,
}

impl ReviewQueue {
    /// Insere um item, ou mantém o existente (não reseta agendamento) se o
    /// `term_path` já estiver na fila. Devolve `true` se inseriu novo.
    pub fn upsert(&mut self, item: ReviewItem) -> bool {
        if self.items.iter().any(|i| i.term_path == item.term_path) {
            return false;
        }
        self.items.push(item);
        true
    }

    pub fn get_mut(&mut self, term_path: &str) -> Option<&mut ReviewItem> {
        self.items.iter_mut().find(|i| i.term_path == term_path)
    }

    /// Itens vencidos (due_at ≤ now), mais antigos primeiro.
    pub fn due(&self, now: DateTime<Utc>) -> Vec<&ReviewItem> {
        let mut due: Vec<&ReviewItem> = self.items.iter().filter(|i| i.is_due(now)).collect();
        due.sort_by(|a, b| a.due_at.cmp(&b.due_at));
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn item_novo_vence_imediatamente() {
        let now = t("2026-06-01T00:00:00Z");
        let item = ReviewItem::new("yoruba/terms/ase.md", "àṣẹ", "yo", None, now);
        assert!(item.is_due(now));
        assert_eq!(item.interval_days, 0);
    }

    #[test]
    fn acerto_dobra_intervalo() {
        let now = t("2026-06-01T00:00:00Z");
        let mut item = ReviewItem::new("p", "w", "yo", None, now);
        item.grade(true, now); // 0 → 1
        assert_eq!(item.interval_days, 1);
        assert_eq!(item.due_at, now + Duration::days(1));
        item.grade(true, item.due_at); // 1 → 2
        assert_eq!(item.interval_days, 2);
        item.grade(true, item.due_at); // 2 → 4
        assert_eq!(item.interval_days, 4);
        assert_eq!(item.reps, 3);
    }

    #[test]
    fn erro_zera_e_revisa_hoje() {
        let now = t("2026-06-01T00:00:00Z");
        let mut item = ReviewItem::new("p", "w", "yo", None, now);
        item.grade(true, now);
        item.grade(true, item.due_at);
        let later = t("2026-06-10T00:00:00Z");
        item.grade(false, later);
        assert_eq!(item.interval_days, 0);
        assert_eq!(item.reps, 0);
        assert_eq!(item.lapses, 1);
        assert!(item.is_due(later));
    }

    #[test]
    fn intervalo_nao_ultrapassa_teto() {
        let now = t("2026-06-01T00:00:00Z");
        let mut item = ReviewItem::new("p", "w", "yo", None, now);
        for _ in 0..20 {
            item.grade(true, item.due_at);
        }
        assert!(item.interval_days <= MAX_INTERVAL_DAYS);
    }

    #[test]
    fn upsert_nao_duplica() {
        let now = t("2026-06-01T00:00:00Z");
        let mut q = ReviewQueue::default();
        assert!(q.upsert(ReviewItem::new("p", "w", "yo", None, now)));
        assert!(!q.upsert(ReviewItem::new("p", "w2", "yo", None, now)));
        assert_eq!(q.items.len(), 1);
    }

    #[test]
    fn due_ordena_por_vencimento() {
        let now = t("2026-06-10T00:00:00Z");
        let mut q = ReviewQueue::default();
        let mut a = ReviewItem::new("a", "a", "yo", None, t("2026-06-01T00:00:00Z"));
        a.due_at = t("2026-06-05T00:00:00Z");
        let mut b = ReviewItem::new("b", "b", "yo", None, t("2026-06-02T00:00:00Z"));
        b.due_at = t("2026-06-03T00:00:00Z");
        let mut future = ReviewItem::new("c", "c", "yo", None, now);
        future.due_at = t("2026-07-01T00:00:00Z");
        q.items = vec![a, b, future];
        let due = q.due(now);
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].term_path, "b"); // vence antes
        assert_eq!(due[1].term_path, "a");
    }
}
