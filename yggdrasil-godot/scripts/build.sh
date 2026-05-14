#!/usr/bin/env bash
# build.sh — exporta o projeto yggdrasil-godot para web (HTML5) e Linux/X11
# headless. Roda a partir da raiz do projeto Godot.
#
# Uso:
#   ./scripts/build.sh             # exporta ambos os targets
#   ./scripts/build.sh web         # somente web
#   ./scripts/build.sh headless    # somente Linux/X11 headless
#
# Requer:
#   - Godot 4.5+ headless no $PATH (binário `godot`).
#   - Export templates 4.5 instalados (geralmente em
#     ~/.local/share/godot/export_templates/4.5.stable/).
#
# Não modifica o estado do repositório; saída vai para ./out/.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Detecta o binário do Godot. Aceita `godot` ou `godot4` no PATH.
GODOT_BIN=""
if command -v godot >/dev/null 2>&1; then
  GODOT_BIN="godot"
elif command -v godot4 >/dev/null 2>&1; then
  GODOT_BIN="godot4"
else
  echo "ERRO: Godot não encontrado no \$PATH." >&2
  echo "" >&2
  echo "Instale Godot 4.5 headless e adicione ao PATH. Veja README.md para detalhes." >&2
  echo "  - Linux: baixe de https://godotengine.org/download/linux/" >&2
  echo "  - Docker: o Dockerfile deste projeto já faz a instalação." >&2
  exit 1
fi

echo "Usando Godot: $($GODOT_BIN --version 2>&1 || echo 'unknown')"
echo "Project dir: ${PROJECT_DIR}"

cd "${PROJECT_DIR}"

mkdir -p out/web out/headless

target="${1:-all}"

build_web() {
  echo ""
  echo ">>> Exportando target Web (HTML5/wasm)..."
  "${GODOT_BIN}" --headless --path "${PROJECT_DIR}" \
    --export-release "Web" "out/web/index.html"
  echo ">>> Web export pronto em ${PROJECT_DIR}/out/web/"
}

build_headless() {
  echo ""
  echo ">>> Exportando target Linux/X11 (headless server)..."
  "${GODOT_BIN}" --headless --path "${PROJECT_DIR}" \
    --export-release "Linux/X11" "out/headless/yggdrasil-godot"
  echo ">>> Headless export pronto em ${PROJECT_DIR}/out/headless/yggdrasil-godot"
}

case "${target}" in
  web)
    build_web
    ;;
  headless|linux)
    build_headless
    ;;
  all|"")
    build_web
    build_headless
    ;;
  *)
    echo "ERRO: target desconhecido '${target}'. Use 'web', 'headless' ou 'all'." >&2
    exit 2
    ;;
esac

echo ""
echo "Build concluído."
