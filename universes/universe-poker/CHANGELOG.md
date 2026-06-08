# Changelog — universe-poker

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
SemVer. Versionamento independente do core (ver `docs/UNIVERSE-VERSIONING.md`).

## [0.8.0] — 2026-05-20

### Added
- Migração inicial para WASM via universe-sdk ABI v1 (parte de YG-57).
- Texas Hold'em com mesa de bots e sementes.

### Changed
- Versionamento desacoplado do core a partir de YG-64 (era `0.1.0` literal).

### Known issues
- Multiplayer ainda incompleto: deadlock da AI dos bots (YG-26). Por isso a
  versão inicial é `0.8.0`, não `1.0.0`.
