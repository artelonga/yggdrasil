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
