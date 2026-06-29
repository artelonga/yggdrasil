# Promover `comunicação` a universo próprio + desacoplar Yggdrasil

> **Status:** plano (não executado). Revisão de 2026-06-29 sobre `co`, `comunicacao`,
> `topologia` e o acoplamento de Yggdrasil. Decisão do dono: **escrever o plano**;
> mudanças em `co`/`topologia` ficam como **instruções** (não aplicadas por agente
> Yggdrasil sem autorização — ver `CLAUDE.md`: "não modificar `co` a partir do Yggdrasil").

## Pergunta

"Devemos promover `comunicação` a seu próprio universo?"

**Resposta curta:** `comunicação` **já é** um universo (sub-universo de `co`,
`parent_key='co'`) e **já é** a topologia de sentido cross-linguística. "Promover" não
é criar — são três limpezas: (1) dar-lhe **superfície própria**, (2) resolver a
**confusão de identidade** com o repo `topologia`, (3) **desacoplar** o Yggdrasil de
ler os arquivos do disco. A (1) custa ~1 linha; a (3) também resolve o problema de
deploy ("onde está live").

## Achados (revisão multi-repo, com evidência)

### A. O modelo de universo do `co` torna a promoção quase grátis
- `_universe.yaml` → `Manifest` (`co/core/src/manifest.rs:101-132`): `schema_version`,
  `name`, `parent`, `surface_dns`, `visibility`, `content_types`, `imports` (YAML opaco,
  papéis via `co://` URIs).
- Universo = manifest + **linha no DB** (`co/co-web/src/content/models.rs:373-443`):
  `key`, `visibility`, `parent_key` (CO-98), `surface_dns` (CO-338), `local_repo_path`,
  `content_subdirs`, etc.
- **Embedded vs first-class**: `parent_key` setado = aninhado (URL herdada);
  `parent_key=NULL` **ou** `surface_dns` setado = unidade deployável.
- **Promoção = ~1 linha** (`co-web/.../migrations/v084.rs`, resolução em
  `co/core/src/surface.rs:197-261`):
  ```sql
  UPDATE universes SET surface_dns = 'comunicacao.artelonga.com.br' WHERE key='comunicacao';
  ```
  **Zero reescrita de entradas**: referências (`source: cadogan-1959-ayvu`) e wikilinks
  (`[[comunicacao::…]]`) resolvem em tempo de leitura/render via `core::surface`
  (`rewrite_surface_links`). O `imports`/papel é o "anchor de promoção recursiva"
  (CO-153/CO-95): trocar o alvo de um papel = uma linha no manifest.
- **API de conteúdo já existe**: `GET /api/v1/universes/{slug}`, `…/{slug}/entries`,
  `…/{slug}/manifest`, `GET /api/v1/universes/public`, `GET /api/v1/resolve?ref=…`.
- **Build**: board interativo (co-web SPA) + site estático Quartz (`co construir` +
  `fly deploy` por universo).

### B. Há uma confusão de identidade `comunicação` ⟷ `topologia`
Não são duplicatas. `topologia` veio primeiro (CO-141, 2026-05-01); `comunicacao` foi
**extraído** dele (2026-05-06); depois **todo o conteúdo migrou** para `comunicacao`:
- **`comunicação` = canônico, ativo** (último commit 2026-06-21): `guarani-mbya/lexicon.mbya.json`
  (657 KB), `yoruba/lexicon.yo.json` (857 KB), `corpus/ayvu-rapyta.json` (1.3 MB — bitext
  Mbyá↔Espanhol de Cadogan, **as traduções ES que a YG-179 usa**), `sources/` (bibliografia),
  `concepts/`, `languages/`, `spanish/`. Links cross-universe para `mbya` (Arandu).
- **`topologia` = casca obsoleta** (idle desde 2026-06-21 09:11): conteúdo esvaziado
  (9 termos guarani desatualizados, yoruba zerado, sem JSON, sem corpus). Único valor
  vivo: os **crates Rust** — `topologia-core` (tipos `Term`/`Concept` + traits
  `LanguagePlane`/`ConceptPlane`), `topologia-co-adapter` (lê markdown CO → `Term`),
  `topologia-mbya-adapter` (lê `mbya_lexicon.db`).
- **Problema de nome**: o manifest de `comunicação` ainda se descreve como "i18n + chat +
  chamadas" (legado, enganoso — é a topologia de sentido); o de `topologia` ainda afirma
  "Topologia da Linguagem" (não é mais). **O nome certo está no repo errado.**
- **`mbya` (Arandu)** = léxico Mbyá profundo canônico (4000+ entradas, `mbya_lexicon.db`).

### C. Yggdrasil está acoplado por disco aos arquivos de `comunicação`
- A feature topologia (YG-175..179) lê `COMUNICACAO_DIR/.../lexicon.*.json` +
  `YGGDRASIL_MBYA_DB` direto do disco no boot, via `public::lexicon_slice`
  (`yggdrasil-web/src/topologia.rs:102-172`; módulo `yggdrasil-core/src/comunicacao/`).
- **Reimplementa** identidade/loja de termos que `topologia-core` já modela (dois
  conceitos incompatíveis de "Term"). A feature **não** está no `UniverseRegistry` do
  Yggdrasil — é um serviço de API sobre os arquivos, roteado à mão em `main.rs`.
- Hard-couplings que quebram se `comunicação` mudar de lugar / for servido pelo `co`:
  caminho `COMUNICACAO_DIR`, `public::lexicon_slice` (lê layout em disco), abertura do
  `mbya_lexicon.db`. Hoje o catálogo é `OnceLock` síncrono no boot, sem invalidação.

## Arquitetura-alvo

> **`comunicação` (no `co`) = universo de conteúdo canônico** — fonte única de léxico +
> corpus + traduções + fontes, com superfície própria. **Topologia do Yggdrasil = a
> lente interativa** (grafo, camada pessoal, lentes de língua) que **consome** isso.

Espelha o bridge CO↔Yggdrasil já em produção, e faz o Yggdrasil de produção **consultar
a API do `co`** em vez de embutir 2 MB de JSON + um SQLite no deploy (resolve o "onde
está live").

## Plano de migração (ordem por valor)

### Passo 1 — Promover + corrigir identidade  ·  *lado `co`/`topologia` (INSTRUÇÕES)*
> Não executado por este agente (escopo Yggdrasil-only). Aplicar você / autorizar depois.
1. **Superfície**: `UPDATE universes SET surface_dns='comunicacao.artelonga.com.br'
   WHERE key='comunicacao';` (+ CNAME DNS; opcional app Fly p/ site estático).
2. **Manifest**: reescrever `comunicacao/_universe.yaml` `description` para o que é (a
   topologia de sentido / léxico+corpus cross-linguístico); remover o enquadramento
   "chat/i18n/chamadas" (legado). Considerar renomear (`name`) para algo como
   "Léxico Compartilhado" / "Topologia da Linguagem" — herdando o nome do repo obsoleto.
3. **`topologia` (repo)**: manter **só os crates** (`topologia-core` + adapters) como lib
   de tipos/adaptadores compartilhada; **arquivar** os dirs de conteúdo obsoletos
   (`guarani-mbya/`, `yoruba/`, …). Opcional: renomear o repo p/ `topologia-rs` para
   deixar claro que é código, não conteúdo.

### Passo 2 — Desacoplar Yggdrasil  ·  *lado Yggdrasil (IMPLEMENTÁVEL aqui)*
Introduzir um trait de carregamento, mantendo o disco como impl padrão:
```rust
pub trait LexiconLoader {
    fn load_nodes(&self) -> Vec<ResolvedNode>;
    fn resolve_node(&self, id: &str) -> Option<ResolvedNode>;
    fn sentences(&self, lang: Option<&str>, q: Option<&str>, limit: usize) -> Vec<SentenceRow>;
    // verses_for / examples_for / tr …
}
```
- `DiskLexiconLoader` (atual): `COMUNICACAO_DIR` + `mbya_lexicon.db` (comportamento de hoje).
- `CoUniverseLoader` (futuro): lê a API do `co` (`GET /{slug}/entries`, `/manifest`) →
  drop-in. Catálogo passa a poder recarregar (invalidação) em vez de `OnceLock` fixo.
- **Ganho de deploy**: produção Yggdrasil consulta `co` em vez de precisar dos arquivos
  embutidos. Resolve a pendência "onde está live" para a topologia.

### Passo 3 — Convergir o modelo de Term  ·  *refator, DEFERIR*
Unificar `yggdrasil-core/src/comunicacao` sobre os tipos de `topologia-core` (um
vocabulário só de `Term`/`Concept`). Só depois de 1–2 estabilizarem.

## Riscos & sequência
- **Promoção (passo 1) é segura e reversível** (uma coluna no DB); fazer primeiro.
- **Renomear repo `topologia`** pode ter referências em outros lugares (crates path-dep,
  CI) — grep antes.
- **Passo 2** muda a fonte de verdade: validar paridade Disco↔API (mesmos nós, mesmas
  sentenças/traduções) com testes antes de trocar o default em produção.
- As **traduções ES** (Cadogan) já vivem em `comunicacao/corpus/ayvu-rapyta.json` e no
  `mbya_lexicon.db` (`alignments`) — a YG-179 lê do DB; ao migrar p/ API, expor as
  traduções como campo da entrada do verso.

## O que NÃO muda
Conteúdo (entradas, `imports`, referências) — promoção é data-driven. Mbyá profundo
segue canônico em `mbya` (Arandu). A camada pessoal/tier-grátis/lentes da YG-178/179
é do Yggdrasil e não depende de onde o léxico é servido.
