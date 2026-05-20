# Yggdrasil — API Catalog

> Source of truth: `yggdrasil-web/src/main.rs` (router wiring) at
> workspace `v0.9.0`. **No WebSocket routes exist** — all surfaces are
> plain HTTP. Listed once per route.

Auth conventions:

- **None** — public.
- **JWT (HS256)** — `Authorization: Bearer <jwt>` minted locally by
  `auth::sign_jwt` (magic-link verify, or CO handover). Optional means
  the route accepts both and degrades anonymously.
- **Admin token** — `Authorization: Bearer <YGGDRASIL_ADMIN_TOKEN>`;
  503 when env var unset.
- **CO JWKS** — inbound ES256 token validated via JWKS from
  `co-artelonga.fly.dev`, then re-minted as local HS256.

## Health

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/health` | None | Liveness probe, returns `"ok"`. |

## Lobby

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/lobby` | None | Returns the 40x20 lobby `Universe` JSON (4 portals at fixed positions). |
| POST | `/api/v1/lobby/enter` | None | Body `{x, y}` → if tile is `Portal(slug)` returns `{slug}`, else 404. |
| GET | `/lobby` | None | Serves `lobby.html` (canvas + vanilla JS). |
| GET | `/` | None | 302 → `/lobby`. |
| GET | `/sobre` | None | Serves `sobre.html`. |
| GET | `/favoritos` | None | Serves `favoritos.html` (poker favourite hands UI). |
| GET | `/perfil/{username}` | None | Serves `perfil.html` (renders profile via API). |

## Auth

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/api/v1/auth/code` | None | Body `{email}` → mails 6-digit verification code (or logs it in dev). |
| POST | `/api/v1/auth/verify` | None | Body `{email, code}` → mints HS256 JWT, returns `{user_id, email, display_name, expires_at}`. |
| GET | `/auth/co-login` | None | 302 → CO login URL preserving `?next=<path>`; server-side so `CO_BASE_URL` is resolved at runtime. |
| GET | `/auth/co-handover-receive` | CO JWKS | `?co_token=<es256>&next=<path>` — verifies CO token, re-mints local HS256, returns HTML that stores `yggdrasil-jwt` in localStorage and redirects. Upserts `user_profiles` row. |
| GET | `/login` | None | Serves `login.html`. |

## Me / users / profiles

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/me` | JWT (required) | Returns the authenticated user's basic profile. |
| GET | `/api/v1/me/sementes` | JWT (required) | Returns sementes balance (currency wallet, read from `game_core::WalletManager`). |
| GET | `/api/v1/me/universos` | JWT (required) | Returns universes the user has interacted with (joined with `default_registry()`). |
| GET | `/api/v1/users/{username}` | None | Public profile fetch by slugified username — includes leaderboard rank + favourite poker hands. |
| GET | `/api/v1/me/favorites/hands` | JWT (required) | Lists user's favourited poker hands. |
| POST | `/api/v1/me/favorites/hands/{table_id}` | JWT (required) | Marks the last hand at `{table_id}` as a favourite for the current user. |

## Scores

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/scores/top` | None | Top scores leaderboard (filterable by `?game=<slug>`). |
| GET | `/api/v1/scores/recent` | None | Recent score submissions across all games. |

## Universes (registry / graph)

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/universes` | None | Flat list of all `UniverseNode`s (root + variants + compositions). |
| GET | `/api/v1/universes/graph` | None | Same data as adjacency graph (nodes + edges) for visualisation. |
| GET | `/api/v1/universes/{*slug}` | None | Single node lookup; `*slug` accepts nested paths like `snake/walls`. |

## Admin

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/api/v1/admin/sementes/credit` | Admin token | Credit sementes to a user (manual grant). 503 when `YGGDRASIL_ADMIN_TOKEN` unset. |

## Game — Snake

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/games/snake/start` | JWT (optional) | `?variant=snake/walls` supported. Creates `YggSnake` session, returns `{id, state, score}`. |
| POST | `/api/v1/games/snake/{id}/input` | JWT (optional) | Body `{direction}` → ticks the session, returns `{action, state, score}`. Saves score on `quit`. |
| GET | `/universos/snake` | None | Serves `universos/snake.html`. |
| GET | `/games/snake` | None | 301 → `/universos/snake` (legacy bookmark). |

## Game — Tetris

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/games/tetris/start` | JWT (optional) | Creates `YggTetris` session. |
| POST | `/api/v1/games/tetris/{id}/input` | JWT (optional) | Tick + score persistence on game-over. |
| GET | `/universos/tetris` | None | Serves `universos/tetris.html`. |
| GET | `/games/tetris` | None | 301 → `/universos/tetris`. |

## Game — Invaders

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/games/invaders/start` | JWT (optional) | Creates `YggInvaders` session. |
| POST | `/api/v1/games/invaders/{id}/input` | JWT (optional) | Tick + score persistence on game-over. |
| GET | `/universos/invaders` | None | Serves `universos/invaders.html`. |
| GET | `/games/invaders` | None | 301 → `/universos/invaders`. |

## Game — Poker (multi-table, HTTP polling)

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/poker/lobbies` | None | Lists all poker tables (seed: `carvalho`, `olmo`, `heads-up`). |
| GET | `/api/v1/poker/lobbies/{id}` | None | Returns table state (seats, blinds, max). |
| POST | `/api/v1/poker/lobbies/{id}/sit` | JWT (required) | Seat the user; locks chips from sementes wallet. |
| POST | `/api/v1/poker/lobbies/{id}/stand` | JWT (required) | Stand up; returns chips to wallet. |
| GET | `/api/v1/poker/lobbies/{id}/hand` | None | Public hand snapshot (community cards, pot, current actor). Polled by client. |
| GET | `/api/v1/poker/lobbies/{id}/hole-cards` | JWT (required) | Private hole cards for the authenticated seat. |
| POST | `/api/v1/poker/lobbies/{id}/action` | JWT (required) | Submit fold/check/call/bet/raise. Persists table snapshot. |
| GET | `/universos/poker` | None | Serves `universos/poker.html`. |
| GET | `/games/poker` | None | 301 → `/universos/poker`. |

## Static

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/static/*` | None | `ServeDir("yggdrasil-web/static")` — HTML, JS, CSS, images. |

## WebSocket routes

**None.** No `axum::extract::ws`, no `tokio::sync::broadcast`, no
`mpsc`/`watch` usage in either crate. Poker is the only multiplayer
surface and is implemented as short-poll HTTP via the routes above.
