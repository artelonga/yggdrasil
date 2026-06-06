#!/usr/bin/env bash
# e2e-comunicacao.sh — review end-to-end do universo **comunicação** (salas de
# léxico, YG-102) contra um servidor real, com autenticação real (magic-link),
# em DB/diretórios efêmeros.
#
# Cobre o caminho completo das rotas `/api/v1/comunicacao/*`:
#   login (magic-link) → criar sala (template yoruba) → publicar um termo no
#   léxico compartilhado → confirmar o `.md` no checkout COMUNICACAO_DIR →
#   registrar uma nota de revisão (SRS-lite).
#
# Uso:
#   bash scripts/e2e-comunicacao.sh            # builda se preciso, sobe, testa, derruba
#   TEMPLATE=mbya bash scripts/e2e-comunicacao.sh  # usa a sala Mbyá Guaraní
#
# O servidor escuta em :3030 (fixo); PORT existe só por simetria com
# e2e-editor.sh e deve casar com a porta real do binário.
#
# Saída: PASS/FAIL por etapa; em falha imprime a resposta HTTP ofensora e
# sai com código ≠ 0. Exit 0 só se todas as asserções passarem.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PORT:-3030}"
BASE="http://127.0.0.1:${PORT}"
API="${BASE}/api/v1/comunicacao"
EMAIL="e2e-comunicacao@yggdrasil.test"
TEMPLATE="${TEMPLATE:-yoruba}"

# Elemento semeado a publicar, por template. `ase` (àṣẹ) não está no léxico
# compartilhado vazio → vira contribuição de usuário (escopo=usuario).
case "${TEMPLATE}" in
  yoruba) ELEM="ase" ;;
  mbya)   ELEM="nhandereko" ;;
  *)      ELEM="ase" ;;
esac

# Binário do servidor: override via $YGGDRASIL_WEB_BIN (ex. target/release no CI).
WEB_BIN="${YGGDRASIL_WEB_BIN:-${ROOT}/target/debug/yggdrasil-web}"

TMP="$(mktemp -d)"
export YGGDRASIL_JWT_SECRET="e2e-secret-$$"
export YGGDRASIL_DB="${TMP}/ygg.db"
export YGGDRASIL_SEMENTES_DB="${TMP}/sementes.db"
export YGGDRASIL_INSTANCES_DIR="${TMP}/instances"
# Salas + filas de revisão (host-side) e o checkout do repo de léxico onde o
# write-back dos termos publicados acontece. Ambos efêmeros e isolados.
export YGGDRASIL_COMUNICACAO_DIR="${TMP}/comunicacao-rooms"
export COMUNICACAO_DIR="${TMP}/comunicacao-lexico"
# Garante modo dev do mail (código vai p/ stdout/log; SMTP desligado).
unset YGGDRASIL_SMTP_HOST || true
LOG="${TMP}/server.log"
SRV_PID=""

pass() { printf '  \033[32m✓\033[0m %s\n' "$1"; }
fail() { printf '  \033[31m✗ %s\033[0m\n' "$1"; cleanup; exit 1; }
step() { printf '\n\033[1m%s\033[0m\n' "$1"; }

cleanup() {
  [[ -n "${SRV_PID}" ]] && kill "${SRV_PID}" 2>/dev/null || true
  wait "${SRV_PID}" 2>/dev/null || true
}
trap cleanup EXIT

start_server() {
  "${WEB_BIN}" >"${LOG}" 2>&1 &
  SRV_PID=$!
  for _ in $(seq 1 40); do
    if curl -fsS -o /dev/null "${BASE}/health" 2>/dev/null; then return 0; fi
    sleep 0.25
  done
  echo "--- server log ---"; cat "${LOG}"
  fail "servidor não respondeu em ${BASE}/health"
}

jqp() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)"; }

# `req METHOD URL [curl-args...]` → ecoa o corpo; em status≥400 imprime a
# resposta ofensora e aborta. Captura status e corpo numa só chamada.
req() {
  local method="$1" url="$2"; shift 2
  local body http
  body="$(curl -sS -w $'\n%{http_code}' -X "${method}" "${url}" "$@")"
  http="${body##*$'\n'}"
  body="${body%$'\n'*}"
  if [[ "${http}" -ge 400 ]]; then
    printf '  \033[31m✗ %s %s → HTTP %s\033[0m\n' "${method}" "${url}" "${http}" >&2
    printf '%s\n' "${body}" >&2
    cleanup; exit 1
  fi
  printf '%s' "${body}"
}

# ── build se preciso ──────────────────────────────────────────────────────────
if [[ ! -x "${WEB_BIN}" ]]; then
  step "Buildando yggdrasil-web (debug)…"
  (cd "${ROOT}" && cargo build -p yggdrasil-web) || fail "build falhou"
fi

# ── 1. boot + login real (magic-link) ─────────────────────────────────────────
step "1. Subindo servidor + login real (magic-link)"
start_server
pass "servidor no ar (${BASE})"

curl -fsS -X POST "${BASE}/api/v1/auth/code" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\"}" -o /dev/null || fail "POST /auth/code falhou"
pass "código solicitado"

# o código vai para a SQLite (verify_codes); provedor de e-mail é stdout em dev
CODE="$(sqlite3 "${YGGDRASIL_DB}" "SELECT code FROM verify_codes WHERE email='${EMAIL}';" 2>/dev/null || true)"
if ! [[ "${CODE}" =~ ^[0-9]{6}$ ]]; then
  # fallback: extrai do log do servidor (corpo do e-mail: "… é: 123456")
  CODE="$(grep -oE 'é: [0-9]{6}' "${LOG}" | grep -oE '[0-9]{6}' | tail -1 || true)"
fi
[[ "${CODE}" =~ ^[0-9]{6}$ ]] || fail "código não obtido (sqlite/log)"
pass "código obtido: ${CODE}"

TOKEN="$(curl -fsS -X POST "${BASE}/api/v1/auth/verify" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\",\"code\":\"${CODE}\"}" | jqp "d['token']")"
[[ -n "${TOKEN}" && "${TOKEN}" != "None" ]] || fail "verify não retornou token"
AUTH="Authorization: Bearer ${TOKEN}"
pass "JWT obtido"

# ── 2. criar sala a partir do template ────────────────────────────────────────
step "2. Criar sala (template=${TEMPLATE})"
SALA="$(req POST "${API}/salas?template=${TEMPLATE}&title=Sala%20E2E&lang=yo" -H "${AUTH}")"
ID="$(echo "${SALA}" | jqp "d['id']")"
[[ -n "${ID}" && "${ID}" != "None" ]] || fail "create não retornou id"
NELEM="$(echo "${SALA}" | jqp "len(d['elements'])")"
[[ "${NELEM}" -ge 10 ]] || fail "esperava ≥10 elementos semeados, veio ${NELEM}"
TITLE="$(echo "${SALA}" | jqp "d['title']")"
[[ "${TITLE}" == "Sala E2E" ]] || fail "title não aplicado: ${TITLE}"
pass "sala ${ID} criada com ${NELEM} elementos (título: ${TITLE})"

# confirma que o elemento que vamos publicar existe no seed
HAS_ELEM="$(echo "${SALA}" | jqp "any(e['id']=='${ELEM}' for e in d['elements'])")"
[[ "${HAS_ELEM}" == "True" ]] || fail "elemento '${ELEM}' não está no seed do template ${TEMPLATE}"
pass "elemento '${ELEM}' presente no seed"

# ── 3. GET confirma a sala persistida (owner-only) ────────────────────────────
step "3. GET confirma a sala persistida"
GOT="$(req GET "${API}/salas/${ID}" -H "${AUTH}")"
echo "${GOT}" | jqp "d['id']" >/dev/null || fail "GET sala não retornou JSON válido"
pass "sala legível pelo dono"

# anônimo (sem JWT) não acessa sala não-publicada → 403
HTTP="$(curl -s -o /dev/null -w '%{http_code}' "${API}/salas/${ID}")"
[[ "${HTTP}" == "403" || "${HTTP}" == "401" ]] || fail "esperava 401/403 sem auth, veio ${HTTP}"
pass "sala não-publicada protegida (HTTP ${HTTP} sem auth)"

# ── 4. publicar termo no léxico (o "PUT" sala → léxico geral) ──────────────────
step "4. Publicar elemento '${ELEM}' no léxico compartilhado"
PUB="$(req POST "${API}/salas/${ID}/elementos/${ELEM}/publicar" -H "${AUTH}")"
CAMINHO="$(echo "${PUB}" | jqp "d['caminho']")"
ESCOPO="$(echo "${PUB}" | jqp "d['escopo']")"
REV_ADD="$(echo "${PUB}" | jqp "d['revisao_adicionada']")"
[[ -n "${CAMINHO}" && "${CAMINHO}" != "None" ]] || fail "publicar não retornou 'caminho'"
[[ "${REV_ADD}" == "True" ]] || fail "termo não foi enfileirado p/ revisão"
STATE="$(echo "${PUB}" | jqp "d['elemento']['lexicon']['state']")"
[[ "${STATE}" == "contributed" || "${STATE}" == "linked" ]] || fail "estado de léxico inesperado: ${STATE}"
pass "termo publicado → ${CAMINHO} (escopo=${ESCOPO}, state=${STATE})"

# ── 5. write-back em disco: o .md existe no checkout COMUNICACAO_DIR ───────────
step "5. Termo publicado existe em disco (COMUNICACAO_DIR)"
MD="${COMUNICACAO_DIR}/${CAMINHO}"
[[ -f "${MD}" ]] || fail "arquivo de léxico não encontrado em disco: ${MD}"
grep -q '^word:' "${MD}" || fail "frontmatter do termo sem 'word:' em ${MD}"
pass "markdown gravado: ${MD}"

# ── 6. revisão: a fila contém o termo, vencido agora ──────────────────────────
step "6. Fila de revisão contém o termo"
REV="$(req GET "${API}/revisao" -H "${AUTH}")"
TOTAL="$(echo "${REV}" | jqp "d['total']")"
VENC="$(echo "${REV}" | jqp "d['vencidos']")"
IN_QUEUE="$(echo "${REV}" | jqp "any(i['term_path']=='${CAMINHO}' for i in d['itens'])")"
[[ "${TOTAL}" -ge 1 ]] || fail "fila de revisão vazia (total=${TOTAL})"
[[ "${VENC}" -ge 1 ]] || fail "nenhum item vencido (vencidos=${VENC})"
[[ "${IN_QUEUE}" == "True" ]] || fail "termo '${CAMINHO}' não está na fila"
pass "fila com ${TOTAL} item(ns), ${VENC} vencido(s); termo presente"

# ── 7. registrar nota de revisão (acerto) → reagenda ──────────────────────────
step "7. Registrar nota de revisão (correct=true)"
NOTA="$(req POST "${API}/revisao/nota" -H "${AUTH}" -H 'Content-Type: application/json' \
  -d "{\"term_path\":\"${CAMINHO}\",\"correct\":true}")"
REPS="$(echo "${NOTA}" | jqp "d['reps']")"
INTERVAL="$(echo "${NOTA}" | jqp "d['interval_days']")"
[[ "${REPS}" -ge 1 ]] || fail "reps não incrementou (reps=${REPS})"
[[ "${INTERVAL}" -ge 1 ]] || fail "intervalo não reagendou (interval_days=${INTERVAL})"
pass "nota registrada → reps=${REPS}, interval_days=${INTERVAL}"

# nota em termo inexistente → 404
HTTP="$(curl -s -o /dev/null -w '%{http_code}' -X POST "${API}/revisao/nota" -H "${AUTH}" \
  -H 'Content-Type: application/json' -d '{"term_path":"nao/existe.md","correct":true}')"
[[ "${HTTP}" == "404" ]] || fail "esperava 404 p/ termo fora da fila, veio ${HTTP}"
pass "nota em termo inexistente rejeitada (404)"

printf '\n\033[32m\033[1mE2E OK — comunicação (salas de léxico) verificada ponta a ponta.\033[0m\n'
