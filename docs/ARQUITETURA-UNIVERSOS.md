# Arquitetura: cada jogo é um universo

> Como Snake, Tetris, Invaders e Pôquer convivem no mesmo workspace sem
> virarem um pacote monolítico — e o que isso implica em commits, versões
> e dependências.

## Princípio

Um **universo** é uma unidade auto-contida de jogabilidade que vive lado a
lado com os outros universos no lobby, mas pode evoluir, falhar ou ser
descartada sem afetar os demais. Não é apenas uma feature: é um módulo com
fronteiras explícitas em três camadas.

## Camadas obrigatórias

Para um universo `<slug>` (`snake`, `tetris`, `invaders`, `poker`, …):

| Camada | Caminho | Responsabilidade | Pode depender de |
|---|---|---|---|
| Domínio | `yggdrasil-core/src/games/<slug>.rs` | Estado, regras, transições | `game_core::*`, `yggdrasil_core::sementes` |
| HTTP | `yggdrasil-web/src/games/<slug>_routes.rs` | Endpoints Axum, autenticação | sua camada de domínio, `crate::auth` |
| Frontend | `yggdrasil-web/static/games/<slug>.{html,js}` | UI canvas / DOM | só as rotas HTTP do próprio universo |
| Portal | `yggdrasil-core/src/lobby.rs::{slug, pos}` | Identificador + posição no mapa | nada |

Um universo **não** importa outro universo. Pôquer não conhece Snake e
vice-versa. A única ponte é o lobby, que despacha por slug.

## Diagrama de dependências

```
                ┌────────────────────────────┐
                │     game_core (engine)     │
                │  Universe, Tile, Wallet,   │
                │  Plugin, MailProvider, …   │
                └──────────────┬─────────────┘
                               │
                ┌──────────────▼─────────────┐
                │      yggdrasil-core        │
                │  ┌──────────────────────┐  │
                │  │ sementes (fachada)   │  │
                │  └──────────────────────┘  │
                │  ┌──────────────────────┐  │
                │  │ lobby (slug + pos)   │  │
                │  └──────────────────────┘  │
                │  ┌──────┬──────┬──────┐    │
                │  │snake │tetris│poker │ …  │  ← universos paralelos
                │  └──────┴──────┴──────┘    │
                └──────────────┬─────────────┘
                               │
                ┌──────────────▼─────────────┐
                │      yggdrasil-web         │
                │  routes/<slug>_routes.rs   │
                │  static/games/<slug>.{html,js} │
                └────────────────────────────┘
```

## Convenção de commits

Conventional Commits, mas com **dois escopos válidos** dependendo do tipo
de mudança:

### Escopo por universo (mudança local)

Quando a mudança toca **apenas** os arquivos de um universo:

```
feat(poker): mesa multiplayer com bot fallback
fix(snake): respeitar bordas em mapas largos
refactor(tetris): extrair Tetromino para tipo dedicado
```

### Escopo por tarefa YG-N (work-driven, padrão atual)

Quando a mudança é prevista por uma user-story do `work/yggdrasil/`:

```
feat(YG-9): adapter Poker com sementes via WalletManager
fix(YG-5): mouse — click em tile move e auto-entra portal
```

### Combinação (recomendada para tarefas que afetam um universo)

Mais informativo, sem ambiguidade:

```
feat(YG-25, poker): dealing + betting rounds
feat(YG-26, poker): bot toma ação aleatória legal
chore(YG-30, poker): release v0.8.0 — pôquer multiplayer
```

### Mudanças cross-cutting

Para infra, auth, lobby, deploy — não use slug de universo:

```
feat(lobby): pathfinding BFS sem limite hardcoded
fix(auth): JWT expiry com Validation explícita
chore(deploy): Dockerfile com glibc 2.40
```

## SemVer

### Hoje (workspace unificado)

O workspace publica uma versão única em `Cargo.toml::workspace.package`.
Bump segue a regra do `CLAUDE.md` raiz:

| Tipo do commit | Bump |
|---|---|
| `feat` | minor (0.X.0) |
| `fix`/`refactor`/`docs`/`chore` | patch (0.0.X) |
| Release marcador (YG-18/19/20/30) | major/minor explícito |

A versão **maior** entre as mudanças do release vence. Ex.: se a release
inclui `feat(poker)` + `fix(snake)`, é minor.

### Futuro (universos como crates independentes)

Quando um universo amadurecer (estável, com API documentada), pode ser
extraído para crate próprio:

```toml
# yggdrasil-core/Cargo.toml passaria a depender de:
ygg-poker = { path = "../universes/poker", version = "0.1" }
ygg-snake = { path = "../universes/snake", version = "0.3" }
```

Cada crate evolui em seu próprio SemVer. O workspace cumpre o papel de
"distribuição" — versão do workspace = `max(universes) + lobby + infra`.

**Critério para extrair**: um universo só sai para crate próprio quando:
1. Sua API com `yggdrasil-core` está congelada por pelo menos 1 minor.
2. Tem suíte de testes que passa isolada.
3. Tem documentação `README.md` no crate.

Antes disso, mora em `yggdrasil-core/src/games/<slug>.rs`.

## Plugin path (terceiros)

`game_core::plugin::Plugin` + `PluginManifest` já existem (YG-15 planeja
o loader runtime). Quando o loader estiver pronto, universos de terceiros
seguem o mesmo contrato das camadas obrigatórias, mas carregados em runtime
em vez de compilados estaticamente. A convenção de slug, commit scope e
SemVer continua valendo.

## Anti-padrões

❌ **Universo importando outro universo.**
`yggdrasil_core::games::poker::*` em `snake.rs` é proibido. Se há código
comum, ele sobe para `yggdrasil_core::sementes`, `game_core::*`, ou um
módulo `yggdrasil_core::games::common::*` dedicado.

❌ **Commit cobrindo dois universos sem escopo claro.**
`feat: poker + snake fixes` perde rastreabilidade. Quebrar em dois commits
ou usar escopo explícito: `feat(poker, snake): …` (raro, mas válido).

❌ **Frontend de um universo chamando rotas de outro.**
`poker.js` não fala com `/api/v1/games/snake/*`. Se precisar, é sinal de
que existe um conceito que pertence ao lobby ou ao módulo de sementes.

❌ **Lógica de domínio em `*_routes.rs`.**
Rotas são tradutoras: deserializam JSON, chamam a camada de domínio,
serializam a resposta. Toda a regra mora em `yggdrasil-core/src/games/`.

## Grafo de universos — composição sobre herança

> Adicionado na rota do node-graph (YG-N). Cada universo é um **nó** num
> grafo recursivo: roots, variantes e composições, formando uma árvore
> queryable via `GET /api/v1/universes`.

### Modelo

```
Root        →  define o engine (`tetris`, `snake`, `invaders`, `poker`)
Variant     →  mesmo engine + parâmetros diferentes (`tetris/sprint-40`)
Composition →  combina universos (`tetris/duel` = dois `tetris/classic`)
```

Variantes **não estendem** o root — instanciam o engine raiz com overrides
de parâmetros. Não há herança de código: `snake/walls` reusa a mesma rota
`POST /api/v1/games/snake/...` do `snake`, mas o servidor lê o parâmetro
`map=walled` da variante e inicia com obstáculos.

### Forma do nó

```rust
struct UniverseNode {
    slug: String,                 // "tetris/sprint-40"
    parent: Option<String>,       // "tetris" (None para roots)
    children: Vec<String>,        // sub-universos diretos
    kind: UniverseKind,           // Root | Variant | Composition
    title: String,                // "Tetris Sprint 40"
    description: String,          // PT-BR
    parameters: BTreeMap<String, Value>, // {"lines_to_clear": 40, "mode": "sprint"}
    api: ApiContract,             // { start, input, page }
}
```

### Contrato HTTP

```
GET /api/v1/universes              → todos os nós (lista plana)
GET /api/v1/universes/{slug}       → um nó com pai + filhos
GET /api/v1/universes/graph        → { nodes, edges } para visualizadores
```

Todos públicos (sem auth). O slug pode conter `/` — endpoint usa
wildcard `{*slug}` no Axum.

### Forward-compat com Godot (YG-31..YG-35)

A árvore de universos corresponde literalmente a uma árvore de **Godot
scenes**:

| Yggdrasil hoje (Rust)      | Godot amanhã (POC YG-31..YG-35) |
|---|---|
| `UniverseNode { kind: Root }` | `Tetris.tscn` — scene principal |
| `UniverseNode { kind: Variant }` | `TetrisSprint40.tscn` — sub-scene que `.instance()` da root e seta props |
| `UniverseNode { kind: Composition }` | `TetrisDuel.tscn` — scene que instancia dois `TetrisClassic.tscn` lado a lado |
| `parameters` (JSON) | `@export` props do Godot |
| Edges `parent → child` | `PackedScene.instantiate()` calls em tempo de boot |

A API HTTP `/api/v1/universes` permanece — clientes web (canvas) e Godot
consomem o mesmo grafo. Quando a POC Godot fechar a decisão, a registry
em `yggdrasil_core::universes` se mantém como source-of-truth do shape.

### Exemplo: árvore atual

```
snake
├── snake/classic       (Variant, difficulty=medium)
└── snake/walls         (Variant, map=walls, difficulty=hard)
tetris
├── tetris/classic      (Variant, mode=marathon)
└── tetris/sprint-40    (Variant, mode=sprint, lines_to_clear=40)
invaders
├── invaders/classic    (Variant, difficulty=medium)
└── invaders/swarm      (Variant, difficulty=hard, lives=1)
poker
├── poker/cash-game     (Variant, buy_in=1000, max_seats=6)
└── poker/heads-up      (Variant, max_seats=2)
```

Adicionar um universo novo é instanciar `UniverseNode` no
`yggdrasil_core::universes::default_registry()` — uma linha por nó. Sem
necessidade de criar arquivos novos por variante: o engine raiz lê a
parameterização em runtime.

### Quando promover uma variante a root?

Quando a variante diverge tanto que **reutilizar o engine raiz custa mais
que reescrever**. Exemplos:

- ✅ Variante: `tetris/sprint-40` (apenas parâmetro `lines_to_clear`).
- ⚠️ Limite: `tetris/4-player-versus` (lógica de turno + chat — talvez root próprio).
- ❌ Root próprio: `chess` (engine sem qualquer overlap com tetris).

A regra: se ≥ 80% do código do engine é reusado, mantenha como variante.
Caso contrário, novo root + caminho `/api/v1/games/<novo-slug>/...`.

## Inventário atual

| Universo | Domínio | HTTP | Frontend | Status |
|---|---|---|---|---|
| snake | `games/snake.rs` | `snake_routes.rs` | `snake.{html,js}` | ✅ jogável |
| tetris | `games/tetris.rs` | `tetris_routes.rs` | `tetris.{html,js}` | ✅ jogável |
| invaders | `games/invaders.rs` | `invaders_routes.rs` | `invaders.{html,js}` | ✅ jogável |
| poker | `games/poker.rs` (single-player) + `games/poker_lobby.rs` (multi) | `poker_routes.rs` | `poker.{html,js}` | 🚧 seating only — gameplay em YG-25 |
