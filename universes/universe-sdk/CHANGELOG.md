# Changelog — universe-sdk

Formato: [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).
SemVer. Um bump de versão do SDK = mudança no contrato da ABI v1 — todas as
universes precisam recompilar (ver `docs/UNIVERSE-VERSIONING.md`).

## [0.1.0] — 2026-05-20

### Added
- ABI v1: tipos primitivos (`pack`/`unpack`), `UniverseManifest`, trait
  `Universe`, helpers de memória e o macro `universe_exports!` (YG-57).
- Macro `pkg_version!()` — expande para o `CARGO_PKG_VERSION` do crate
  consumidor, para `manifest().version` ser derivada do `Cargo.toml` (YG-66).
