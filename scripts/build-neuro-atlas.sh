#!/usr/bin/env bash
# Gera o atlas Precomputed (núcleos subcorticais Harvard-Oxford via nilearn) em
# yggdrasil-web/static/neuro-data/ho-sub para o viewer Neuroglancer em /neuro.
# Reproduz dados gerados (gitignored). Requer python3.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV="${NEURO_VENV:-$ROOT/.neuro-venv}"
if [ ! -x "$VENV/bin/python" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q --disable-pip-version-check cloud-volume igneous-pipeline nilearn nibabel
fi
cd "$ROOT"
"$VENV/bin/python" scripts/convert_atlas.py
MESH="yggdrasil-web/static/neuro-data/ho-sub/mesh"
for f in "$MESH"/*.gz; do [ -e "$f" ] && gunzip -c "$f" > "${f%.gz}" && rm "$f"; done  # NG estático: fragmentos sem .gz
echo ">>> pronto: yggdrasil-web/static/neuro-data/ho-sub  (servido em /neuro)"
