# Changelog

Todas as mudanças relevantes ao projeto Yggdrasil. Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/). Versionamento: [SemVer](https://semver.org/lang/pt-BR/).

## [Unreleased]

## [0.1.0] — 2026-05-09

### Added
- `yggdrasil_core::lobby` — Universe 40x20 com 4 portais (snake/tetris/invaders/poker) em grid 2x2 (YG-2).
- Constantes públicas `lobby::slug::*` e `lobby::pos::*` para uso por adaptadores e testes.
- 8 testes unitários cobrindo dimensões, objetivo PT-BR, posição de cada portal, ausência de pointset, e transição via `Session::teleport_to`.

### Notes
- Pointset removido conforme decisão de produto (ver `YG-2.md`).
- Adaptadores que conectam cada portal aos jogos reais entram em YG-6..YG-9.

## [0.0.1] — 2026-05-09

### Added
- Bootstrap do workspace Rust (`yggdrasil-core`, `yggdrasil-web`).
- Importação do engine `co/game-core` via path dep.
- Estrutura `work/yggdrasil/` compatível com `co-auto`.
- Lista inicial de tarefas YG-1 .. YG-20 mapeando o caminho até `v1.0.0`.
- Documentos de visão (`docs/YGGDRASIL.docx`) e UX (`docs/Yggdrasil — Experiência do Usuário (UX).docx`) movidos para `docs/`.
- Registro de universo (`co-universes.yaml`) para inscrição via `co`.

### Notes
- `pointset` foi excluído do conjunto inicial de jogos (decisão de produto).
- Jogos planejados para o lobby v0.5.0: Snake, Tetris, Space Invaders, Poker.
