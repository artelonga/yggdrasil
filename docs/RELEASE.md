# Release & versionamento — Yggdrasil

> **Causa-raiz que isto corrige:** com várias sessões/agentes em paralelo, bumpar
> `Cargo.toml` em cada PR de feature faz vários PRs brigarem pela mesma linha de
> versão. Merges acabam sem bump → `main` e prod ficam com o mesmo rótulo mas
> conteúdos diferentes (aconteceu em 2026-06-14: YG-148/154/155/156 mergearam sem
> bump; prod e main ambos "2.30.0", divergentes). `/version` deixa de ser verdade.

## Regra: CHANGELOG-PENDING + commit de release

**Features e fixes NÃO bumpam a versão.** Cada tarefa escreve um **fragmento**
`CHANGELOG-PENDING/YG-<n>.md` (convenção já adotada na YG-94, espelha o `co`). Um
commit dedicado consolida e corta a versão, na hora do deploy.

> Por que fragmento por-arquivo (e não uma seção `[Unreleased]` compartilhada):
> uma seção única no `CHANGELOG.md` **ainda colide** quando vários PRs paralelos a
> editam. Um arquivo por tarefa em `CHANGELOG-PENDING/` nunca conflita. Ver
> `CHANGELOG-PENDING/README.md`.

### 1. Em todo PR de feature/fix
- Crie `CHANGELOG-PENDING/YG-<n>.md` (`## YG-<n> — título` + o que mudou, user-facing).
- **NÃO** altere `Cargo.toml` (`[workspace.package] version`) nem `CHANGELOG.md`.
- Resultado: PRs paralelos nunca colidem na versão nem no changelog.

### 2. Cortar um release (quando for deployar)
Um único commit `chore(release): X.Y.Z`:
1. Funde **todos** os `CHANGELOG-PENDING/*.md` numa nova seção `## [X.Y.Z] —
   AAAA-MM-DD — <resumo>` no topo do `CHANGELOG.md` (agrupe por Added/Changed/Fixed).
2. **Deleta** os fragmentos consumidos (mantém só `CHANGELOG-PENDING/README.md`).
3. Bumpa `Cargo.toml` → `version = "X.Y.Z"` (bump pelo maior tipo: qualquer `feat`
   → minor; só `fix`/`docs` → patch).
4. Esse é o **único** commit que toca a versão.

### 3. Deploy
- **Deploy só de um commit de release** (de `origin/main` sincronizada).
- Verificação inegociável: `curl https://yggdrasil-artelonga.fly.dev/version`
  == `X.Y.Z`. Se não bater, o deploy não pegou — não declarar feito.
- Contexto/comando: ver `~/.claude` memory `deploy-and-bridge-golive`
  (`flyctl deploy` de `~/projects` com whitelist `.dockerignore`).

## Por quê isto funciona
- `/version` volta a ser **fonte de verdade** (sempre = último release deployado).
- PRs paralelos só adicionam um arquivo em `CHANGELOG-PENDING/` — sem conflito.
- O release é um ato **único e auditável** (um commit `chore(release)`).
- Uma feature mergeada sem deploy é normal: fica em `[Unreleased]` até o próximo
  release cortá-la.
