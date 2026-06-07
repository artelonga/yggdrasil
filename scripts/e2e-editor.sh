#!/usr/bin/env bash
# e2e-editor.sh — review end-to-end do editor de universos (YG-73) contra um
# servidor real, com autenticação real (magic-link), em DB/instances efêmeros.
#
# Cobre o caminho completo do backend host-neutral que web e Godot consomem:
#   login → criar de template → upload de anexo → EditOps → persistência →
#   restart do servidor → re-leitura do disco.
#
# Uso:
#   bash scripts/e2e-editor.sh            # builda se preciso, sobe, testa, derruba
#   PORT=3055 bash scripts/e2e-editor.sh  # porta alternativa
#
# Saída: PASS/FAIL por etapa; exit≠0 se qualquer asserção falhar.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PORT:-3030}"
BASE="http://127.0.0.1:${PORT}"
API="${BASE}/api/v1"
EMAIL="e2e@yggdrasil.test"

# Binário do servidor: override via $YGGDRASIL_WEB_BIN (ex. target/release no CI).
WEB_BIN="${YGGDRASIL_WEB_BIN:-${ROOT}/target/debug/yggdrasil-web}"

TMP="$(mktemp -d)"
export YGGDRASIL_JWT_SECRET="e2e-secret-$$"
export YGGDRASIL_DB="${TMP}/ygg.db"
export YGGDRASIL_SEMENTES_DB="${TMP}/sementes.db"
export YGGDRASIL_INSTANCES_DIR="${TMP}/instances"
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

stop_server() {
  kill "${SRV_PID}" 2>/dev/null || true
  wait "${SRV_PID}" 2>/dev/null || true
  SRV_PID=""
}

jqp() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)"; }

# ── build se preciso ──────────────────────────────────────────────────────────
if [[ ! -x "${WEB_BIN}" ]]; then
  step "Buildando yggdrasil-web (debug)…"
  (cd "${ROOT}" && cargo build -p yggdrasil-web) || fail "build falhou"
fi

# ── 1. boot + login real (magic-link) ─────────────────────────────────────────
step "1. Subindo servidor + login real (magic-link)"
start_server
pass "servidor no ar (${BASE})"

curl -fsS -X POST "${API}/auth/code" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\"}" -o /dev/null || fail "POST /auth/code falhou"
pass "código solicitado"

# o código vai para a SQLite (verify_codes) — provedor de e-mail é stdout em dev
CODE="$(sqlite3 "${YGGDRASIL_DB}" "SELECT code FROM verify_codes WHERE email='${EMAIL}';" 2>/dev/null || true)"
if ! [[ "${CODE}" =~ ^[0-9]{6}$ ]]; then
  # fallback: extrai do log do servidor
  CODE="$(grep -oE 'é: [0-9]{6}' "${LOG}" | grep -oE '[0-9]{6}' | tail -1 || true)"
fi
[[ "${CODE}" =~ ^[0-9]{6}$ ]] || fail "código não obtido (sqlite/log)"
pass "código obtido: ${CODE}"

TOKEN="$(curl -fsS -X POST "${API}/auth/verify" -H 'Content-Type: application/json' \
  -d "{\"email\":\"${EMAIL}\",\"code\":\"${CODE}\"}" | jqp "d['token']")"
[[ -n "${TOKEN}" && "${TOKEN}" != "None" ]] || fail "verify não retornou token"
AUTH="Authorization: Bearer ${TOKEN}"
pass "JWT obtido"

# ── 2. criar instância a partir do template neuroanatomia ─────────────────────
step "2. Criar instância (template=neuroanatomia)"
INST="$(curl -fsS -X POST "${API}/instances?template=neuroanatomia" -H "${AUTH}")"
ID="$(echo "${INST}" | jqp "d['id']")"
[[ -n "${ID}" ]] || fail "create não retornou id"
LAYERS="$(echo "${INST}" | jqp "len(d['layers'])")"
[[ "${LAYERS}" == "3" ]] || fail "esperava 3 camadas no seed, veio ${LAYERS}"
pass "instância ${ID} criada com 3 camadas + conexões do seed"

# ── 3. upload de anexo (SVG) + ligar a um landmark ────────────────────────────
step "3. Upload de anexo (SVG) + attach_content"
SVG="${TMP}/corpo.svg"
printf '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>' >"${SVG}"
HASH="$(curl -fsS -X POST "${API}/instances/${ID}/attachments" -H "${AUTH}" \
  -F "file=@${SVG};type=image/svg+xml" | jqp "d['hash']")"
[[ -n "${HASH}" ]] || fail "upload não retornou hash"
pass "blob content-addressed: ${HASH:0:12}…"

curl -fsS -X PATCH "${API}/instances/${ID}" -H "${AUTH}" -H 'Content-Type: application/json' \
  -d "{\"op\":\"attach_content\",\"layer\":\"landmarks\",\"block_id\":\"cortex\",\"content\":{\"kind\":\"image\",\"hash\":\"${HASH}\",\"filename\":\"corpo.svg\",\"mime\":\"image/svg+xml\",\"size\":80}}" \
  -o /dev/null || fail "attach_content falhou"
pass "anexo ligado ao landmark 'cortex'"

# ── 4. EditOps: place_block, edit_layer (transparência), add_connection ───────
step "4. EditOps (place_block · edit_layer opacity · add_connection)"
curl -fsS -X PATCH "${API}/instances/${ID}" -H "${AUTH}" -H 'Content-Type: application/json' \
  -d '{"op":"place_block","layer":"landmarks","block":{"id":"hipocampo","block_type":"landmark","pos":{"x":10,"y":5},"label":"Hipocampo"}}' \
  -o /dev/null || fail "place_block falhou"
pass "bloco 'hipocampo' colocado"

curl -fsS -X PATCH "${API}/instances/${ID}" -H "${AUTH}" -H 'Content-Type: application/json' \
  -d '{"op":"edit_layer","layer":"snc","opacity":0.2}' -o /dev/null || fail "edit_layer falhou"
pass "opacity da camada SNC → 0.2 (toggle de transparência)"

curl -fsS -X PATCH "${API}/instances/${ID}" -H "${AUTH}" -H 'Content-Type: application/json' \
  -d '{"op":"add_connection","connection":{"id":"hc","from":"hipocampo","to":"cortex","directed":true}}' \
  -o /dev/null || fail "add_connection falhou"
pass "conexão hipocampo→cortex adicionada"

# ── 4b. validação negativa (endpoint inexistente → 422) ───────────────────────
CODE_HTTP="$(curl -s -o /dev/null -w '%{http_code}' -X PATCH "${API}/instances/${ID}" -H "${AUTH}" \
  -H 'Content-Type: application/json' \
  -d '{"op":"add_connection","connection":{"id":"bad","from":"nada","to":"nada2"}}')"
[[ "${CODE_HTTP}" == "422" ]] || fail "esperava 422 p/ endpoint inválido, veio ${CODE_HTTP}"
pass "conexão inválida rejeitada (422)"

# ── 5. persistência: GET confirma estado ──────────────────────────────────────
step "5. GET confirma estado persistido"
GOT="$(curl -fsS "${API}/instances/${ID}" -H "${AUTH}")"
echo "${GOT}" | python3 -c "
import sys,json
d=json.load(sys.stdin)
lm=[l for l in d['layers'] if l['id']=='landmarks'][0]
snc=[l for l in d['layers'] if l['id']=='snc'][0]
cortex=[b for b in lm['blocks'] if b['id']=='cortex'][0]
assert snc['opacity']==0.2, 'opacity não persistiu: '+str(snc['opacity'])
assert any(b['id']=='hipocampo' for b in lm['blocks']), 'bloco não persistiu'
assert len(cortex.get('attachments',[]))>=1, 'anexo não persistiu'
assert len(d['connections'])>=3, 'conexões: '+str(len(d['connections']))
print('  opacity=0.2 · hipocampo presente · anexo no cortex · '+str(len(d['connections']))+' conexões')
" || fail "estado persistido divergente"
pass "estado consistente no GET"

# ── 6. serve do anexo content-addressed ───────────────────────────────────────
step "6. Serve do anexo (content-addressed)"
HTTP="$(curl -s -o /dev/null -w '%{http_code}' "${API}/instances/${ID}/attachments/${HASH}" -H "${AUTH}")"
[[ "${HTTP}" == "200" ]] || fail "serve do anexo: HTTP ${HTTP}"
pass "anexo servido (200)"

# ── 7. restart → disco é a fonte da verdade ───────────────────────────────────
step "7. Restart do servidor → re-leitura do disco"
stop_server
start_server
HTTP="$(curl -s -o /dev/null -w '%{http_code}' "${API}/instances/${ID}" -H "${AUTH}")"
[[ "${HTTP}" == "200" ]] || fail "instância sumiu após restart: HTTP ${HTTP}"
pass "instância sobreviveu ao restart (disco = fonte da verdade)"

# ── 8. player + arcade (sem regressão) ────────────────────────────────────────
step "8. Páginas servidas (player + sem regressão de arcade)"
for path in "/universos/instance/${ID}" "/universos/snake"; do
  HTTP="$(curl -s -o /dev/null -w '%{http_code}' "${BASE}${path}")"
  [[ "${HTTP}" == "200" ]] || fail "${path}: HTTP ${HTTP}"
  pass "${path} (200)"
done

printf '\n\033[32m\033[1mE2E OK — editor de universos verificado ponta a ponta.\033[0m\n'
