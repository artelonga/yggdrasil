# Yggdrasil — Refactor Plan (Gap Analysis)

> Audit only — no code changed. Highest existing task ID in
> `work/yggdrasil/` is **YG-37**, so new tasks start at **YG-38**.
> Reference principles (numbered 1–7) match the brief.

## Quick scorecard

| Principle | State | Worst offender(s) |
|---|---|---|
| 1. Composition over inheritance (game adapters) | Partial | `YggGame` trait lives in `snake.rs` and is used only by snake; tetris/invaders/poker don't implement it. |
| 2. Single responsibility | Mixed | `poker_routes.rs` 1189 LOC fuses HTTP, persistence, sementes side-effects, and seat lifecycle. `auth.rs` 630 LOC and `api/me.rs` 625 LOC similar. |
| 3. Static typing | Mostly OK | `serde_json::Value` is leaked as the API state payload (`StartResponse.state`, `TickResponse.state`). One `Arc<dyn MailProvider>` (legitimate). No `Box<dyn Any>` soup. |
| 4. Reduced coupling | Single biggest debt | `path = "../co/game-core"` (tracked **YG-17**). Every game adapter + `main.rs` reaches into `game_core::*`. |
| 5. Segregated state | Partial | Each game owns its own `Arc<Mutex<…>>`, fine. But four `Mutex<rusqlite::Connection>` open the **same** `yggdrasil.db` file — segregation by accident, not design. No `RwLock`; coarse `Mutex<Vec<PokerTable>>` for the whole poker app. |
| 6. Folders encapsulate features | Mostly OK | `yggdrasil-core/src/games/` holds 5 poker files and 3 single-file games. Lobby is split across `core::lobby` and `web::lobby_routes`. There's no `lobby/` folder; no `games/poker/` folder. |
| 7. Event-driven | Absent | No `tokio::sync::broadcast`/`mpsc`/`watch`. Multiplayer (poker) is HTTP poll. No event spine at all. The brief's "WebSocket session layer" is unbuilt. |

---

## Proposed YG tasks (ordered by priority)

### YG-38 — Carry YG-17 through: pin `game-core` to git rev + delete path dep

- **Principle:** 4 (Reduced coupling)
- **Scope:** YG-17 already specifies the chore. Status today is `todo`,
  release `0.5.0`, but workspace is at `0.9.0` and still on the path dep.
  Either re-confirm YG-17 acceptance or supersede it.
- **Acceptance:**
  - `Cargo.toml` has `game-core = { git = "https://github.com/artelonga/co", rev = "<sha>" }`.
  - `fly.toml` build comment removed (no more parent-dir trick).
  - `docs/DEPENDENCIES.md` documents the bump policy.
  - CI green on a clean checkout with no `co/` clone next to the repo.
- **Blast radius:** Workspace-wide rebuild; surfaces every compile-time
  type assumption against `game-core`. Risk: drift between local `co/`
  and pinned rev causes confusing "works locally, breaks in CI" until
  policy is documented.
- **Priority:** **P0** — gates every other refactor that touches engine
  types and unblocks isolated CI for yggdrasil.

### YG-39 — Promote `YggGame` to a real adapter trait used by all four games

- **Principle:** 1 (Composition), 5 (Segregated state)
- **Scope:** `YggGame` is declared in `yggdrasil-core/src/games/snake.rs`
  and re-exported, but only `YggSnake` implements it. `YggTetris`,
  `YggInvaders`, `YggPoker` each expose ad-hoc `tick`/`render_json`/`score`
  shapes, and the route layer special-cases each. Move `YggGame` to its
  own module (`yggdrasil-core/src/games/adapter.rs`), implement it for
  all four, then generalise `snake_routes`/`tetris_routes`/`invaders_routes`
  into a single `make_session_router::<G: YggGame>(…)`.
- **Acceptance:**
  - One `pub trait YggGame` in `yggdrasil-core::games::adapter`.
  - 4 impls (`YggSnake`, `YggTetris`, `YggInvaders`, `YggPoker` or its
    single-player sub-component).
  - `make_session_router::<G>()` exists; `snake_routes`, `tetris_routes`,
    `invaders_routes` collapse to ~30 LOC each (state struct + boot).
  - No behaviour change visible to clients (same JSON shapes, same routes).
- **Blast radius:** Single-player game routes only. Poker untouched
  unless we want it. Backwards-compatible at the HTTP layer.
- **Priority:** **P1**

### YG-40 — Split `poker_routes.rs` (1189 LOC) by responsibility

- **Principle:** 2 (SRP), 6 (folders per feature)
- **Scope:** Today `yggdrasil-web/src/games/poker_routes.rs` mixes:
  HTTP handlers, `PokerState` lifecycle (seeding, persistence boot),
  sementes credit/debit on sit/stand, hole-card auth, snapshot
  serialisation, and inline test app builders. Move to
  `yggdrasil-web/src/games/poker/` with files: `state.rs`, `routes.rs`,
  `chip_flow.rs` (sementes ↔ table), `serialization.rs`, `tests.rs`.
  Mirror with `yggdrasil-core/src/games/poker/` (already partially
  done — 5 files, but not a folder).
- **Acceptance:**
  - `poker_routes.rs` ≤ 250 LOC after split.
  - `yggdrasil-core/src/games/poker.rs` becomes `poker/mod.rs` re-exporting
    the existing 5 sibling files.
  - Test coverage preserved (count `cargo test -p yggdrasil-web poker` before/after).
- **Blast radius:** internal only (no public API or route change). Touches
  one big file plus a couple of imports.
- **Priority:** **P1**

### YG-41 — Introduce an event spine (`tokio::sync::broadcast`) and WS for poker

- **Principle:** 7 (Event-driven), 2 (SRP), 5 (Segregated state)
- **Scope:** Poker today is HTTP polling (`GET .../hand` repeatedly).
  The user-story brief mentions a "WebSocket session layer" — it doesn't
  exist. Introduce per-table `tokio::sync::broadcast::Sender<TableEvent>`
  inside `PokerTable`; expose `GET /api/v1/poker/lobbies/{id}/ws` upgrading
  to WebSocket; emit `TableEvent::{Seated, HandStarted, ActionTaken,
  HandEnded}` from mutators. Existing HTTP poll endpoints stay (for
  backwards compat) but become thin reads of the same in-memory state.
- **Acceptance:**
  - `TableEvent` enum (statically typed; no `serde_json::Value`).
  - One `broadcast::Sender` per `PokerTable`, segregated state.
  - `/api/v1/poker/lobbies/{id}/ws` upgrades, streams JSON events.
  - One integration test: two clients sit at the same table, both receive
    a `HandStarted` event when blinds post.
  - Frontend `static/universos/poker.html` switched to WS subscribe
    (poll remains as fallback for v1).
- **Blast radius:** Largest of the set. Adds runtime tasks per table,
  Tokio scheduling concerns, and surface area for race conditions in
  poker mutations. Should land after YG-40.
- **Priority:** **P1**

### YG-42 — Replace `serde_json::Value` in game state payloads with concrete types

- **Principle:** 3 (Static typing)
- **Scope:** `StartResponse.state: serde_json::Value` and
  `TickResponse.state: serde_json::Value` in `games/common.rs` propagate
  through every single-player route. Each game produces JSON via a
  `render_json() -> String` then re-parses it (`map_to_value`). Replace
  with a per-game `GameState` struct that implements `Serialize`, and
  parameterise `StartResponse<S>`/`TickResponse<S>`.
- **Acceptance:**
  - `pub trait YggGame { type State: Serialize; fn render(&self) -> Self::State; }`.
  - `map_to_value` deleted.
  - Same wire JSON (regression test against current snapshots).
- **Blast radius:** Single-player games only; poker is unaffected
  (it already uses concrete `Serialize` structs).
- **Priority:** **P2** — needs YG-39 first (shared trait).

### YG-43 — Carve out a `lobby/` folder; collapse the split between core::lobby and web::lobby_routes

- **Principle:** 6 (Feature folders), 2 (SRP)
- **Scope:** `yggdrasil-core/src/lobby.rs` builds the `Universe`;
  `yggdrasil-web/src/lobby_routes.rs` exposes it. Today they are
  files at different layers, with no obvious feature folder. Introduce
  `yggdrasil-core/src/lobby/{mod,grid,portals}.rs` and
  `yggdrasil-web/src/lobby/{mod,routes,html}.rs`. The HTML serving in
  `main.rs` (`serve_lobby`) moves into `web::lobby::routes` too.
- **Acceptance:**
  - `main.rs` only calls `lobby::router()` for both HTML and JSON.
  - All lobby-related strings (`"Escolha um universo para entrar"`, portal
    positions) live in `core::lobby`.
- **Blast radius:** Cosmetic; no behaviour change.
- **Priority:** **P2**

### YG-44 — Segregate per-game DB connections behind a `ScoresStore` trait

- **Principle:** 5 (Segregated state), 4 (Coupling)
- **Scope:** `make_snake_state`, `make_tetris_state`, `make_invaders_state`,
  and `ScoresState` each open their own `Mutex<rusqlite::Connection>` to
  the same `yggdrasil.db`. SQLite tolerates this but the design is
  unintentional. Either (a) one shared `Arc<Mutex<Connection>>` injected
  into all four, or (b) a `ScoresStore` trait abstraction (so prod uses
  shared SQLite, tests use in-memory) is cleaner.
- **Acceptance:**
  - One `Arc<dyn ScoresStore>` (or generic) passed to all four game
    states; four parallel `Connection::open` calls deleted.
  - In-memory test impl provided.
- **Blast radius:** Boot logic in `main.rs` + the four `make_*_state` factories.
- **Priority:** **P2**

### YG-45 — Trim `auth.rs` and `api/me.rs` (each >600 LOC)

- **Principle:** 2 (SRP)
- **Scope:** Both files mix domain logic, HTTP handlers, and large
  `#[cfg(test)]` blocks. Move tests into sibling `tests/auth.rs` and
  `tests/me.rs` (integration test crates) or `_tests.rs` modules.
  Inside `auth.rs`, split: `auth/jwt.rs` (sign/verify), `auth/magic_link.rs`
  (request/verify code), `auth/state.rs` (`AuthState`, DB schema).
- **Acceptance:**
  - No single file > 300 LOC after split (target, not strict).
  - All tests still pass.
- **Blast radius:** Imports only; no public API change.
- **Priority:** **P3**

### YG-46 — Document "no per-game DB" + correct the persistence model

- **Principle:** none directly (audit hygiene)
- **Scope:** The story brief assumes per-game SQLite files. Reality is
  one shared `yggdrasil.db` + one `yggdrasil-sementes.db`. Either update
  `docs/ARQUITETURA-UNIVERSOS.md` to reflect reality, or actually split
  the DBs per game (probably not worth it for SQLite). Cheap doc-only
  task to retire the confusion.
- **Acceptance:**
  - `docs/ARQUITETURA-UNIVERSOS.md` (or new `docs/architecture/data-model.md`)
    describes the two-DB layout, the shared `scores` table, and poker's
    own tables.
- **Blast radius:** zero — docs only.
- **Priority:** **P3**

---

## Summary table

| ID | Title | Principles | Priority |
|---|---|---|---|
| YG-38 | Git-rev pin for `game-core` (carry YG-17) | 4 | P0 |
| YG-39 | Generic `YggGame` adapter trait + shared router | 1, 5 | P1 |
| YG-40 | Split `poker_routes.rs` into a folder | 2, 6 | P1 |
| YG-41 | Event spine + WebSocket layer for poker | 7, 2, 5 | P1 |
| YG-42 | Static `GameState` types (drop `serde_json::Value`) | 3 | P2 |
| YG-43 | `lobby/` folder per feature | 6, 2 | P2 |
| YG-44 | Segregate game DB connections via `ScoresStore` | 5, 4 | P2 |
| YG-45 | Trim `auth.rs` / `api/me.rs` | 2 | P3 |
| YG-46 | Document data-model (correct the per-game DB myth) | — | P3 |

The single highest-leverage item is **YG-38** (the path dep). Everything
else is cleanup that can land after CI is stable on a pinned rev.
