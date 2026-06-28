//! Universo centralizado — store SQLite do grafo de sentido (YG-175).
//!
//! Persiste **arestas** (cross-pack, não-direcionadas) e **referências** (links)
//! sobre nós cuja identidade vem de [`yggdrasil_core::comunicacao::topologia`].
//! As arestas nascem por **exploração** (`bump_edge` = co-visitação → `weight++`,
//! idempotente pelo par canônico) e podem ser **promovidas** a relação nomeada
//! (`promote_edge`, `explore → user`, nunca regride). A resolução de um nó
//! contra os packs-seed vive aqui também ([`resolve_node`]) — o domínio puro não
//! conhece os packs.
//!
//! Mesmo padrão das outras stores web (campanha/scores): `Arc<Mutex<Connection>>`,
//! aberto do `YGGDRASIL_DB`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;

use yggdrasil_core::comunicacao::lexicon::slugify;
use yggdrasil_core::comunicacao::public;
use yggdrasil_core::comunicacao::semantica::ContextDoc;
use yggdrasil_core::comunicacao::topologia::{EdgeSource, RefKind, canonical_pair, split_node_id};

/// Profundidade máxima de subgrafo (memory-light: evita varrer o grafo inteiro).
pub const MAX_DEPTH: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum TopoError {
    #[error("nó '{0}' não existe em nenhum pack")]
    UnknownNode(String),
    #[error("uma aresta precisa de dois nós distintos")]
    SelfLoop,
    #[error(transparent)]
    Db(#[from] rusqlite::Error),
}

/// Um nó resolvido contra o léxico REAL — o conteúdo por trás do `NodeId`.
/// `pack` = código de língua (`gn-mbya`, `yo`); `x,y` = posição de mundo
/// (espiral de phyllotaxis por rank, [`public::lexicon_slice`]); `pop` = nº de
/// exemplos no corpus (rank de popularidade).
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedNode {
    pub id: String,
    pub pack: String,
    pub kind: String,
    pub term: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gloss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub x: f64,
    pub y: f64,
    pub pop: i64,
}

// ─── Catálogo REAL (léxico comunicação + contexto do mbya_lexicon.db) ──────────
//
// Princípio: NENHUM dado fabricado. Os nós vêm do léxico publicado da `comunicacao`
// (`guarani-mbya/lexicon.mbya.json` = 4837 entradas, `yoruba/lexicon.yo.json`), e o
// **contexto** de cada termo (para o cosseno de sentido) é enriquecido com os
// exemplos bilíngues e os versos do **Ayvu Rapytã** do `mbya_lexicon.db`.

/// Um verso do Ayvu Rapytã onde o termo ocorre — a "instância em sentença" real
/// (YG-177), com sua posição hierárquica (capítulo · verso) que embui o sentido.
#[derive(Debug, Clone, Serialize)]
pub struct VerseRef {
    pub chapter: String,
    pub verse: i64,
    pub text: String,
}

/// Um exemplo ilustrativo do dicionário (sentença bilíngue).
#[derive(Debug, Clone, Serialize)]
pub struct ExampleRef {
    pub gn: String,
    pub pt: String,
}

/// Uma sentença (verso do Ayvu Rapytã) com seus termos resolvidos — o ponto de
/// entrada "ler primeiro": o grafo se filtra às PALAVRAS desta sentença, não a
/// tudo (YG-177). `terms` em ordem de leitura, ids resolvíveis no léxico.
#[derive(Debug, Clone, Serialize)]
pub struct SentenceRow {
    pub id: String,
    pub lang: String,
    pub loc: String,
    pub text: String,
    pub terms: Vec<String>,
}

struct Catalog {
    nodes: Vec<ResolvedNode>,
    by_id: HashMap<String, usize>,
    /// Contexto LÉXICO por id: gloss + decomp + definição + exemplos do dicionário.
    lexico_ctx: HashMap<String, String>,
    /// Contexto CORPUS por id: os ids dos versos do Ayvu Rapytã onde o termo
    /// ocorre (co-ocorrência distribucional — `v{sentence_id}` por ocorrência).
    corpus_ctx: HashMap<String, String>,
    /// Instâncias reais no Ayvu Rapytã (referências por termo, com hierarquia).
    verses: HashMap<String, Vec<VerseRef>>,
    /// Exemplos do dicionário (referências por termo).
    examples: HashMap<String, Vec<ExampleRef>>,
    /// Sentenças (versos do Ayvu Rapytã) com seus termos — entrada "ler primeiro".
    sentences: Vec<SentenceRow>,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(load_catalog)
}

/// Diretório raiz do universo `comunicacao` (fonte dos nós). Default de dev.
fn comunicacao_dir() -> String {
    std::env::var("COMUNICACAO_DIR").unwrap_or_else(|_| "../comunicacao".to_string())
}

/// Caminho do `mbya_lexicon.db` (fonte do contexto: exemplos + Ayvu Rapytã).
fn mbya_db_path() -> String {
    std::env::var("YGGDRASIL_MBYA_DB").unwrap_or_else(|_| "../mbya/mbya_lexicon.db".to_string())
}

fn load_catalog() -> Catalog {
    let root = comunicacao_dir();
    let root = Path::new(&root);
    let mut nodes: Vec<ResolvedNode> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut lexico_ctx: HashMap<String, String> = HashMap::new();

    // offset por língua: clusters não se sobrepõem (a espiral começa na origem).
    for (lang, dx) in [("gn-mbya", 0.0_f64), ("yo", 6000.0_f64)] {
        let slice = public::lexicon_slice(root, lang, 0, usize::MAX);
        for e in slice.entries {
            let id = format!("{}:{}", lang, slugify(&e.word));
            if by_id.contains_key(&id) {
                continue; // 1ª ocorrência (maior pop) vence; mantém ids únicos
            }
            let mut ctx = String::new();
            if let Some(g) = &e.gloss {
                ctx.push_str(g);
                ctx.push(' ');
            }
            if let Some(d) = &e.decomp {
                ctx.push_str(d);
                ctx.push(' ');
            }
            lexico_ctx.insert(id.clone(), ctx);
            by_id.insert(id.clone(), nodes.len());
            nodes.push(ResolvedNode {
                id,
                pack: lang.to_string(),
                kind: "language".to_string(),
                term: e.word,
                gloss: e.gloss,
                role: None,
                x: e.x + dx,
                y: e.y,
                pop: e.pop,
            });
        }
    }

    // Duas fontes DISTINTAS do mbya_lexicon.db: o léxico (definição+exemplos) e o
    // Ayvu Rapytã (versos hierárquicos). Mantidas separadas (YG-177).
    let mut corpus_ctx: HashMap<String, String> = HashMap::new();
    let mut verses: HashMap<String, Vec<VerseRef>> = HashMap::new();
    let mut examples: HashMap<String, Vec<ExampleRef>> = HashMap::new();
    let mut sentences: Vec<SentenceRow> = Vec::new();
    read_db_sources(
        &mbya_db_path(),
        &mut lexico_ctx,
        &mut corpus_ctx,
        &mut verses,
        &mut examples,
        &mut sentences,
    );

    Catalog {
        nodes,
        by_id,
        lexico_ctx,
        corpus_ctx,
        verses,
        examples,
        sentences,
    }
}

/// Lê as DUAS fontes do `mbya_lexicon.db` (read-only) por nó `gn-mbya:{slug}`:
/// - **léxico**: acrescenta definição + exemplos bilíngues ao `lexico_ctx`, e
///   guarda os exemplos como referências (`examples`).
/// - **corpus (Ayvu Rapytã)**: monta `corpus_ctx` com os ids dos versos onde o
///   termo ocorre (co-ocorrência), e guarda os versos com hierarquia (`verses`).
///
/// DB ausente (ex.: prod sem o artefato) → no-op (o léxico cai na glosa).
fn read_db_sources(
    path: &str,
    lexico_ctx: &mut HashMap<String, String>,
    corpus_ctx: &mut HashMap<String, String>,
    verses: &mut HashMap<String, Vec<VerseRef>>,
    examples: &mut HashMap<String, Vec<ExampleRef>>,
    sentences: &mut Vec<SentenceRow>,
) {
    const MAX_REF: usize = 8; // limita só as REFERÊNCIAS exibidas (não o contexto)
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return;
    };
    // entry_id → node id (e a definição entra no contexto léxico)
    let mut head: HashMap<i64, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, headword, definition FROM entries")
        && let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            ))
        })
    {
        for (eid, hw, def) in rows.flatten() {
            let id = format!("gn-mbya:{}", slugify(&hw));
            head.insert(eid, id.clone());
            if let Some(c) = lexico_ctx.get_mut(&id) {
                c.push(' ');
                c.push_str(&def);
            }
        }
    }
    // LÉXICO: exemplos (guarani + português) → contexto léxico + referências
    if let Ok(mut stmt) =
        conn.prepare("SELECT entry_id, guarani_text, portuguese_text FROM examples")
        && let Ok(rows) = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
    {
        for (eid, gn, pt) in rows.flatten() {
            let Some(id) = head.get(&eid) else { continue };
            if let Some(c) = lexico_ctx.get_mut(id) {
                c.push(' ');
                c.push_str(&gn);
                c.push(' ');
                c.push_str(&pt);
            }
            let v = examples.entry(id.clone()).or_default();
            if v.len() < MAX_REF {
                v.push(ExampleRef { gn, pt });
            }
        }
    }
    // CORPUS (Ayvu Rapytã): versos onde o termo ocorre → co-ocorrência + refs.
    if let Ok(mut stmt) = conn.prepare(
        "SELECT ct.entry_id, cs.id, cs.verse_num, cs.text,
                COALESCE(NULLIF(ch.title_gn,''), NULLIF(ch.title_es,''),
                         'Cap. ' || COALESCE(ch.number_roman, CAST(ch.number_int AS TEXT)))
         FROM corpus_tokens ct
         JOIN corpus_sentences cs ON cs.id = ct.sentence_id
         JOIN chapters ch ON ch.id = cs.chapter_id
         WHERE ct.entry_id IS NOT NULL AND cs.lang = 'gn-mbya'",
    ) && let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Option<i64>>(2)?.unwrap_or(0),
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?.unwrap_or_default(),
        ))
    }) {
        // sentença → termos (ordem de leitura), p/ a entrada "ler primeiro".
        let mut sent_map: std::collections::BTreeMap<i64, SentenceRow> =
            std::collections::BTreeMap::new();
        for (eid, sid, vnum, text, chapter) in rows.flatten() {
            let Some(id) = head.get(&eid) else { continue };
            // co-ocorrência: cada ocorrência num verso é um token `v{sid}`
            let c = corpus_ctx.entry(id.clone()).or_default();
            c.push_str(" v");
            c.push_str(&sid.to_string());
            // sentença: acumula o verso + seus termos (dedup, ordem de chegada)
            let s = sent_map.entry(sid).or_insert_with(|| SentenceRow {
                id: format!("gn-mbya#{sid}"),
                lang: "gn-mbya".to_string(),
                loc: format!("{chapter} · v{vnum}"),
                text: text.clone(),
                terms: Vec::new(),
            });
            if !s.terms.contains(id) {
                s.terms.push(id.clone());
            }
            // referência: o verso (dedup por sentence id, com hierarquia)
            let v = verses.entry(id.clone()).or_default();
            if !v.iter().any(|x| x.verse == vnum && x.text == text) && v.len() < MAX_REF {
                v.push(VerseRef {
                    chapter,
                    verse: vnum,
                    text,
                });
            }
        }
        // só sentenças com ≥2 termos resolvidos valem como grafo de palavras
        sentences.extend(sent_map.into_values().filter(|s| s.terms.len() >= 2));
    }
}

/// Resolve um `NodeId` contra o catálogo real. `None` se não existe — guarda que
/// mantém o grafo limpo: só se liga/refere nós que existem num léxico real.
pub fn resolve_node(id: &str) -> Option<ResolvedNode> {
    split_node_id(id)?; // forma válida `{lang}:{slug}`
    let c = catalog();
    c.by_id.get(id).map(|&i| c.nodes[i].clone())
}

/// Catálogo de nós (paleta do front): todo o léxico real, opcionalmente filtrado
/// por `pack` (código de língua).
pub fn all_nodes(pack: Option<&str>) -> Vec<ResolvedNode> {
    catalog()
        .nodes
        .iter()
        .filter(|n| pack.is_none_or(|pk| n.pack == pk))
        .cloned()
        .collect()
}

/// Versos do Ayvu Rapytã onde o termo ocorre (instâncias reais em sentenças).
pub fn verses_for(id: &str) -> Vec<VerseRef> {
    catalog().verses.get(id).cloned().unwrap_or_default()
}

/// Exemplos do dicionário para o termo.
pub fn examples_for(id: &str) -> Vec<ExampleRef> {
    catalog().examples.get(id).cloned().unwrap_or_default()
}

/// Sentenças (entrada "ler primeiro"): versos do Ayvu Rapytã com seus termos.
/// Filtra por `lang` e por busca `q` (no texto), limitado a `limit`.
pub fn sentences(lang: Option<&str>, q: Option<&str>, limit: usize) -> Vec<SentenceRow> {
    let ql = q.map(|s| s.to_lowercase());
    catalog()
        .sentences
        .iter()
        .filter(|s| lang.is_none_or(|l| s.lang == l))
        .filter(|s| {
            ql.as_deref()
                .is_none_or(|q| s.text.to_lowercase().contains(q))
        })
        .take(limit)
        .cloned()
        .collect()
}

/// Documentos de contexto por overlay: `"lexico"` (gloss+def+exemplos) ou
/// `"corpus"` (co-ocorrência em versos do Ayvu Rapytã). Fontes distintas (YG-177).
fn context_docs(overlay: &str) -> Vec<ContextDoc> {
    let c = catalog();
    let src = if overlay == "corpus" {
        &c.corpus_ctx
    } else {
        &c.lexico_ctx
    };
    c.nodes
        .iter()
        .map(|n| ContextDoc::new(n.id.clone(), src.get(&n.id).cloned().unwrap_or_default()))
        .collect()
}

/// Aresta não-direcionada já no par canônico.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeRow {
    pub a: String,
    pub b: String,
    pub weight: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    pub source: EdgeSource,
    /// Cosseno de sentido (YG-176) — só em arestas `semantic`; `None` nas demais.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Overlay semântico (YG-177): `"lexico"` | `"corpus"`. Só em `semantic`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// Vizinho de um nó — o **outro** extremo da aresta, do ponto de vista do nó
/// consultado (já desempacotado do par canônico).
#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub node: String,
    pub weight: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    pub source: EdgeSource,
}

/// Referência (link) anexada a um nó.
#[derive(Debug, Clone, Serialize)]
pub struct RefRow {
    pub id: i64,
    pub node: String,
    pub kind: RefKind,
    pub href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

pub fn init_topologia_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS topo_edges (
            a          TEXT NOT NULL,
            b          TEXT NOT NULL,
            weight     INTEGER NOT NULL DEFAULT 1,
            relation   TEXT,
            source     TEXT NOT NULL DEFAULT 'explore',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (a, b)
        );
        CREATE INDEX IF NOT EXISTS idx_topo_edges_a ON topo_edges(a);
        CREATE INDEX IF NOT EXISTS idx_topo_edges_b ON topo_edges(b);
        CREATE TABLE IF NOT EXISTS topo_refs (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            node       TEXT NOT NULL,
            kind       TEXT NOT NULL,
            href       TEXT NOT NULL,
            label      TEXT,
            owner      TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_topo_refs_node ON topo_refs(node);
        -- YG-176: overlay de similaridade semântica (computado, recomputável;
        -- separado do grafo humano de topo_edges).
        CREATE TABLE IF NOT EXISTS topo_semantic (
            a           TEXT NOT NULL,
            b           TEXT NOT NULL,
            score       REAL NOT NULL,
            method      TEXT NOT NULL,
            computed_at INTEGER NOT NULL,
            PRIMARY KEY (a, b)
        );
        CREATE INDEX IF NOT EXISTS idx_topo_sem_a ON topo_semantic(a);
        CREATE INDEX IF NOT EXISTS idx_topo_sem_b ON topo_semantic(b);",
    )
}

pub struct TopologiaDb {
    db: Arc<Mutex<Connection>>,
}

impl TopologiaDb {
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        init_topologia_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    #[cfg(test)]
    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        init_topologia_db(&conn)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Co-visitação → aresta. Cria com `weight=1, source=explore` ou **incrementa**
    /// o `weight` de uma já existente (idempotente pelo par canônico; preserva
    /// `source`/`relation`). Erro em loop (`a == b`).
    pub fn bump_edge(&self, a: &str, b: &str) -> Result<EdgeRow, TopoError> {
        if a == b {
            return Err(TopoError::SelfLoop);
        }
        let (a, b) = canonical_pair(a, b);
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().unwrap();
        conn.execute(
            "INSERT INTO topo_edges (a, b, weight, relation, source, created_at, updated_at)
             VALUES (?1, ?2, 1, NULL, 'explore', ?3, ?3)
             ON CONFLICT(a, b) DO UPDATE SET weight = weight + 1, updated_at = ?3",
            params![a, b, now],
        )?;
        read_edge(&conn, &a, &b)
    }

    /// Promove/cria uma aresta `source = user` com `relation` opcional. Se já
    /// existe (mesmo `explore`), vira `user` e mantém o `weight`; `relation = None`
    /// preserva a relação anterior.
    pub fn promote_edge(
        &self,
        a: &str,
        b: &str,
        relation: Option<&str>,
    ) -> Result<EdgeRow, TopoError> {
        if a == b {
            return Err(TopoError::SelfLoop);
        }
        let (a, b) = canonical_pair(a, b);
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().unwrap();
        conn.execute(
            "INSERT INTO topo_edges (a, b, weight, relation, source, created_at, updated_at)
             VALUES (?1, ?2, 1, ?3, 'user', ?4, ?4)
             ON CONFLICT(a, b) DO UPDATE SET
               source = 'user',
               relation = COALESCE(?3, relation),
               updated_at = ?4",
            params![a, b, relation, now],
        )?;
        read_edge(&conn, &a, &b)
    }

    /// Vizinhos de um nó (ambas as direções), mais "pesados" primeiro.
    pub fn neighbors(&self, node: &str) -> Vec<Neighbor> {
        let conn = self.db.lock().unwrap();
        neighbors_conn(&conn, node)
    }

    /// Subgrafo em torno de `around` até `depth` saltos (BFS). Devolve a lista de
    /// `NodeId`s alcançados e as arestas entre eles (sem duplicatas).
    pub fn subgraph(&self, around: &str, depth: u32) -> (Vec<String>, Vec<EdgeRow>) {
        let depth = depth.min(MAX_DEPTH);
        let conn = self.db.lock().unwrap();
        let mut visited: Vec<String> = vec![around.to_string()];
        let mut edges: Vec<EdgeRow> = Vec::new();
        let mut frontier: Vec<String> = vec![around.to_string()];
        for _ in 0..depth {
            let mut next: Vec<String> = Vec::new();
            for n in &frontier {
                for nb in neighbors_conn(&conn, n) {
                    let (a, b) = canonical_pair(n, &nb.node);
                    if !edges.iter().any(|e| e.a == a && e.b == b) {
                        edges.push(EdgeRow {
                            a,
                            b,
                            weight: nb.weight,
                            relation: nb.relation.clone(),
                            source: nb.source,
                            score: None,
                            method: None,
                        });
                    }
                    if !visited.contains(&nb.node) {
                        visited.push(nb.node.clone());
                        next.push(nb.node);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        (visited, edges)
    }

    /// Anexa uma referência (link) a um nó. Devolve a linha criada.
    pub fn add_ref(
        &self,
        node: &str,
        kind: RefKind,
        href: &str,
        label: Option<&str>,
        owner: Option<&str>,
    ) -> Result<RefRow, TopoError> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().unwrap();
        conn.execute(
            "INSERT INTO topo_refs (node, kind, href, label, owner, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![node, kind.as_str(), href, label, owner, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(RefRow {
            id,
            node: node.to_string(),
            kind,
            href: href.to_string(),
            label: label.map(|s| s.to_string()),
        })
    }

    /// Referências de um nó, mais recentes primeiro.
    pub fn refs_for(&self, node: &str) -> Vec<RefRow> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, node, kind, href, label FROM topo_refs WHERE node = ?1 ORDER BY id DESC",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("topo refs prepare: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![node], |row| {
            let kind: String = row.get(2)?;
            Ok(RefRow {
                id: row.get(0)?,
                node: row.get(1)?,
                kind: RefKind::parse(&kind).unwrap_or(RefKind::Link),
                href: row.get(3)?,
                label: row.get(4)?,
            })
        })
        .and_then(|m| m.collect::<Result<Vec<_>, _>>())
        .unwrap_or_else(|e| {
            tracing::error!("topo refs query: {e}");
            vec![]
        })
    }

    /// YG-176/173: recalcula os DOIS overlays semânticos (`lexico` e `corpus`),
    /// cada um do seu contexto (definição+exemplos vs co-ocorrência em versos do
    /// Ayvu Rapytã), e **substitui** o overlay (idempotente). Devolve o total de
    /// pares (lexico + corpus).
    pub fn recompute_semantic(&self, threshold: f64, k: usize) -> Result<usize, TopoError> {
        use yggdrasil_core::comunicacao::semantica::top_pairs_indexed;
        let lexico = top_pairs_indexed(&context_docs("lexico"), threshold, k);
        let corpus = top_pairs_indexed(&context_docs("corpus"), threshold, k);
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.db.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM topo_semantic", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO topo_semantic (a, b, score, method, computed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (method, pairs) in [("lexico", &lexico), ("corpus", &corpus)] {
                for (a, b, score) in pairs {
                    stmt.execute(params![a, b, score, method, now])?;
                }
            }
        }
        tx.commit()?;
        Ok(lexico.len() + corpus.len())
    }

    /// Arestas semânticas de um overlay (`method` = `"lexico"` | `"corpus"`)
    /// incidentes a qualquer nó de `node_ids`. Carrega o overlay e filtra em memória.
    pub fn semantic_incident(&self, node_ids: &[String], method: &str) -> Vec<EdgeRow> {
        let set: std::collections::HashSet<&str> = node_ids.iter().map(|s| s.as_str()).collect();
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT a, b, score FROM topo_semantic WHERE method = ?1")
        {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("topo semantic prepare: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![method], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .and_then(|m| m.collect::<Result<Vec<_>, _>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|(a, b, _)| set.contains(a.as_str()) || set.contains(b.as_str()))
        .map(|(a, b, score)| EdgeRow {
            a,
            b,
            weight: (score * 100.0).round() as i64,
            relation: None,
            source: EdgeSource::Semantic,
            score: Some(score),
            method: Some(method.to_string()),
        })
        .collect()
    }
}

/// Lê uma aresta já no par canônico (após um upsert).
fn read_edge(conn: &Connection, a: &str, b: &str) -> Result<EdgeRow, TopoError> {
    let row = conn
        .query_row(
            "SELECT a, b, weight, relation, source FROM topo_edges WHERE a = ?1 AND b = ?2",
            params![a, b],
            |row| {
                let source: String = row.get(4)?;
                Ok(EdgeRow {
                    a: row.get(0)?,
                    b: row.get(1)?,
                    weight: row.get(2)?,
                    relation: row.get(3)?,
                    source: EdgeSource::parse(&source).unwrap_or(EdgeSource::Explore),
                    score: None,
                    method: None,
                })
            },
        )
        .optional()?;
    row.ok_or_else(|| TopoError::UnknownNode(format!("{a}|{b}")))
}

/// Vizinhos via uma conexão já travada (reusado pelo BFS do subgrafo).
fn neighbors_conn(conn: &Connection, node: &str) -> Vec<Neighbor> {
    let mut stmt = match conn.prepare(
        "SELECT CASE WHEN a = ?1 THEN b ELSE a END AS other, weight, relation, source
         FROM topo_edges WHERE a = ?1 OR b = ?1
         ORDER BY weight DESC, other ASC",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("topo neighbors prepare: {e}");
            return vec![];
        }
    };
    stmt.query_map(params![node], |row| {
        let source: String = row.get(3)?;
        Ok(Neighbor {
            node: row.get(0)?,
            weight: row.get(1)?,
            relation: row.get(2)?,
            source: EdgeSource::parse(&source).unwrap_or(EdgeSource::Explore),
        })
    })
    .and_then(|m| m.collect::<Result<Vec<_>, _>>())
    .unwrap_or_else(|e| {
        tracing::error!("topo neighbors query: {e}");
        vec![]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ids OPACOS para o edge store (bump/promote/neighbors/subgraph/refs não
    // resolvem contra o catálogo — só guardam strings).
    const NEE: &str = "guarani-mbya:ne-e";
    const AYVU: &str = "guarani-mbya:ayvu";
    const C4: &str = "musica:c4";

    // Os testes de catálogo/semântica leem o léxico REAL (../comunicacao +
    // mbya_lexicon.db). Em ambiente sem esses artefatos, `all_nodes` vem vazio →
    // o teste é pulado (não há dado fabricado de fallback).
    #[test]
    fn resolve_node_round_trip_com_no_real_e_rejeita_inexistente() {
        let todos = all_nodes(None);
        if todos.is_empty() {
            eprintln!("(skip) catálogo real ausente — sem ../comunicacao");
            return;
        }
        let first = &todos[0];
        let n = resolve_node(&first.id).expect("nó real resolve");
        assert_eq!(n.id, first.id);
        assert_eq!(n.kind, "language");
        assert!(resolve_node("guarani-mbya:naoexiste-zzz").is_none());
        assert!(resolve_node("pack-fantasma:x").is_none());
        assert!(resolve_node("semdoispontos").is_none());
    }

    #[test]
    fn all_nodes_filtra_por_lingua() {
        let todos = all_nodes(None);
        if todos.is_empty() {
            eprintln!("(skip) catálogo real ausente");
            return;
        }
        let gn = all_nodes(Some("gn-mbya"));
        assert!(!gn.is_empty(), "léxico Mbyá real tem nós");
        assert!(gn.iter().all(|n| n.pack == "gn-mbya"));
        assert!(gn.len() <= todos.len());
        assert!(all_nodes(Some("pack-fantasma")).is_empty());
    }

    #[test]
    fn bump_edge_idempotente_pelo_par_canonico() {
        let db = TopologiaDb::in_memory().unwrap();
        let e1 = db.bump_edge(NEE, C4).unwrap();
        assert_eq!(e1.weight, 1);
        assert_eq!(e1.source, EdgeSource::Explore);
        // ordem invertida → MESMA aresta, weight sobe
        let e2 = db.bump_edge(C4, NEE).unwrap();
        assert_eq!(e2.weight, 2);
        assert_eq!(e2.a, e1.a);
        assert_eq!(e2.b, e1.b);
    }

    #[test]
    fn self_loop_rejeitado() {
        let db = TopologiaDb::in_memory().unwrap();
        assert!(matches!(db.bump_edge(NEE, NEE), Err(TopoError::SelfLoop)));
        assert!(matches!(
            db.promote_edge(NEE, NEE, None),
            Err(TopoError::SelfLoop)
        ));
    }

    #[test]
    fn promote_explore_para_user_mantem_weight() {
        let db = TopologiaDb::in_memory().unwrap();
        db.bump_edge(NEE, C4).unwrap();
        db.bump_edge(NEE, C4).unwrap(); // weight = 2, explore
        let p = db.promote_edge(C4, NEE, Some("mesmo conceito")).unwrap();
        assert_eq!(p.source, EdgeSource::User);
        assert_eq!(p.weight, 2, "promover não zera o peso ganho na exploração");
        assert_eq!(p.relation.as_deref(), Some("mesmo conceito"));
        // promover sem relation preserva a anterior
        let p2 = db.promote_edge(NEE, C4, None).unwrap();
        assert_eq!(p2.relation.as_deref(), Some("mesmo conceito"));
    }

    #[test]
    fn neighbors_ambas_direcoes_ordenado_por_weight() {
        let db = TopologiaDb::in_memory().unwrap();
        db.bump_edge(NEE, C4).unwrap(); // weight 1
        db.bump_edge(NEE, AYVU).unwrap();
        db.bump_edge(AYVU, NEE).unwrap(); // NEE–AYVU weight 2
        let nbs = db.neighbors(NEE);
        assert_eq!(nbs.len(), 2);
        assert_eq!(nbs[0].node, AYVU, "mais pesado primeiro");
        assert_eq!(nbs[0].weight, 2);
        assert_eq!(nbs[1].node, C4);
    }

    #[test]
    fn subgraph_bfs_atravessa_linguagens() {
        let db = TopologiaDb::in_memory().unwrap();
        // cadeia cross-pack: C4 — NEE — AYVU
        db.bump_edge(C4, NEE).unwrap();
        db.bump_edge(NEE, AYVU).unwrap();
        // depth 1 a partir de C4 alcança só NEE
        let (nodes1, edges1) = db.subgraph(C4, 1);
        assert!(nodes1.contains(&NEE.to_string()));
        assert!(!nodes1.contains(&AYVU.to_string()));
        assert_eq!(edges1.len(), 1);
        // depth 2 alcança AYVU (atravessou musica → guarani-mbya)
        let (nodes2, edges2) = db.subgraph(C4, 2);
        assert!(nodes2.contains(&AYVU.to_string()));
        assert_eq!(edges2.len(), 2);
    }

    #[test]
    fn semantica_recompute_idempotente_sobre_lexico_real() {
        if all_nodes(None).is_empty() {
            eprintln!("(skip) catálogo real ausente");
            return;
        }
        let db = TopologiaDb::in_memory().unwrap();
        let n = db.recompute_semantic(0.05, 8).unwrap();
        assert!(n > 0, "deve achar pares semânticos no léxico real");
        // recompute substitui (não duplica)
        assert_eq!(db.recompute_semantic(0.05, 8).unwrap(), n);
        // toda aresta do overlay é semantic, com score
        let any = all_nodes(None)[0].id.clone();
        assert!(
            db.semantic_incident(&[any], "corpus")
                .iter()
                .all(|e| e.source == EdgeSource::Semantic && e.score.is_some())
        );
    }

    #[test]
    fn refs_por_link_anexam_e_listam() {
        let db = TopologiaDb::in_memory().unwrap();
        db.add_ref(
            NEE,
            RefKind::Etymology,
            "/comunicacao/lexico/nee",
            Some("étimo"),
            Some("alice"),
        )
        .unwrap();
        db.add_ref(NEE, RefKind::Sentence, "https://ex/frase", None, None)
            .unwrap();
        let refs = db.refs_for(NEE);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].kind, RefKind::Sentence, "mais recente primeiro");
        assert_eq!(refs[1].kind, RefKind::Etymology);
        assert!(db.refs_for(C4).is_empty());
    }
}
