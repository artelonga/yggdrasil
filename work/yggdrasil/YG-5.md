---
id: 5
title: "Input de mouse: click em tile = move + auto-entra em portal"
status: done
priority: high
type: feat
release: 0.1.0
parent: 18
blocked_by: [4]
labels:
  - frontend
  - input
  - lobby
  - acessibilidade
module: yggdrasil-web
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T13:35:10.941072+00:00
---

GIVEN o lobby aceita input de teclado (YG-4),
WHEN o usuário clica em qualquer tile do mapa,
THEN o avatar se move até esse tile (pathfinding simples) e, se o destino
for um portal, executa a mesma ação de Enter automaticamente — efeito
"clicar = ir + entrar".

## Critérios de aceitação

- [ ] Listener `canvas.onclick` calcula `(tileX, tileY)` do click.
- [ ] Pathfinding: BFS no grid, ignorando paredes. Limite 200 nós.
- [ ] Avatar anima passo-a-passo (50ms por tile) percorrendo o caminho.
- [ ] Se tile final for `Tile::Portal(_)`, dispara `POST /lobby/enter` ao chegar.
- [ ] Click em tile inacessível (cercado de paredes) não move o avatar; mensagem "Sem caminho" no rodapé.
- [ ] Acessibilidade: `aria-label` dinâmico no canvas anuncia "movendo para portal X".

## Commit

`feat(YG-5): mouse — click em tile move e auto-entra portal`
