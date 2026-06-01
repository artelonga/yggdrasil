# Yggdrasil — Roadmap

> Versão por versão, do estado atual (v1.0.0, shipado em 2026-05-20) até
> v2.0. Foco em **comportamento esperado por release** — o que o visitante
> consegue fazer em cada versão.

Última atualização: 2026-05-23.

## Princípios de versionamento

### Core (`/Cargo.toml::workspace.package.version`)

| Tipo de mudança | Bump | Exemplo |
|---|---|---|
| `feat(<escopo>):` cross-cutting (plataforma, API, runtime) | **minor** | `feat(catalog): filtros dinâmicos` |
| `fix(<escopo>):` correção comportamental | **patch** | `fix(telemetria): timezone do session_complete` |
| `refactor(<escopo>):` invisível ao usuário | **patch** | `refactor(types): drop serde_json::Value` |
| Bump de universo embedado | **patch** | Rebuild WASM, sem mudança de API |
| Mudança no `universe-sdk` ABI | **major** | Todas as universes recompilam |
| Mudança no contrato `/api/v1/*` (não-aditiva) | **major** | Quebra clientes existentes |
| Aditiva ao contrato `/api/v1/*` (novo campo opcional) | **minor** | YG-70 catálogo |

### Universos (`/universes/universe-<slug>/Cargo.toml`)

A partir do **YG-63 epic**, cada universo tem SemVer próprio. Regras de
bump por universo (independentes do core — ver `docs/UNIVERSE-VERSIONING.md`
após YG-67):

| Tipo de mudança no universo | Bump no universo | Bump no core |
|---|---|---|
| Novo nível/conteúdo, nova capability | **minor** universe | **patch** core |
| Bug fix, ajuste de balanceamento, visual | **patch** universe | **patch** core |
| Quebra de input/output JSON ou manifest | **major** universe | **minor** core |
| Conteúdo SRD novo (Shandara: aventura, povo, regra) | **minor** universe | **patch** core |

## Estado atual: v1.0.0 (2026-05-20)

**Universe Platform v1.0** — entregue via YG-54 epic (8 user stories).

### O que funciona hoje

- 6 universes embedados (WASM, ABI v1, fuel-limited 10M instr/tick):
  snake, tetris, invaders, pointset, poker, vim
- API unificada `GET /api/v1/universos` + session CRUD + WebSocket
- Legacy `/api/v1/games/{game}/start` preservados (backwards-compat)
- Universo Vim com hints adaptativos via Claude API
- OpenAPI 3.x em `/openapi.json` + `/openapi.yaml`
- Telemetria (`funnel_events`, `session_records`) + `/api/v1/admin/analytics`
- CI/CD com build pipeline + size budgets (2 MB total)
- Single self-contained binary deployado em
  https://yggdrasil-artelonga.fly.dev

### Pendências conhecidas

- **Pôquer:** bot AI deadlock (YG-26 marcado done mas comportamento incompleto)
- **Universes versioning:** todos travados em "0.1.0" hardcoded (universe-vim
  herda 1.0.0 silenciosamente do workspace) — resolvido pelo YG-63 epic

---

## v1.1.0 — Per-universe versioning (target: 1-2 semanas)

**Tema:** universes ganham vida própria. Cada uma com SemVer, CHANGELOG,
tag git. Manifest WASM expõe versão real, visível no catálogo.

### Tasks (YG-63 epic)

- **YG-64** — Padronizar Cargo.toml por universe (version + description próprios)
- **YG-65** — CHANGELOG.md por universe (Keep a Changelog PT-BR)
- **YG-66** — `universe_sdk::pkg_version!()` macro propaga `CARGO_PKG_VERSION` para o manifest WASM
- **YG-67** — `docs/UNIVERSE-VERSIONING.md` + convenção de tags `universe-<nome>-v<X.Y.Z>`

### Comportamento esperado pós-release

```bash
$ curl https://yggdrasil-artelonga.fly.dev/api/v1/universos
{
  "universos": [
    { "id": "snake", "version": "1.0.0", ... },  # vindo de Cargo.toml real
    { "id": "poker", "version": "0.8.0", ... },  # versão honesta (multiplayer incompleto)
    { "id": "vim", "version": "1.0.0", ... },
    ...
  ]
}

$ git tag --list 'universe-*-v*'
universe-snake-v1.0.0
universe-poker-v0.8.0
universe-vim-v1.0.0
...

$ ls universes/universe-poker/
Cargo.toml  CHANGELOG.md  src/  README.md
```

---

## v1.2.0 — Catálogo + Shandara + tooling jj-compatible (target: 3-4 semanas)

**Tema:** Yggdrasil deixa de ser "lobby de 6 jogos" e vira "catálogo de
universos abertos". Primeiro universo não-arcade (Shandara, RPG SRD aberto).
Tooling para tratar cada universo como módulo independente sem usar
submodules.

### Tasks

- **YG-69** — `universe-shandara` v0.1.0: SRD CC-BY-SA do mundo Shandara
  (6 forças primordiais, Grande Guerra, 2 povos, criação de personagem,
  atributos). Conteúdo markdown embedado via `include_str!`.
- **YG-70** — `REGISTRY.yaml` + filtros dinâmicos + placeholders. API
  retorna embedded + planned + external; UI ganha página `/catalog` com
  filtros (genre, origin, license, status).
- **YG-71** — `scripts/universe-changelog.sh` gera CHANGELOG por universe
  via `git log -- universes/universe-X/` (jj-compatible, sem submodules).
- **YG-72** — Seed do REGISTRY com ~40 RPGs brasileiros de
  `docs/RPGs Brasileiros.md` como placeholders.

### Comportamento esperado pós-release

```bash
$ curl 'https://yggdrasil-artelonga.fly.dev/api/v1/universos?origin=brazilian'
{ "universos": [ ...37 entradas... ], "total": 37 }

$ curl 'https://yggdrasil-artelonga.fly.dev/api/v1/universos?status=embedded&type=rpg'
{ "universos": [ { "slug": "shandara", "version": "0.1.0", ... } ], "total": 1 }

# UI: /catalog renderiza cards com badges 🟢 jogável / 🟡 planejado / 🔗 externo

$ bash scripts/universe-changelog.sh shandara --bump minor
==> Bumping universes/universe-shandara from 0.1.0 → 0.2.0
    Adding 4 commits to CHANGELOG.md
    Created tag: universe-shandara-v0.2.0
```

**Visitante:** abre `/catalog`, vê 7 universos jogáveis + 35+ planejados
(majoritariamente RPG brasileiro) + alguns externos. Filtra por
`origin: brazilian` e entende que esta plataforma é casa pra eles.

---

## v1.3.0 — Shandara expandido + ABI v2 (turn-based) (target: 1-2 meses)

**Tema:** mecânica de RPG ganha suporte de runtime. Shandara cresce de
conteúdo navegável para sistema jogável (rolagem de dados, criação de
personagem persistida, encontros).

### Tasks (a abrir)

- **YG-?** — `universe-shandara` v0.5.0: regras de combate + magias +
  bestiário com 20 criaturas
- **YG-?** — Extensão do `universe-sdk` para ABI v2 (turn-based):
  - Host import `roll_dice(spec: &str) -> Vec<u8>` (e.g. "3d6+2")
  - Estado de sessão persistido entre turnos via KV (já existe no ABI v1)
  - Ação discreta `tick(action)` em vez de loop tempo-real
- **YG-?** — `universe-shandara` v0.8.0: aventura introdutória "A Cicatriz
  da Guerra" jogável end-to-end (criação de personagem → 3 encontros →
  desfecho)
- **YG-?** — i18n: descrições do REGISTRY em PT + EN (campo
  `description: { pt: ..., en: ... }`)

### Comportamento esperado pós-release

Visitante cria personagem em Shandara (escolhe povo, distribui atributos,
escolhe vínculo com uma das forças primordiais), começa a aventura,
recebe descrições narrativas com Claude, rola dados via host import,
ganha XP, salva progresso (sessão persistida no KV).

---

## v1.4.0 — Primeiro port de RPG brasileiro (target: 2-3 meses)

**Tema:** mostra que o caminho de "RPG aberto BR → universo Yggdrasil"
funciona com sistema de terceiros. Tagmar é o primeiro candidato (open
source desde 1991, comunidade ativa).

### Tasks (a abrir)

- **YG-?** — Contato + acordo com mantenedores de Tagmar
- **YG-?** — `universe-tagmar` v0.1.0: SRD em markdown embedado + ABI v2
  turn-based reutilizando Shandara
- **YG-?** — `docs/PORT-GUIDE.md`: passo-a-passo para portar outro RPG
  aberto BR (template baseado na experiência Tagmar)

### Comportamento esperado pós-release

Catálogo agora tem 2 RPGs jogáveis (Shandara + Tagmar), Tagmar com
versão própria evoluindo independente. Outros mantenedores podem usar
`docs/PORT-GUIDE.md` para portar seu sistema.

---

## v1.5.0 — Godot client 2D/3D + asset management (target: 3-4 meses)

**Tema:** universes ganham renderização rica via cliente Godot 4. Asset
management (sprites, modelos 3D, sons) integrado ao registry.

### Tasks (YG-32-35 ressuscitadas, hoje reapontadas para v1.5)

Estas 4 tasks foram criadas em 2026-05-12 com release alvo "v0.9.0" mas
nunca executadas — o trilho WASM (v1.0) pulou na frente. Permanecem
válidas como POC do trilho 2D/3D Godot, agora reapontadas para depois do
catálogo + universos não-arcade.

- **YG-32** — `Lobby.tscn` com 4 `GamePortal` nested scenes (composability
  básica em Godot 4)
- **YG-33** — Signal bus autoritativo + event-sourcing (server emite,
  cliente ouve; log replay-able)
- **YG-34** — Multiplayer JWT-validado: WebSocketMultiplayerPeer no Godot
  valida JWT emitido pela auth Rust, cliente conecta de qualquer browser
  logado
- **YG-35** — `PokerTable.tscn` E2E em Godot multiplayer + decisão
  formal: migrar canvas → Godot, manter ambos, ou arquivar (ADR-002)

### Asset pipeline (novo, complementar)

- **YG-?** — REGISTRY.yaml ganha campo `assets:` (cover image, video,
  modelos 3D opcionais por universe), servidos via CDN ou path local
- **YG-?** — Suporte a 3D rendering: universes podem declarar
  `capabilities: ["3d"]`; client Godot escolhe pipeline 3D

### Comportamento esperado pós-release

Visitante escolhe rodar Yggdrasil no browser (canvas atual) OU baixar
client Godot (desktop/Android/iOS) para experiência 3D. Mesma backend
(API, telemetria, auth), dois clientes. PokerTable jogável em qualquer
um. Decisão sobre migrar todos vs. coexistir documentada em ADR-002.

---

## v1.6.0 — Sementes integradas + recompensas por universe (target: 4-5 meses)

**Tema:** o sistema de sementes (já existe via `WalletManager` do
game-core) ganha integração visível no catálogo: cada universe declara
suas recompensas, jogadores ganham/gastam sementes cross-universe.

### Tasks (a abrir)

- **YG-?** — REGISTRY ganha `rewards:` (tipos de recompensas, condições)
- **YG-?** — UI do catálogo mostra "ganhe X sementes por completar Y"
- **YG-?** — Pôquer YG-26 retomado: bot AI funcional (atualmente
  deadlocka), permitindo cash games end-to-end

---

## v2.0.0 — Marketplace + user-uploaded universes (target: 6+ meses)

**Tema:** quebra de ABI maior. Universos deixam de ser embedados em
compile-time — passam a ser carregados via WebAssembly Component Model
(WASI Preview 2). Usuários autenticados podem fazer upload de universes
próprios para o catálogo, sujeitos a moderação.

### Tasks (a abrir)

- **YG-?** — Migração ABI v1 → Component Model
- **YG-?** — Upload endpoint + moderation queue
- **YG-?** — Cada universo extraído para repo próprio (`artelonga/universe-X`)
  via `git filter-repo` preservando 100% da história filtrada (viabilizada
  pelo YG-71 tooling)
- **YG-?** — `co-universes.yaml` listando todos os repos extraídos +
  versão pinada por release de plataforma

### Comportamento esperado pós-release

Visitante navega catálogo agora com 100+ universes (de mantenedores
diversos). Cria uma conta. Faz upload do próprio universe (compilado
para wasm32-wasi component model) via `POST /api/v1/universos/upload`.
Aprovado por moderador, vira card no catálogo, ganha SemVer próprio,
métricas independentes.

---

## Comportamento esperado por release (resumo)

| Versão | Visitante consegue… | Universos jogáveis | Total no catálogo |
|---|---|---|---|
| **1.0.0** (hoje) | Jogar 6 mini-games + Vim | 6 | 6 |
| **1.1.0** | Mesmo de 1.0 + versão real visível por universe | 6 | 6 |
| **1.2.0** | Navegar catálogo com filtros, ler Shandara SRD | 7 | ~45 |
| **1.3.0** | Criar personagem em Shandara, jogar aventura | 7 | ~45 |
| **1.4.0** | Jogar Tagmar (primeiro RPG BR portado) | 8 | ~45 |
| **1.5.0** | Mesmo de 1.4 com cliente Godot 2D/3D opcional | 8 | ~45 |
| **1.6.0** | Ganhar/gastar sementes cross-universe | 8 | ~45 |
| **2.0.0** | Upload de universos próprios | 100+ (com mods) | 100+ |

## Sobre os refactor epics YG-47/48/49 (já fechados em 2026-05-23)

Esses 3 epics existiram para zelar de dívida técnica acumulada entre v0.5
e v1.0:

- **YG-47** Cross-repo coupling: tirou `path = "../co/game-core"` →
  pinned git rev (CI clean checkout possível). Bump no core: **patch**
  (refactor invisível).
- **YG-48** Game adapter + multiplayer: promoveu `YggGame` a trait usada
  pelos 4 jogos single-player; introduziu event spine + WS para pôquer.
  Bump no core: **minor** (event spine é novo `feat`).
- **YG-49** State + types: tirou `serde_json::Value` do payload de jogo;
  segregou DB connections via `ScoresStore` trait; trimou `auth.rs` e
  `api/me.rs`. Bump no core: **patch** (refactors invisíveis).

**Impacto nos bumps futuros:** o padrão estabelecido aqui (refactor =
patch, novo feat = minor) é o que `universe-changelog.sh` (YG-71) vai
automatizar lendo escopo do Conventional Commit. Refactors em
`universe-X` não bumpam o core além de patch; só uma quebra de
universe-sdk ABI obriga major bump cross-cutting.

## Sobre o trilho Godot (YG-32-35)

O trilho Godot **não morreu** — foi pausado. As 4 user-stories continuam
válidas, hoje reapontadas para v1.5.0 (depois do catálogo + Shandara
existirem, mostrar rendering rico faz mais sentido). Estado atual:

- YG-32 (Lobby.tscn nested scenes) — pendente, demonstra composability
- YG-33 (Signal bus + event-sourcing) — pendente, base para multiplayer
  determinístico
- YG-34 (Multiplayer JWT) — pendente, mesma auth Rust para ambos clientes
- YG-35 (PokerTable E2E + ADR-002) — pendente, decisão formal sobre
  migrar/coexistir/arquivar

Quando v1.5 chegar, primeiro confirma se Godot ainda é o caminho certo
(podem ter surgido alternativas: Bevy WASM, Macroquad, Three.js + ECS).
A decisão fica em `docs/ADR-002` ao final do POC.

## Princípio operacional: monorepo agora, repos próprios depois

Universes vivem em `universes/universe-X/` no monorepo Yggdrasil até v2.x.
Cada um tem:

- `Cargo.toml` com versão própria
- `CHANGELOG.md` populado por `scripts/universe-changelog.sh` lendo git
  pathspec sobre o próprio diretório
- Tag git `universe-X-v<semver>`
- Card próprio no `REGISTRY.yaml`

Quando v2.x chegar e um universe estiver estável + popular o suficiente
pra justificar repo próprio:

```bash
git filter-repo \
  --path universes/universe-shandara \
  --path-rename universes/universe-shandara/:
```

…produz um repo novo com 100% da história filtrada, preservando autoria
e datas. Submodules NÃO entram nessa migração (peso operacional não
compensa — REGISTRY.yaml passa a referenciar `git = "https://..."` em
vez de path local).

Isso é o "submodule funcional sem submodule" que a infraestrutura YG-71
viabiliza.
