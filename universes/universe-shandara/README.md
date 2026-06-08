# universe-shandara

**Shandara** é um universo Yggdrasil do tipo "livro vivo": um **System Reference
Document (SRD)** completo, em PT-BR, de um mundo de fantasia original regido por
**seis forças primordiais**. Não é um jogo arcade tick-based — é um **navegador
de conteúdo** (content reader) servido como WASM pela plataforma.

> Esta é a **v0.1.0** — um scaffold com a lore inicial, dois povos e a mecânica
> central. Combate, magias, bestiário, aventuras e os povos restantes chegam em
> versões seguintes (cada bump é uma release, ver `CHANGELOG.md`).

## O que é Shandara

Um mundo de fantasia projetado para ser a base de experiências interativas —
RPGs de mesa, jogos digitais, histórias. Seu diferencial é ser estruturado em
torno de forças primordiais (vida, matéria, tempo, transformação, energia — e
uma sexta em aberto) que moldam ambiente, culturas, criaturas e conflitos. Ver
[`content/index.md`](content/index.md) para a descrição canônica completa.

## Licença dual

Esta crate tem **duas licenças**, por design:

| Parte                     | Licença          | Onde                          |
|---------------------------|------------------|-------------------------------|
| **Conteúdo** (`content/`) | **CC-BY-SA 4.0** | `content/LICENSE-CONTENT.md`  |
| **Código** (`src/`)       | **MIT**          | igual aos outros universos    |

O `Cargo.toml` declara a expressão SPDX `MIT AND CC-BY-SA-4.0`.

- O **conteúdo** é CC-BY-SA para garantir que adaptações (digitais, impressas,
  derivadas) permaneçam livres — alinhado ao ethos "free software para mundos
  imaginários". Mestres podem usar Shandara em mesas reais, sem royalties.
- O **código** é MIT, como os demais crates de universo.

## Estrutura

```
universe-shandara/
├── Cargo.toml            # version 0.1.0, license = "MIT AND CC-BY-SA-4.0"
├── CHANGELOG.md
├── README.md             # este arquivo
├── src/
│   └── lib.rs            # content reader (ABI v1 do universe-sdk)
└── content/              # o SRD em markdown PT-BR (CC-BY-SA 4.0)
    ├── LICENSE-CONTENT.md
    ├── index.md
    ├── mundo/{forcas-primordiais,grande-guerra}.md
    ├── povos/{_index,verdejantes,transmutos}.md
    ├── regras/{atributos,criacao-personagem}.md
    ├── bestiario/_index.md   # TODO v0.3.0
    └── aventuras/_index.md   # TODO v0.4.0
```

## Como funciona o content reader

Implementa a ABI v1 do `universe-sdk` com semântica de navegação:

- `create({ "section": "mundo/forcas-primordiais" })` — abre uma sessão de
  leitura apontando para uma seção (default: `index`).
- `tick({ "action": "navigate", "to": "regras/atributos" })` — retorna
  `{ "section", "markdown", "exists", "sections" }`.
- `manifest()` — `capabilities: ["content", "rpg", "srd"]`.

O markdown é embedado em compile-time via `include_str!`.

## Como contribuir

Contribuições de **conteúdo** (lore, povos, regras, aventuras) são bem-vindas e
ficam sob CC-BY-SA 4.0. Boas primeiras tarefas:

- Resolver a **questão em aberto** da sexta força (ver
  `content/mundo/forcas-primordiais.md`).
- Detalhar um povo vinculado à Matéria, ao Tempo ou à Energia.
- Esboçar a aventura introdutória *"A Cicatriz da Guerra"*.

Abra uma issue/PR no repositório `artelonga/yggdrasil` descrevendo a seção que
pretende adicionar ou expandir.

## Estado

- [x] v0.1.0 — lore inicial, 2 povos, mecânica central (atributos + criação)
- [ ] v0.2.0 — combate + magias
- [ ] v0.3.0 — bestiário completo
- [ ] v0.4.0 — aventura introdutória
- [ ] v1.0.0 — seis forças confirmadas, todos os povos, SRD pronto para mesas
