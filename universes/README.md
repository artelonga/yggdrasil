# Universes — crates WASM do Yggdrasil

Cada universo é um crate Rust compilado para `wasm32-unknown-unknown` via
[`universe-sdk`](universe-sdk/) (ABI v1) e embarcado pelo `yggdrasil-web`.
Cada crate carrega **versão SemVer própria**, independente do core
(ver [`../docs/UNIVERSE-VERSIONING.md`](../docs/UNIVERSE-VERSIONING.md)).

## Catálogo

| Universo | Versão | Descrição | CHANGELOG |
|---|---|---|---|
| `universe-snake` | 1.0.0 | Snake — grid 40×20, pure-Rust | [CHANGELOG](universe-snake/CHANGELOG.md) |
| `universe-tetris` | 1.0.0 | Tetris — peças clássicas, linhas, score | [CHANGELOG](universe-tetris/CHANGELOG.md) |
| `universe-invaders` | 1.0.0 | Space Invaders — frota + tiros + ondas | [CHANGELOG](universe-invaders/CHANGELOG.md) |
| `universe-pointset` | 1.0.0 | PointSet — sandbox geométrico (descontinuado do catálogo) | [CHANGELOG](universe-pointset/CHANGELOG.md) |
| `universe-poker` | 0.8.0 | Texas Hold'em — multiplayer com sementes (em progresso) | [CHANGELOG](universe-poker/CHANGELOG.md) |
| `universe-vim` | 1.0.0 | Vim — emulador modal + 10 níveis | [CHANGELOG](universe-vim/CHANGELOG.md) |

Infra compartilhada:

| Crate | Versão | Descrição | CHANGELOG |
|---|---|---|---|
| `universe-sdk` | 0.1.0 | ABI v1 + macros (`universe_exports!`, `pkg_version!`) | [CHANGELOG](universe-sdk/CHANGELOG.md) |

> As versões acima vêm do `version` literal em cada `universe-*/Cargo.toml`
> (YG-64). Confirme com `cargo metadata --format-version 1 --no-deps`.

## Build

```bash
# Da raiz do repo
bash scripts/build-universes.sh           # compila + valida size budgets
bash scripts/build-universes.sh --skip-opt  # sem wasm-opt
```

A build imprime ao final uma tabela `Universe / Version / Size` (YG-66) e
embarca os `.wasm` em `yggdrasil-web/embedded/`.

## Versionamento

Quando e como bumpar major/minor/patch de um universo (vs. do core), formato de
tag git e o workflow passo-a-passo estão em
[`../docs/UNIVERSE-VERSIONING.md`](../docs/UNIVERSE-VERSIONING.md).
