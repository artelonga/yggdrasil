//! Similaridade de **sentido por contexto** (YG-176) — a hipótese distribucional
//! tornada literal: o sentido de um símbolo é o conjunto de seus contextos;
//! vetorize o contexto, tire o **cosseno**.
//!
//! Puro e determinístico (sem serviço de embedding externo na v1): cada nó vira
//! um "documento de contexto" (`gloss + examples + role + rótulos de relação`);
//! TF-IDF sobre o catálogo → vetor L2-normalizado; `cosine` = produto interno;
//! `top_pairs` = os pares acima de um limiar. O vetorizador é **trocável** por
//! embeddings neurais depois (mesmo output). Cross-language sai de graça enquanto
//! as glosas dividem o mesmo idioma (espaço vetorial compartilhado).

use std::collections::HashMap;

use super::lexicon::slugify;

/// Stopwords PT-BR mínimas (ruído de função, não de conteúdo).
const STOP: &[&str] = &[
    "de", "a", "o", "e", "do", "da", "das", "dos", "em", "um", "uma", "uns", "umas", "os", "as",
    "que", "com", "para", "por", "mais", "ou", "no", "na", "nos", "nas", "se", "sua", "seu", "ao",
    "à", "the", "of",
];

/// Documento de contexto de um nó: id + texto livre (gloss/exemplos/relações).
#[derive(Debug, Clone)]
pub struct ContextDoc {
    pub id: String,
    pub text: String,
}

impl ContextDoc {
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

/// Tokeniza: quebra em palavras (qualquer não-alfanumérico separa), dobra
/// diacrítico/tom e baixa caixa via [`slugify`] por palavra, descarta stopwords
/// e tokens de 1 caractere. `slugify` pode emitir `-` em palavras com pontuação
/// interna (ex.: `ñe'ẽ` → `ne-e`); tratamos como um token só, o que é coerente.
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(slugify)
        .filter(|t| t.len() >= 2 && t != "termo" && !STOP.contains(&t.as_str()))
        .collect()
}

/// Vetor esparso L2-normalizado (token → peso).
pub type Vector = HashMap<String, f64>;

/// Vetoriza o catálogo em TF-IDF normalizado. Retorna `(id, vetor)` preservando
/// a ordem de entrada. Documentos sem token viram vetor vazio (cosseno 0 com
/// tudo) — não quebram nada.
pub fn vectorize(docs: &[ContextDoc]) -> Vec<(String, Vector)> {
    let n = docs.len().max(1) as f64;
    let toks: Vec<Vec<String>> = docs.iter().map(|d| tokens(&d.text)).collect();

    // document frequency
    let mut df: HashMap<&str, usize> = HashMap::new();
    for t in &toks {
        let mut seen: HashMap<&str, ()> = HashMap::new();
        for tok in t {
            if seen.insert(tok.as_str(), ()).is_none() {
                *df.entry(tok.as_str()).or_insert(0) += 1;
            }
        }
    }

    docs.iter()
        .zip(toks.iter())
        .map(|(d, t)| {
            let total = t.len().max(1) as f64;
            let mut tf: HashMap<String, f64> = HashMap::new();
            for tok in t {
                *tf.entry(tok.clone()).or_insert(0.0) += 1.0;
            }
            let mut v: Vector = HashMap::new();
            for (tok, count) in tf {
                let dfi = *df.get(tok.as_str()).unwrap_or(&1) as f64;
                // idf suavizado (1+ln) — evita zerar tokens presentes em todos os docs
                let idf = 1.0 + (n / dfi).ln();
                v.insert(tok, (count / total) * idf);
            }
            // L2 normalize → cosseno vira produto interno
            let norm = v.values().map(|x| x * x).sum::<f64>().sqrt();
            if norm > 0.0 {
                for x in v.values_mut() {
                    *x /= norm;
                }
            }
            (d.id.clone(), v)
        })
        .collect()
}

/// Cosseno entre vetores já normalizados (= produto interno sobre tokens comuns).
pub fn cosine(a: &Vector, b: &Vector) -> f64 {
    // itera no menor por eficiência
    let (small, big) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    small
        .iter()
        .filter_map(|(t, x)| big.get(t).map(|y| x * y))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

/// Pares (id_a, id_b, score) com cosseno `>= threshold`, mais similares primeiro,
/// no máximo `k`. Par canônico (a < b) e sem loops — pronto p/ virar aresta.
pub fn top_pairs(docs: &[ContextDoc], threshold: f64, k: usize) -> Vec<(String, String, f64)> {
    let vecs = vectorize(docs);
    let mut out: Vec<(String, String, f64)> = Vec::new();
    for i in 0..vecs.len() {
        for j in (i + 1)..vecs.len() {
            let s = cosine(&vecs[i].1, &vecs[j].1);
            if s >= threshold {
                let (a, b) = (&vecs[i].0, &vecs[j].0);
                if a < b {
                    out.push((a.clone(), b.clone(), s));
                } else {
                    out.push((b.clone(), a.clone(), s));
                }
            }
        }
    }
    out.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(k);
    out
}

/// Top-`k` vizinhos semânticos **por nó**, via índice invertido — escala para
/// milhares de nós (só compara documentos que compartilham ≥1 token, em vez de
/// O(n²) cego). Devolve pares canônicos `(a, b, score)` com `score >= threshold`,
/// deduplicados, ordenados por score desc. `k_per_node` limita a vizinhança de
/// cada nó (mantém o overlay enxuto).
pub fn top_pairs_indexed(
    docs: &[ContextDoc],
    threshold: f64,
    k_per_node: usize,
) -> Vec<(String, String, f64)> {
    let vecs = vectorize(docs);
    // índice invertido: token → [(idx do doc, peso)]
    let mut index: HashMap<&str, Vec<(usize, f64)>> = HashMap::new();
    for (i, (_, v)) in vecs.iter().enumerate() {
        for (tok, w) in v {
            index.entry(tok.as_str()).or_default().push((i, *w));
        }
    }
    use std::collections::BTreeSet;
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out: Vec<(String, String, f64)> = Vec::new();
    for (i, (id_i, v_i)) in vecs.iter().enumerate() {
        // acumula produto interno só com docs que compartilham token
        let mut acc: HashMap<usize, f64> = HashMap::new();
        for (tok, w_i) in v_i {
            if let Some(postings) = index.get(tok.as_str()) {
                for &(j, w_j) in postings {
                    if j != i {
                        *acc.entry(j).or_insert(0.0) += w_i * w_j;
                    }
                }
            }
        }
        let mut neigh: Vec<(usize, f64)> =
            acc.into_iter().filter(|&(_, s)| s >= threshold).collect();
        // ordena por score desc, com desempate DETERMINÍSTICO pelo índice (a ordem
        // do HashMap é aleatória; sem isto o top-k varia entre execuções).
        neigh.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        neigh.truncate(k_per_node);
        for (j, s) in neigh {
            let id_j = &vecs[j].0;
            let (a, b) = if id_i < id_j {
                (id_i.clone(), id_j.clone())
            } else {
                (id_j.clone(), id_i.clone())
            };
            if pairs.insert((a.clone(), b.clone())) {
                out.push((a, b, s.clamp(0.0, 1.0)));
            }
        }
    }
    out.sort_by(|x, y| y.2.partial_cmp(&x.2).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> Vec<ContextDoc> {
        // glosas reais dos packs-seed
        vec![
            ContextDoc::new("guarani-mbya:ne-e", "palavra; fala; alma-linguagem"),
            ContextDoc::new("guarani-mbya:ayvu", "linguagem; som-fundamento"),
            ContextDoc::new("musica:c4", "Nota C4 — 261.63 Hz"),
            ContextDoc::new("guarani-mbya:kuarahy", "sol"),
        ]
    }

    #[test]
    fn identico_tem_cosseno_um_e_ortogonal_zero() {
        let v = vectorize(&docs());
        let map: HashMap<_, _> = v.into_iter().collect();
        let nee = &map["guarani-mbya:ne-e"];
        let c4 = &map["musica:c4"];
        assert!((cosine(nee, nee) - 1.0).abs() < 1e-9, "idêntico = 1");
        assert!(cosine(nee, c4) < 1e-9, "sem tokens em comum = 0");
    }

    #[test]
    fn sentido_por_contexto_aproxima_linguagem_e_afasta_nota() {
        let v = vectorize(&docs());
        let map: HashMap<_, _> = v.into_iter().collect();
        let nee = &map["guarani-mbya:ne-e"];
        let ayvu = &map["guarani-mbya:ayvu"];
        let c4 = &map["musica:c4"];
        // ñe'ẽ e ayvu compartilham "linguagem" → mais próximos que ñe'ẽ e C4
        assert!(
            cosine(nee, ayvu) > cosine(nee, c4),
            "ñe'ẽ~ayvu ({}) deve superar ñe'ẽ~C4 ({})",
            cosine(nee, ayvu),
            cosine(nee, c4)
        );
        assert!(cosine(nee, ayvu) > 0.0);
    }

    #[test]
    fn top_pairs_canonico_ordenado_e_limitado() {
        let pairs = top_pairs(&docs(), 0.01, 10);
        assert!(!pairs.is_empty());
        // par canônico (a < b)
        for (a, b, _) in &pairs {
            assert!(a < b, "{a} < {b}");
        }
        // ordenado desc
        for w in pairs.windows(2) {
            assert!(w[0].2 >= w[1].2);
        }
        // o par mais forte deve ser ñe'ẽ–ayvu
        assert_eq!(pairs[0].0, "guarani-mbya:ayvu");
        assert_eq!(pairs[0].1, "guarani-mbya:ne-e");
        // limiar alto → poda
        assert!(top_pairs(&docs(), 0.99, 10).is_empty());
        // k limita
        assert!(top_pairs(&docs(), 0.0, 1).len() <= 1);
    }

    #[test]
    fn top_pairs_indexed_concorda_com_o_par_mais_forte() {
        // o índice invertido deve achar o mesmo par dominante que o O(n²)
        let idx = top_pairs_indexed(&docs(), 0.01, 8);
        assert!(!idx.is_empty());
        assert_eq!(idx[0].0, "guarani-mbya:ayvu");
        assert_eq!(idx[0].1, "guarani-mbya:ne-e");
        // par canônico + ordenado desc, sem duplicatas
        let mut seen = std::collections::HashSet::new();
        for w in idx.windows(2) {
            assert!(w[0].2 >= w[1].2);
        }
        for (a, b, _) in &idx {
            assert!(a < b);
            assert!(seen.insert((a.clone(), b.clone())), "sem duplicatas");
        }
        // k_per_node=0 → vazio
        assert!(top_pairs_indexed(&docs(), 0.01, 0).is_empty());
    }
}
