//! Gerador de mundo-timeline (YG-123) — protótipo do time-rendering lens CO-387.
//!
//! Recebe entradas com `at_iso` (chave canônica de ordenação do universo `time`)
//! e `kind` (moon.*, eclipse, equinox, entry.*, …) e emite uma
//! [`UniverseInstance`] com `projection: Timeline`: eixo X = tempo, uma layer
//! (faixa Y) por família de `kind` — os toggles de layer existentes da UI viram
//! o filtro por kind de graça.
//!
//! **Layout = funções puras** ([`x_for`], [`lane_rows`]) deliberadamente sem
//! dependência do schema: são o candidato a extração para o crate compartilhado
//! do CO-396 ("project timeline lens" do co usa a MESMA matemática). Quando o
//! crate existir, este módulo passa a adaptá-lo para Block/Cell — não duplicar
//! evolução de layout aqui (ver docs/architecture/co-387-time-lens-spec-draft.md).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::super::schema::{
    Block, Cell, GridSpec, Layer, Projection, SCHEMA_VERSION, UniverseInstance,
};

/// Uma entrada de conteúdo com posição no tempo.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    /// Id estável (vira `Block::id`).
    pub id: String,
    /// Rótulo humano (vira `Block::label`).
    pub title: String,
    /// Instante canônico (`at_iso` do universo `time`).
    pub at: DateTime<Utc>,
    /// Família/tipo (`moon.full`, `eclipse.solar`, `equinox`, `entry.audit`…).
    pub kind: String,
    /// Slug de nota ligada (opcional — vira `props.note_slug`).
    pub note_slug: Option<String>,
}

/// Posição X (coluna) de um instante num intervalo, em `width` colunas.
/// Interpolação linear; intervalo degenerado (min == max) → coluna central.
/// Pura e host-agnóstica — candidata ao crate compartilhado (CO-396).
pub fn x_for(at: DateTime<Utc>, min: DateTime<Utc>, max: DateTime<Utc>, width: u32) -> u32 {
    if width == 0 {
        return 0;
    }
    let span = (max - min).num_seconds();
    if span <= 0 {
        return width / 2;
    }
    let off = (at - min).num_seconds().clamp(0, span);
    // a última coluna é alcançável (off == span → width-1)
    ((off as f64 / span as f64) * (width.saturating_sub(1)) as f64).round() as u32
}

/// Agrupa kinds em faixas (lanes) Y estáveis: a família é o prefixo até o
/// primeiro `.` (`moon.full` → `moon`), ordenação determinística (BTreeMap).
/// Pura — candidata ao crate compartilhado (CO-396).
pub fn lane_rows(kinds: &[String]) -> BTreeMap<String, u32> {
    let mut lanes = BTreeMap::new();
    for k in kinds {
        let fam = k.split('.').next().unwrap_or(k).to_string();
        let next = lanes.len() as u32;
        lanes.entry(fam).or_insert(next);
    }
    lanes
}

fn family(kind: &str) -> String {
    kind.split('.').next().unwrap_or(kind).to_string()
}

/// Ícone default por família — só açúcar visual; o canônico é `props.kind`.
fn icon_for(kind: &str) -> &'static str {
    match family(kind).as_str() {
        "moon" => "🌙",
        "eclipse" => "🌘",
        "equinox" | "solstice" => "☀️",
        "entry" => "📓",
        _ => "📌",
    }
}

/// Gera a instância-timeline: uma layer por família de `kind` (faixa Y própria),
/// blocos posicionados por [`x_for`]; colisões na mesma célula empilham dentro
/// da faixa (até `LANE_HEIGHT` linhas; excedente fica na última linha).
pub fn generate(id: &str, owner: &str, title: &str, entries: &[TimelineEntry]) -> UniverseInstance {
    const LANE_HEIGHT: u32 = 3;
    let width: u32 = 48;

    let mut sorted: Vec<&TimelineEntry> = entries.iter().collect();
    sorted.sort_by_key(|e| e.at);
    let (min, max) = match (sorted.first(), sorted.last()) {
        (Some(a), Some(b)) => (a.at, b.at),
        _ => (Utc::now(), Utc::now()),
    };

    let kinds: Vec<String> = sorted.iter().map(|e| e.kind.clone()).collect();
    let lanes = lane_rows(&kinds);
    let height = (lanes.len() as u32 * LANE_HEIGHT).max(LANE_HEIGHT);

    // uma layer por família, na ordem das lanes
    let mut layers: BTreeMap<String, Layer> = lanes
        .keys()
        .map(|fam| (fam.clone(), Layer::blocks(fam, fam)))
        .collect();

    // empilhamento por célula dentro da faixa
    let mut occupied: BTreeMap<(String, u32), u32> = BTreeMap::new();
    for e in &sorted {
        let fam = family(&e.kind);
        let lane = *lanes.get(&fam).unwrap_or(&0);
        let x = x_for(e.at, min, max, width);
        let stack = occupied.entry((fam.clone(), x)).or_insert(0);
        let y = (lane * LANE_HEIGHT + (*stack).min(LANE_HEIGHT - 1)).min(height - 1);
        *stack += 1;

        let mut props = BTreeMap::new();
        props.insert("at_iso".into(), serde_json::json!(e.at.to_rfc3339()));
        props.insert("kind".into(), serde_json::json!(e.kind));
        props.insert("icon".into(), serde_json::json!(icon_for(&e.kind)));
        if let Some(slug) = &e.note_slug {
            props.insert("note_slug".into(), serde_json::json!(slug));
        }
        if let Some(layer) = layers.get_mut(&fam) {
            layer.blocks.push(Block {
                id: e.id.clone(),
                block_type: "evento".into(),
                pos: Cell { x, y },
                label: Some(e.title.clone()),
                attachments: vec![],
                props,
            });
        }
    }

    let now = Utc::now();
    UniverseInstance {
        id: id.to_string(),
        schema_version: SCHEMA_VERSION,
        owner: owner.to_string(),
        title: title.to_string(),
        description: "Mundo-timeline: eixo X = tempo, uma faixa por kind.".into(),
        template: "timeline".into(),
        grid: GridSpec {
            width,
            height,
            cell_size: 28,
        },
        projection: Projection::Timeline,
        layers: layers.into_values().collect(),
        connections: Vec::new(),
        published: false,
        meta: BTreeMap::new(),
        created_at: now,
        updated_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn entry(id: &str, iso: &str, kind: &str) -> TimelineEntry {
        TimelineEntry {
            id: id.into(),
            title: id.to_uppercase(),
            at: at(iso),
            kind: kind.into(),
            note_slug: None,
        }
    }

    #[test]
    fn x_for_interpola_e_e_monotonico_no_tempo() {
        let min = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let max = Utc.with_ymd_and_hms(2026, 12, 31, 0, 0, 0).unwrap();
        assert_eq!(x_for(min, min, max, 48), 0);
        assert_eq!(x_for(max, min, max, 48), 47);
        let meio = Utc.with_ymd_and_hms(2026, 7, 2, 0, 0, 0).unwrap();
        let xm = x_for(meio, min, max, 48);
        assert!((22..=25).contains(&xm), "meio do ano ≈ meio do eixo: {xm}");
        // degenerado: intervalo vazio → centro
        assert_eq!(x_for(min, min, min, 48), 24);
    }

    #[test]
    fn lane_rows_agrupa_por_familia_deterministico() {
        let kinds: Vec<String> = ["moon.full", "moon.new", "eclipse.solar", "equinox"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let lanes = lane_rows(&kinds);
        assert_eq!(lanes.len(), 3); // moon, eclipse, equinox
        assert!(lanes.contains_key("moon"));
        assert!(lanes.contains_key("eclipse"));
        assert!(lanes.contains_key("equinox"));
    }

    #[test]
    fn generate_projecao_timeline_layers_por_kind_e_at_iso_nos_props() {
        let entries = vec![
            entry("lua-cheia", "2026-01-03T10:00:00Z", "moon.full"),
            entry("lua-nova", "2026-06-15T10:00:00Z", "moon.new"),
            entry("equinocio", "2026-03-20T14:46:00Z", "equinox"),
            entry("eclipse", "2026-08-12T17:46:00Z", "eclipse.solar"),
        ];
        let inst = generate("tl-1", "ana", "Céu 2026", &entries);
        assert_eq!(inst.projection, Projection::Timeline);
        assert_eq!(inst.schema_version, SCHEMA_VERSION);
        // uma layer por família
        let mut names: Vec<&str> = inst.layers.iter().map(|l| l.id.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["eclipse", "equinox", "moon"]);
        // X cresce com o tempo dentro da faixa moon
        let moon = inst.layers.iter().find(|l| l.id == "moon").unwrap();
        let xs: Vec<u32> = moon.blocks.iter().map(|b| b.pos.x).collect();
        assert!(xs.windows(2).all(|w| w[0] <= w[1]), "X monotônico: {xs:?}");
        // props canônicos preservados
        let b = &moon.blocks[0];
        assert_eq!(b.props["kind"], serde_json::json!("moon.full"));
        assert!(
            b.props["at_iso"]
                .as_str()
                .unwrap()
                .starts_with("2026-01-03")
        );
        // dentro da grade
        for l in &inst.layers {
            for b in &l.blocks {
                assert!(inst.grid.contains(b.pos), "{:?} fora da grade", b.pos);
            }
        }
    }

    #[test]
    fn generate_empilha_colisoes_na_mesma_celula() {
        let entries = vec![
            entry("a", "2026-01-01T00:00:00Z", "entry.audit"),
            entry("b", "2026-01-01T00:00:00Z", "entry.audit"),
        ];
        let inst = generate("tl-2", "ana", "Audit", &entries);
        let l = &inst.layers[0];
        assert_eq!(l.blocks[0].pos.x, l.blocks[1].pos.x);
        assert_ne!(l.blocks[0].pos.y, l.blocks[1].pos.y, "colisão empilha em Y");
    }
}
