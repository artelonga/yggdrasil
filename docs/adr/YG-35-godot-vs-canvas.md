# ADR YG-35 — Migrar para Godot vs manter Canvas

- **Status:** Aceito (decisão do spike YG-35)
- **Data:** 2026-06-08
- **Contexto da decisão:** [YG-35](../../work/yggdrasil/YG-35.md) (tarefa-decisão do POC Godot, epic [YG-22](../../work/yggdrasil/YG-22.md) trilho B)
- **Tarefas avaliadas:** YG-31 (scaffold), YG-32 (Lobby nested scenes), YG-33 (signal bus + event-sourcing), YG-34 (multiplayer JWT gateway), YG-35 (PokerTable E2E)
- **Pipeline de referência:** [docs/release-pipeline.md](../release-pipeline.md)

> **TL;DR — Recomendação: HÍBRIDO, com canvas como cliente de produção.**
> Manter `yggdrasil-web/static` (canvas + Rust) como o cliente que sustenta o
> release v3.0. **Não migrar** lobby/jogos 2D/editor/comunicação para Godot.
> Promover **apenas o `anatomy3d`** (visualizador 3D, YG-83/87) a alvo Godot de
> primeira-classe — é o único caso onde Godot entrega algo que o canvas não
> entrega de forma realista. O resto do `yggdrasil-godot/` é arquivado como
> aprendizado/laboratório, fora do caminho crítico de release.

---

## 1. Estado real do POC (auditoria read-only, 2026-06-08)

A premissa estratégica que justificou avaliar Godot (epic YG-22 / trilho B) eram
**quatro pilares**: scene tree composável (nested scenes), comunicação por
signals autoritativos, lazy spawn, e **multiplayer nativo** com JWT compartilhado
com o gateway Rust. O POC tinha que *provar esses pilares* — não construir um
cliente single-player alternativo.

O que foi efetivamente construído diverge do que as tarefas especificaram. Há
~3.2k LOC de GDScript, mas concentrados em **lógica de jogo single-player local**
(poker vs IA, snake, tetris, invaders, pointset), um visualizador `anatomy3d`
(329 LOC, genuinamente novo) e um `grid_editor` que consome a REST host-neutra.
Os pilares que *motivaram* o POC — signals autoritativos, event-sourcing,
multiplayer, JWT gateway — **não existem em código**.

Evidências (busca em `yggdrasil-godot/scripts/`):

- `@rpc`: **0 ocorrências**. `WebSocketMultiplayerPeer`, `MultiplayerSpawner`,
  `MultiplayerSynchronizer`: **0**. Não há `server_main.gd`, `client_main.gd`
  nem `lib/jwt.gd` (todos exigidos por YG-34).
- `signal_bus.gd`: **7 linhas** — apenas declarações de `signal`. Nenhuma das
  primitivas de YG-33 (`register_authoritative`, event log JSON-lines,
  flush, modo `--replay`). Os hits de "replay"/"event" no grep eram falsos
  positivos (comentários e termos de pôquer).
- `lobby.gd`: **8 linhas** — reposiciona o player; não escuta `player_entered`
  de portais, não há `GamePortal.tscn` instanciado N vezes. A demonstração de
  **nested scenes** (o ponto central de YG-32) não foi feita.
- `poker.tscn` e `anatomy_viewer.tscn`: **6 linhas cada** — shells que carregam
  um script que desenha tudo via `queue_redraw()`. Não existem as cenas
  `Seat.tscn`/`Pot.tscn`/`CommunityCards.tscn`/`ActionBar.tscn` que YG-35 pedia;
  logo a tese de composição via PackedScene também não foi exercida.
- `main_scene` em `project.godot` aponta para `anatomy3d/anatomy_viewer.tscn`,
  não para o lobby — sintoma de que o foco real virou o viewer 3D.
- `README.md` do Godot ainda diz textualmente *"Status: scaffold. Nenhum jogo
  aqui ainda"* e *"Multiplayer (vem em YG-34)"* — ou seja, o próprio projeto se
  considera pré-YG-34.

O pôquer **multiplayer server-authoritative** que YG-35 queria provar em Godot
**já existe e está pronto no lado Rust/canvas**: `/api/v1/poker/lobbies/{id}/`
`sit|stand|action|hand|hole-cards` com persistência (epic YG-22 = `done`,
release 0.8.0), consumido por `static/universos/poker.js` (510 LOC). O pôquer do
Godot é uma reimplementação **local vs IA** que ignora esse backend.

### Tabela de completude por tarefa

| Tarefa | Escopo central | Estado real | Veredito |
|---|---|---|---|
| **YG-31** scaffold (export web+headless, Docker, CLI) | `done` no board; toolchain `godot.sh` + Dockerfile + presets existem e o lint headless roda no CI | Infra real e útil | ✅ **Feito** |
| **YG-32** Lobby com 4 GamePortal nested | `todo`. Existe `lobby.tscn` (122 ln) + `player.gd` + `arcade_cabinet.gd`, mas **não** há `GamePortal.tscn` instanciado 4× nem escuta de `player_entered`. Usa "arcade cabinets" ad-hoc | Tese de **nested scenes não provada** | 🟡 **Parcial / divergente** |
| **YG-33** signal bus autoritativo + event-sourcing + replay | `todo`. `signal_bus.gd` = 7 linhas de `signal`. Sem event log, sem flush, sem `--replay`, sem teste de crash/restart | Pilar **não construído** | 🔴 **Não feito** |
| **YG-34** multiplayer JWT Godot↔Rust gateway | `todo`. **Zero** `@rpc`/WebSocket/Spawner/Synchronizer; sem `server_main`/`client_main`/`jwt.gd`; sem segundo service no `fly.toml` | Pilar **multiplayer — a razão de ser do POC — não construído** | 🔴 **Não feito** |
| **YG-35** PokerTable E2E + métricas + ADR | `todo`. Pôquer Godot é **single-player vs IA**, shell scene de 6 linhas; sem Seat/Pot/CommunityCards/ActionBar; sem bridge `/sementes/debit\|credit`; sem demo 2-browsers; **sem `docs/POC-METRICS.md`** | E2E multiplayer **não demonstrado**; sem dados de métrica | 🔴 **Não feito** |

**Bônus fora-de-escopo construído** (não pedido por YG-31..35, mas real e de
valor): `anatomy3d/anatomy_viewer.gd` (329 ln) — viewer 3D orbital com malhas
`.obj` do BodyParts3D (YG-83/87); 4 jogos arcade single-player; `grid_editor` +
`instance_api.gd` consumindo a REST host-neutra de instâncias.

**Conclusão da auditoria:** o POC **não atingiu seu critério de saída**. A
hipótese explícita de YG-35 — *"se o resultado superar a versão canvas em
developer experience + UX multiplayer, migramos"* — **não pode ser confirmada
nem refutada com dados**, porque a parte multiplayer/event-sourced/composável
nunca foi escrita. O que existe prova, no máximo, que Godot consegue renderizar
jogos 2D single-player e um viewer 3D — coisas que nunca estiveram em dúvida.

---

## 2. Custo/benefício das três opções

### Inventário do que está em jogo (lado canvas, o incumbente)

- **Frontend:** 14 páginas HTML, ~4k LOC JS — `lobby.js` (416), `instance.js`
  (editor de universos, 854), `comunicacao.js` (674), `poker.js` (510), snake/
  tetris/invaders, `vim.js`, feedback, login, nav.
- **Backend:** ~23,5k LOC Rust — auth/JWT magic-link, **co-bridge
  producer+inbound (1.784 LOC)**, universos/instances, pôquer multiplayer
  persistido, comunicação (léxico/salas/revisão), scores, telemetria, OpenAPI.
- **O seam de federação v3.0 (co-bridge) vive aqui.** Pelo
  `release-pipeline.md`, YG-93/97/103/101 já estão `done` e CI-green; o lançamento
  espera só `CO-384` no lado co. Esse caminho crítico é **inteiramente
  canvas/Rust**; Godot não participa dele.

### Opção A — Migrar tudo para Godot

| | |
|---|---|
| **Benefícios** | Stack única de cliente; multiplayer/3D nativos; composição por scenes. |
| **Custos** | Re-portar editor de instâncias (854 ln), comunicação (674 ln), lobby, 4–5 jogos, feedback, nav, login → semanas-equipe. **Reescrever o seam de federação** ou criar uma ponte Godot↔co-bridge. Pagar agora **toda a dívida YG-33/34** (event-sourcing, WS, JWT gateway, 2º service no Fly) que o POC *não* fez. Payload `.wasm`+`.pck` e COOP/COEP em mobile **sem métrica medida** (YG-35 nunca produziu `POC-METRICS.md`). |
| **Risco vs v3.0** | **Inaceitável.** Joga fora o trilho que está a um `CO-384` de lançar e o substitui por trabalho não iniciado. Atrasa v3.0 por meses. |

### Opção B — Manter canvas (arquivar Godot por inteiro)

| | |
|---|---|
| **Benefícios** | Zero risco ao v3.0/v3.1; foco total no caminho crítico (bridge + corpus + hardening). Sem dívida nova. |
| **Custos** | Descarta o `anatomy3d` — o único artefato Godot com valor real e difícil de replicar em canvas 2D. Para anatomia 3D de verdade (YG-83/87), canvas exigiria three.js/WebGL — proibido pelo CLAUDE.md ("sem framework JS pesado") e fora do padrão atual. |
| **Risco vs v3.0** | Nenhum. Mas joga fora trabalho 3D bom. |

### Opção C — Híbrido (recomendado): canvas é produção, Godot é alvo cirúrgico

| | |
|---|---|
| **Benefícios** | Mantém intacto o trilho de release canvas/Rust e o seam de federação. Preserva o `anatomy3d` como um **universo Godot embutido** (iframe / web export numa rota dedicada), onde o 3D nativo justifica o peso. Cliente canvas continua dono de auth, lobby, editor, comunicação e do contrato `/api/v1` — que **o Godot já reusa via HTTP** (`instance_api.gd`, `api_client.gd`). |
| **Custos** | Manter (mínimo) o pipeline de export web do Godot só para o viewer 3D. Dois toolchains no repo — mas já é a realidade, e o híbrido apenas formaliza a fronteira em vez de manter um cliente Godot inteiro fantasma. |
| **Risco vs v3.0** | Baixo e **isolado**: o anatomy3d é pós-lançamento e independente do caminho crítico (lane F no pipeline = "decidida independentemente; não é filler bloqueante de release"). |

**Por que híbrido e não A nem B:** a fronteira natural já apareceu no próprio POC.
Tudo que Godot construiu e que o canvas *também* faz bem (lobby, jogos 2D, editor
via REST) é **redundância**, não vantagem — e re-portar isso é custo puro. A única
coisa que Godot fez e o canvas **não faz** é 3D real (anatomy3d). Híbrido fica
exatamente nessa linha: canvas onde canvas ganha, Godot onde 3D nativo é a
diferença.

---

## 3. Recomendação e rationale

**Recomendação: HÍBRIDO. Canvas/Rust permanece o cliente de produção do v3.0;
Godot é promovido a alvo de primeira-classe apenas para `anatomy3d`. O restante de
`yggdrasil-godot/` é congelado como laboratório, fora do caminho de release.**

Rationale amarrado ao timeline e ao pipeline multi-time:

1. **O caminho crítico do v3.0 é canvas/Rust e está quase lá.** O
   `release-pipeline.md` é explícito: a metade yggdrasil do bridge (YG-93/97/103/101)
   está `done` + CI-green, e o lançamento espera só `CO-384` no lado co. Migrar
   para Godot trocaria um trilho a-um-passo-do-fim por trabalho não iniciado.
   Nenhuma vantagem de Godot compensa esse atraso.

2. **O POC falhou em produzir os dados que justificariam migrar.** A regra de
   decisão de YG-35 era *medir* dev-velocity e UX multiplayer contra canvas.
   Sem `POC-METRICS.md`, sem multiplayer, sem event-sourcing e sem composição de
   scenes, **não há evidência a favor de migrar** — e "na dúvida, não reescreva o
   que funciona e fatura".

3. **O pipeline já trata Godot como decisão lateral, não como release.** Lane F:
   *"Separate stack. YG-35 = migration DECISION — a strategic gate, timebox it;
   not release-blocking filler."* Híbrido honra isso: tira Godot do caminho
   crítico sem jogar fora o `anatomy3d`.

4. **O contrato compartilhado torna o híbrido barato.** Ambos os clientes falam o
   mesmo `/api/v1` host-neutro com `Authorization: Bearer <JWT>`. O Godot **já**
   consome instâncias/templates/auth via HTTP (`instance_api.gd`, `api_client.gd`).
   Um universo `anatomy3d` em Godot não precisa do gateway WS/JWT de YG-34 (é
   single-user, read-only) — então a dívida cara de YG-33/34 **pode permanecer não
   paga** sem bloquear o caso de uso que sobrevive.

---

## 4. O que YG-35 resolve e follow-ups por caminho

**Resolução de YG-35 (a "decisão de migração"):** **NÃO MIGRAR.** Adotar híbrido.
Canvas/Rust segue como cliente de produção. Godot é retido **apenas** para o
universo 3D `anatomy3d`. Marcar YG-32/33/34 como **`wontfix`/`descoped`** (não
serão concluídos como especificados — a tese multiplayer-em-Godot foi descartada).
Marcar YG-35 como `done` com este ADR como entregável. A nota de "arquivar a
branch se não migrar" do YG-35 vira "arquivar o cliente Godot **exceto** o
anatomy3d".

> Nota: o YG-35 também pedia `docs/ADR-002-godot-poc-resultado.md` e
> `docs/POC-METRICS.md`. Este ADR (`docs/adr/YG-35-godot-vs-canvas.md`) **é** o
> registro de decisão. `POC-METRICS.md` fica como dívida explícita **somente se**
> alguém quiser quantificar antes de arquivar — não é pré-requisito da decisão,
> que se sustenta na ausência dos pilares.

### Follow-ups se HÍBRIDO (caminho recomendado)

- **YG-36 (novo):** extrair `scenes/anatomy3d/` + `scripts/anatomy3d/` +
  `assets/anatomia/` para um export web Godot mínimo (preset Web já existe);
  servir numa rota dedicada (ex.: `/universos/anatomia`) embutida via iframe na
  shell canvas. Sem WS/JWT gateway — read-only, single-user.
- **YG-37 (novo):** medir e fixar orçamento de payload do `.wasm`+`.pck` do
  anatomy3d e validar COOP/COEP + carregamento em Chrome mobile (a métrica que o
  POC nunca coletou) — gate de "vale a pena embutir".
- **YG-38 (chore):** congelar/arquivar o restante de `yggdrasil-godot/` (lobby,
  jogos 2D, grid_editor, poker local) num diretório `lab/` ou tag, com README
  apontando para este ADR. Remover o cliente Godot 2D dos alvos de CI ativos
  (manter só o lint do que sobrar).
- **YG-32/33/34:** marcar `descoped` com referência a este ADR.
- **Pipeline:** nenhuma mudança no caminho crítico; lane F encerrada como
  "decidida: híbrido".

### Follow-ups se (contrafactual) MIGRAR

- Pagar integralmente YG-33 (event-sourcing+replay) e YG-34 (WS+JWT gateway+2º
  service Fly) **antes** de qualquer porte.
- Re-portar editor de instâncias, comunicação, lobby, 5 jogos, feedback, login.
- Construir ponte Godot↔co-bridge (ou re-hospedar o seam de federação) — risco
  alto sobre o caminho crítico do v3.0. **Não recomendado.**

### Follow-ups se (contrafactual) MANTER CANVAS PURO (arquivar tudo)

- `git rm -r yggdrasil-godot/` e fechar a lane F.
- Aceitar que anatomia 3D real fica fora do produto até reavaliação — perda do
  único ativo Godot com valor diferenciado.

---

## Apêndice — Métricas-âncora coletadas neste spike

| Métrica | Valor |
|---|---|
| GDScript total | ~3.201 LOC (poker_game 632, hand_evaluator 245, tetris 240, pointset 237, invaders 201, snake 167, **anatomy3d 329**) |
| `signal_bus.gd` (deveria ser o core de YG-33) | 7 linhas |
| `@rpc` / WebSocket / Spawner no Godot | 0 ocorrências |
| Canvas JS produção | ~3.966 LOC / 14 páginas HTML |
| Rust backend (yggdrasil-web + core) | ~23.590 LOC |
| co-bridge (seam de federação v3.0) | 1.784 LOC — 100% lado canvas/Rust |
| Pôquer multiplayer server-authoritative | `done` no Rust (epic YG-22, 0.8.0); ausente no Godot |
| `docs/POC-METRICS.md` (exigido por YG-35) | inexistente |
