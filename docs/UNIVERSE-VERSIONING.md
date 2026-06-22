# Versionamento de universos (per-universe)

Cada crate em [`universes/`](../universes/) tem **SemVer própria**, CHANGELOG
próprio e tag git própria, independentes do core (`yggdrasil-web`/
`yggdrasil-core`). Mudar `universe-poker` sobe `universe-poker-v0.9.0` sem
forçar um bump do core de `2.5.0` → `2.6.0`.

Introduzido pelo épico **YG-63** (YG-64 Cargo.toml · YG-65 CHANGELOG ·
YG-66 propagação de versão · YG-67 este documento).

## Por que

- Libera o ritmo de release dos universos do ciclo do core: ajuste de UI de um
  jogo não precisa bumpar a plataforma inteira.
- Telemetria (YG-62) consegue segmentar sessões por *versão* do universo
  (detecção de regressão de balanceamento, A/B testing).
- Trampolim para o futuro split em repos separados (YG-54, v2.x).

## Onde a versão vive

| Fonte | Papel |
|---|---|
| `universes/universe-<id>/Cargo.toml` → `version` | **Fonte da verdade** (literal, sem herança de `workspace.package`). |
| `universes/universe-<id>/CHANGELOG.md` | Histórico legível, Keep a Changelog PT-BR. |
| `universe_sdk::pkg_version!()` | Macro que expande para o `CARGO_PKG_VERSION` do crate consumidor; usado em `Universe::manifest()` para derivar `manifest().version` do Cargo.toml. |
| `GET /api/v1/universos` → campo `version` | Superfície pública; reflete a SemVer do crate. |
| Tag git `universe-<id>-v<X.Y.Z>` | Marca o ponto de release no monorepo. |

> **Estado atual:** a runtime de produção roda os jogos via `game-core` nativo
> (não via WASM), então `universos_routes.rs::universo_list()` ainda traz a
> `version` como string literal — mantida em sincronia manual com o `Cargo.toml`
> do crate. Quando o caminho WASM (YG-104) destravar, `version` passa a derivar
> de `manifest().version` (= `pkg_version!()`) automaticamente.

## Quando bumpar — universo

Segue SemVer aplicado ao contrato observável do universo (manifest + JSON de
tick + eventos emitidos):

| Bump | Quando |
|---|---|
| **major** (`X`) | Quebra do input/output JSON do `tick`, mudança incompatível de manifest, remoção de funcionalidade que clientes consumiam. |
| **minor** (`Y`) | Novo nível/conteúdo, nova capability no manifest, novo evento via `emit_event` (consumível mas opcional). |
| **patch** (`Z`) | Bug fix, ajuste de balanceamento, melhoria visual sem mudança de protocolo. |

## Quando bumpar — core (quando um universo muda)

| Mudança no universo | Bump no core |
|---|---|
| patch/minor de um universo | **patch** do core (rebuild WASM, novo binário; sem mudança de API). |
| major de um universo | **minor** do core (mudança comportamental visível para clientes). |
| Mudança no `universe-sdk` (ABI v1) | **major** do core (todas as universes recompilam). |

O `universe-sdk` segue SemVer normal: um bump = mudança no contrato da ABI.

## Convenção de tags git

| Alvo | Formato | Exemplo |
|---|---|---|
| Core | `v<X.Y.Z>` (existente) | `v2.5.0` |
| Universo | `universe-<id>-v<X.Y.Z>` | `universe-poker-v0.9.0` |

`git log --tags --oneline` lista todos os releases (core + universos) numa linha
do tempo única.

## Workflow — shipar uma mudança num universo

1. Mudar o código em `universes/universe-<id>/`.
2. Bumpar `version` em `universes/universe-<id>/Cargo.toml` (regra acima).
3. Adicionar a entrada em `universes/universe-<id>/CHANGELOG.md`.
4. Atualizar `universes/README.md` (coluna de versão).
5. Se a runtime servir o catálogo nativamente, sincronizar `version` em
   `yggdrasil-web/src/universos_routes.rs::universo_list()`.
6. Bumpar o core (geralmente **patch**) — via `CHANGELOG-PENDING/` na wave atual
   (não edite `Cargo.toml`/`CHANGELOG.md` raiz diretamente em waves paralelas;
   ver [`../CHANGELOG-PENDING/README.md`](../CHANGELOG-PENDING/README.md)).
7. Commit Conventional: `feat(universe-<id>): descrição`.
8. Tag dupla (universo + core), ou só a do universo se o bump do core ainda
   estiver pendente na wave.

### Exemplo end-to-end

`universe-poker` recebe um fix no deadlock da AI dos bots:

```bash
# 1-3. código + Cargo.toml (0.8.0 → 0.8.1) + CHANGELOG
$EDITOR universes/universe-poker/src/lib.rs
$EDITOR universes/universe-poker/Cargo.toml      # version = "0.8.1"
$EDITOR universes/universe-poker/CHANGELOG.md    # ## [0.8.1] — fix bot AI deadlock

# 4-5. README + (se aplicável) o catálogo nativo
$EDITOR universes/README.md
$EDITOR yggdrasil-web/src/universos_routes.rs    # version: "0.8.1"

# 6. core: fragmento de changelog (patch) na wave
$EDITOR CHANGELOG-PENDING/YG-XYZ.md

# 7. commit
git add universes/universe-poker yggdrasil-web/src/universos_routes.rs \
        universes/README.md CHANGELOG-PENDING/YG-XYZ.md
git commit -m "fix(universe-poker): resolve deadlock da AI dos bots"

# 8. tags (patch de universo → patch do core)
git tag universe-poker-v0.8.1
git tag v2.5.1                # se o bump do core for consolidado agora
git push origin HEAD --tags
```

## Tooling — `scripts/universe-changelog.sh`

Script shell para gerar ou atualizar o `CHANGELOG.md` de cada universo a partir do
histórico filtrado do monorepo via `git log -- universes/universe-<slug>/`. Sem
submodules. Compatível com jujutsu (`jj log <path>`).

```bash
# Gerar/atualizar [Unreleased] de um universo
bash scripts/universe-changelog.sh shandara

# Fazer para todos os universos rastreados (status: embedded + versions_tracked: true)
bash scripts/universe-changelog.sh --all

# Congelar [Unreleased] como versão X.Y.Z e bumpar Cargo.toml
bash scripts/universe-changelog.sh shandara --bump minor

# Verificar se algum CHANGELOG está atrás dos commits no path (CI / pre-commit)
bash scripts/universe-changelog.sh --check
```

### Como funciona

1. Lê `version` do `universes/universe-<slug>/Cargo.toml`.
2. Acha a tag mais recente do universo: `git tag --list 'universe-<slug>-v*' | sort -V | tail -1`.
3. Roda `git log <last-tag>..HEAD -- universes/universe-<slug>/`.
4. Classifica commits por tipo Conventional (`feat`→Added, `fix`→Fixed,
   `refactor`/`chore`/`docs`/`test`→Changed) com escopo `universe-<slug>`,
   `<slug>`, ou qualquer combinação. Commits que tocaram o path mas sem escopo
   explícito vão em "Other".
5. Reescreve a seção `[Unreleased]` do CHANGELOG do universo (ou cria se não
   existe).
6. Com `--bump`: renomeia `[Unreleased]` → `[X.Y.Z] — <hoje>` e bumpa
   `Cargo.toml`. Valida que o novo SemVer é maior que o atual.

### Modo `--check` (CI / lefthook)

`--check` retorna exit 1 se qualquer universo com `versions_tracked: true` no
`REGISTRY.yaml` tem commits não refletidos em `[Unreleased]`. Não edita nada.

Como hook `lefthook` opcional:

```yaml
pre-commit:
  commands:
    universe-changelog:
      glob: "universes/universe-*/**/*"
      run: bash scripts/universe-changelog.sh --check
```

### jj-compatible

Nenhuma operação destrutiva. O script só lê o histórico git. Se `jj` estiver no
`PATH`, um comentário HTML é adicionado ao cabeçalho `[Unreleased]` indicando o
equivalente `jj log universes/universe-<slug>/`.

## Ver também

- [`../universes/README.md`](../universes/README.md) — catálogo + build.
- [`../CHANGELOG-PENDING/README.md`](../CHANGELOG-PENDING/README.md) — convenção
  de fragmentos de changelog para waves paralelas.
- [`../scripts/universe-changelog.sh`](../scripts/universe-changelog.sh) — script
  de tooling per-universe changelog (YG-71).
