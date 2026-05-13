---
id: 6
title: "Adapter para Snake (game_core::SnakeGame)"
status: done
priority: high
type: feat
release: 0.1.0
parent: 18
blocked_by: [5]
labels:
  - games
  - adapter
module: yggdrasil-core
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T13:46:36.830644+00:00
---

GIVEN o usuário entrou no portal "snake" (YG-5),
WHEN o servidor inicia uma sessão `SnakeGame` do `game-core`,
THEN o cliente recebe ticks do jogo e renderiza no mesmo canvas, e ao
fim da partida volta para o lobby Yggdrasil.

## Referência em game-core

- `co/game-core/src/cd7ac4c6/538d7d9f.rs` — `SnakeGame`.
- `co/game-core/src/cd7ac4c6/mod.rs:30` — trait `Game` (`tick(input) -> GameAction`, `render() -> Map`).
- Cliente JS de referência: `co/co-web/static/games/snake.js`.

## Critérios de aceitação

- [ ] `yggdrasil-core/src/games/snake.rs` exporta `pub struct YggSnake { inner: SnakeGame }` que implementa um trait Yggdrasil-side `YggGame { tick, render_json }`.
- [ ] `yggdrasil-web/src/games/snake_routes.rs` expõe `GET /api/v1/games/snake/start` e `POST /api/v1/games/snake/<id>/input`.
- [ ] `static/games/snake.js` renderiza usando o JSON enviado (mesma fonte de cor/tile do lobby).
- [ ] Ao receber `GameAction::Quit` do tick, redireciona para `/lobby`.
- [ ] Score persistido (em SQLite local na fase 1) chave `(user_id, "snake", score, ts)`.

## Commit

`feat(YG-6): adapter Snake reusando game_core::SnakeGame`
