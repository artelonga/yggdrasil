# Yggdrasil — HTTP API

> Base URL produção: `https://yggdrasil-artelonga.fly.dev`
> Versão atual: `v0.8.0+` (working toward `v0.9.0`)

Esta API expõe o lobby, os universos (Snake/Tetris/Invaders/Pôquer), o
grafo de universos, autenticação via CO, e operações de carteira em
sementes. Inspirada na superfície de quilomboaraucaria / co — qualquer
propriedade artelonga pode adotar o mesmo shape.

## Convenções

- **Linguagem**: respostas e mensagens de erro em PT-BR.
- **Auth**: `Authorization: Bearer <jwt>`. JWT HS256 emitido pelo
  `/auth/co-handover-receive` (vide [Auth](#auth-sso-via-co)). Sem header
  = anonymous.
- **Erros**: `{"erro": "código_curto"}` com status HTTP semântico (401, 403, 404, 409, 422).
- **JSON-only**: requisições com body fazem `Content-Type: application/json`.

---

## Auth (SSO via CO)

Yggdrasil não tem usuários próprios. Toda identidade vem do CO
(`co.artelonga.com.br`). Dois caminhos do navegador:

### `GET /auth/co-login?next=<path>`

Redirect 302 para `https://co.artelonga.com.br/api/v1/auth/google/start?return_to=<ygg>/auth/co-handover-receive?next=<path>`. CO trata Google
OAuth, cria/atualiza a conta CO, e bounce de volta com `?co_token=<jwt_es256>`.

### `GET /auth/co-handover-receive?co_token=<jwt>&next=<path>`

Recebe `co_token` ES256 assinado pelo CO. Valida via JWKS de CO
(`/.well-known/jwks.json`, cache TTL 1h). Sucesso → HTML inline que
armazena JWT HS256 local em `localStorage.yggdrasil-jwt` e navega para
`next` (default `/lobby`). Falha → 401 com página de erro PT-BR.

### Fluxo email-código (sem Google)

Cliente posta direto em CO (CORS já permite `yggdrasil-artelonga.fly.dev`):

```
POST https://co.artelonga.com.br/api/v1/auth/onboard-with-email
  body: {email, return_to: "<ygg>/auth/co-handover-receive", intent: "login_or_signup"}
  → 202 (envia código de 6 dígitos via email)

POST https://co.artelonga.com.br/api/v1/auth/onboard-with-email/verify
  body: {email, code}
  → 200 (set-cookie de sessão CO em co.artelonga.com.br)

# Cliente então navega para CO handover:
GET https://co.artelonga.com.br/auth/co-handover?return_to=<ygg>/auth/co-handover-receive
  → 303 → <ygg>/auth/co-handover-receive?co_token=<jwt>
```

---

## Me (usuário autenticado)

### `GET /api/v1/me`

Retorna identidade do usuário atual.

**Auth**: obrigatória.

```json
200 OK
{"user_id": "yuri-co-uuid", "email": "yuri@artelonga.com.br"}
```

```json
401 Unauthorized
{"erro": "nao_autenticado"}
```

### `GET /api/v1/me/sementes`

Saldo atual de sementes do usuário.

**Auth**: obrigatória.

```json
200 OK
{"saldo": 1500, "moeda": "sementes", "atualizado_em": "2026-05-14T12:30:00Z"}
```

---

## Universos (catálogo)

Grafo recursivo de universos jogáveis. Cada nó é Root / Variant / Composition.

### `GET /api/v1/universes`

Lista todos os nós em ordem alfabética de slug. Anônimo.

```json
200 OK
{"universes": [
  {"slug": "snake", "parent": null, "children": ["snake/classic", "snake/walls"], "kind": "root",
   "title": "Snake", "description": "...", "parameters": {},
   "api": {"start": "/api/v1/games/snake/start", "input": "...", "page": "/universos/snake"}},
  ...
]}
```

### `GET /api/v1/universes/{*slug}`

Um nó individual. Slug pode conter `/` (ex: `tetris/sprint-40`). Anônimo.

```json
200 OK
{"slug": "tetris/sprint-40", "parent": "tetris", "children": [], "kind": "variant",
 "title": "Tetris Sprint 40", "description": "Limpe 40 linhas o mais rápido possível.",
 "parameters": {"lines_to_clear": 40, "mode": "sprint"},
 "api": {"start": "/api/v1/games/tetris/start?variant=tetris/sprint-40", ...}}
```

```json
404 Not Found
{"erro": "Universo 'foo' não encontrado"}
```

### `GET /api/v1/universes/graph`

Forma de grafo para visualizadores. Anônimo.

```json
200 OK
{"nodes": [...], "edges": [{"from": "tetris", "to": "tetris/sprint-40"}, ...]}
```

---

## Jogos single-player (snake / tetris / invaders)

### `GET /api/v1/games/{game}/start[?variant=<slug>]`

Cria sessão e retorna estado inicial. `variant` opcional aplica overrides
de parâmetros (ver `/api/v1/universes/{game}/children`).

```json
200 OK
{"id": "abc123", "state": {...}, "score": 0}
```

### `POST /api/v1/games/{game}/{id}/input`

Avança um tick com o input recebido.

**Auth**: opcional. Se JWT presente, score é persistido sob `claims.sub`;
caso contrário, sob `"anonymous"`.

```json
Request: {"direction": "Right"}
Response 200 OK: {"action": "continue"|"quit", "state": {...}, "score": 120}
404 Not Found: sessão inexistente
```

---

## Pôquer multiplayer

3 mesas provisionadas no boot: `carvalho` (6 seats, cash game), `olmo`
(6 seats, cash game), `heads-up` (2 seats, duelo). Estado persistido em
SQLite (YG-29) — restart preserva seating + chip stacks.

### `GET /api/v1/poker/lobbies`

Lista mesas. **Auth**: obrigatória.

### `GET /api/v1/poker/lobbies/{id}`

Estado da mesa (seats, max_seats). **Auth**: obrigatória.

### `POST /api/v1/poker/lobbies/{id}/sit`

Senta em assento. Buy-in 1.000 sementes. **Auth**: obrigatória.

```json
Request: {"seat": 0}
200 OK: {"id": "carvalho", "name": "Mesa Carvalho", "seats": [...]}
402 Payment Required: {"erro": "Saldo insuficiente para sentar"}
409 Conflict: assento ocupado, já sentado em outra mesa
```

### `POST /api/v1/poker/lobbies/{id}/stand`

Levanta da mesa. Credita stack remanescente. **Auth**: obrigatória.

### `GET /api/v1/poker/lobbies/{id}/hand`

Estado público da mão (community cards, pot, current actor). Auto-inicia
nova mão se ≥ 2 ocupantes. **Auth**: obrigatória.

### `GET /api/v1/poker/lobbies/{id}/hole-cards`

Cartas privadas do usuário autenticado. **Auth**: obrigatória.

### `POST /api/v1/poker/lobbies/{id}/action`

Aplica ação na mão.

```json
Request: {"action": "fold"|"check"|"call"|"raise", "amount": 40}
200 OK: estado público da mão
409 Conflict: fora-da-vez
422 Unprocessable: ação inválida (ex: check com aposta pendente)
```

---

## Scores (leaderboard)

### `GET /api/v1/scores/top?limit=N`

Top N (default 3) por universo, em ordem de score desc. Anônimo.

```json
200 OK
{"scores": [
  {"user_id": "yuri-co-uuid", "game": "snake", "score": 280, "ts": "2026-05-14T12:30:00Z"},
  ...
]}
```

### `GET /api/v1/scores/recent`

Últimas 10 entradas, ordem temporal desc. Anônimo.

---

## Admin (privilegiada)

Auth via `Authorization: Bearer <YGGDRASIL_ADMIN_TOKEN>`. Token é
configurado via `flyctl secrets set YGGDRASIL_ADMIN_TOKEN=...`. Sem o
secret, todos os endpoints admin retornam 503.

### `POST /api/v1/admin/sementes/credit`

Credita sementes na carteira de um usuário. Útil para seed inicial,
recompensas operacionais, correção manual.

```json
Request: {"user_id": "yuri-co-uuid", "amount": 1000}
200 OK: {"user_id": "yuri-co-uuid", "creditado": 1000, "saldo_apos": 2500}
400 Bad Request: amount = 0
401 Unauthorized: sem token
403 Forbidden: token inválido
503 Service Unavailable: admin_token não configurado no servidor
```

**Audit**: cada credit emite log `INFO admin credit: user=... amount=... saldo_apos=...`.

---

## Lobby HTML routes

| Path | Descrição |
|---|---|
| `/lobby` | Lobby principal com canvas + sidebar (high scores, atividade, reviews) |
| `/login` | Tela de login (Google + email-código) |
| `/universos/{slug}` | Página jogável do universo |
| `/games/{slug}` | 301 → `/universos/{slug}` (legacy) |

---

## Versionamento

Esta API segue [SemVer](https://semver.org/). Breaking changes em rotas
serão precedidos de deprecation period documentado em CHANGELOG. Rotas
sob `/api/v1/...` são estáveis dentro de `0.x`.

## Padrão para outras propriedades

Quilomboaraucaria e CO seguem padrão similar de `/api/v1/auth/...`,
`/api/v1/me`, `/api/v1/scores`, etc. Adotar este shape em novas
propriedades artelonga = cliente unificado, identidade unificada, menos
surpresas para usuários.
