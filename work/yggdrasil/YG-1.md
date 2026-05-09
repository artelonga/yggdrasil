---
id: 1
title: "Bootstrap do workspace + dependência de co/game-core"
status: done
priority: critical
type: feat
release: 0.0.1
labels:
  - bootstrap
  - workspace
module: workspace
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T00:00:00Z
---

GIVEN o repositório Yggdrasil existe vazio em `~/projects/yggdrasil`,
WHEN inicializo o workspace Rust e configuro a dependência do `co/game-core`,
THEN devem existir os arquivos abaixo e `cargo check --workspace` passar.

## Critérios de aceitação

- [x] `Cargo.toml` workspace com membros `yggdrasil-core` e `yggdrasil-web`.
- [x] `[workspace.dependencies] game-core = { path = "../co/game-core" }`.
- [x] `yggdrasil-core/src/lib.rs` re-exporta `engine` e `upstream_games` do `game-core`.
- [x] `yggdrasil-web/src/main.rs` sobe um axum em `:3030` com `/health`.
- [x] `CHANGELOG.md`, `README.md`, `LICENSE`, `CLAUDE.md` (raiz e do espaço).
- [x] `.co/marker` para co-auto.
- [x] `co-universes.yaml` para inscrição via co.
- [x] Documentos `YGGDRASIL.docx`, `Yggdrasil — Experiência do Usuário (UX).docx` movidos para `docs/`.

## Commit

`chore(YG-1): bootstrap workspace + co/game-core path dep`

## Notas

Versão fixada em `0.0.1`. A primeira release pública será `v0.1.0` em YG-18.
