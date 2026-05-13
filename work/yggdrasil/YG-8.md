---
id: 8
title: "Adapter para Space Invaders (game_core::InvadersGame)"
status: done
priority: medium
type: feat
release: 0.5.0
parent: 19
blocked_by: [7]
labels:
  - games
  - adapter
module: yggdrasil-core
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T14:08:47.707579+00:00
---

GIVEN o trait comum `YggGame` está consolidado (YG-7),
WHEN aplico o mesmo adapter para `InvadersGame`,
THEN entrar no portal "invaders" inicia a partida.

## Referência em game-core

- `co/game-core/src/cd7ac4c6/63e3a0b8.rs` — `InvadersGame`.
- `co/co-web/static/games/invaders.js`.

## Critérios de aceitação

- [x] `yggdrasil-core/src/games/invaders.rs`.
- [x] Rotas em `yggdrasil-web/src/games/invaders_routes.rs`.
- [x] `static/games/invaders.js`.
- [x] Score persistido `(user_id, "invaders", score, ts)`.

## Commit

`feat(YG-8): adapter Space Invaders`
