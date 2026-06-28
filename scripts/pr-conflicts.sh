#!/usr/bin/env bash
#
# pr-conflicts.sh — revisa conflitos de merge dos PRs abertos contra a base.
#
# Para cada PR aberto: pergunta ao GitHub o estado de merge E faz um merge-tree
# local (dry-run, não toca a árvore de trabalho) para listar os ARQUIVOS que
# conflitam de fato. Também sinaliza PRs que tocam os mesmos arquivos entre si
# (conflito provável quando um merge depois do outro).
#
# Uso:
#   scripts/pr-conflicts.sh            # todos os PRs abertos
#   scripts/pr-conflicts.sh <pr> ...   # só os PRs listados
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"
command -v gh >/dev/null 2>&1 || { echo "erro: gh não encontrado" >&2; exit 1; }

BASE="$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name 2>/dev/null || echo main)"
git fetch -q origin "$BASE"
BASE_REF="origin/$BASE"

# PRs a examinar
if [ $# -ge 1 ]; then NUMS="$*"; else
  NUMS="$(gh pr list --state open --json number -q '.[].number' | tr '\n' ' ')"
fi
[ -n "${NUMS// }" ] || { echo "✓ nenhum PR aberto — nada para conferir contra $BASE."; exit 0; }

echo "== conflitos vs $BASE =="
declare -a TOUCHED_KEYS=()   # "pr:arquivo" p/ detectar sobreposição entre PRs

for n in $NUMS; do
  read -r BRANCH STATE MERGEABLE < <(gh pr view "$n" \
    --json headRefName,mergeable,mergeStateStatus \
    -q '[.headRefName, .mergeStateStatus, .mergeable] | @tsv') || { echo "PR #$n: não encontrado"; continue; }
  git fetch -q origin "$BRANCH" 2>/dev/null || { echo "PR #$n ($BRANCH): branch some no remoto"; continue; }

  # merge-tree dry-run: lista arquivos com marcadores de conflito
  base_sha="$(git merge-base "$BASE_REF" "origin/$BRANCH")"
  conflicts="$(git merge-tree --write-tree --name-only "$BASE_REF" "origin/$BRANCH" 2>/dev/null | tail -n +2 || true)"
  # fallback p/ git antigo sem --write-tree
  if [ -z "$conflicts" ]; then
    conflicts="$(git merge-tree "$base_sha" "$BASE_REF" "origin/$BRANCH" 2>/dev/null \
                 | grep -E '^\+<{7}|^changed in both' -A0 >/dev/null 2>&1 && echo "(ver merge-tree)" || true)"
  fi

  if [ -n "$conflicts" ]; then
    echo "✗ PR #$n ($BRANCH)  [gh: $STATE/$MERGEABLE]  CONFLITA:"
    echo "$conflicts" | sed 's/^/      /'
  else
    echo "✓ PR #$n ($BRANCH)  [gh: $STATE/$MERGEABLE]  sem conflito vs $BASE"
  fi

  # registra arquivos tocados (p/ sobreposição entre PRs)
  while IFS= read -r f; do [ -n "$f" ] && TOUCHED_KEYS+=("$n:$f"); done < <(
    git diff --name-only "$base_sha" "origin/$BRANCH" 2>/dev/null)
done

# sobreposição entre PRs: mesmo arquivo tocado por >1 PR
echo
echo "== arquivos tocados por mais de um PR (conflito provável ao empilhar) =="
printf '%s\n' "${TOUCHED_KEYS[@]:-}" | awk -F: 'NF==2{ f=$2; sub(/^[0-9]+:/,"",f); prs[f]=prs[f]" #"$1 } END{
  hot=0; for (k in prs){ nseen=gsub(/#/,"#",prs[k]); if(nseen>1){ print "  "k" →"prs[k]; hot=1 } }
  if(!hot) print "  (nenhum)" }'
