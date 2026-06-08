# Release v1.2.0 — Catálogo expandido + Shandara

> Epic YG-68 — "Catálogo expandido de universos (pós-1.0)".

A v1.2.0 muda o framing do Yggdrasil: de "lobby com 6 jogos" para **catálogo de
universos abertos**, PT-BR first, com foco em RPG brasileiro.

## Destaques

### 🌳 Shandara — RPG open-source brasileiro (YG-69)

O grande destaque da v1.2.0 é **Shandara**: um universo de fantasia original,
servido como um **System Reference Document (SRD)** livre em PT-BR, licenciado
**CC-BY-SA 4.0**. Shandara prova a tese central da plataforma — *"um universo é
mais que um jogo: é um ecossistema expansível"* — entregando um "livro vivo"
navegável ao lado dos mini-games arcade.

- Mundo regido por **seis forças primordiais** (com uma sexta em aberto,
  documentada como questão a resolver).
- Dois povos completos (Verdejantes / Vida, Transmutos / Transformação),
  lore da Grande Guerra e a mecânica central de regras (atributos + criação
  de personagem).
- Licença dual: conteúdo CC-BY-SA 4.0, código MIT.
- Servido como **content reader** WASM (ABI v1), navegável seção a seção.

Cada versão futura (combate, magias, bestiário, aventura introdutória) é uma
release visível em `universes/universe-shandara/CHANGELOG.md`.

### 🗂 Catálogo de universos com filtros dinâmicos (YG-70)

- `universes/REGISTRY.yaml` — fonte da verdade de TODAS as entradas do catálogo
  (embedados, planejados, externos).
- `GET /api/v1/universos` agora faz o merge do REGISTRY com o runtime, com
  filtros por `status`, `type`, `origin`, `genre`, `license` e `search`.
- Página `/universos` ganha badges de situação (🟢 jogável / 🟡 planejado /
  🔗 externo) e filtros client-side.

### 🇧🇷 ~40 RPGs brasileiros no catálogo (YG-72)

O catálogo é semeado com ~30 RPGs nacionais open-source/indie (status `planned`,
candidatos a port) e os principais comerciais (status `external`, link out),
derivados de `docs/RPGs Brasileiros.md`. Visitantes veem a escala da promessa;
contribuidores veem onde podem ajudar (template
`.github/ISSUE_TEMPLATE/portar-rpg.md`).

## Documentação

- `docs/architecture/catalog.md` — schema do REGISTRY e contrato da API.
- `universes/universe-shandara/README.md` — o que é Shandara, licença dual,
  como contribuir.

## Notas

- Budget total dos WASM elevado de 2 MB → 3 MB para acomodar o conteúdo
  markdown do Shandara conforme o SRD cresce.
