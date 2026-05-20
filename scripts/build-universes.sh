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
    BUDGET="${BUDGETS[$i]}"
    if [ "$SIZE" -gt "$BUDGET" ]; then
        echo "❌ $U.wasm: ${SIZE}B > budget ${BUDGET}B" >&2
        exit 1
    fi
    echo "✅ $U.wasm: ${SIZE}B (budget: ${BUDGET}B)"
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
echo ""
echo "All universos compiled. Run 'cargo build' to embed them."
