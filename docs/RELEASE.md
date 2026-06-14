# Release & versionamento — Yggdrasil

> **Causa-raiz que isto corrige:** com várias sessões/agentes em paralelo, bumpar
> `Cargo.toml` em cada PR de feature faz vários PRs brigarem pela mesma linha de
> versão. Merges acabam sem bump → `main` e prod ficam com o mesmo rótulo mas
> conteúdos diferentes (aconteceu em 2026-06-14: YG-148/154/155/156 mergearam sem
> bump; prod e main ambos "2.30.0", divergentes). `/version` deixa de ser verdade.

## Regra: CHANGELOG-PENDING + commit de release

**Features e fixes NÃO bumpam a versão.** Elas só entram no CHANGELOG sob
`## [Unreleased]`. A versão é cortada por um commit dedicado, na hora do deploy.

### 1. Em todo PR de feature/fix
- Adicione a mudança em `CHANGELOG.md`, sob `## [Unreleased]` (crie a seção se não
  existir, no topo).
- **NÃO** altere `Cargo.toml` (`[workspace.package] version`).
- Resultado: PRs paralelos nunca colidem na linha de versão.

### 2. Cortar um release (quando for deployar)
Um único commit `chore(release): X.Y.Z`:
1. Renomeia `## [Unreleased]` → `## [X.Y.Z] — AAAA-MM-DD — <resumo>`.
2. Adiciona um novo `## [Unreleased]` vazio no topo.
3. Bumpa `Cargo.toml` → `version = "X.Y.Z"` (escolha o bump pelo maior tipo no
   Unreleased: qualquer `feat` → minor; só `fix`/`docs` → patch).
4. Esse é o **único** commit que toca a versão.

### 3. Deploy
- **Deploy só de um commit de release** (de `origin/main` sincronizada).
- Verificação inegociável: `curl https://yggdrasil-artelonga.fly.dev/version`
  == `X.Y.Z`. Se não bater, o deploy não pegou — não declarar feito.
- Contexto/comando: ver `~/.claude` memory `deploy-and-bridge-golive`
  (`flyctl deploy` de `~/projects` com whitelist `.dockerignore`).

## Por quê isto funciona
- `/version` volta a ser **fonte de verdade** (sempre = último release deployado).
- PRs paralelos só anexam linhas ao `[Unreleased]` — sem conflito de versão.
- O release é um ato **único e auditável** (um commit `chore(release)`).
- Uma feature mergeada sem deploy é normal: fica em `[Unreleased]` até o próximo
  release cortá-la.
