# Changelog — universe-tetris

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
SemVer. Versionamento independente do core (ver `docs/UNIVERSE-VERSIONING.md`).

## [Unreleased] <!-- regenerated via git pathspec; jj log universes/universe-tetris/ also works -->

### Other
- chore(universes): per-universe changelog via git pathspec (YG-71)
- feat(YG-63): versionamento independente por universe (#38)
- fix(YG-104): reparar crates WASM v1.0 (drift universe-sdk) + lint Godot; CI bloqueante de novo (#40)
- feat(wasm): YG-57 — Migrate 5 existing universos to WASM (snake, tetris, invaders, pointset, poker) (#22)

## [1.0.0] — 2026-05-20

### Added
- Migração inicial para WASM via universe-sdk ABI v1 (parte de YG-57).
- Sete peças clássicas, rotação, gravidade e limpeza de linhas com pontuação.

### Changed
- Versionamento desacoplado do core a partir de YG-64 (era `0.1.0` literal).
