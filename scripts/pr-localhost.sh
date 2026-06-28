#!/usr/bin/env bash
#
# pr-localhost.sh — sobe um deployment localhost ISOLADO para um PR (ou branch).
#
# Cada PR roda no seu próprio git worktree, numa porta determinística e com data
# dir próprio (DB/sementes/comprovantes/comunicação) — vários PRs lado a lado,
# sem clobber. Serve para revisar um PR de verdade no navegador e comparar PRs
# que conflitam, antes de qualquer deploy no Fly.
#
# Uso:
#   scripts/pr-localhost.sh <pr-number|branch>     # sobe (build + run, foreground)
#   scripts/pr-localhost.sh <pr-number|branch> --build-only
#   scripts/pr-localhost.sh <pr-number|branch> --port 8123
#   scripts/pr-localhost.sh <pr-number|branch> --stop   # mata o processo + remove worktree
#   scripts/pr-localhost.sh --list                       # deployments ativos
#
# Notas:
# - Porta = 8100 + (pr_number | hash(branch)) % 800, salvo --port.
# - JWT secret de dev default (YGGDRASIL_JWT_SECRET); sobreponha exportando antes.
# - Worktrees ficam em .worktrees/<slug>; data em .worktrees/_data/<slug>.
# - CARGO_TARGET_DIR é compartilhado (target/) → build incremental rápido; o
#   binário já carregado em memória sobrevive a um rebuild de outro PR.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

DATA_BASE="$REPO_ROOT/.worktrees/_data"

die() { echo "erro: $*" >&2; exit 1; }
usage() { sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-1}"; }

# ── --list ───────────────────────────────────────────────────────────────────
if [ "${1:-}" = "--list" ]; then
  echo "deployments localhost (worktrees):"
  git worktree list | grep -F "/.worktrees/" || echo "  (nenhum)"
  echo
  echo "portas em uso (faixa 8100-8899):"
  lsof -nP -iTCP -sTCP:LISTEN 2>/dev/null | awk '/:8[1-8][0-9][0-9] /{print "  "$1" "$9}' || true
  exit 0
fi

[ $# -ge 1 ] || usage
REF="$1"; shift || true

STOP=0; BUILD_ONLY=0; PORT_OVERRIDE=""
while [ $# -gt 0 ]; do
  case "$1" in
    --stop)       STOP=1 ;;
    --build-only) BUILD_ONLY=1 ;;
    --port)       PORT_OVERRIDE="${2:-}"; shift ;;
    -h|--help)    usage 0 ;;
    *) die "flag desconhecida: $1" ;;
  esac
  shift
done

command -v gh >/dev/null 2>&1 || die "gh (GitHub CLI) não encontrado"

# ── resolve REF → branch + slug + chave de porta ─────────────────────────────
if [[ "$REF" =~ ^[0-9]+$ ]]; then
  BRANCH="$(gh pr view "$REF" --json headRefName -q .headRefName 2>/dev/null)" \
    || die "PR #$REF não encontrado"
  SLUG="pr-$REF"
  PORT_KEY="$REF"
else
  BRANCH="$REF"
  SLUG="branch-$(printf '%s' "$REF" | tr '/ ' '--' | tr -cd 'a-zA-Z0-9-')"
  PORT_KEY="$(printf '%s' "$REF" | cksum | cut -d' ' -f1)"
fi

WT="$REPO_ROOT/.worktrees/$SLUG"
DATA="$DATA_BASE/$SLUG"
PORT="${PORT_OVERRIDE:-$((8100 + PORT_KEY % 800))}"

# ── --stop ───────────────────────────────────────────────────────────────────
if [ "$STOP" = 1 ]; then
  if pid="$(lsof -nP -tiTCP:"$PORT" -sTCP:LISTEN 2>/dev/null)"; then
    echo "matando processo na porta $PORT (pid $pid)…"; kill "$pid" 2>/dev/null || true
  fi
  if [ -d "$WT" ]; then
    echo "removendo worktree ${WT}…"; git worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
  fi
  echo "data preservado em $DATA (apague à mão se quiser zerar)."
  exit 0
fi

# ── fetch + worktree (detached no HEAD do PR) ────────────────────────────────
echo "→ PR/branch: $BRANCH   slug: $SLUG   porta: $PORT"
git fetch -q origin "$BRANCH" || die "falha ao buscar origin/$BRANCH"
if [ -d "$WT" ]; then
  echo "→ atualizando worktree existente…"
  git -C "$WT" reset -q --hard "origin/$BRANCH"
else
  echo "→ criando worktree em ${WT}…"
  git worktree add -q --detach "$WT" "origin/$BRANCH"
fi

mkdir -p "$DATA/comprovantes" "$DATA/comunicacao"

# ── env isolado ──────────────────────────────────────────────────────────────
export PORT="$PORT"
export YGGDRASIL_JWT_SECRET="${YGGDRASIL_JWT_SECRET:-dev-localhost-secret}"
export YGGDRASIL_DB="$DATA/yggdrasil.db"
export YGGDRASIL_SEMENTES_DB="$DATA/sementes.db"
export YGGDRASIL_COMPROVANTES_DIR="$DATA/comprovantes"
export YGGDRASIL_COMUNICACAO_DIR="$DATA/comunicacao"
# target compartilhado: build incremental entre PRs (rápido)
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"

cd "$WT"
echo "→ cargo build -p yggdrasil-web…"
cargo build -q -p yggdrasil-web || die "build falhou em $BRANCH"

if [ "$BUILD_ONLY" = 1 ]; then
  echo "✓ build OK ($BRANCH). Pronto para rodar:  PORT=$PORT cargo run -p yggdrasil-web  (em $WT)"
  exit 0
fi

echo
echo "✓ subindo $BRANCH → http://localhost:$PORT   (Ctrl-C para parar; --stop para limpar)"
echo
exec cargo run -q -p yggdrasil-web
