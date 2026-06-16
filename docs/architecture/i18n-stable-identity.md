# ADR — Identidade estável + i18n (tradução como camada, não espelho)

> Status: aceito (2026-06-16). Motivado pela revisão de `translator_sync.pyw`
> (sync PT→EN do vault do Miguel/`mse`) + a revisão de compat CO (YG-159).

## Contexto / problema

O `translator_sync.pyw` traduz um vault Obsidian gerando um **espelho `en/`**:
traduz **paths** (`translate_path`), traduz **conteúdo** (protegendo
frontmatter/código/links), e **reescreve wikilinks** pra resolverem dentro de
`en/` (`_att_index` + `resolve_wikilink_target`, partial-path match). Mantém um
cache de termos (`name_dict`) e um ciclo de sync (watchdog).

**Toda essa complexidade existe por uma causa raiz:** Obsidian/markdown usa
**display-name como identidade** (link e hierarquia por título/path). Ao traduzir,
a identidade muda → links quebram → precisa de espelho + reescrita heurística.

## Decisão

Separar três conceitos que o markdown funde:

| Conceito | Campo | Locale-dependente? |
|---|---|---|
| **Identidade** | `slug`/`key` (+ `parent` por id) | ❌ estável |
| **Display** | `title` | ✅ por locale |
| **Conteúdo** | `body` | ✅ por locale |
| **Layout** | `pos{x,y}` / path | ❌ (não é identidade) |

Consequências diretas:
- **Links apontam pro `slug` estável**, não pro título → traduzir **nunca quebra
  link**; o render resolve `slug → título-localizado`. (Elimina `resolve_wikilink_target`.)
- **Hierarquia por `parent`-id estável**, não por path traduzido → traduzir **não
  reformata a árvore**. (Elimina `translate_path` + a árvore-espelho.)
- **Conteúdo localizado por nó**: `title`/`body` por locale — uma árvore só, N renders.
- **Glossário = léxico**: o `name_dict` (PT→EN) vira entradas curáveis do universo
  **`comunicacao`** (que já é um léxico cross-linguístico), compartilhado e vivo.

Resultado: tradução vira **uma camada fina** ("gere `title/body` no locale X,
protegendo código/frontmatter/links"), não um motor de espelho de árvore.

## Passos habilitadores (os "1/2/3" da revisão YG-159 — pré-requisitos do i18n)

1. **Path estável id-based** (título localizado no render) — o path deixa de ser
   identidade; tradução não mexe na estrutura.
2. **`co-vault` lê `frontmatter.parent` (+ `pos.room`), não só o path** —
   hierarquia por id estável. **Primeiro passo, mais barato.**
3. **Convergir links em FK tipado (`links: [[slug]]`)** — link por id sobrevive à tradução.

O `co-vault` e o `loader.js` do Mundo já têm metade: o `loader` (YG-154) honra
`note.parent`/`pos.room` do `.md`; falta o `co-vault` fazer o mesmo (passo 2).

## UI

- **Toggle de idioma** no Mundo/instance (PT · EN · …): re-render do MESMO grafo
  com títulos/corpos no locale — caminha-se o MESMO mundo em inglês, mesma
  estrutura, só rótulos mudam. Sem "universo `en/`".
- **Link nunca quebra** ao trocar locale (resolve por slug).
- **Status de tradução por nó**: badge *autoral · MT · sem-tradução* no locale atual.
- **"Traduzir esta nota → EN"** inline (MT sob demanda, editável), com glossário do `comunicacao`.
- **Proteções de não-traduzível** (frontmatter/código/URL/embed/placeholder) herdadas
  do `.pyw`, mas server-side e por-nó.

## Migração

O vault do Miguel (`mse`/Alpha Scholars) hoje depende do `.pyw` externo (frágil,
espelho `en/`). Com 1/2/3 + i18n na plataforma, a tradução é nativa e o `.pyw` se
aposenta — sem espelho, sem links quebrados em rename, sem `.sync_map.json`.

## Sequência

1. **(2)** `co-vault` honra `frontmatter.parent`/`pos.room` (este PR).
2. **(3)** links por slug (FK tipado) no render/grafo.
3. **(1)** path estável id-based p/ árvore CO nativa.
4. **i18n MVP**: `title/body` por locale + toggle no Mundo + glossário no `comunicacao`.
5. **MT pipeline**: portar proteções + chunking do `.pyw` pro server, por-nó, sob demanda.
