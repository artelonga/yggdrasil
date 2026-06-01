#!/usr/bin/env bash
# godot.sh — integração CLI do Godot para o yggdrasil-godot.
#
# Resolve (ou provisiona) um binário Godot 4.5 headless e expõe subcomandos de
# CI/dev que NÃO exigem export templates: provisionamento, import e validação
# de GDScript. Exports (que exigem templates) continuam em build.sh.
#
# Uso:
#   ./scripts/godot.sh install     # baixa Godot 4.5 headless p/ cache local
#   ./scripts/godot.sh bin         # imprime o caminho do binário resolvido
#   ./scripts/godot.sh version     # imprime a versão do Godot resolvido
#   ./scripts/godot.sh import      # importa recursos (cria .godot/)
#   ./scripts/godot.sh check       # valida TODOS os .gd (parse-check headless)
#   ./scripts/godot.sh editor      # abre o editor
#   ./scripts/godot.sh run [scene] # roda uma cena headless (default: editor)
#
# Resolução do binário (primeiro que existir):
#   1. $GODOT_BIN                       (override explícito)
#   2. cache local .godot-bin/          (criado por `install`)
#   3. `godot` ou `godot4` no $PATH
#
# Versão fixada para casar com project.godot (config/features "4.5").

set -euo pipefail

GODOT_VERSION="${GODOT_VERSION:-4.5-stable}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
CACHE_DIR="${PROJECT_DIR}/.godot-bin"

# ── Resolução de plataforma ───────────────────────────────────────────────────
detect_asset() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "${os}" in
    Linux)
      case "${arch}" in
        x86_64|amd64) echo "Godot_v${GODOT_VERSION}_linux.x86_64.zip|Godot_v${GODOT_VERSION}_linux.x86_64" ;;
        aarch64|arm64) echo "Godot_v${GODOT_VERSION}_linux.arm64.zip|Godot_v${GODOT_VERSION}_linux.arm64" ;;
        *) echo "" ;;
      esac
      ;;
    Darwin)
      # macOS é universal; o binário fica dentro do .app
      echo "Godot_v${GODOT_VERSION}_macos.universal.zip|Godot.app/Contents/MacOS/Godot"
      ;;
    *) echo "" ;;
  esac
}

cached_bin_path() {
  echo "${CACHE_DIR}/${GODOT_VERSION}/godot"
}

# ── install ───────────────────────────────────────────────────────────────────
do_install() {
  local spec asset inner url dest
  spec="$(detect_asset)"
  if [[ -z "${spec}" ]]; then
    echo "ERRO: plataforma não suportada para download automático ($(uname -s)/$(uname -m))." >&2
    echo "Defina \$GODOT_BIN apontando para um Godot 4.5 manualmente." >&2
    exit 1
  fi
  asset="${spec%%|*}"
  inner="${spec##*|}"
  dest="$(cached_bin_path)"

  if [[ -x "${dest}" ]]; then
    echo "Godot já em cache: ${dest}"
    return 0
  fi

  url="https://github.com/godotengine/godot/releases/download/${GODOT_VERSION}/${asset}"
  local tmp
  tmp="$(mktemp -d)"
  echo ">>> Baixando ${url}"
  curl -fsSL -o "${tmp}/godot.zip" "${url}"
  echo ">>> Extraindo..."
  unzip -q "${tmp}/godot.zip" -d "${tmp}/extract"

  mkdir -p "$(dirname "${dest}")"
  cp "${tmp}/extract/${inner}" "${dest}"
  chmod +x "${dest}"
  rm -rf "${tmp}"

  # macOS: o binário baixado fica em quarentena (Gatekeeper) e é morto com
  # SIGKILL. Remove o atributo e assina ad-hoc para liberar execução headless.
  if [[ "$(uname -s)" == "Darwin" ]]; then
    xattr -dr com.apple.quarantine "${dest}" 2>/dev/null || true
    codesign --force --sign - "${dest}" 2>/dev/null || true
  fi

  echo ">>> Godot instalado em ${dest}"
  "${dest}" --version || true
}

# ── install-templates ─────────────────────────────────────────────────────────
# Baixa os export templates 4.5 (necessários para --export-release, ex. Web).
templates_dir() {
  local ver="${GODOT_VERSION%-*}.stable"  # 4.5-stable → 4.5.stable
  if [[ "$(uname -s)" == "Darwin" ]]; then
    echo "${HOME}/Library/Application Support/Godot/export_templates/${ver}"
  else
    echo "${HOME}/.local/share/godot/export_templates/${ver}"
  fi
}

do_install_templates() {
  local dir
  dir="$(templates_dir)"
  if [[ -f "${dir}/version.txt" ]]; then
    echo "Export templates já instalados em ${dir}"
    return 0
  fi
  local url="https://github.com/godotengine/godot/releases/download/${GODOT_VERSION}/Godot_v${GODOT_VERSION}_export_templates.tpz"
  mkdir -p "${dir}"
  local tmp
  tmp="$(mktemp -d)"
  echo ">>> Baixando export templates (~1.3 GB)..."
  curl -fsSL -o "${tmp}/t.tpz" "${url}"
  echo ">>> Extraindo para ${dir}"
  unzip -o -j "${tmp}/t.tpz" -d "${dir}" >/dev/null 2>&1
  rm -rf "${tmp}"
  echo ">>> Templates: $(cat "${dir}/version.txt" 2>/dev/null)"
}

# ── resolução ─────────────────────────────────────────────────────────────────
resolve_bin() {
  if [[ -n "${GODOT_BIN:-}" ]]; then
    echo "${GODOT_BIN}"; return 0
  fi
  local cached
  cached="$(cached_bin_path)"
  if [[ -x "${cached}" ]]; then
    echo "${cached}"; return 0
  fi
  if command -v godot >/dev/null 2>&1; then echo "godot"; return 0; fi
  if command -v godot4 >/dev/null 2>&1; then echo "godot4"; return 0; fi
  echo ""
}

require_bin() {
  local b
  b="$(resolve_bin)"
  if [[ -z "${b}" ]]; then
    echo "ERRO: Godot não encontrado." >&2
    echo "Rode './scripts/godot.sh install' ou defina \$GODOT_BIN." >&2
    exit 1
  fi
  echo "${b}"
}

# ── import ────────────────────────────────────────────────────────────────────
do_import() {
  local bin; bin="$(require_bin)"
  echo ">>> Importando recursos do projeto..."
  # --import roda o importador headless; --quit garante saída.
  "${bin}" --headless --path "${PROJECT_DIR}" --import --quit 2>&1 | tail -5 || true
}

# ── check (validação de GDScript) ─────────────────────────────────────────────
# Estratégia: um harness SceneTree (scripts/dev/lint.gd) carrega TODO .gd em
# contexto de runtime — autoloads registrados (sem falsos positivos de
# "Identifier not found") e cobrindo scripts não referenciados por cena. O Godot
# loga "SCRIPT ERROR"/"Failed to load" mas NÃO retorna não-nulo nem exit≠0 em
# erro de parse, então a fonte da verdade é o grep do log.
do_check() {
  local bin; bin="$(require_bin)"
  local harness="res://scripts/dev/lint.gd"
  if [[ ! -f "${PROJECT_DIR}/scripts/dev/lint.gd" ]]; then
    echo "ERRO: harness ${harness} ausente." >&2
    exit 1
  fi
  echo ">>> Godot: $("${bin}" --version 2>&1 | head -1)"
  echo ">>> Validando GDScript (harness em contexto de runtime)..."

  local log
  log="$(mktemp)"
  "${bin}" --headless --path "${PROJECT_DIR}" --script "${harness}" >"${log}" 2>&1 || true

  # erros reais de compilação/parse (|| true: grep sai 1 sem matches e mataria
  # o script sob `set -e`)
  local errors
  errors="$(grep -iE "SCRIPT ERROR|Parse Error|Failed to load|Compile Error|LINT-FAIL" "${log}" | sort -u || true)"

  grep -E "^LINT OK|^LINT FAILED" "${log}" || true

  if [[ -n "${errors}" ]]; then
    echo ""
    echo "FALHOU — erros de GDScript:"
    echo "${errors}" | sed 's/^/  /'
    rm -f "${log}"
    exit 1
  fi
  rm -f "${log}"
  echo "OK: nenhum erro de GDScript."
}

do_editor() {
  local bin; bin="$(require_bin)"
  exec "${bin}" --editor --path "${PROJECT_DIR}"
}

do_run() {
  local bin; bin="$(require_bin)"
  local scene="${1:-}"
  if [[ -n "${scene}" ]]; then
    exec "${bin}" --headless --path "${PROJECT_DIR}" --rendering-driver dummy "${scene}"
  fi
  exec "${bin}" --headless --path "${PROJECT_DIR}" --rendering-driver dummy
}

# ── dispatch ──────────────────────────────────────────────────────────────────
cmd="${1:-check}"
shift || true
case "${cmd}" in
  install) do_install ;;
  install-templates) do_install_templates ;;
  bin)     require_bin ;;
  version) b="$(require_bin)"; "${b}" --version ;;
  import)  do_import ;;
  check)   do_check ;;
  editor)  do_editor ;;
  run)     do_run "${1:-}" ;;
  *)
    echo "Uso: $0 {install|install-templates|bin|version|import|check|editor|run [scene]}" >&2
    exit 2
    ;;
esac
