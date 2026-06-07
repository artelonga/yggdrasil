# Modelo de dados — Yggdrasil

> Por que não há um banco por jogo — e onde cada tabela realmente vive.

## Dois bancos, dois propósitos

O Yggdrasil usa dois arquivos de banco em runtime:

| Banco | Var de ambiente | Padrão | Tecnologia | O que guarda |
|---|---|---|---|---|
| `yggdrasil.db` | `YGGDRASIL_DB` | `yggdrasil.db` | SQLite (rusqlite) | Auth, scores, pôquer |
| `yggdrasil-sementes.db` | `YGGDRASIL_SEMENTES_DB` | `yggdrasil-sementes.db` | redb (game_core::storage::Storage) | Saldos de sementes |

**Não há um SQLite por jogo.** Snake, Tetris e Invaders gravam na mesma tabela
`scores` de `yggdrasil.db`. Pôquer adiciona sua própria tabela `poker_lobbies`
no mesmo arquivo. Criar um arquivo separado por universo adicionaria complexidade
operacional (N conexões, N backups, N migrações) sem benefício real enquanto o
banco for SQLite embarcado.

---

## `yggdrasil.db` — detalhes

### `usuarios`

```sql
CREATE TABLE IF NOT EXISTS usuarios (
    email   TEXT PRIMARY KEY,
    user_id TEXT NOT NULL
);
```

Registra usuários autenticados. Criado por `auth::init_auth_db`.

### `verify_codes`

```sql
CREATE TABLE IF NOT EXISTS verify_codes (
    email      TEXT PRIMARY KEY,
    code       TEXT NOT NULL,
    user_id    TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 0
);
```

Códigos de login (magic link / email OTP). Um registro por email; substituído a
cada novo `POST /api/v1/auth/code`.

### `rate_limits`

```sql
CREATE TABLE IF NOT EXISTS rate_limits (
    email    TEXT PRIMARY KEY,
    requests TEXT NOT NULL
);
```

Janela deslizante de requisições por email, serializada como JSON em `requests`.

### `scores` — compartilhada entre todos os jogos single-player

```sql
CREATE TABLE IF NOT EXISTS scores (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    game    TEXT NOT NULL,
    score   INTEGER NOT NULL,
    ts      TEXT NOT NULL          -- RFC 3339
);
```

Criada por `games::common::init_db`. Snake, Tetris e Invaders gravam aqui via
`common::save_score`. O campo `game` diferencia os universos (`"snake"`,
`"tetris"`, `"invaders"`). Lida por `GET /api/v1/scores/top` e
`GET /api/v1/scores/recent`.

### `poker_lobbies` — snapshot de mesas de pôquer

```sql
CREATE TABLE IF NOT EXISTS poker_lobbies (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL,
    state TEXT NOT NULL   -- JSON: PokerTableSnapshot { lobby, stacks }
);
```

Criada por `games::poker_persistence::init_poker_db`. Persiste seating e
chip-stacks entre reinicializações do servidor. Mãos em curso **não** são
persistidas — um restart no meio de uma mão é forfeit; buy-ins (stacks)
sobrevivem. Upsert após cada `sit`, `stand` e `act` bem-sucedido.

---

## `yggdrasil-sementes.db` — detalhes

Gerenciado exclusivamente por `game_core::storage::Storage`, que usa
[redb](https://github.com/cberner/redb) — banco de chave-valor embarcado. **Não
é SQLite.**

### tabela `wallet`

| Chave | Valor |
|---|---|
| `"wallet:{user_id}"` | Protobuf `Wallet { balance: u64, last_updated: i64 }` |

Acessado por `yggdrasil_core::sementes::Sementes`, que expõe as operações:

- `saldo(user_id)` → u64
- `creditar(user_id, qtd)` → Ok(())
- `debitar(user_id, qtd)` → Ok(restante)

O pôquer chama `creditar`/`debitar` durante `sit_with_sementes` e
`stand_with_sementes`. A API pública expõe o saldo via
`GET /api/v1/me/sementes`.

---

## Diagrama de conexões em runtime

```
┌───────────────────────────────────────────────┐
│                yggdrasil-web                  │
│                                               │
│  auth::AuthState ──────────────────┐          │
│  games::{snake,tetris,invaders}    │          │
│  api::scores::ScoresState          ├──► yggdrasil.db  (SQLite)
│  games::poker::PokerState          │          │
│    └── poker_persistence           │          │
│                                    ┘          │
│  yggdrasil_core::sementes::Sementes ─────────► yggdrasil-sementes.db  (redb)
└───────────────────────────────────────────────┘
```

Múltiplas `rusqlite::Connection` apontam para o mesmo arquivo `yggdrasil.db`; o
SQLite serializa acessos concorrentes via WAL. O redb da `Sementes` tem sua
própria trava interna.

---

## Mito aposentado: "um banco por jogo"

A task story YG-46 corrige uma premissa errada nos rascunhos iniciais do projeto:
a ideia de que cada universo (snake, tetris, invaders, poker) teria seu próprio
arquivo `.db`. Na prática, desde o início, o modelo é o acima — um `yggdrasil.db`
compartilhado + um `yggdrasil-sementes.db` para a moeda interna.

Separar por jogo só faria sentido se:
- Universos forem extraídos para processos independentes (hoje são rotas num único processo), **ou**
- O volume de scores justificar particionamento (não é o caso em SQLite embarcado).
