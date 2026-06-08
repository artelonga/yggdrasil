---
name: Portar RPG para universe-WASM
about: Proponha portar um RPG do catálogo (status planned) para um universo Yggdrasil
title: "Portar <slug>"
labels: ["content", "rpg", "module:universes", "port"]
---

## Universo a portar

- **Slug no REGISTRY:** `<slug>`  <!-- ex.: tagmar -->
- **Título:** <!-- ex.: Tagmar 3 -->
- **Card do catálogo:** /universos (filtro situação: 🟡 planejado)

## Checklist de port

> Cada item destravado vira um passo concreto. O card já existe no
> `universes/REGISTRY.yaml` com `status: planned`; o port o promove a
> `status: embedded` quando o crate `universe-<slug>` estiver em produção.

- [ ] **Contato com o criador** — autor(es) ciente(s) e de acordo com a inclusão
- [ ] **Licença confirmada** — open-source declarada / permissão por escrito
      (preencher `license:` no REGISTRY com a SPDX correta)
- [ ] **SRD em markdown** — conteúdo navegável em `universes/universe-<slug>/content/`
      (PT-BR, mesmo padrão de `universe-shandara`)
- [ ] **Crate `universe-<slug>`** — `Cargo.toml` + `src/lib.rs` (content reader
      ABI v1, ou tick-based se aplicável), compila para `wasm32-unknown-unknown`
- [ ] **Budget WASM** — adicionado a `scripts/build-universes.sh` dentro do orçamento
- [ ] **Testes** — `cargo test -p universe-<slug>` passa
- [ ] **REGISTRY atualizado** — entrada movida de `planned` → `embedded`,
      `versions_tracked: true`
- [ ] **CHANGELOG** — entrada `[0.1.0]` em `universes/universe-<slug>/CHANGELOG.md`

## Contexto

<!-- Links: site oficial, repositório do sistema, PDF/SRD, contato do criador. -->

## Notas de licenciamento

<!-- Comercial proprietário? Então o destino é `status: external` (link out),
     não um port. Indie/open-source com permissão? Então `planned` → `embedded`. -->
