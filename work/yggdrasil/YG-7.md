---
id: 7
title: "Adapter para Tetris (game_core::TetrisGame)"
status: done
priority: medium
type: feat
release: 0.5.0
parent: 19
blocked_by: [6]
labels:
  - games
  - adapter
module: yggdrasil-core
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T13:56:42.105232+00:00
---

GIVEN o adapter Snake já estabeleceu o padrão (YG-6),
WHEN replico o padrão para `TetrisGame`,
THEN entrar no portal "tetris" abre uma sessão jogável e ao fim retorna ao lobby.

## Referência em game-core

- `co/game-core/src/cd7ac4c6/a10335a2.rs` — `TetrisGame`.
- `co/co-web/static/games/tetris.js` — frontend de referência.

## Critérios de aceitação

- [ ] `yggdrasil-core/src/games/tetris.rs` segue o mesmo trait `YggGame` de YG-6.
- [ ] Rotas em `yggdrasil-web/src/games/tetris_routes.rs`.
- [ ] `static/games/tetris.js`.
- [ ] Score persistido com chave `(user_id, "tetris", score, ts)`.
- [ ] Reuso de helpers comuns extraídos em `yggdrasil-web/src/games/common.rs` (sessão, score) — refatorar Snake junto se necessário.

## Commit

`feat(YG-7): adapter Tetris`
