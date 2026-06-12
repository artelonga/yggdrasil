//! Formato data-driven de universos autorados — `UniverseInstance` (YG-74).
//!
//! Conceito **paralelo** ao [`crate::universes`] (`UniverseNode`/`UniverseKind`),
//! que é o catálogo de engines compiladas. Uma instância autorada não tem engine:
//! é dado puro renderizado por um player genérico client-side. Por isso vive num
//! módulo próprio e — diferente de `UniverseNode` — deriva `Deserialize`.
//!
//! Layout sparse de propósito: blocos por coordenada ([`Block::pos`]), não matriz
//! densa como `game_core::engine::Map`. Grades autoradas são grandes e
//! majoritariamente vazias, e cada bloco carrega payload rico.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Versão do schema serializado. Incrementar ao introduzir migração incompatível.
///
/// v2: notas Markdown ligadas por wikilinks (referenciadas por
/// `Block.props.note_slug`). Mudança puramente aditiva em disco — instâncias v1
/// carregam sem migração (campos/props novos têm default); só são reescritas como
/// v2 no próximo `save`.
pub const SCHEMA_VERSION: u32 = 3;

/// Um universo autorado por usuário: grade + camadas + blocos + conexões.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseInstance {
    /// Identificador estável (nanoid); também o nome do diretório em disco.
    pub id: String,
    /// Versão do schema (= [`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// `sub` do JWT do autor/dono.
    pub owner: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Slug do template-semente usado na criação (ex. `"neuroanatomia"`).
    #[serde(default)]
    pub template: String,
    pub grid: GridSpec,
    pub projection: Projection,
    /// Camadas z-ordenadas; índice 0 = base.
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    /// Se a instância é visível publicamente (feed `?published=true`).
    #[serde(default)]
    pub published: bool,
    /// Metadata livre (opções de analytics etc.).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UniverseInstance {
    /// Cria uma instância vazia com uma única camada de blocos.
    pub fn empty(
        id: impl Into<String>,
        owner: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            schema_version: SCHEMA_VERSION,
            owner: owner.into(),
            title: title.into(),
            description: String::new(),
            template: String::new(),
            grid: GridSpec::default(),
            projection: Projection::TwoDGrid,
            layers: vec![Layer::blocks("base", "Base")],
            connections: Vec::new(),
            published: false,
            meta: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Encontra uma camada mutável pelo id.
    pub fn layer_mut(&mut self, layer_id: &str) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == layer_id)
    }

    /// Encontra uma camada pelo id.
    pub fn layer(&self, layer_id: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == layer_id)
    }

    /// Verdadeiro se algum bloco (em qualquer camada) tem o id dado.
    pub fn has_block(&self, block_id: &str) -> bool {
        self.layers
            .iter()
            .any(|l| l.blocks.iter().any(|b| b.id == block_id))
    }
}

/// Dimensões da grade e tamanho de célula em pixels (dica de render).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSpec {
    pub width: u32,
    pub height: u32,
    pub cell_size: u32,
}

impl Default for GridSpec {
    fn default() -> Self {
        Self {
            width: 40,
            height: 20,
            cell_size: 16,
        }
    }
}

impl GridSpec {
    /// Verdadeiro se a célula está dentro dos limites.
    pub fn contains(&self, cell: Cell) -> bool {
        cell.x < self.width && cell.y < self.height
    }
}

/// Projeção de render. Começa em `TwoDGrid` (grade tipo xadrez); iso depois.
/// `Timeline` (YG-123, SCHEMA_VERSION 3): eixo X = tempo (`props.at_iso` dos
/// blocos), uma faixa Y por layer/`kind` — protótipo do time-rendering lens
/// CO-387.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Projection {
    TwoDGrid,
    Isometric,
    Timeline,
}

/// Uma camada z-ordenada da instância.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub kind: LayerKind,
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Opacidade 0.0..=1.0 — o toggle de transparência (ex. overlay do SNC).
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    /// Imagem de fundo da camada (ex. silhueta do corpo / overlay do SNC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<ContentRef>,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

impl Layer {
    /// Camada de blocos vazia, totalmente opaca e visível.
    pub fn blocks(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: LayerKind::Blocks,
            visible: true,
            opacity: 1.0,
            background: None,
            blocks: Vec::new(),
        }
    }

    /// Camada de fundo com uma imagem e opacidade dada.
    pub fn background(
        id: impl Into<String>,
        name: impl Into<String>,
        opacity: f32,
        image: Option<ContentRef>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind: LayerKind::Background,
            visible: true,
            opacity,
            background: image,
            blocks: Vec::new(),
        }
    }
}

/// Papel de uma camada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    /// Imagem de fundo (silhueta, overlay anatômico…).
    Background,
    /// Blocos posicionados na grade.
    Blocks,
    /// Anotações livres sobre as demais camadas.
    Annotation,
}

/// Um bloco posicionado numa célula da grade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Id estável; referenciado por [`Connection::from`]/[`Connection::to`].
    pub id: String,
    /// Chave de paleta (ex. `"landmark"`, `"note"`, `"portal"`).
    pub block_type: String,
    pub pos: Cell,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<ContentRef>,
    /// Dicas de render e metadata (cor, ícone, atribuição/licença…).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, serde_json::Value>,
}

/// Coordenada de grade (não pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub x: u32,
    pub y: u32,
}

/// Uma conexão (aresta) entre dois blocos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    /// [`Block::id`] de origem.
    pub from: String,
    /// [`Block::id`] de destino.
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub directed: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub props: BTreeMap<String, serde_json::Value>,
}

/// Referência content-addressed a um anexo. Os bytes vivem em
/// `_blobs/<shard>/<hash>` (ver [`crate::instance::store`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRef {
    pub kind: AttachmentKind,
    /// SHA-256 hex dos bytes.
    pub hash: String,
    /// Nome original, para `Content-Disposition` no download.
    pub filename: String,
    pub mime: String,
    pub size: u64,
}

/// Categoria de anexo, derivada do MIME no upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Pdf,
    Image,
    Sound,
    Markdown,
    Metadata,
}

impl AttachmentKind {
    /// Deriva a categoria a partir de um MIME type. `None` se não permitido.
    pub fn from_mime(mime: &str) -> Option<Self> {
        let base = mime.split(';').next().unwrap_or(mime).trim();
        match base {
            "application/pdf" => Some(Self::Pdf),
            "image/png" | "image/jpeg" | "image/svg+xml" | "image/webp" | "image/gif" => {
                Some(Self::Image)
            }
            "audio/mpeg" | "audio/wav" | "audio/ogg" | "audio/webm" => Some(Self::Sound),
            "text/markdown" | "text/x-markdown" => Some(Self::Markdown),
            "application/json" => Some(Self::Metadata),
            _ => None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constrói uma instância neuroanatomia mínima para testes de round-trip.
    pub(crate) fn sample_neuroanatomia() -> UniverseInstance {
        let fixed = DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let silhueta = ContentRef {
            kind: AttachmentKind::Image,
            hash: "a".repeat(64),
            filename: "corpo.svg".into(),
            mime: "image/svg+xml".into(),
            size: 1234,
        };
        let cns = ContentRef {
            kind: AttachmentKind::Image,
            hash: "b".repeat(64),
            filename: "snc.svg".into(),
            mime: "image/svg+xml".into(),
            size: 2345,
        };
        let mut blocks_layer = Layer::blocks("landmarks", "Landmarks");
        blocks_layer.blocks = vec![
            Block {
                id: "cortex".into(),
                block_type: "landmark".into(),
                pos: Cell { x: 19, y: 2 },
                label: Some("Córtex frontal".into()),
                attachments: vec![],
                props: BTreeMap::new(),
            },
            Block {
                id: "medula".into(),
                block_type: "landmark".into(),
                pos: Cell { x: 19, y: 14 },
                label: Some("Medula espinhal".into()),
                attachments: vec![],
                props: BTreeMap::new(),
            },
        ];

        UniverseInstance {
            id: "inst-1".into(),
            schema_version: SCHEMA_VERSION,
            owner: "user-1".into(),
            title: "Neuroanatomia".into(),
            description: "SNC dentro do corpo humano".into(),
            template: "neuroanatomia".into(),
            grid: GridSpec {
                width: 40,
                height: 20,
                cell_size: 16,
            },
            projection: Projection::TwoDGrid,
            layers: vec![
                Layer::background("corpo", "Silhueta", 1.0, Some(silhueta)),
                Layer::background("snc", "Sistema Nervoso Central", 0.5, Some(cns)),
                blocks_layer,
            ],
            connections: vec![Connection {
                id: "c1".into(),
                from: "cortex".into(),
                to: "medula".into(),
                label: Some("Trato corticoespinhal".into()),
                directed: true,
                props: BTreeMap::new(),
            }],
            published: false,
            meta: BTreeMap::new(),
            created_at: fixed,
            updated_at: fixed,
        }
    }

    #[test]
    fn round_trip_preserva_instancia() {
        let original = sample_neuroanatomia();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let restored: UniverseInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn projecao_serializa_snake_case() {
        assert_eq!(
            serde_json::to_string(&Projection::TwoDGrid).unwrap(),
            "\"two_d_grid\""
        );
        assert_eq!(
            serde_json::to_string(&Projection::Isometric).unwrap(),
            "\"isometric\""
        );
        assert_eq!(
            serde_json::to_string(&Projection::Timeline).unwrap(),
            "\"timeline\""
        );
    }

    #[test]
    fn layer_kind_serializa_snake_case() {
        assert_eq!(
            serde_json::to_string(&LayerKind::Background).unwrap(),
            "\"background\""
        );
        assert_eq!(
            serde_json::to_string(&LayerKind::Blocks).unwrap(),
            "\"blocks\""
        );
    }

    #[test]
    fn attachment_kind_from_mime() {
        assert_eq!(
            AttachmentKind::from_mime("image/svg+xml"),
            Some(AttachmentKind::Image)
        );
        assert_eq!(
            AttachmentKind::from_mime("application/pdf"),
            Some(AttachmentKind::Pdf)
        );
        assert_eq!(
            AttachmentKind::from_mime("audio/mpeg"),
            Some(AttachmentKind::Sound)
        );
        assert_eq!(AttachmentKind::from_mime("application/x-evil"), None);
    }

    #[test]
    fn campos_default_omitidos_no_json() {
        let inst = UniverseInstance::empty("x", "o", "t");
        let json = serde_json::to_string(&inst).unwrap();
        // meta vazio é omitido
        assert!(!json.contains("\"meta\""));
    }

    #[test]
    fn deserializa_instancia_v1() {
        // Fixture gravado sob SCHEMA_VERSION=1 (sem props especiais nos blocos).
        // Deve carregar sem erro sob v2 — a migração forward é no-op.
        let v1 = r#"{
            "id": "i", "schema_version": 1, "owner": "o", "title": "T",
            "grid": { "width": 40, "height": 20, "cell_size": 16 },
            "projection": "two_d_grid",
            "layers": [ { "id": "base", "name": "Base", "kind": "blocks", "blocks": [
                { "id": "b1", "block_type": "note", "pos": { "x": 1, "y": 1 } }
            ] } ],
            "connections": [],
            "created_at": "2026-05-29T12:00:00Z",
            "updated_at": "2026-05-29T12:00:00Z"
        }"#;
        let inst: UniverseInstance = serde_json::from_str(v1).unwrap();
        assert_eq!(inst.schema_version, 1, "campo cru preservado na leitura");
        assert_eq!(inst.id, "i");
        assert_eq!(inst.layers[0].blocks[0].id, "b1");
        // Reserializa na versão corrente só quando explicitamente carimbado.
        // v3 = Projection::Timeline (YG-123) — variante aditiva, leitura de
        // v1/v2 segue intacta (serde default não exige a variante nova).
        assert_eq!(SCHEMA_VERSION, 3);
    }
}
