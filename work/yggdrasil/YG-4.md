---
id: 4
title: "Input de teclado no lobby (WASD/setas + Enter)"
status: todo
priority: high
type: feat
release: 0.1.0
parent: 18
blocked_by: [3]
labels:
  - frontend
  - input
  - lobby
module: yggdrasil-web
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T00:00:00Z
---

GIVEN o lobby está renderizado (YG-3),
WHEN o usuário pressiona setas/WASD para mover o avatar e Enter sobre um
tile de portal,
THEN o avatar se move de tile em tile, e ao pressionar Enter sobre portal
o cliente faz `POST /api/v1/lobby/enter` que retorna o slug do jogo alvo.

## Referência em game-core

`co/game-core/src/ed9f6f25/c96c6d5b.rs` — `Input`, `Direction`, `poll_input`.
Espelhar a enumeração `Direction { Up, Down, Left, Right }` no JS.

## Critérios de aceitação

- [ ] `static/lobby.js` mantém estado `{playerX, playerY}`.
- [ ] Setas/WASD movem o avatar 1 tile, bloqueando paredes.
- [ ] Enter sobre portal chama backend e redireciona para `/games/<slug>`.
- [ ] Avatar renderizado como `@` dourado sobre o tile atual.
- [ ] Sem dependências JS externas (vanilla, espelho do `co-web/static/games/snake.js`).

## Commit

`feat(YG-4): input de teclado no lobby`
