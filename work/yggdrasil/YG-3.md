---
id: 3
title: "Renderização <canvas> do lobby"
status: todo
priority: critical
type: feat
release: 0.1.0
parent: 18
blocked_by: [2]
labels:
  - frontend
  - canvas
  - lobby
module: yggdrasil-web
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T00:00:00Z
---

GIVEN o lobby Universe está construído (YG-2),
WHEN o servidor expõe `GET /api/v1/lobby` retornando JSON do Universe e o
frontend `static/lobby.js` desenha o tile-grid em um `<canvas>`,
THEN ao acessar `/` o usuário vê o mapa com 4 portais visíveis e legendas em PT-BR.

## Referência em game-core

`co/game-core/src/ed9f6f25/6bd52b20.rs` — `Renderer` (terminal). **Não copiar**:
o renderer terminal usa `crossterm`. O frontend Yggdrasil é canvas. A
referência aqui é apenas o mapeamento `Tile -> char/cor` que deve ser
replicado em JS.

## Critérios de aceitação

- [ ] `GET /api/v1/lobby` em `yggdrasil-web/src/lobby_routes.rs` retorna `serde_json` do `Universe`.
- [ ] `yggdrasil-web/static/lobby.html` carrega `lobby.js`.
- [ ] `static/lobby.js` desenha grid 40x20 (16px por tile) em canvas.
- [ ] Cores: parede `#1a1a2e`, portal `#d4af37` com símbolo do jogo, vazio `#0d0d12`.
- [ ] Legenda fixa abaixo do canvas: "🐍 Snake · 🟦 Tetris · 👾 Invaders · 🃏 Poker" (em PT-BR sem emoji se preferir, ver `i18n/pt.yaml`).
- [ ] `index.html` redireciona para `/lobby` ou serve o lobby.

## Commit

`feat(YG-3): renderização <canvas> do lobby`
