//! Universe graph — universos como nós num grafo recursivo.
//!
//! Cada universo é um nó (`UniverseNode`) que pode ter sub-universos
//! (variantes ou composições). O grafo é o contrato público: a API
//! `GET /api/v1/universes` expõe nós queryable, documentáveis e
//! navegáveis.
//!
//! Padrão **composição sobre herança**: uma variante não _estende_ o
//! universo raiz, ela _instancia_ o engine raiz com overrides de
//! parâmetros. Snake/walls = Snake + Map(walled). Tetris/sprint-40 =
//! Tetris + LinesToClear(40). Poker/heads-up = Poker + MaxSeats(2).
//!
//! Forward-compat com a POC Godot (YG-31..YG-35): o mesmo grafo
//! corresponde a uma árvore de scenes — root = .tscn principal,
//! variante = sub-scene que instancia o root com props alteradas.

use serde::Serialize;
use std::collections::BTreeMap;

/// Tipo do nó. Determina como o engine subjacente é invocado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UniverseKind {
    /// Universo fundacional — define o engine. Ex: `tetris`, `snake`.
    Root,
    /// Mesmo engine do root, parâmetros diferentes. Ex: `tetris/sprint-40`.
    Variant,
    /// Combina múltiplos universos (mesmos ou diferentes engines).
    /// Ex: `tetris/duel` = dois tetris/classic lado a lado.
    Composition,
}

/// Contrato de API de um universo — endpoints que o cliente usa para iniciar
/// e enviar inputs. Universos `Root` apontam para sua própria rota
/// (`/api/v1/games/<root>/...`); variantes apontam para a mesma rota com
/// query string `?variant=<slug>`.
#[derive(Debug, Clone, Serialize)]
pub struct ApiContract {
    /// Caminho HTTP para iniciar uma sessão.
    pub start: String,
    /// Caminho HTTP para enviar input (placeholder `{id}` para session id).
    pub input: String,
    /// Caminho HTTP para a página jogável (frontend).
    pub page: String,
}

/// Um nó do grafo de universos.
#[derive(Debug, Clone, Serialize)]
pub struct UniverseNode {
    /// Identificador canônico — pode conter `/` para indicar caminho no
    /// grafo (`tetris/sprint-40`). Único globalmente.
    pub slug: String,
    /// Slug do pai. `None` para roots.
    pub parent: Option<String>,
    /// Slugs dos filhos diretos.
    pub children: Vec<String>,
    pub kind: UniverseKind,
    /// Nome humano (PT-BR).
    pub title: String,
    /// Descrição curta (PT-BR).
    pub description: String,
    /// Parâmetros que definem essa variação. Para roots = defaults.
    /// Para variantes = overrides do default do root.
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    pub api: ApiContract,
}

impl UniverseNode {
    pub fn root(slug: &str, title: &str, description: &str) -> Self {
        let api = root_api_contract(slug);
        Self {
            slug: slug.to_string(),
            parent: None,
            children: Vec::new(),
            kind: UniverseKind::Root,
            title: title.to_string(),
            description: description.to_string(),
            parameters: BTreeMap::new(),
            api,
        }
    }

    pub fn variant(slug: &str, parent: &str, title: &str, description: &str) -> Self {
        let api = variant_api_contract(parent, slug);
        Self {
            slug: slug.to_string(),
            parent: Some(parent.to_string()),
            children: Vec::new(),
            kind: UniverseKind::Variant,
            title: title.to_string(),
            description: description.to_string(),
            parameters: BTreeMap::new(),
            api,
        }
    }

    pub fn composition(slug: &str, parent: Option<&str>, title: &str, description: &str) -> Self {
        let api = match parent {
            Some(p) => variant_api_contract(p, slug),
            None => root_api_contract(slug),
        };
        Self {
            slug: slug.to_string(),
            parent: parent.map(String::from),
            children: Vec::new(),
            kind: UniverseKind::Composition,
            title: title.to_string(),
            description: description.to_string(),
            parameters: BTreeMap::new(),
            api,
        }
    }

    pub fn with_param(mut self, key: &str, value: serde_json::Value) -> Self {
        self.parameters.insert(key.to_string(), value);
        self
    }
}

fn root_api_contract(slug: &str) -> ApiContract {
    ApiContract {
        start: format!("/api/v1/games/{slug}/start"),
        input: format!("/api/v1/games/{slug}/{{id}}/input"),
        page: format!("/universos/{slug}"),
    }
}

fn variant_api_contract(parent: &str, slug: &str) -> ApiContract {
    // Variantes herdam a rota do parent (root) — a parameterização real
    // virá como query string `?variant=<slug>` num task seguinte.
    let parent_root = parent.split('/').next().unwrap_or(parent);
    ApiContract {
        start: format!("/api/v1/games/{parent_root}/start?variant={slug}"),
        input: format!("/api/v1/games/{parent_root}/{{id}}/input"),
        // Página fica na raiz; variant pode ser selecionada por query string ou rota futura.
        page: format!("/universos/{parent_root}?variant={slug}"),
    }
}

/// Edge do grafo (parent → child). Útil para visualizadores.
#[derive(Debug, Clone, Serialize)]
pub struct UniverseEdge {
    pub from: String,
    pub to: String,
}

/// Vista do grafo: nós + arestas.
#[derive(Debug, Clone, Serialize)]
pub struct UniverseGraph {
    pub nodes: Vec<UniverseNode>,
    pub edges: Vec<UniverseEdge>,
}

/// Registro estático dos universos conhecidos. Construído via builder ou
/// pelo seed default em [`default_registry`].
#[derive(Debug, Clone, Default)]
pub struct UniverseRegistry {
    nodes: BTreeMap<String, UniverseNode>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Universo desconhecido: {0}")]
    NotFound(String),
    #[error("Universo {0} já registrado")]
    AlreadyExists(String),
    #[error("Pai {parent} de {child} não está no registro")]
    DanglingParent { parent: String, child: String },
}

impl UniverseRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insere um nó. Atualiza `children` do pai automaticamente.
    pub fn register(&mut self, node: UniverseNode) -> Result<(), RegistryError> {
        if self.nodes.contains_key(&node.slug) {
            return Err(RegistryError::AlreadyExists(node.slug.clone()));
        }
        if let Some(parent_slug) = node.parent.clone() {
            let parent =
                self.nodes
                    .get_mut(&parent_slug)
                    .ok_or_else(|| RegistryError::DanglingParent {
                        parent: parent_slug.clone(),
                        child: node.slug.clone(),
                    })?;
            parent.children.push(node.slug.clone());
        }
        self.nodes.insert(node.slug.clone(), node);
        Ok(())
    }

    pub fn get(&self, slug: &str) -> Option<&UniverseNode> {
        self.nodes.get(slug)
    }

    pub fn list(&self) -> Vec<&UniverseNode> {
        self.nodes.values().collect()
    }

    pub fn roots(&self) -> Vec<&UniverseNode> {
        self.nodes.values().filter(|n| n.parent.is_none()).collect()
    }

    pub fn children_of(&self, slug: &str) -> Vec<&UniverseNode> {
        self.nodes
            .values()
            .filter(|n| n.parent.as_deref() == Some(slug))
            .collect()
    }

    /// Retorna toda a árvore como `UniverseGraph`.
    pub fn graph(&self) -> UniverseGraph {
        let nodes: Vec<UniverseNode> = self.nodes.values().cloned().collect();
        let edges: Vec<UniverseEdge> = self
            .nodes
            .values()
            .filter_map(|n| {
                n.parent.as_ref().map(|p| UniverseEdge {
                    from: p.clone(),
                    to: n.slug.clone(),
                })
            })
            .collect();
        UniverseGraph { nodes, edges }
    }
}

/// Construtor do registro default — 4 roots + variantes de exemplo.
/// Cresce conforme universos novos são definidos.
pub fn default_registry() -> UniverseRegistry {
    let mut reg = UniverseRegistry::new();

    // ── Roots ─────────────────────────────────────────────────────────────
    reg.register(UniverseNode::root(
        "snake",
        "Snake",
        "Coma maçãs sem morder a própria cauda. Clássico desde 1976.",
    ))
    .expect("snake root");
    reg.register(UniverseNode::root(
        "tetris",
        "Tetris",
        "Encaixe peças em linhas — clássico desde 1984.",
    ))
    .expect("tetris root");
    reg.register(UniverseNode::root(
        "invaders",
        "Invaders",
        "Defenda a Terra de alienígenas que descem em formação.",
    ))
    .expect("invaders root");
    reg.register(UniverseNode::root(
        "poker",
        "Pôquer",
        "Texas Hold'em multiplayer com aposta em sementes.",
    ))
    .expect("poker root");

    // ── Variantes Snake ───────────────────────────────────────────────────
    reg.register(
        UniverseNode::variant(
            "snake/classic",
            "snake",
            "Snake Clássico",
            "Mapa vazio, velocidade média.",
        )
        .with_param("difficulty", serde_json::json!("medium")),
    )
    .expect("snake/classic");
    reg.register(
        UniverseNode::variant(
            "snake/walls",
            "snake",
            "Snake com Paredes",
            "Mapa com obstáculos internos — pathfinding mais exigente.",
        )
        .with_param("map", serde_json::json!("walls"))
        .with_param("difficulty", serde_json::json!("hard")),
    )
    .expect("snake/walls");

    // ── Variantes Tetris ──────────────────────────────────────────────────
    reg.register(
        UniverseNode::variant(
            "tetris/classic",
            "tetris",
            "Tetris Clássico",
            "Sem limite de linhas, dificuldade aumenta com o nível.",
        )
        .with_param("mode", serde_json::json!("marathon")),
    )
    .expect("tetris/classic");
    reg.register(
        UniverseNode::variant(
            "tetris/sprint-40",
            "tetris",
            "Tetris Sprint 40",
            "Limpe 40 linhas o mais rápido possível.",
        )
        .with_param("mode", serde_json::json!("sprint"))
        .with_param("lines_to_clear", serde_json::json!(40)),
    )
    .expect("tetris/sprint-40");

    // ── Variantes Invaders ────────────────────────────────────────────────
    reg.register(
        UniverseNode::variant(
            "invaders/classic",
            "invaders",
            "Invaders Clássico",
            "Grade 4×10 de aliens, 3 vidas.",
        )
        .with_param("difficulty", serde_json::json!("medium")),
    )
    .expect("invaders/classic");
    reg.register(
        UniverseNode::variant(
            "invaders/swarm",
            "invaders",
            "Invaders Swarm",
            "Mais aliens, descem mais rápido, 1 vida.",
        )
        .with_param("difficulty", serde_json::json!("hard"))
        .with_param("lives", serde_json::json!(1)),
    )
    .expect("invaders/swarm");

    // ── Variantes Poker ───────────────────────────────────────────────────
    reg.register(
        UniverseNode::variant(
            "poker/cash-game",
            "poker",
            "Pôquer Cash Game",
            "Mesas Carvalho e Olmo — buy-in 1000 sementes.",
        )
        .with_param("buy_in_sementes", serde_json::json!(1000))
        .with_param("max_seats", serde_json::json!(6)),
    )
    .expect("poker/cash-game");
    reg.register(
        UniverseNode::variant(
            "poker/heads-up",
            "poker",
            "Pôquer Heads-Up",
            "Duelo 1×1, sem mesas, blinds aceleradas.",
        )
        .with_param("max_seats", serde_json::json!(2)),
    )
    .expect("poker/heads-up");

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_starts_empty() {
        let reg = UniverseRegistry::new();
        assert!(reg.list().is_empty());
        assert!(reg.roots().is_empty());
    }

    #[test]
    fn register_root_node() {
        let mut reg = UniverseRegistry::new();
        reg.register(UniverseNode::root("snake", "Snake", "..."))
            .unwrap();
        let node = reg.get("snake").unwrap();
        assert_eq!(node.slug, "snake");
        assert!(node.parent.is_none());
        assert_eq!(node.kind, UniverseKind::Root);
    }

    #[test]
    fn register_variant_atualiza_children_do_parent() {
        let mut reg = UniverseRegistry::new();
        reg.register(UniverseNode::root("tetris", "Tetris", "..."))
            .unwrap();
        reg.register(UniverseNode::variant(
            "tetris/sprint-40",
            "tetris",
            "Sprint",
            "...",
        ))
        .unwrap();
        let parent = reg.get("tetris").unwrap();
        assert_eq!(parent.children, vec!["tetris/sprint-40"]);
        let child = reg.get("tetris/sprint-40").unwrap();
        assert_eq!(child.parent.as_deref(), Some("tetris"));
    }

    #[test]
    fn register_variant_com_pai_inexistente_falha() {
        let mut reg = UniverseRegistry::new();
        let err = reg
            .register(UniverseNode::variant("tetris/sprint", "tetris", "S", "..."))
            .unwrap_err();
        assert!(matches!(err, RegistryError::DanglingParent { .. }));
    }

    #[test]
    fn register_duplicado_falha() {
        let mut reg = UniverseRegistry::new();
        reg.register(UniverseNode::root("snake", "Snake", "..."))
            .unwrap();
        let err = reg
            .register(UniverseNode::root("snake", "Snake 2", "..."))
            .unwrap_err();
        assert!(matches!(err, RegistryError::AlreadyExists(_)));
    }

    #[test]
    fn parameters_via_builder() {
        let n = UniverseNode::variant("tetris/sprint-40", "tetris", "S40", "...")
            .with_param("lines_to_clear", serde_json::json!(40));
        assert_eq!(
            n.parameters.get("lines_to_clear"),
            Some(&serde_json::json!(40))
        );
    }

    #[test]
    fn graph_inclui_edges_de_cada_filho_para_pai() {
        let reg = default_registry();
        let g = reg.graph();
        // Cada variante gera uma edge parent->child.
        let snake_edges: Vec<_> = g.edges.iter().filter(|e| e.from == "snake").collect();
        assert!(!snake_edges.is_empty());
        // E o snake aparece como nó.
        assert!(g.nodes.iter().any(|n| n.slug == "snake"));
    }

    #[test]
    fn default_registry_tem_quatro_roots() {
        let reg = default_registry();
        let roots = reg.roots();
        assert_eq!(roots.len(), 4);
        let root_slugs: Vec<&str> = roots.iter().map(|n| n.slug.as_str()).collect();
        for expected in &["snake", "tetris", "invaders", "poker"] {
            assert!(
                root_slugs.contains(expected),
                "esperava {expected} em {root_slugs:?}"
            );
        }
    }

    #[test]
    fn default_registry_variantes_apontam_para_root() {
        let reg = default_registry();
        for variant_slug in &["snake/walls", "tetris/sprint-40", "poker/heads-up"] {
            let n = reg.get(variant_slug).unwrap();
            assert_eq!(n.kind, UniverseKind::Variant);
            let parent_slug = variant_slug.split('/').next().unwrap();
            assert_eq!(n.parent.as_deref(), Some(parent_slug));
        }
    }

    #[test]
    fn variant_api_contract_aponta_para_root_com_query_param() {
        let n = UniverseNode::variant("tetris/sprint-40", "tetris", "S40", "...");
        assert_eq!(
            n.api.start,
            "/api/v1/games/tetris/start?variant=tetris/sprint-40"
        );
        assert_eq!(n.api.page, "/universos/tetris?variant=tetris/sprint-40");
    }

    #[test]
    fn root_api_contract_canonico() {
        let n = UniverseNode::root("snake", "Snake", "...");
        assert_eq!(n.api.start, "/api/v1/games/snake/start");
        assert_eq!(n.api.page, "/universos/snake");
    }

    #[test]
    fn children_of_filtra_corretamente() {
        let reg = default_registry();
        let snake_children = reg.children_of("snake");
        assert_eq!(snake_children.len(), 2);
        let slugs: Vec<&str> = snake_children.iter().map(|n| n.slug.as_str()).collect();
        assert!(slugs.contains(&"snake/classic"));
        assert!(slugs.contains(&"snake/walls"));
    }

    #[test]
    fn composition_aceita_pai_root_ou_variante() {
        let mut reg = UniverseRegistry::new();
        reg.register(UniverseNode::root("tetris", "Tetris", "..."))
            .unwrap();
        reg.register(UniverseNode::composition(
            "tetris/duel",
            Some("tetris"),
            "Duel",
            "Dois tetris lado a lado.",
        ))
        .unwrap();
        let n = reg.get("tetris/duel").unwrap();
        assert_eq!(n.kind, UniverseKind::Composition);
        assert_eq!(n.parent.as_deref(), Some("tetris"));
    }
}
