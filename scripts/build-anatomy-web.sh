#!/usr/bin/env bash
# build-anatomy-web.sh — exporta o visualizador 3D de anatomia (Godot) para a
# web e o coloca onde o yggdrasil-web serve estático (YG-84).
#
# Resultado: yggdrasil-web/static/anatomia/{index.html,index.wasm,index.pck,...}
# servido em /anatomia (→ /static/anatomia/). Single-thread → sem COOP/COEP.
#
# Pré-requisitos:
#   - Godot 4.5 headless: yggdrasil-godot/scripts/godot.sh install
#   - Export templates 4.5: yggdrasil-godot/scripts/godot.sh install-templates
#       (ou baixe Godot_v4.5-stable_export_templates.tpz manualmente)
#   - Malhas: yggdrasil-godot/scripts/fetch-anatomy.sh
#
# Uso: bash scripts/build-anatomy-web.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GODOT="$("${ROOT}/yggdrasil-godot/scripts/godot.sh" bin)"
OUT="${ROOT}/yggdrasil-web/static/anatomia"

if [[ ! -f "${ROOT}/yggdrasil-godot/assets/anatomia/brain.obj" ]]; then
  echo "ERRO: malhas ausentes. Rode primeiro: yggdrasil-godot/scripts/fetch-anatomy.sh" >&2
  exit 1
fi

mkdir -p "${OUT}"
echo ">>> importando recursos..."
"${GODOT}" --headless --path "${ROOT}/yggdrasil-godot" --import >/dev/null 2>&1 || true
echo ">>> exportando preset Web (single-thread)..."
"${GODOT}" --headless --path "${ROOT}/yggdrasil-godot" \
  --export-release "Web" "${OUT}/index.html"

echo ">>> pronto. Servido por yggdrasil-web em /anatomia"
du -sh "${OUT}"
ls "${OUT}"
