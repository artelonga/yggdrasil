# Dependências externas — Yggdrasil

## game-core (artelonga/co)

`game-core` é importado via git rev pin no workspace `Cargo.toml`:

```toml
game-core = { git = "https://github.com/artelonga/co", rev = "<sha>" }
```

### Política de bump

| Situação | Ação |
|---|---|
| Bugfix no engine que afeta Yggdrasil | Pin para o novo SHA; `fix(deps):` no commit; bump de patch na versão do workspace. |
| Nova funcionalidade do engine necessária | Pin para o novo SHA; `chore(deps):` no commit; bump de patch. |
| Breaking change no engine | Pin para o novo SHA + ajustar código de adaptadores; `chore(deps)!:` (breaking); bump de minor (ou major se Yggdrasil ainda não está em 1.0). |
| Rotina trimestral de segurança | Avançar para HEAD do `co` main; validar com `cargo test --workspace`; commit `chore(deps):`. |

### Como atualizar o pin

1. Obter o SHA do commit alvo em `artelonga/co`:
   ```bash
   git -C ../co log --oneline -5
   git -C ../co rev-parse <tag-ou-hash-curto>
   ```
2. Substituir `rev = "..."` no `Cargo.toml` raiz pelo SHA completo (40 caracteres).
3. Rodar `cargo test --workspace` e `cargo clippy --workspace -- -D warnings`.
4. Commit: `chore(deps): bump game-core to <sha-curto>`.
5. Atualizar este documento se a política mudar.

### Rationale

O pin por SHA garante que CI passe em qualquer checkout limpo sem depender de um clone adjacente do repo `co`.
O risco de drift entre o ambiente local (que pode ter `../co/` diferente) e o CI é eliminado.
