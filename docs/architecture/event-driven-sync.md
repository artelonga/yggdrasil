# ADR — Event-driven bridge between Yggdrasil (housed) and co (central)

> **Status:** Accepted (design) — 2026-06-06. Supersedes the poll-based bridge sketch
> (CO-337 15-min pull) for universe content. Frames the build phases P-A…P-E below.
>
> **Canonical co specs (this ADR is the Yggdrasil-side view of them):** CO-380 (universal
> event bus / EDA spine), CO-381 (`/agora` live timeline), CO-383 (Yggdrasil notes ingestion,
> event-driven), CO-384 (federated WS bridge CO↔Yggdrasil↔devices), CO-385 (Mac-style UPSERT
> CRUD action tree — *spec file not yet written*), CO-387 (time-rendering lens). Product-level
> articulation lives in `co/docs/use-cases.md` (UC 1, 2, 8). The conflict-merge primitive it
> reuses is CO-162 (`mesclar` 3-way merge).
>
> **Wire decision (2026-06-06): notes ingest rides a `FederatedEvent` over CO-384's
> `/api/v1/events/bridge`** — per-entry EDA, one save = one event, fanned out to every subscriber
> (atividades, `/agora`, KbIndexer in v3.1). The `sync_ws` / `SyncBatch` primitive (CO-151, listed
> below as "shipped") is a **separate** path reserved for **bulk** content/blob/vault catch-up —
> *not* this ingest. **Cold-start = warm-reconnect, same code path:** the producer (YG-93) tracks
> `last_delivered_event_id`; NULL → streams the entire `event_log`; reconnect → streams from the
> last ACK'd id; CO dedupes by ULID. No separate bulk endpoint.
> **Scope:** how content **housed in Yggdrasil** becomes **visible and editable at co** and
> converges **live across devices**, without polling. Phase 0 (notes + wikilinks in the
> instance editor, Markdown canonical on `/data`) is already shipped to prod — see
> [`editor.md`](editor.md) and the `instance/note` module.

## 1. Problem

co runs **local-first on the user's machine** (`co serve` + the `co-sync` daemon). A bridge
that makes Yggdrasil's notes editable at co must therefore be **event-driven**: an edit emits
a delta the instant it is saved, the delta rides a live bus, every device converges, and
divergence is reconciled with **jujutsu-style** conflict handling surfaced as a Finder-style
UPSERT action tree. A server-side poll (the earlier CO-337 framing) is the wrong model.

## 2. Decisive finding — co already ships ~90% of the spine

| Capability | Status | Location |
|---|---|---|
| Live delta bus *(bulk content path — not notes ingest; see Wire decision)* — WS, broadcast, 24h resume log, echo-filter | **shipped** | `co-web/src/social/sync_ws.rs` — `GET /api/v1/sync/ws?universe=&token=` |
| Local-first daemon — `notify` file-watch → WS deltas; CI push mode | **shipped** | `co-agent/src/bin/co-sync.rs`, `co-agent/src/sync_config.rs` |
| Bulk delta wire *(bulk catch-up, not notes ingest)* — protobuf+zstd `SyncBatch`, `Upserted`/`Deleted` | **shipped** | `co/core/src/sync/delta.rs` |
| **Notes-ingest wire — `FederatedEvent` over CO-380/384** | **planned** (CO-384) | `co-web/src/eda/bridge/` (the `events/bridge` hub) |
| **jj-shaped op log** — immutable `Operacao`, Hybrid Logical Clock, causal DAG (`pai`), `mesclar()` 3-way merge, `Conflito{opcoes, sugestao}`, `Proposta` | **shipped** | `co/core/src/sync/mod.rs` |
| Snapshot-on-reconnect — stale resume → "snapshot-needed" → REST vault dump | **shipped** | `sync_ws.rs` resume path |
| Vault ingest — `PUT /api/v1/universes/{slug}/vault/{path}` + entry index + relations | **shipped** | `co-web/src/content/vault_routes.rs` |
| Domain event bus (in-process) + worker supervisor | **shipped** | `co-web/src/platform/events.rs`, `workers.rs` |
| **Finder UPSERT conflict modal + executor** (apply decision → resolver op) | **planned** (CO-385 — spec file not yet written) | not built |
| **Yggdrasil participating on the bus** (emit/apply deltas as a peer) | **not built** | — |

The op log in `co/core/src/sync/mod.rs` *is* the "jujutsu-based" framework: an immutable DAG of
operations keyed by Hybrid Logical Clock, with `mesclar()` performing the 3-way merge and
emitting `Conflito`s. jj's operation log has the same shape. We **extend** this — we do not
shell out to the `jj` binary, and we do not introduce a second conflict model.

## 3. Decisions

1. **Hub = co central.** Yggdrasil and every local-`co` device are **peers** dialing into co.
   The **canonical hub endpoint is CO-384's `wss://co.artelonga.com.br/api/v1/events/bridge`**
   (federated event bus over the CO-380 spine); the older `/api/v1/sync/ws` (CO-151) is the
   lower-level content-delta primitive it converges with. Yggdrasil adds **only a WebSocket
   client** (YG-93) — no second bus. See CO-383/CO-384 for the co-side contract.
2. **Conflict engine = extend the op log + `mesclar()`**, lifting it to note (file)
   granularity, with the UPSERT modal/executor (CO-385) layered on top.
3. **Notes are file-granular** on the bus (one `nota` = one `Upserted` path), matching the
   vault layer and the Finder UPSERT semantics. co's finer field-level ops remain available for
   structured content but are out of scope for notes.

## 4. Topology

```
   ┌─────────── user's machine ───────────┐         ┌──────── co central (hub) ────────┐
   │  co serve (local-first editor)        │  WS     │  /api/v1/sync/ws?universe=ygg     │
   │  co-sync daemon (notify → deltas) ────┼────────►│  SyncRoom: store + broadcast +    │
   │                                       │◄────────┤  24h resume log + echo-filter     │
   └───────────────────────────────────────┘         │  op log + mesclar() + vault index │
   ┌─────────── 2nd device (mobile) ───────┐  WS     │                                   │
   │  local-co peer ───────────────────────┼────────►│  (broadcasts to ALL peers)        │
   └───────────────────────────────────────┘         └───────────────┬───────────────────┘
                                                            WS peer    │  (Yggdrasil = a peer)
                                              ┌────────────────────────▼───────────────────┐
                                              │  Yggdrasil prod (notes housed on /data)     │
                                              │  WS client ⇄ NoteStore (emit + apply deltas)│
                                              └─────────────────────────────────────────────┘
```

## 5. The flow (the five steps, grounded in the code)

1. **View prod.** User reads Yggdrasil prod, decides on a change.
2. **Open co's latest snapshot of Yggdrasil.** Local-`co` connects to the room; on first or
   stale connect the bus emits *snapshot-needed* → local-`co` pulls the full vault dump over
   REST. That dump **is** the snapshot. Live deltas then stream.
3. **Edit locally.** `co serve` + the `co-sync` daemon (`notify` watch) emit a `SyncDelta` the
   instant a file is saved — no poll.
4. **Publish proactively.** The delta reaches co central, which applies + indexes + **broadcasts**.
   The proactive pipeline routes feature/fix through the funnel (localhost → staging/uat), runs
   the CI/CD gate, consolidates the changelog (`CHANGELOG-PENDING/` → `scripts/release-commit.sh`),
   auto-resolves with the default `sugestao`, and goes live. Yggdrasil — a peer — receives the
   broadcast and writes the note to `/data`, live on prod.
5. **Cross-device convergence.** A second device subscribed to the same room receives the
   broadcast live. Genuinely divergent offline edits are detected by `mesclar()` (concurrent ops
   on the same target, neither a causal ancestor of the other) and reconciled by the UPSERT tree.

## 6. Yggdrasil-as-peer protocol (to build)

- **Identity.** Yggdrasil gets a stable `node_id` (HLC `Ator`); a long-lived **service JWT**
  authenticates its WS connection (`?token=`). A distinct node id makes its writes attributable
  and echo-filterable.
- **Emit on write.** Hook `NoteStore::save` (`yggdrasil-core/src/instance/note.rs`) to emit an
  internal `NoteWritten { instance, slug, sha }` on a `tokio::sync::broadcast` channel — reusing
  the **poker WS pattern** (`yggdrasil-web/src/games/poker/ws.rs`, `yggdrasil-core/src/games/poker/events.rs`).
  A background task (reuse the `spawn_cleanup_job` pattern in `main.rs`) holds the **outbound WS
  client** (reuse the `reqwest` pattern from `auth_co.rs` / `hint_engine.rs`, plus a WS client lib),
  wraps it in a `FederatedEvent` (`entry.{created,updated,deleted}`, `universe_key=yggdrasil`,
  `path=notes/<slug>.md`, `payload={title,body,updated_at}`) and sends it to CO's hub
  (`/api/v1/events/bridge`). Tracks `last_delivered_event_id` (NULL → full `event_log` on cold start).
- **Apply on receive (P-B, v3.1 — not in v3.0).** The same task decodes inbound `FederatedEvent`s;
  `entry.{created,updated}` → `NoteStore::save` (atomic temp+rename); `entry.deleted` →
  `NoteStore::delete`. Loop-guarded by `hop_count` so Yggdrasil's own writes do not echo back.
- **Resume / snapshot.** On (re)connect Yggdrasil sends `X-Sync-Resume`; a stale token triggers a
  vault dump to rebuild `/data/instances/<id>/notes/`. The shipped resume machinery is reused as is.

## 7. Conflict resolution — UPSERT tree on the op-log primitives

Reuse `co/core/src/sync/mod.rs`: HLC ordering, `causal_ancestor()`, `Conflito`, `mesclar()` —
**lifted to note granularity** (`Alvo { tipo: "nota", id: slug }`). Concurrency is two ops on the
same slug where neither is a causal ancestor of the other — exactly `mesclar`'s existing test. The
missing piece is the **UPSERT executor + modal** (CO-385): given a `Conflito`, apply the
user's verb and emit a `conflito.resolver` op.

**Action tree — your Finder verbs → op log → CRUD:**

| Verb | Trigger | Op-log action | CRUD on `/data` | Copy / scale behavior |
|---|---|---|---|---|
| **skip** | sha256 unchanged (`is_unchanged`, push-cache) | not loaded | no-op | unchanged items never transferred (scaling win) |
| **update** | changed, no concurrent op | `mesclar` clean-apply | UPDATE | unmodified notes ignored |
| **upsert** | changed **or** absent locally | merge + create-if-absent | UPDATE **or** CREATE | new notes inserted |
| **replace** | force (remote authoritative) | remote op wins | UPDATE (overwrite) | local edits discarded |
| **keep-both** | true conflict, keep both | `copia` → `conflito.resolver` op | CREATE `<slug>_1.md` | both retained; newest-by-name suffixed |
| **(delete)** | remote `Deleted` delta | tombstone op | DELETE | — |

The default `sugestao` (today `"prod"`/keep-local; to be renamed to this verb set) is the
**auto-resolve** for headless/proactive publishes. The modal surfaces **only** when a human is
interactively reconciling two divergent devices; it is a SvelteKit component in co-web (the
"Apple-style 4-way" of CO-385) bound to `RelatorioMesclagem.conflitos`.

## 8. Proactive publish pipeline (mostly exists; wire the trigger)

Exists: `co serve` local (`CO_ENV=local`), the fly **uat / staging / prod** tiers,
`scripts/release-commit.sh` + `CHANGELOG-PENDING/`, the `RemoteSisterRepoWorker` /
`ReleaseNotesWorker`. **Build:** the *event-driven* trigger — a publish event (not a 5s poll) that
routes feature/fix to the right funnel, runs the CI/CD gate, consolidates the changelog, and on
green broadcasts the live delta. Reuse the domain event bus (`platform/events.rs`) + worker
supervisor rather than adding a poller.

## 9. Reuse vs. build

- **Reuse verbatim:** `sync_ws.rs` (bus), `delta.rs` (wire), `core/sync/mod.rs` (HLC / merge /
  `Conflito`), vault ingest, resume/snapshot, the event bus + worker supervisor, and — on the
  Yggdrasil side — the poker-WS `broadcast` pattern and the `reqwest` outbound pattern.
- **Build:** (1) the Yggdrasil WS-client peer (emit/apply, `node_id`, service-JWT auth);
  (2) the UPSERT executor + modal + resolver ops (CO-385); (3) note-granular `Alvo`
  mapping in `mesclar`; (4) the event-driven publish trigger.

## 10. Build phases (each ships behind its own approval)

- **P-A — Yggdrasil → co live push (one-way).** `NoteWritten` event + outbound WS client → notes
  appear live at co central. Smallest end-to-end proof of the bus. Touches
  `yggdrasil-core/src/instance/note.rs` (emit), a new `yggdrasil-web/src/co_sync_client.rs`, and
  `main.rs` (spawn the task).
- **P-B — bidirectional apply.** Yggdrasil applies inbound deltas to `NoteStore`; echo-filter +
  resume on reconnect.
- **P-C — UPSERT executor + modal.** co-web endpoint to apply a `Conflito` decision → emit a
  `conflito.resolver` op; SvelteKit modal; note-granular `mesclar`.
- **P-D — proactive publish trigger.** Event-driven funnel + CI/CD gate + changelog consolidation
  on publish.
- **P-E — (optional) frozen snapshot export.** Only if "open snapshot" must freeze a point-in-time
  beyond the current vault-dump-on-reconnect.

## 11. Open questions / risks

- **Machine-peer auth.** A long-lived Yggdrasil service JWT vs. mTLS for the WS — the bus today
  expects a user JWT/session, not a headless service.
- **Op-log persistence per universe** on co. `mesclar` needs the post-base local ops to detect
  concurrency; confirm the `Operacao` log is durably stored per universe, not only the vault entries.
- **File-level vs field-level.** Notes are whole files; co's ops are field-level. This ADR fixes
  notes at file granularity (UPSERT) and leaves field-level ops for structured content.
- **Ordering & scaling.** Assign an HLC per note save for causality; the skip-by-hash path
  (`is_unchanged`) must extend to the bus *apply* side so unchanged notes are never loaded.

## 12. Verification of the design (before building)

- Trace one concrete edit end-to-end on paper through the named files
  (`NoteStore::save` → `FederatedEvent` → `events/bridge` → CO subscriber upsert), and one concurrent edit
  through `mesclar()` → `Conflito` → each UPSERT verb → CRUD result.
- Optional spike (separate approval): a throwaway script that connects to
  `/api/v1/events/bridge?source=yggdrasil.artelonga.com.br` with a CO-issued service token, sends one
  `entry.created` `FederatedEvent`, and confirms co upserts it and rebroadcasts — validating the peer
  contract before P-A.

## Related

- [`editor.md`](editor.md) — the instance editor that houses notes (Phase 0).
- [`data-model.md`](data-model.md) — Yggdrasil's on-disk + DB model.
- co: `core/src/sync/mod.rs`, `co-web/src/social/sync_ws.rs`, `co-agent/src/bin/co-sync.rs`.
