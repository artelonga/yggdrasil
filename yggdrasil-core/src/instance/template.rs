//! Templates — instâncias-semente + paleta de blocos + dicas de render
//! (YG-79 mecânica, YG-80 neuroanatomia).
//!
//! Um template é três coisas: um `seed` (esqueleto de [`UniverseInstance`]), uma
//! `palette` (os `block_type`s que o editor oferece) e `render_hints` livres. O
//! registro é estático, espelhando [`crate::universes::default_registry`].

use std::collections::BTreeMap;

use chrono::Utc;
use serde::Serialize;
use serde_json::json;

use super::schema::{
    Block, Cell, Connection, GridSpec, Layer, Projection, SCHEMA_VERSION, UniverseInstance,
};

/// Item de paleta: um `block_type` oferecido pelo editor, com props default.
#[derive(Debug, Clone, Serialize)]
pub struct PaletteItem {
    pub block_type: String,
    pub label: String,
    /// Props default aplicadas a um bloco recém-criado deste tipo (cor, ícone…).
    pub default_props: serde_json::Value,
}

impl PaletteItem {
    fn new(block_type: &str, label: &str, default_props: serde_json::Value) -> Self {
        Self {
            block_type: block_type.into(),
            label: label.into(),
            default_props,
        }
    }
}

/// Um template do catálogo.
#[derive(Debug, Clone, Serialize)]
pub struct Template {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub seed: UniverseInstance,
    pub palette: Vec<PaletteItem>,
    pub render_hints: serde_json::Value,
}

impl Template {
    /// Instancia o template para um novo dono: clona o seed e atribui
    /// id/owner/template/timestamps frescos.
    pub fn instantiate(&self, id: impl Into<String>, owner: impl Into<String>) -> UniverseInstance {
        let now = Utc::now();
        let mut inst = self.seed.clone();
        inst.id = id.into();
        inst.owner = owner.into();
        inst.template = self.slug.clone();
        inst.schema_version = SCHEMA_VERSION;
        inst.published = false;
        inst.created_at = now;
        inst.updated_at = now;
        inst
    }
}

/// Resumo leve para a listagem `GET /api/v1/templates`.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateSummary {
    pub slug: String,
    pub title: String,
    pub description: String,
}

impl From<&Template> for TemplateSummary {
    fn from(t: &Template) -> Self {
        Self {
            slug: t.slug.clone(),
            title: t.title.clone(),
            description: t.description.clone(),
        }
    }
}

/// Templates oferecidos na CRIAÇÃO (YG-126: um único tipo de universo — as
/// formas Mapa/Timeline/Grafo são views de runtime do player, padrão co).
pub fn all_templates() -> Vec<Template> {
    vec![universo_template()]
}

/// Templates legados: instâncias existentes resolvem a paleta por estes slugs
/// (`template_by_slug`), mas eles não aparecem mais na criação.
fn legacy_templates() -> Vec<Template> {
    vec![
        blank_template(),
        jardim_template(),
        neuroanatomia_template(),
        timeline_template(),
    ]
}

/// Resumos dos templates de criação.
pub fn template_summaries() -> Vec<TemplateSummary> {
    all_templates().iter().map(TemplateSummary::from).collect()
}

/// Busca um template pelo slug — inclui os legados (paleta de instâncias antigas).
pub fn template_by_slug(slug: &str) -> Option<Template> {
    all_templates()
        .into_iter()
        .chain(legacy_templates())
        .find(|t| t.slug == slug)
}

/// Template único `universo` (YG-126): nota + pasta + evento. A forma é
/// escolhida no player (views), não na criação; eventos (`props.at_iso`)
/// aparecem na view timeline junto com criação de notas e do próprio universo.
pub fn universo_template() -> Template {
    let grid = GridSpec {
        width: 32,
        height: 20,
        cell_size: 32,
    };
    let mut seed = empty_seed(
        Projection::TwoDGrid,
        grid,
        vec![Layer::blocks("conteudo", "Conteúdo")],
    );
    seed.title = "Universo".into();
    seed.description =
        "Notas, pastas e eventos num só lugar — alterne entre Mapa, Timeline e Grafo.".into();

    Template {
        slug: "universo".into(),
        title: "Universo".into(),
        description: "Um universo, todas as formas — mapa, timeline e grafo do mesmo conteúdo."
            .into(),
        seed,
        palette: vec![
            PaletteItem::new("note", "Nota", json!({ "color": "#b48ead", "icon": "📝" })),
            PaletteItem::new(
                "pasta",
                "Pasta",
                json!({ "color": "#e9c349", "icon": "📁" }),
            ),
            PaletteItem::new(
                "evento",
                "Evento",
                json!({ "color": "#7ec8e3", "icon": "📌" }),
            ),
        ],
        render_hints: json!({ "grid_lines": true, "graph_view": true, "time_axis": true }),
    }
}

fn empty_seed(projection: Projection, grid: GridSpec, layers: Vec<Layer>) -> UniverseInstance {
    let epoch = chrono::DateTime::UNIX_EPOCH.with_timezone(&Utc);
    UniverseInstance {
        id: String::new(),
        schema_version: SCHEMA_VERSION,
        owner: String::new(),
        title: String::new(),
        description: String::new(),
        template: String::new(),
        grid,
        projection,
        layers,
        connections: Vec::new(),
        published: false,
        meta: BTreeMap::new(),
        created_at: epoch,
        updated_at: epoch,
    }
}

/// Template `blank` — grade 2D vazia com uma camada de blocos e paleta de notas.
pub fn blank_template() -> Template {
    let mut seed = empty_seed(
        Projection::TwoDGrid,
        GridSpec::default(),
        vec![Layer::blocks("base", "Base")],
    );
    seed.title = "Universo em branco".into();
    seed.description = "Uma grade vazia para começar do zero.".into();

    Template {
        slug: "blank".into(),
        title: "Em branco".into(),
        description: "Grade 2D vazia — coloque blocos e anexe conteúdo.".into(),
        seed,
        palette: vec![
            PaletteItem::new("note", "Nota", json!({ "color": "#d4af37", "icon": "📝" })),
            PaletteItem::new(
                "pasta",
                "Pasta",
                json!({ "color": "#e9c349", "icon": "📁" }),
            ),
        ],
        render_hints: json!({ "grid_lines": true }),
    }
}

/// Template `jardim` — visão notes-first: grade leve, paleta só-`note`.
///
/// Pensado para anotações ligadas por wikilinks (o "jardim"): a única coisa que
/// o editor oferece é colocar notas, e a grade fica clara/leve para a visão de
/// grafo ser o foco. As notas em si vivem como Markdown via `NoteStore` (Phase
/// 0); aqui só semeamos o esqueleto e a paleta.
pub fn jardim_template() -> Template {
    let grid = GridSpec {
        width: 24,
        height: 16,
        cell_size: 32,
    };
    let mut seed = empty_seed(
        Projection::TwoDGrid,
        grid,
        vec![Layer::blocks("notas", "Notas")],
    );
    seed.title = "Jardim".into();
    seed.description = "Comece a anotar: cada nota é um nó, wikilinks são as arestas.".into();

    Template {
        slug: "jardim".into(),
        title: "Jardim".into(),
        description: "Visão notes-first — escreva notas e ligue-as com [[wikilinks]].".into(),
        seed,
        // `pasta` simula diretórios na grade: arraste uma nota para cima de uma
        // pasta para criar a ligação `parent` (filho→pasta); irmãos da mesma
        // pasta viram um tipo de ligação derivado no player.
        palette: vec![
            PaletteItem::new("note", "Nota", json!({ "color": "#b48ead", "icon": "📝" })),
            PaletteItem::new(
                "pasta",
                "Pasta",
                json!({ "color": "#e9c349", "icon": "📁" }),
            ),
        ],
        render_hints: json!({ "grid_lines": true, "graph_view": true }),
    }
}

/// Template `timeline` (YG-123) — mundo-timeline: eixo X = tempo, faixa Y por
/// `kind`. Semeado com eventos astronômicos de 2026 (equinócios, solstícios,
/// luas cheias, eclipse) para algo renderizar antes do conteúdo do universo
/// `time` fluir pelo bridge (P-B). Datas fixas → seed determinístico.
pub fn timeline_template() -> Template {
    use super::generators::timeline::{TimelineEntry, generate};
    let at = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .expect("data fixa válida")
            .with_timezone(&Utc)
    };
    let ev = |id: &str, title: &str, iso: &str, kind: &str| TimelineEntry {
        id: id.into(),
        title: title.into(),
        at: at(iso),
        kind: kind.into(),
        note_slug: None,
    };
    let entries = vec![
        ev(
            "equinocio-marco",
            "Equinócio de março",
            "2026-03-20T14:46:00Z",
            "equinox",
        ),
        ev(
            "solsticio-junho",
            "Solstício de junho",
            "2026-06-21T08:25:00Z",
            "solstice",
        ),
        ev(
            "equinocio-setembro",
            "Equinócio de setembro",
            "2026-09-23T00:05:00Z",
            "equinox",
        ),
        ev(
            "solsticio-dezembro",
            "Solstício de dezembro",
            "2026-12-21T20:50:00Z",
            "solstice",
        ),
        ev(
            "lua-cheia-jan",
            "Lua cheia de janeiro",
            "2026-01-03T10:03:00Z",
            "moon.full",
        ),
        ev(
            "lua-cheia-jul",
            "Lua cheia de julho",
            "2026-07-29T14:36:00Z",
            "moon.full",
        ),
        ev(
            "eclipse-solar-ago",
            "Eclipse solar total",
            "2026-08-12T17:46:00Z",
            "eclipse.solar",
        ),
    ];
    let mut seed = generate("", "", "Timeline", &entries);
    // seed de template é atemporal (espelha `empty_seed`): timestamps no epoch
    let epoch = chrono::DateTime::UNIX_EPOCH.with_timezone(&Utc);
    seed.template = String::new();
    seed.created_at = epoch;
    seed.updated_at = epoch;

    Template {
        slug: "timeline".into(),
        title: "Timeline".into(),
        description: "Mundo-timeline — eixo X é o tempo; faixas por tipo de evento.".into(),
        seed,
        palette: vec![PaletteItem::new(
            "evento",
            "Evento",
            json!({ "color": "#7ec8e3", "icon": "📌" }),
        )],
        render_hints: json!({ "grid_lines": false, "time_axis": true }),
    }
}

/// Template `neuroanatomia` — silhueta do corpo + overlay do SNC + landmarks.
///
/// As imagens de fundo (silhueta, SNC) são anexadas pelo usuário (CC0/CC-BY —
/// ver `docs/architecture/editor.md`); o seed já traz as camadas com a opacidade
/// correta (SNC a 0.5 → o toggle de transparência) e landmarks/conexões de
/// exemplo, para algo renderizar mesmo antes do upload.
pub fn neuroanatomia_template() -> Template {
    let grid = GridSpec {
        width: 24,
        height: 40,
        cell_size: 20,
    };

    let corpo = Layer::background("corpo", "Silhueta do corpo", 1.0, None);
    let snc = Layer::background("snc", "Sistema Nervoso Central", 0.5, None);

    let landmark = |id: &str, label: &str, x: u32, y: u32| Block {
        id: id.into(),
        block_type: "landmark".into(),
        pos: Cell { x, y },
        label: Some(label.into()),
        attachments: vec![],
        props: {
            let mut m = BTreeMap::new();
            m.insert("color".into(), json!("#7ec8e3"));
            m.insert("icon".into(), json!("🧠"));
            m
        },
    };

    let mut landmarks = Layer::blocks("landmarks", "Landmarks");
    landmarks.blocks = vec![
        landmark("cortex", "Córtex cerebral", 12, 3),
        landmark("cerebelo", "Cerebelo", 12, 6),
        landmark("tronco", "Tronco encefálico", 12, 8),
        landmark("medula", "Medula espinhal", 12, 22),
    ];

    let conn = |id: &str, from: &str, to: &str, label: &str| Connection {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        label: Some(label.into()),
        directed: true,
        props: BTreeMap::new(),
    };

    let mut seed = empty_seed(Projection::TwoDGrid, grid, vec![corpo, snc, landmarks]);
    seed.title = "Neuroanatomia".into();
    seed.description =
        "Sistema nervoso central dentro do corpo humano, com toggle de transparência.".into();
    seed.connections = vec![
        conn("cortex-tronco", "cortex", "tronco", "Vias descendentes"),
        conn("tronco-medula", "tronco", "medula", "Trato corticoespinhal"),
    ];

    Template {
        slug: "neuroanatomia".into(),
        title: "Neuroanatomia".into(),
        description:
            "Renderize o SNC dentro do corpo, alterne a transparência, marque landmarks e conecte vias."
                .into(),
        seed,
        palette: vec![
            PaletteItem::new(
                "landmark",
                "Landmark anatômico",
                json!({ "color": "#7ec8e3", "icon": "🧠" }),
            ),
            PaletteItem::new(
                "region",
                "Região",
                json!({ "color": "#b388ff", "icon": "🫧" }),
            ),
            PaletteItem::new(
                "annotation",
                "Anotação",
                json!({ "color": "#ffd166", "icon": "🏷️" }),
            ),
        ],
        render_hints: json!({
            "transparency_layer": "snc",
            "connection_style": "arrow",
            "attribution_required": true
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_instancia_com_owner_e_id() {
        let t = blank_template();
        let inst = t.instantiate("inst-9", "alice");
        assert_eq!(inst.id, "inst-9");
        assert_eq!(inst.owner, "alice");
        assert_eq!(inst.template, "blank");
        assert_eq!(inst.layers.len(), 1);
    }

    #[test]
    fn neuroanatomia_seed_estrutura() {
        let t = neuroanatomia_template();
        let inst = t.instantiate("n1", "bob");
        // 2 camadas de fundo + 1 de blocos
        assert_eq!(inst.layers.len(), 3);
        // camada SNC a 0.5 (o toggle de transparência)
        let snc = inst.layer("snc").unwrap();
        assert_eq!(snc.opacity, 0.5);
        // 4 landmarks + 2 conexões
        assert_eq!(inst.layer("landmarks").unwrap().blocks.len(), 4);
        assert_eq!(inst.connections.len(), 2);
        // endpoints das conexões existem
        for c in &inst.connections {
            assert!(inst.has_block(&c.from));
            assert!(inst.has_block(&c.to));
        }
    }

    #[test]
    fn jardim_palette_nota_e_pasta() {
        // Está registrado no catálogo e acessível por slug.
        assert!(template_by_slug("jardim").is_some());
        let t = jardim_template();
        // Paleta notes-first: `note` + `pasta` (diretório simulado; ligações
        // `parent` por drag-and-drop no player).
        assert_eq!(t.palette.len(), 2);
        assert_eq!(t.palette[0].block_type, "note");
        assert_eq!(t.palette[1].block_type, "pasta");
        // Seed tem uma única camada de notas, sem fundos.
        let inst = t.instantiate("j1", "ana");
        assert_eq!(inst.template, "jardim");
        assert_eq!(inst.layers.len(), 1);
        assert_eq!(inst.layer("notas").unwrap().blocks.len(), 0);
    }

    #[test]
    fn lookup_por_slug() {
        assert!(template_by_slug("neuroanatomia").is_some());
        assert!(template_by_slug("blank").is_some());
        assert!(template_by_slug("jardim").is_some());
        assert!(template_by_slug("inexistente").is_none());
    }

    #[test]
    fn summaries_lista_todos() {
        // YG-126: a criação oferece UM tipo; formas são views de runtime.
        assert_eq!(template_summaries().len(), 1);
        assert_eq!(template_summaries()[0].slug, "universo");
    }

    #[test]
    fn universo_template_nota_pasta_evento() {
        let t = template_by_slug("universo").unwrap();
        let tipos: Vec<&str> = t.palette.iter().map(|p| p.block_type.as_str()).collect();
        assert_eq!(tipos, vec!["note", "pasta", "evento"]);
        assert_eq!(t.seed.projection, Projection::TwoDGrid);
    }

    #[test]
    fn templates_legados_seguem_resolvendo_por_slug() {
        // instâncias antigas mantêm a paleta (não aparecem na criação)
        for slug in ["blank", "jardim", "neuroanatomia", "timeline"] {
            assert!(template_by_slug(slug).is_some(), "legado {slug} resolve");
        }
    }

    #[test]
    fn timeline_template_semeia_eventos_em_projecao_timeline() {
        let t = template_by_slug("timeline").unwrap();
        assert_eq!(t.seed.projection, Projection::Timeline);
        let total: usize = t.seed.layers.iter().map(|l| l.blocks.len()).sum();
        assert_eq!(total, 7, "7 eventos astronômicos de 2026 no seed");
        // instanciar carimba dono/template e preserva a projeção
        let inst = t.instantiate("t1", "ana");
        assert_eq!(inst.template, "timeline");
        assert_eq!(inst.projection, Projection::Timeline);
    }
}
