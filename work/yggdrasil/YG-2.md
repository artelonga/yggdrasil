---
id: 2
title: "Lobby Universe com 4 portais (snake, tetris, invaders, poker)"
status: done
priority: critical
type: feat
release: 0.1.0
parent: 18
labels:
  - lobby
  - engine
module: yggdrasil-core
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T00:00:00Z
---

GIVEN o engine `game_core::engine::{Universe, Map, Tile}` está disponível,
WHEN crio um módulo `yggdrasil_core::lobby` que monta um Universe 40x20 com
4 tiles `Tile::Portal(target)` apontando para `snake`, `tetris`, `invaders`,
`poker`,
THEN ao construir o lobby cada portal deve estar na coordenada documentada
e `Session::teleport_to` deve mover entre eles.

## Referência em game-core

`co/game-core/src/ed9f6f25/327a7380.rs:158` (`Universe::lobby`) — modelo a
imitar, removendo o portal `pointset` e ajustando posições para um layout
2x2 limpo.

## Critérios de aceitação

- [x] `yggdrasil-core/src/lobby.rs` exporta `pub fn lobby() -> Universe`.
- [x] 4 portais nas coordenadas: snake (10,8), tetris (30,8), invaders (10,12), poker (30,12).
- [x] Cada portal tem `Tile::Portal(slug)` onde slug = nome do jogo.
- [x] `Universe.objective.description` = `"Escolha um universo para entrar"`.
- [x] Teste unitário: `lobby().map.get_tile(10, 8) == Some(&Tile::Portal("snake"))`.
- [x] Teste de teleporte: `Session::new("yggdrasil").teleport_to("snake")` registra transição.

## Commit

`feat(YG-2): lobby com 4 portais (snake/tetris/invaders/poker)`

Bump: `0.0.1` → `0.1.0` apenas no release marker (YG-18). Aqui mantém `0.0.x`.
