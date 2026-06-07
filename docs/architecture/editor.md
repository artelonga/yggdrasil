# Arquitetura — Editor de universos data-driven (Sims/Paralives)

> Epic **YG-73**. Documento vivo: a fonte da verdade do formato e do contrato REST
> é este arquivo. Atualizar junto com as stories YG-74…YG-81.

## Problema

Hoje todo universo é **código**: um crate Rust em `universes/universe-*` compilado
para WASM e embutido via `include_bytes!` em `yggdrasil-web/src/wasm_runtime.rs`.
Os dois front-ends (cliente Godot 4.5 em `yggdrasil-godot/` e o web canvas em
`yggdrasil-web/static/`) são **players finos** — não há grid editável, projeção
isométrica, tilemap, nem caminho para um usuário criar/editar um universo sem
escrever um crate.

Queremos um editor estilo **The Sims / Paralives**: colocar blocos numa grade
2D/isométrica para **gerar** novos universos e **editar** os próprios, anexando
conteúdo multiformato (arquivos, metadata, analytics, PDF/imagem/som — reusando
a compatibilidade de arquivos do `co`) a cada bloco. Primeiro caso concreto:
**neuroanatomia** (silhueta do corpo + overlay do SNC com toggle de transparência,
landmarks anatômicos e conexões entre eles, a partir de material open-source).

## Princípios de design (decisões tomadas)

1. **Formato data-driven host-neutral + REST API primeiro.** A fronteira estável
   é o JSON + os endpoints, não o front-end. Escolher Godot vs web como host do
   editor é uma fase posterior — ambos consomem o mesmo contrato REST.
2. **Editor genérico primeiro.** Neuroanatomia é o primeiro **template** autorado
   com ele, não um vertical bespoke.
3. **Aditivo.** Zero mudanças no runtime WASM, no `universe-sdk`, nos routers de
   arcade ou no `default_registry()` hardcoded. O editor coexiste com a interface
   de jogo atual.
4. **Reuso > reescrita** (CLAUDE.md): reaproveitar `co/core` para I/O de conteúdo;
   não reescrever o que já existe no `co`.

## Conceito central — `UniverseInstance` é paralelo, não `UniverseKind`

O grafo `UniverseNode`/`UniverseKind` (`yggdrasil-core/src/universes.rs`) é o
**catálogo de engines compiladas** — todo nó resolve para um crate WASM + params.
Uma instância autorada por usuário **não tem engine**: é dado puro renderizado por
um player genérico. Forçá-la em `UniverseKind` poluiria o catálogo com milhares de
linhas de usuário e quebraria a invariante "todo nó → um crate WASM".

Logo: novo módulo `yggdrasil-core/src/instance/` paralelo ao registry. Instâncias
publicadas, na v1, aparecem num feed separado (`?published=true`), **não** no
`GET /api/v1/universos` — que continua hardcoded e intocado.

## Formato (schema `schema_version = 1`)

Módulo `yggdrasil-core/src/instance/schema.rs`. Sparse (blocos por coordenada, não
matriz densa — grades autoradas são grandes e majoritariamente vazias). Diferente
de `UniverseNode`, `UniverseInstance` deriva `Deserialize`.

```rust
pub struct UniverseInstance {
    pub id: String,                 // nanoid, também o nome do dir em disco
    pub schema_version: u32,        // = 1; gate de migrações futuras
    pub owner: String,              // `sub` do JWT do autor
    pub title: String,
    pub description: String,
    pub template: String,           // slug do template-semente, ex. "neuroanatomia"
    pub grid: GridSpec,             // { width, height, cell_size }
    pub projection: Projection,     // TwoDGrid | Isometric
    pub layers: Vec<Layer>,         // z-ordenadas, índice 0 = base
    pub connections: Vec<Connection>,
    pub meta: BTreeMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum Projection { TwoDGrid, Isometric }   // começa em TwoDGrid; iso depois

pub struct Layer {
    pub id: String,
    pub name: String,
    pub kind: LayerKind,            // Background | Blocks | Annotation
    pub visible: bool,
    pub opacity: f32,               // 0.0..=1.0 — o toggle de transparência
    pub background: Option<ContentRef>,  // ex. silhueta do corpo / overlay do SNC
    pub blocks: Vec<Block>,
}

pub struct Block {
    pub id: String,                 // estável; referenciado por Connection
    pub block_type: String,         // chave de paleta: "landmark", "note", "portal"
    pub pos: Cell,                  // coords de grade (não pixels)
    pub label: Option<String>,
    pub attachments: Vec<ContentRef>,
    pub props: BTreeMap<String, serde_json::Value>,  // cor, ícone, atribuição/licença
}

pub struct Cell { pub x: u32, pub y: u32 }

pub struct Connection {
    pub id: String,
    pub from: String,               // Block.id
    pub to: String,                 // Block.id
    pub label: Option<String>,
    pub directed: bool,
    pub props: BTreeMap<String, serde_json::Value>,
}

/// Referência content-addressed a um anexo. Bytes em _blobs/<shard>/<hash>.
pub struct ContentRef {
    pub kind: AttachmentKind,       // Pdf | Image | Sound | Markdown | Metadata
    pub hash: String,               // sha-256 hex
    pub filename: String,           // nome original, p/ Content-Disposition
    pub mime: String,
    pub size: u64,
}
```

Relação com `game-core::Map`/`Tile`: uma `Layer{kind:Blocks}` é o equivalente
generalizado de um `Map`, mas multicamada e content-bearing. **Não estender**
`Map`/`Tile` (single-layer, char-display, gameplay-oriented).

## Anexos de conteúdo (reuso do `co`)

- Reusar `co/core/src/storage.rs` (`ContentStore`, escrita atômica temp+rename) e
  o padrão SHA-256 de `co/core/src/entry.rs`. `yggdrasil-core` ainda **não** depende
  do `co/core` (só de `game-core`) → adicionar dep por path e pinar. Adicionar
  `write_bytes`/`read_bytes` ao `ContentStore` (hoje só `&str`).
- Bytes **content-addressed por SHA-256**: `data/instances/_blobs/<shard>/<hash>`
  (prefixo de 2 chars; dedupe automático). Bloco referencia por **hash**, não path
  → mover/renomear instância nunca quebra link; serve é lookup de hash.
- `Cache-Control: immutable` é seguro (conteúdo é endereçado pelo próprio hash).
- Allowlist de MIME por `AttachmentKind` + cap de tamanho (config).

## REST API (host-neutral)

Novo `yggdrasil-web/src/api/instances.rs`, um `instances_router` `.merge`d em
`main.rs`, espelhando o padrão de `api/universes.rs`. Auth via `sub` do JWT.

| Método | Rota | Função |
|---|---|---|
| POST | `/api/v1/instances` (`?template=`) | criar (opc. semear de template) |
| GET | `/api/v1/instances` | listar do caller (`?published=true` p/ feed público) |
| GET | `/api/v1/instances/{id}` | carregar JSON completo |
| PUT | `/api/v1/instances/{id}` | salvar instância inteira |
| PATCH | `/api/v1/instances/{id}` | aplicar `EditOp` granular |
| DELETE | `/api/v1/instances/{id}` | apagar + GC de blobs órfãos |
| POST | `/api/v1/instances/{id}/attachments` | upload multipart → `ContentRef` |
| GET | `/api/v1/instances/{id}/attachments/{hash}` | servir bytes |
| GET | `/api/v1/instances/{id}/play` | view do player |
| GET | `/api/v1/templates` | listar templates |
| GET | `/api/v1/templates/{slug}` | detalhe (seed + palette + render_hints) |

`EditOp` é enum serde `#[serde(tag="op", rename_all="snake_case")]` — ponto único
de validação no servidor:

```rust
pub enum EditOp {
    PlaceBlock { layer: String, block: Block },
    MoveBlock { layer: String, block_id: String, to: Cell },
    DeleteBlock { layer: String, block_id: String },
    EditLayer { layer: String, visible: Option<bool>, opacity: Option<f32>, name: Option<String> },
    AddLayer { layer: Layer },
    AddConnection { connection: Connection },
    DeleteConnection { connection_id: String },
    AttachContent { layer: String, block_id: String, content: ContentRef },
}
```

Validações: rejeitar `Connection` com endpoint inexistente, `Cell` fora dos limites
do `GridSpec`, `opacity` fora de `0.0..=1.0`, `block_id` duplicado.

## Persistência (disco = verdade + índice SQLite fino)

Espelha a divisão do repo e do `co`: SQLite para dados **derivados/consultáveis**,
arquivos para conteúdo **autorado**.

```
data/instances/                      # YGGDRASIL_INSTANCES_DIR (default)
  <instance_id>/
    instance.json                    # UniverseInstance (escrita atômica)
  _blobs/<shard>/<hash>              # anexos content-addressed, compartilhados
```

`InstanceStore` (`yggdrasil-core/src/instance/store.rs`) embrulha um `ContentStore`
do `co`. Índice SQLite no DB de `YGGDRASIL_DB` (mesmo de scores/telemetria):

```sql
CREATE TABLE IF NOT EXISTS instances (
  id TEXT PRIMARY KEY, owner TEXT NOT NULL, title TEXT NOT NULL,
  template TEXT, published INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_instances_owner ON instances(owner);
```

`reindex()` no boot reconstrói o índice do disco (índice descartável, princípio do
`co`). Criada via `execute_batch` como em `telemetria::init_telemetry_db`.

## Runtime/serving — render client-side puro (sem WASM)

Universos de arcade têm simulação por tick → WASM. Uma instância autorada é dado
espacial **estático** com toggles (opacidade, pan/zoom, clicar landmark → abre
anexo) — isso é **renderização, não tick**. Logo, **nenhum** módulo WASM
"instance-player".

- `GET /api/v1/instances/{id}` devolve JSON; nova página estática
  `yggdrasil-web/static/universos/instance.html` (player canvas/SVG genérico) busca
  e renderiza. Servida por `serve_instance_player` em `main.rs` espelhando
  `serve_snake` (`Html(include_str!())`), em `/universos/instance/{id}`.
- Player: trata ambas projeções (iso: `screen = ((x−y)·w/2, (x+y)·h/2)`), z-ordena
  layers, aplica `opacity` por layer, desenha `connections` como arestas, e ao
  clicar num bloco abre `attachments` (imagem inline, PDF em viewer, som via
  `<audio>`, markdown renderizado).
- O **editor** é o mesmo player + paleta + affordances de edição falando com os
  endpoints PATCH/upload.

## Templates

`yggdrasil-core/src/instance/template.rs`. Mecanicamente três coisas:

```rust
pub struct Template {
    pub slug: String, pub title: String, pub description: String,
    pub seed: UniverseInstance,    // layers/projeção/grid/backgrounds pré-criados
    pub palette: Vec<PaletteItem>, // block_types permitidos + props default + schema opc.
    pub render_hints: serde_json::Value,
}
```

`PaletteItem` pode reusar `co/core/src/manifest.rs` (`ContentType`/`FieldType`)
para validar `props`/metadata estruturada por `block_type` — defer para depois da v1.

### Template neuroanatomia (zero adições de schema)

- **Silhueta do corpo** = `Layer{kind:Background, opacity:1.0, background:Image}`.
- **Overlay do SNC** = segundo `Layer{kind:Background, opacity:0.5}` empilhado; o
  toggle de transparência é um controle ligado a `layer.opacity` (PATCH
  `EditLayer{opacity}` para persistir, ou client-only para view).
- **Landmarks** = `Block{block_type:"landmark", pos, label, attachments}` numa layer
  `Blocks` (cada um carrega PDF/imagem/som/markdown).
- **Conexões neurais** = `Connection{from, to, directed, label}` renderizadas como
  linhas/setas entre centros de bloco.

## Fontes open-source de anatomia

**2D / SVG / figuras (uso imediato — silhueta + overlay SNC + landmarks):**

| Fonte | Licença | Nota |
|---|---|---|
| [OpenStax Anatomy & Physiology 2e](https://openstax.org/books/anatomy-and-physiology-2e/pages/12-introduction) ([fig.12.2 via AnatomyTOOL](https://anatomytool.org/content/openstax-anatphys-fig122-overview-nervous-system-english-labels)) | CC-BY 4.0 | Cap. de sistema nervoso; atribuição obrigatória; uso comercial requer contato |
| [AnatomyTOOL.org](https://anatomytool.org/content/best-open-anatomy-learning-resources) | por item | Agregador de material anatômico aberto |
| [Wikimedia Commons — SVG human anatomy](https://commons.wikimedia.org/wiki/Category:SVG_human_anatomy) ([organs.svg](https://commons.wikimedia.org/wiki/File:202403_human_anatomy_organs.svg)) | PD / CC-BY-SA | Verificar por arquivo |
| [FreeSVG.org](https://freesvg.org/organs-of-the-human-body) · [SVG Silh](https://svgsilh.com/tag/anatomy-1.html) | **CC0** | Sem atribuição — começar por aqui |

**Conexões / tratos (fase posterior, para as `Connection`):**

| Fonte | Nota |
|---|---|
| [SlicerDMRI white matter atlas](https://dmri.slicer.org/atlases/) | 58 tratos profundos + clusters (Human Connectome Project) |
| [IIT Human Brain Atlas v5.0](https://www.nitrc.org/projects/iit) | Tractograma/conectoma + labels (NITRC) |

**Volumétrico / 3D (futuro, se evoluir para iso/3D — não fase 1):**

| Fonte | Licença |
|---|---|
| [The Extremely Brilliant Brain (EBB)](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12874060/) | CC-BY 4.0 (OME-Zarr/NIfTI) |
| [Visible Brain Atlas](https://www.anatomylibrary.org/3d-models/visible-brain-atlas) · Waxholm (rato) | open / CC-BY |

> **Licenciamento:** começar com **CC0** (FreeSVG/SVG Silh) evita fricção de
> atribuição. OpenStax/Wikimedia exigem crédito → `ContentRef`/`Block.props` deve
> carregar campo de atribuição/licença, e o player deve exibir o crédito. YG-80
> inclui esse modelo de atribuição.

## Decisões em aberto

1. **Dep do `co/core`:** adicionar por path e pinar (recomendado, alinhado ao
   CLAUDE.md) vs portar ~120 linhas de `ContentStore`+frontmatter. → adicionar dep
   (pré-requisito de YG-75).
2. **Publish → catálogo:** instâncias publicadas em `GET /api/v1/universos` (exige
   `UniverseRegistry` mutável + `UniverseNode: Deserialize`) vs feed separado
   `?published=true`. → **feed separado na v1**; revisitar depois.

## Mapa de stories

| ID | Título | blocked_by |
|---|---|---|
| YG-73 | Epic: Editor de universos data-driven | — |
| YG-74 | Schema `UniverseInstance` + módulo `instance/` | 73 |
| YG-75 | `InstanceStore` (disco co-style + índice SQLite) | 74 |
| YG-76 | REST CRUD de instâncias + PATCH `EditOp` | 75 |
| YG-77 | Anexos content-addressed (upload + serve) | 75 |
| YG-78 | Player genérico client-side | 76, 77 |
| YG-79 | Mecânica de templates + endpoints | 76 |
| YG-80 | Template neuroanatomia | 78, 79 |
| YG-81 | Modo edição no player + paleta | 78, 79 |

Grafo: 73→74→75→{76,77}; {76,77}→78; 76→79; {78,79}→80; {78,79}→81.
</content>
