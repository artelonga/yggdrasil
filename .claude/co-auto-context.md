## Development Conventions (CLAUDE.md)

# Instruções para Claude — Yggdrasil

Este arquivo orienta agentes Claude (via Claude Code, co-auto, ou direto) que trabalham no repo Yggdrasil.

## Visão geral

Yggdrasil é uma plataforma de universos digitais. Cada universo é um espaço de criação que pode virar um mundo 2D jogável reaproveitando o `co/game-core`. A campanha de financiamento (Catarse-style) requer um produto demonstrável com 4 jogos no lobby, API pública e plano de release até `v1.0.0`.

## Stack

- **Rust** — workspace com `yggdrasil-core` (lógica) e `yggdrasil-web` (servidor HTTP).
- **Engine 2D** — `co/game-core` (path dep). Não reescrever; estender ou wrap.
- **Frontend** — `<canvas>` + JS estático em `yggdrasil-web/static/`, mesmo padrão do `co-web/static/games/*.js`.
- **Auth + JWT** — espelhar `co-web/src/auth.rs` e `co-web/src/game_routes.rs`.
- **DB** — SQLite (`rusqlite`) na fase 0; migrar para Postgres só se necessário.

## Convenções

### Idioma

- Copy de produto, nomes de tier, mensagens de UI: **PT-BR**.
- Termos técnicos (config, plugin, wallet, tile): **inglês**.
- Termos de domínio (universo, elemento, conexão, semente, modelo, assinatura): **PT-BR**, conforme glossário.
- i18n preparado em `i18n/pt.yaml` e `i18n/en.yaml` mas só preencher PT na fase 1.

### Commits

Conventional Commits, sempre:

```
tipo(escopo): descrição curta

Co-Authored-By: Claude <noreply@anthropic.com>
```

Tipos: `feat`, `fix`, `docs`, `refactor`, `chore`, `test`. Branches: `feat/YG-<n>-descricao`.

### Versionamento

SemVer. `feat` = bump minor. `fix`/`docs`/`refactor` = bump patch. Atualizar `Cargo.toml` (workspace.package.version) e `CHANGELOG.md` no mesmo commit que a mudança.

### Regras de git

- `git add file1 file2` (nunca `git add -A`).
- Não force push em main.
- Não amend commits sem pedido explícito.
- Nunca skip hooks.

## Tarefas (co-auto)

Lista em `work/yggdrasil/YG-*.md`. Estrutura segue `co/work/co/CO-*.md` (frontmatter + GIVEN/WHEN/THEN). Para executar a próxima tarefa:

```bash
# Do repo co
cargo run -p co-auto -- --workdir ../yggdrasil --space yggdrasil
```

Cada tarefa fechada deve resultar em um commit (ou PR) com o tipo Conventional adequado e bump de versão se necessário.

## Importações de game-core

Padrão preferido para reuso:

```rust
use game_core::{
    engine::{Universe, Map, Tile, Session},
    games::{SnakeGame, TetrisGame, InvadersGame, PokerGame, Game, GameAction},
    storage::{Storage, WalletManager},
    plugin::{Plugin, PluginManifest, PluginRegistry},
};
```

Para wrappear um jogo do game-core dentro do Yggdrasil, criar um adaptador em `yggdrasil-core/src/games/<nome>.rs` que exponha:
- Construtor a partir de `Universe` Yggdrasil.
- Renderização compatível com o frontend `<canvas>`.
- Hook de wallet → sementes (ver YG-10).

## O que NÃO fazer

- Não copiar código do `game-core` — sempre depender via path/git.
- Não introduzir framework JS pesado no frontend (Svelte/React) na fase 1; canvas + vanilla JS é suficiente.
- Não traduzir copy para EN sem pedido.
- Não publicar releases sem atualizar `CHANGELOG.md`.


---

## Current Task: YG-58 — Universo Vim — emulador modal Rust + 10 níveis progressivos

---
id: 58
title: "Universo Vim — emulador modal Rust + 10 níveis progressivos"
type: user-story
status: todo
priority: high
conventional_commit: "feat(universo-vim):"
semver_bump: minor
labels:
  - type:feat
  - module:wasm
  - module:universo-vim
module: yggdrasil
parent: 54
created_at: 2026-05-20T00:00:00Z
updated_at: 2026-05-20T00:00:00Z
---

## As

A player who wants to learn Vim commands

## I Need

A new universo `universe-vim` no estilo VimAdventures.com — puzzles de edição de texto
onde os comandos Vim são a mecânica, com 10 níveis progressivos que ensinam do hjkl
até substituição `:s/find/replace/`

## So That

O yggdrasil tem um universo educativo único que diferencia a plataforma, gera engajamento
recorrente (os jogadores voltam para completar níveis), e demonstra que universos WASM
podem ter mecânicas completamente diferentes dos jogos arcade existentes

## Context

- **Princípios:** §6 (folder encapsulates feature), §2 (no framework JS pesado)
- **Depende de:** YG-55 (WasmRuntime), YG-56 (SDK)
- **Referência:** `docs/UNIVERSO-VIM.md` — stack comparison completo e design dos níveis
- **Stack escolhido:** emulador Vim mínimo em Rust puro (Option E no doc) — ≤ 250KB WASM.
  Não usar vim.wasm (7MB), CodeMirror (JS), helix-core (5MB), ou server-side Vim (inseguro).
- **Escopo do editor** (`universes/universe-vim/src/editor.rs`):
  - Normal mode: `h j k l w b e 0 $ ^ gg G x dd yy p P dw d$ u . /text n N`
  - Insert mode: `i I a A o O` + `Esc`
  - Visual mode: `v V` + `d y p` na seleção
  - Commands: `:s/find/replace/`, `:w` (marca nível completo), `:q` (quit)
  - Pending operator state (ex: `d` + `w` = `dw`) gerenciado internamente
- **Níveis** (`universes/universe-vim/src/levels.rs`) — 10 puzzles, cada um com:
  - `initial_buffer: &str`, `cursor_start: (row, col)`
  - `success_fn: fn(&str) -> bool` — compara buffer resultante (determinístico)
  - Narrativa PT-BR (texto do puzzle integrado ao buffer)
  - Solução ótima em ≤ 5 keypresses (níveis 1-5) ou ≤ 15 (níveis 6-10)
- **Estado JSON** retornado por tick:
  `{ lines, cursor, mode, selection, status_line, hint, level, completed_level, score, game_completed }`
- **Hints:** emitidos via `request_hint(ctx_json)` após 3 tentativas erradas no nível;
  hint retorna de forma assíncrona no próximo tick via `state.hint`. Implementação
  da engine de hints está em YG-59.
- **Sementes:**
  - Níveis 1-3: +10 por nível; 4-7: +20; 8-10: +30
  - Sem hints no nível: +50% bônus
  - Primeiro clear completo: +100 bônus único (idempotente por user_id)
- **Canvas JS** (`yggdrasil-web/static/universos/vim.js`): renderiza buffer linha por
  linha, highlight de cursor, modo no status bar — mesmo padrão dos outros universos.
- **Budget:** `universe-vim.wasm` ≤ 250KB após `wasm-opt -O3`

## Acceptance

- 100+ testes de unidade no editor: cada comando testado com `(buffer_before, cursor_before) → (buffer_after, cursor_after)`.
- Pending operator funciona: `d` + `w` deleta palavra; `d` + `$` deleta até fim de linha.
- `u` desfaz o último change; `.` repete o último change.
- `/text` move cursor para próxima ocorrência; `n`/`N` avança/retrocede.
- `:s/find/replace/` substitui primeira ocorrência na linha atual.
- Todos os 10 níveis têm `success_fn` testada com solução ótima e ≥ 1 solução alternativa.
- `POST /api/v1/universos/vim/sessoes` retorna `{ session_id, state }` com `level: 1`.
- Completar nível N → `state.completed_level: N` e sementes creditadas via `emit_event`.
- Completar todos os 10 níveis → `state.game_completed: true` e +100 bônus únicos creditados.
- Canvas renderiza buffer, cursor highlight, modo (NORMAL/INSERT/VISUAL), status line.
- `universe-vim.wasm` ≤ 250KB após `wasm-opt -O3` (CI gate).

## Blast radius

Low — novo crate em `universes/`, nova rota `/api/v1/universos/vim/*`, nova página
`static/universos/vim.html`. Não toca em código existente.


---

## Parent Epic: YG-54 — Epic — Universe Platform v1.0 — Embedded WASM

---
id: 54
title: "Epic — Universe Platform v1.0 — Embedded WASM"
type: epic
status: todo
priority: high
conventional_commit_default: "feat(wasm):"
semver_bump_aggregate: minor
milestone: "v1.0.0"
labels:
  - type:feat
  - epic
  - module:wasm
module: yggdrasil
created_at: 2026-05-20T00:00:00Z
updated_at: 2026-05-20T00:00:00Z
---

## Goal

Ship yggdrasil v1.0.0 with all universos compiled to WASM and embedded in the binary,
a new Vim learning universe with Claude-powered hints, and a unified API surface.
After this release the universe crates are open-sourced as separate repositories.

## Children

- YG-55 — WASM Runtime Host (wasmtime integration + fuel enforcement)
- YG-56 — Universe SDK Rust (ABI v1 + build tooling)
- YG-57 — Migrate 5 existing universos to WASM (snake, tetris, invaders, pointset, poker)
- YG-58 — Universo Vim — emulador modal + 10 níveis progressivos
- YG-59 — Claude hint engine (host-side LLM bridge for Vim universe)
- YG-60 — Unified API /api/v1/universos (session routes + WS)
- YG-61 — Build pipeline CI/CD (build-universes.sh + size gates)
- YG-62 — Telemetria e funil básico (funnel_events + session_records)

## Acceptance

- `cargo build` produces a single self-contained binary with 6 WASM universos embedded (total ≤ 2 MB).
- `GET /api/v1/universos` lists all 6 universos.
- All legacy routes (`/api/v1/games/{game}/start`) continue to work unchanged.
- Vim universe playable end-to-end with hints from Claude API.
- `cargo test` passes; `cargo clippy -- -D warnings` clean.
- CHANGELOG entry `[1.0.0]` and `Cargo.toml` version bumped to `1.0.0`.

## Reference

- `docs/RELEASE-V1.md` — acceptance criteria detalhados por epic
- `docs/UNIVERSO-VIM.md` — stack comparison e design do Vim universe
- `docs/PLATAFORMA-UNIVERSOS.md` — arquitetura de longo prazo


---

## Project Configuration

```yaml
name: Yggdrasil
key: YG
description: >
  Yggdrasil — plataforma de universos digitais. Lobby 2D com jogos
  (Snake/Tetris/Invaders/Poker), API pública, ponte Godot e modelo de
  recompensas (sementes).
created_at: 2026-05-09T00:00:00Z
next_id: 63
```

---

## Completed Tasks (already merged — do NOT re-implement)

- YG-42 — Replace serde_json::Value in game state payloads with concrete types (DONE, already merged into main)
- YG-27 — Pôquer: buy-in/cash-out em sementes por partida (DONE, already merged into main)
- YG-37 — Variantes parametrizam engines (lê `?variant=` nas rotas de jogo) (DONE, already merged into main)
- YG-52 — Reconcile game-core drift between universos/core and co/game-core (DONE, already merged into main)
- YG-23 — Pôquer: lobby seating + bot presence rule (DONE, already merged into main)
- YG-46 — Document 'no per-game DB' + correct the persistence model (DONE, already merged into main)
- YG-4 — Input de teclado no lobby (WASD/setas + Enter) (DONE, already merged into main)
- YG-22 — EPIC: Pôquer multiplayer (universo Pôquer 1.0) (DONE, already merged into main)
- YG-5 — Input de mouse: click em tile = move + auto-entra em portal (DONE, already merged into main)
- YG-43 — Carve out lobby/ folder; collapse core::lobby ↔ web::lobby_routes split (DONE, already merged into main)
- YG-12 — GET /api/v1/me/sementes — saldo do usuário (DONE, already merged into main)
- YG-1 — Bootstrap do workspace + dependência de co/game-core (DONE, already merged into main)
- YG-26 — Pôquer: bot AI — ação aleatória legal (DONE, already merged into main)
- YG-36 — Universos como grafo de nós (composição sobre herança) (DONE, already merged into main)
- YG-53 — Archive universos repo after YG-51 + YG-52 land (DONE, already merged into main)
- YG-29 — Pôquer: persistir estado da mesa em SQLite (DONE, already merged into main)
- YG-39 — Promote YggGame to a real adapter trait used by all four games (DONE, already merged into main)
- YG-38 — Pin game-core to git rev + delete path dep + drop fly.toml hack (DONE, already merged into main)
- YG-8 — Adapter para Space Invaders (game_core::InvadersGame) (DONE, already merged into main)
- YG-9 — Adapter para Poker com sementes (WalletManager) (DONE, already merged into main)
- YG-21 — Tier system de recompensas (sementes, PT-BR) (DONE, already merged into main)
- YG-6 — Adapter para Snake (game_core::SnakeGame) (DONE, already merged into main)
- YG-44 — Segregate per-game DB connections behind a ScoresStore trait (DONE, already merged into main)
- YG-31 — Godot POC: scaffold yggdrasil-godot/ com export web + headless (DONE, already merged into main)
- YG-2 — Lobby Universe com 4 portais (snake, tetris, invaders, poker) (DONE, already merged into main)
- YG-11 — Auth pública: login por email + JWT (DONE, already merged into main)
- YG-40 — Split poker_routes.rs (1189 LOC) by responsibility (DONE, already merged into main)
- YG-25 — Pôquer: dealing, betting rounds, showdown (DONE, already merged into main)
- YG-3 — Renderização <canvas> do lobby (DONE, already merged into main)
- YG-10 — Renomeação semântica: wallet/balance → sementes/saldo (camada Yggdrasil) (DONE, already merged into main)
- YG-41 — Introduce event spine (tokio::sync::broadcast) and WS for poker (DONE, already merged into main)
- YG-24 — Mail provider real (SMTP) substituindo LogMailProvider (DONE, already merged into main)
- YG-51 — Port Godot games from universos into yggdrasil-godot (DONE, already merged into main)
- YG-7 — Adapter para Tetris (game_core::TetrisGame) (DONE, already merged into main)
- YG-45 — Trim auth.rs and api/me.rs (each >600 LOC) (DONE, already merged into main)
- YG-55 — WASM Runtime Host — wasmtime integration + fuel enforcement (DONE, already merged into main)
- YG-30 — RELEASE v0.8.0 — pôquer multiplayer (DONE, already merged into main)

---

## Execution Instructions

**YOUR TASK IS: YG-58 — Universo Vim — emulador modal Rust + 10 níveis progressivos**

IMPORTANT: Only implement YG-58. Do NOT implement or modify any other task.
Dependencies listed in the roadmap (e.g., 'Depends On: GP-8') mean those tasks are ALREADY DONE and merged into main. Their code is already in the codebase. Do not look for them or re-implement them.

Follow the acceptance criteria exactly. Each `- [ ]` item is a required deliverable.
Use conventional commits: the task specifies the commit message format.
Run `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt` before committing.
After completing all criteria, commit with the specified message.

## Test Isolation Rules

- All tests MUST run without opening network ports. Use in-process test servers (e.g., `axum::test::TestClient`, `tower::ServiceExt`) instead of spawning HTTP listeners.
- Never bind to `0.0.0.0`. If a test requires a port, bind to `127.0.0.1` only.
- Use temp directories for test databases (e.g., `tempfile::tempdir()`) — never write to user paths.
- Tests must be fully deterministic: no sleeps, no real network calls, no system time dependencies.
- Set `JWT_SECRET=test-secret` and `RUST_LOG=off` in test harness setup.