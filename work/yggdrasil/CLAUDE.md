# Espaço Yggdrasil — Instruções para Claude / co-auto

## Contexto

Este espaço (`work/yggdrasil/`) é o board de desenvolvimento do Yggdrasil. Cada arquivo `YG-<n>.md` é uma user-story com critérios de aceitação em formato GIVEN/WHEN/THEN.

## Como executar uma tarefa

1. Ler o arquivo `YG-<n>.md` integralmente, incluindo `parent` e `module`.
2. Verificar dependências (`blocks` / `blocked_by` no frontmatter, se presentes) — não executar tarefa bloqueada.
3. Implementar **estritamente** o que está nos critérios de aceitação. Não adicionar features fora do escopo.
4. Atualizar `CHANGELOG.md` (seção `[Unreleased]`) descrevendo o que mudou.
5. Bump de versão se aplicável (ver `CLAUDE.md` raiz, regra SemVer).
6. Commit conventional: `<tipo>(YG-<n>): <descrição>`.
7. Marcar `status: done` no frontmatter da tarefa.

## Mapeamento de tipos de mudança → bump

| Tipo do commit | Bump | Exemplo |
|---|---|---|
| `feat` | minor | YG-2 (lobby Universe) → 0.1.0 |
| `fix` | patch | correção de bug → 0.1.1 |
| `refactor` | patch | renomear módulo → 0.1.2 |
| `docs`/`chore`/`test` | patch | docs → 0.1.3 |
| Release marcador (YG-18, 19, 20) | major/minor explícito | tag `v0.5.0` |

## Princípios

- **Reuso > Reescrita.** Se algo existe em `co/game-core`, importe via `game_core::...`.
- **Estabilidade do engine.** Não modificar arquivos do `co` a partir do Yggdrasil. Se faltar algo no engine, abrir issue no `co`, congelar dependência por hash, e prosseguir.
- **PT-BR primeiro.** Toda copy de UI em PT. Chaves i18n como `lobby.titulo`, `jogos.snake.descricao`.
- **Sem heroísmos.** Cada tarefa cabe em um commit ou PR; se não couber, dividir antes de começar.

## Validação automatizada

Antes de fechar uma tarefa, rodar:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Os 3 devem passar. Falhas viram pré-condição da próxima tarefa, nunca `--no-verify`.
