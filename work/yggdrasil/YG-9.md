---
id: 9
title: "Adapter para Poker com sementes (WalletManager)"
status: done
priority: high
type: feat
release: 0.5.0
parent: 19
blocked_by: [8, 10]
labels:
  - games
  - adapter
  - wallet
  - sementes
module: yggdrasil-core
created_at: 2026-05-09T00:00:00Z
updated_at: 2026-05-09T14:21:32.285106+00:00
---

GIVEN o conceito de "sementes" está estabelecido (YG-10) e os outros 3
jogos rodam (YG-8),
WHEN integro `PokerGame` usando `WalletManager` para buy-in/cash-out
denominados em sementes,
THEN o usuário precisa ter sementes para sentar à mesa, ganha/perde
sementes, e o saldo persiste.

## Referência em game-core

- `co/game-core/src/cd7ac4c6/fba2eac3/` — pasta do `PokerGame`.
- `co/game-core/src/49a25f9f/e8d44050.rs` — `WalletManager`.
- `co/game-core/src/bin/6ca5cab7/10f5f77e/fba2eac3.rs` — fluxo CLI buy-in/cash-out (espelho).
- `co/co-web/static/games/poker.js`.

## Critérios de aceitação

- [ ] `yggdrasil-core/src/games/poker.rs` com `pub struct YggPoker`.
- [ ] Saldo inicial: 10.000 sementes (alinhado com `INITIAL_BALANCE` em `co-web/src/game_routes.rs:17`).
- [ ] Buy-in e cash-out usam `WalletManager` mas com chave de tabela `sementes` (ver YG-10 para naming).
- [ ] Recusa entrada se `saldo == 0` com mensagem PT-BR "Sem sementes para apostar".
- [ ] Histórico de mãos opcional (reusa `HandRecorder` se houver tempo; se não, deferir).

## Commit

`feat(YG-9): adapter Poker com sementes via WalletManager`
