# Changelog — universe-vim

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
SemVer. Versionamento independente do core (ver `docs/UNIVERSE-VERSIONING.md`).

## [Unreleased] <!-- regenerated via git pathspec; jj log universes/universe-vim/ also works -->

### Added
- YG-58 — emulador modal Rust + 10 níveis progressivos (#23)

### Other
- chore(universes): per-universe changelog via git pathspec (YG-71)
- feat(YG-63): versionamento independente por universe (#38)

## [1.0.0] — 2026-05-22

### Added
- Emulador modal (normal/insert/visual) em Rust puro, parte de YG-58.
- 10 níveis progressivos de aprendizado do editor.

### Changed
- Versionamento desacoplado do core a partir de YG-64 (deixou de usar
  `version.workspace = true`).
