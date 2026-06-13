//! Framework NLP de corpus (YG-139) — frequência por corpus + joins
//! cross-linguísticos, sobre DuckDB em memória.
//!
//! Ver `docs/architecture/nlp-corpus-framework.md`. O canônico continua sendo o
//! corpus JSON + os léxicos; isto é um índice **derivado, reconstruível no boot**
//! (mesmo padrão do índice do léxico). DuckDB é colunar → frequência e os joins
//! (inner/left) são SQL direto; "salvar como corpus" no futuro = CREATE TABLE AS.
//!
//! Corpora semeados: `ayvu-rapyta` (transcrição), `mbya-lexico` (4.837 termos),
//! `yoruba-lexico`. Links entre corpora = **pivô português**: dois termos de
//! línguas diferentes ligam-se quando compartilham uma palavra de conteúdo da
//! glosa (PT). Bridge de tradução sem precisar casar ortografias (lição YG-134).

use std::path::Path;
use std::sync::Mutex;

use duckdb::{Connection, params};
use serde::Serialize;

pub struct CorpusDb {
    conn: Mutex<Connection>,
}

#[derive(Serialize)]
pub struct FreqRow {
    pub term: String,
    pub count: i64,
    pub rank: i64,
}

#[derive(Serialize)]
pub struct CompareRow {
    pub term_a: String,
    pub count_a: i64,
    /// Equivalente no corpus B via pivô-português (None no modo `left` sem par).
    pub term_b: Option<String>,
    pub count_b: Option<i64>,
}

#[derive(Serialize)]
pub struct CorpusInfo {
    pub name: String,
    pub lang: String,
    pub terms: i64,
}

// stopwords PT mínimas — tiram ruído do pivô de glosa sem precisar de lista enorme.
const STOP_PT: &[&str] = &[
    "que",
    "com",
    "uma",
    "para",
    "por",
    "dos",
    "das",
    "como",
    "mais",
    "sua",
    "seu",
    "ele",
    "ela",
    "isto",
    "esse",
    "essa",
    "este",
    "esta",
    "ser",
    "ter",
    "the",
    "and",
    "função",
    "termo",
    "palavra",
    "sentido",
    "ocorrendo",
    "geralmente",
    "etc",
];

fn glosa_tokens(gloss: &str) -> Vec<String> {
    gloss
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4 && !STOP_PT.contains(w))
        .map(|w| w.to_string())
        .collect()
}

#[derive(serde::Deserialize)]
struct LexRow {
    word: String,
    #[serde(default)]
    gloss: Option<String>,
    #[serde(default)]
    pop: i64,
}

impl CorpusDb {
    /// Monta o índice no boot a partir do canônico em `root` (= COMUNICACAO_DIR).
    /// Falha de leitura de uma fonte é tolerada (corpus fica vazio) — best-effort.
    pub fn build(root: &Path) -> duckdb::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE corpus_term (
                corpus  TEXT NOT NULL,
                lang    TEXT NOT NULL,
                term    TEXT NOT NULL,
                count   BIGINT NOT NULL,
                gloss   TEXT
            );
            CREATE TABLE gloss_tok (
                corpus TEXT NOT NULL,
                term   TEXT NOT NULL,
                w      TEXT NOT NULL
            );",
        )?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        // léxicos (termo + glosa PT + pop)
        db.ingest_lexicon(
            root,
            "mbya-lexico",
            "gn-mbya",
            "guarani-mbya/lexicon.mbya.json",
        );
        db.ingest_lexicon(root, "yoruba-lexico", "yo", "yoruba/lexicon.yo.json");
        // corpus de transcrição (ocorrências de palavra do Ayvu Rapyta)
        db.ingest_ayvu(root, "ayvu-rapyta");
        db.conn.lock().unwrap().execute_batch(
            "CREATE INDEX idx_ct ON corpus_term(corpus); CREATE INDEX idx_gt ON gloss_tok(w);",
        )?;
        Ok(db)
    }

    fn ingest_lexicon(&self, root: &Path, corpus: &str, lang: &str, rel: &str) {
        let Ok(bytes) = std::fs::read(root.join(rel)) else {
            return;
        };
        let Ok(rows) = serde_json::from_slice::<Vec<LexRow>>(&bytes) else {
            return;
        };
        let conn = self.conn.lock().unwrap();
        for r in rows {
            let count = r.pop.max(1);
            let _ = conn.execute(
                "INSERT INTO corpus_term (corpus, lang, term, count, gloss) VALUES (?, ?, ?, ?, ?)",
                params![corpus, lang, r.word, count, r.gloss],
            );
            if let Some(g) = &r.gloss {
                for w in glosa_tokens(g) {
                    let _ = conn.execute(
                        "INSERT INTO gloss_tok (corpus, term, w) VALUES (?, ?, ?)",
                        params![corpus, r.word, w],
                    );
                }
            }
        }
    }

    fn ingest_ayvu(&self, root: &Path, corpus: &str) {
        let Ok(s) = std::fs::read_to_string(root.join("corpus/ayvu-rapyta.json")) else {
            return;
        };
        let Ok(j) = serde_json::from_str::<serde_json::Value>(&s) else {
            return;
        };
        // conta ocorrências de cada forma de superfície (nível palavra)
        let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        if let Some(chs) = j.get("chapters").and_then(|c| c.as_array()) {
            for ch in chs {
                if let Some(vs) = ch.get("verses").and_then(|v| v.as_array()) {
                    for v in vs {
                        if let Some(ws) = v.get("words").and_then(|w| w.as_array()) {
                            for w in ws {
                                if let Some(t) = w.get("w").and_then(|x| x.as_str()) {
                                    let t = t.trim().to_lowercase();
                                    if !t.is_empty() {
                                        *counts.entry(t).or_insert(0) += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // glosas do mapa `lex` curado (para o pivô, quando houver)
        let lexmap = j.get("lex").and_then(|l| l.as_object());
        let conn = self.conn.lock().unwrap();
        for (term, count) in counts {
            let gloss = lexmap
                .and_then(|m| m.get(&term))
                .and_then(|s| s.as_array())
                .and_then(|a| a.first())
                .and_then(|e| e.get("g"))
                .and_then(|g| g.as_str())
                .map(str::to_string);
            let _ = conn.execute(
                "INSERT INTO corpus_term (corpus, lang, term, count, gloss) VALUES (?, ?, ?, ?, ?)",
                params![corpus, "gn-mbya", term, count, gloss],
            );
            if let Some(g) = &gloss {
                for w in glosa_tokens(g) {
                    let _ = conn.execute(
                        "INSERT INTO gloss_tok (corpus, term, w) VALUES (?, ?, ?)",
                        params![corpus, term, w],
                    );
                }
            }
        }
    }

    pub fn list_corpora(&self) -> duckdb::Result<Vec<CorpusInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT corpus, any_value(lang), COUNT(DISTINCT term)
             FROM corpus_term GROUP BY corpus ORDER BY corpus",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CorpusInfo {
                    name: r.get(0)?,
                    lang: r.get(1)?,
                    terms: r.get(2)?,
                })
            })?
            .collect::<duckdb::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Frequência (por popularidade) de um corpus, top-`limit`.
    pub fn freq(&self, corpus: &str, limit: usize) -> duckdb::Result<Vec<FreqRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT term, SUM(count) c,
                    rank() OVER (ORDER BY SUM(count) DESC) r
             FROM corpus_term WHERE corpus = ?
             GROUP BY term ORDER BY c DESC LIMIT ?",
        )?;
        let rows = stmt
            .query_map(params![corpus, limit as i64], |r| {
                Ok(FreqRow {
                    term: r.get(0)?,
                    count: r.get(1)?,
                    rank: r.get::<_, i64>(2)?,
                })
            })?
            .collect::<duckdb::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Compara dois corpora via pivô-português. `mode`: "inner" (só termos de A
    /// com equivalente em B) ou "left" (todos de A, equivalente quando houver).
    /// O resultado é, ele próprio, uma tabela de corpus (salvável — fase futura).
    pub fn compare(
        &self,
        a: &str,
        b: &str,
        mode: &str,
        limit: usize,
    ) -> duckdb::Result<Vec<CompareRow>> {
        let join = if mode == "inner" { "JOIN" } else { "LEFT JOIN" };
        let sql = format!(
            "WITH fa AS (SELECT term, SUM(count) c FROM corpus_term WHERE corpus = ? GROUP BY term),
                  fb AS (SELECT term, SUM(count) c FROM corpus_term WHERE corpus = ? GROUP BY term),
                  link AS (
                    SELECT ga.term ta, gb.term tb, COUNT(*) shared
                    FROM gloss_tok ga JOIN gloss_tok gb ON ga.w = gb.w
                    WHERE ga.corpus = ? AND gb.corpus = ?
                    GROUP BY ga.term, gb.term
                  ),
                  best AS (
                    SELECT ta, tb FROM (
                      SELECT ta, tb, row_number() OVER (PARTITION BY ta ORDER BY shared DESC) rn
                      FROM link
                    ) WHERE rn = 1
                  )
             SELECT fa.term, fa.c, best.tb, fb.c
             FROM fa {join} best ON best.ta = fa.term
             LEFT JOIN fb ON fb.term = best.tb
             ORDER BY fa.c DESC LIMIT ?"
        );
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![a, b, a, b, limit as i64], |r| {
                Ok(CompareRow {
                    term_a: r.get(0)?,
                    count_a: r.get(1)?,
                    term_b: r.get(2).ok(),
                    count_b: r.get(3).ok(),
                })
            })?
            .collect::<duckdb::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // monta um CorpusDb sintético direto nas tabelas (sem ler arquivos).
    fn synthetic() -> CorpusDb {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE corpus_term (corpus TEXT, lang TEXT, term TEXT, count BIGINT, gloss TEXT);
             CREATE TABLE gloss_tok (corpus TEXT, term TEXT, w TEXT);
             INSERT INTO corpus_term VALUES
               ('mbya','gn','yvy',10,'terra mundo'),
               ('mbya','gn','ara',5,'tempo dia'),
               ('yoruba','yo','ile',7,'terra casa'),
               ('yoruba','yo','orun',3,'ceu firmamento');
             INSERT INTO gloss_tok VALUES
               ('mbya','yvy','terra'),('mbya','yvy','mundo'),
               ('mbya','ara','tempo'),('mbya','ara','dia'),
               ('yoruba','ile','terra'),('yoruba','ile','casa'),
               ('yoruba','orun','ceu'),('yoruba','orun','firmamento');",
        )
        .unwrap();
        CorpusDb {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn freq_ordena_por_popularidade() {
        let db = synthetic();
        let f = db.freq("mbya", 10).unwrap();
        assert_eq!(f[0].term, "yvy");
        assert_eq!(f[0].count, 10);
        assert_eq!(f[0].rank, 1);
    }

    #[test]
    fn compare_inner_so_com_equivalente_pivô_portugues() {
        let db = synthetic();
        // yvy(terra) ↔ ile(terra); ara/orun não compartilham glosa → inner exclui
        let inner = db.compare("mbya", "yoruba", "inner", 10).unwrap();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].term_a, "yvy");
        assert_eq!(inner[0].term_b.as_deref(), Some("ile"));
        // left inclui ara (sem equivalente)
        let left = db.compare("mbya", "yoruba", "left", 10).unwrap();
        assert_eq!(left.len(), 2);
        let ara = left.iter().find(|r| r.term_a == "ara").unwrap();
        assert!(ara.term_b.is_none());
    }
}
