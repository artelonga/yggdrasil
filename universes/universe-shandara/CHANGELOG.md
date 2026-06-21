# Changelog — universe-shandara

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/),
versionamento [SemVer](https://semver.org/lang/pt-BR/).

> Histórico rastreável via git pathspec (`git log -- universes/universe-shandara/`),
> jj-compatível. Ver `scripts/universe-changelog.sh` (YG-71).

## [Unreleased] <!-- regenerated via git pathspec; jj log universes/universe-shandara/ also works -->

### Other
- feat(YG-162): Shandara — stubs navegáveis de povos, combate, magia e bestiário (#101)
- feat(YG-68): catálogo expandido de universos — REGISTRY + filtros + Shandara (#39)

## [0.1.0] — 2026-06-08

### Added

- Crate `universe-shandara` — content reader WASM (ABI v1 do `universe-sdk`).
  Não é tick-based: navega seções de um SRD embedado em compile-time.
- `manifest()` com `capabilities: ["content", "rpg", "srd"]`.
- SRD em PT-BR (CC-BY-SA 4.0), v0.1.0:
  - **Mundo:** descrição canônica (`index.md`), `mundo/forcas-primordiais.md`
    (stub das seis forças + ⚠ questão em aberto sobre a sexta força),
    `mundo/grande-guerra.md` (lore inicial).
  - **Povos:** `povos/verdejantes.md` (Vida) e `povos/transmutos.md`
    (Transformação) completos; `povos/_index.md`.
  - **Regras:** `regras/atributos.md` (mecânica central — d6 pool, afinidade
    com forças) e `regras/criacao-personagem.md`.
  - **Placeholders:** `bestiario/_index.md` (TODO v0.3.0),
    `aventuras/_index.md` (TODO v0.4.0).
- Licença dual: código MIT (`src/`), conteúdo CC-BY-SA 4.0 (`content/`),
  SPDX `MIT AND CC-BY-SA-4.0` no `Cargo.toml`; `content/LICENSE-CONTENT.md`.
- README explicando o universo, a licença dual, o content reader e como
  contribuir.
- Testes nativos da lógica de leitura (`cargo test -p universe-shandara`).

### Open questions

- Sexta força primordial: a descrição canônica lista cinco (vida, matéria,
  tempo, transformação, energia) mas afirma "seis". Propostas em discussão:
  **Vazio** ou **Vínculo** — a confirmar com o autor antes do bump 1.0.0.
