#!/usr/bin/env bash
# scripts/build-universes.sh — Compile all 6 universe WASM crates and validate size budgets.
#
# Usage: bash scripts/build-universes.sh [--skip-opt]
#
# Outputs optimised .wasm files to yggdrasil-web/embedded/.
# Run before `cargo build` to replace build.rs stubs with real game WASMs.
#
# Requirements:
#   - Rust toolchain with wasm32-unknown-unknown target
#       rustup target add wasm32-unknown-unknown
#   - wasm-opt (optional but recommended — skipped if absent)
#       cargo install wasm-opt --locked   (CI)
#       brew install binaryen             (macOS local)
#       apt-get install binaryen          (Debian/Ubuntu)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
UNIVERSES_DIR="$REPO_ROOT/universes"
OUT="$REPO_ROOT/yggdrasil-web/embedded"
TARGET=wasm32-unknown-unknown
SKIP_OPT="${1:-}"

UNIVERSES=(snake tetris invaders pointset poker vim)
BUDGETS=(300000 300000 300000 200000 600000 250000)   # bytes

mkdir -p "$OUT"

# YG-66: ler a versão de cada crate do Cargo.toml (fonte: cargo metadata) para
# anexar à tabela final. Cada universo carrega sua própria SemVer (YG-64).
declare -A VERSIONS
declare -a SIZES
crate_version() {
    # $1 = nome do crate (ex.: universe-snake). Lê de cargo metadata; fallback
    # para grep no Cargo.toml se python3/cargo-metadata indisponível.
    local crate="$1"
    if command -v python3 &>/dev/null; then
        cargo metadata --format-version 1 --no-deps --manifest-path "$UNIVERSES_DIR/Cargo.toml" 2>/dev/null \
            | python3 -c "import json,sys;d=json.load(sys.stdin);print(next((p['version'] for p in d['packages'] if p['name']=='$crate'),'?'))" 2>/dev/null \
            || echo "?"
    else
        grep -m1 '^version' "$UNIVERSES_DIR/$crate/Cargo.toml" 2>/dev/null \
            | sed -E 's/version[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/' || echo "?"
    fi
}

echo "==> Building universe WASM crates (target: $TARGET)"
cd "$UNIVERSES_DIR"

for i in "${!UNIVERSES[@]}"; do
    U="${UNIVERSES[$i]}"
    echo "  -> cargo build -p universe-$U --target $TARGET --release"
    cargo build -p "universe-$U" --target "$TARGET" --release

    SRC="target/$TARGET/release/universe_${U//-/_}.wasm"
    DST="$OUT/$U.wasm"

    if [ "$SKIP_OPT" = "--skip-opt" ] || ! command -v wasm-opt &>/dev/null; then
        cp "$SRC" "$DST"
        echo "  $U: copied (wasm-opt not available or skipped)"
    else
        wasm-opt -O3 -s 100 "$SRC" -o "$DST"
    fi

    SIZE=$(wc -c < "$DST")
    SIZES[$i]="$SIZE"
    VERSIONS[$U]="$(crate_version "universe-$U")"
    BUDGET="${BUDGETS[$i]}"
    if [ "$SIZE" -gt "$BUDGET" ]; then
        echo "❌ $U.wasm: ${SIZE}B > budget ${BUDGET}B" >&2
        exit 1
    fi
    echo "✅ $U.wasm: ${SIZE}B (budget: ${BUDGET}B) — v${VERSIONS[$U]}"
done

echo ""
echo "==> Total size gate (budget: 2MB = 2097152B)"
TOTAL=0
for f in "$OUT"/*.wasm; do
    TOTAL=$((TOTAL + $(wc -c < "$f")))
done
MAX_TOTAL=2097152
if [ "$TOTAL" -gt "$MAX_TOTAL" ]; then
    echo "❌ Total WASM: ${TOTAL}B > budget 2MB (${MAX_TOTAL}B)" >&2
    exit 1
fi
echo "✅ Total: ${TOTAL}B / ${MAX_TOTAL}B"

# YG-66: tabela final nome/versão/tamanho. A versão vem do Cargo.toml de cada
# crate (versionamento independente — YG-64). Consumida por auditoria de release
# e cruzada com `GET /api/v1/universos`.
echo ""
printf "%-12s %-9s %s\n" "Universe" "Version" "Size"
for i in "${!UNIVERSES[@]}"; do
    U="${UNIVERSES[$i]}"
    KB=$(( (${SIZES[$i]} + 1023) / 1024 ))
    printf "%-12s %-9s %s KB\n" "$U" "${VERSIONS[$U]}" "$KB"
done

echo ""
echo "All universos compiled. Run 'cargo build' to embed them."
