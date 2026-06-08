# Release Execution Pipeline — Yggdrasil × co

> PM view as of 2026-06-08. Spans two repos (`yggdrasil`, `co`). The bridge is the
> seam; this doc sequences the whole portfolio into parallel lanes with one critical path.

## TL;DR (updated 2026-06-08 — CO-384 merged)

- **CO-384 (hub) is DONE; CO-389 is in CI.** The old bottleneck cleared — but the bridge **does
  not carry a single byte yet**, for two reasons (audited 2026-06-08):
  1. **yggdrasil's producer never opens the socket** — `connect_and_stream` is still the stub
     that "awaited CO-384". → **YG-119** (real WS dial).
  2. **The two halves drifted on the wire** — envelope/payload/path/sala-event-types don't match
     between YG-93/103 and CO-384/389 (the `_users/` vs `<lang>/terms/` path mismatch silently
     matches nothing). → **YG-118** freezes the contract + aligns the producer.
- **New critical path = the bridge-go-live epic (YG-117): YG-118 (wire) → YG-119 (dial) → YG-120
  (JWKS/trust) → YG-121 (secrets + E2E)**, paired with CO-389 + CO-374. This is the real gate to
  v3.0 full-federation (your launch-scope decision).
- yggdrasil's *logic* halves (YG-93/97/103/101/112/114) are built; what's missing is the
  **transport + contract**, not the features.

## Critical path (live federation → launch) — revised

```
CO-380 ✅  CO-381 ✅  CO-384 ✅ (hub)        ┌─► CO-389 (em CI) ─┐
                          │                  │                   │
  YG-118 (freeze wire +   │   YG-119 (real   │   CO-383 (notes)  ├─► CO-374 (staging E2E) ─► v3.0
  align producer) ────────┼─► WS dial) ──────┼─► CO-385 (UPSERT) │       full-federation launch
        ▲ co-auto first   │   YG-120 (JWKS)  │   (v3.1 editable) │
        │                 │   YG-121 (secrets+E2E) ──────────────┘
  [YG-93/97/103/101/112/114 ✅ — lógica pronta; falta transporte+contrato]
            launch gates (CO-internal): CO-278/278-B (public API), CO-145 (encrypted assets)
```

The instance-qualified note path `instances/<id>/notes/<slug>.md` (YG-97) and the comunicação
term path `<lang>/terms/_users/<u>/<slug>.md` (YG-118) are the wire contract CO-383/389 consume —
**frozen in YG-118's spec** as the single source of truth for both repos.

## Parallel lanes (what can run concurrently)

| Lane | Owner | Tasks | Notes |
|---|---|---|---|
| **A — Bridge go-live** (CRITICAL, now) | yggdrasil + co | **YG-117** epic: `YG-118` (freeze wire+align) → `YG-119` (WS dial) → `YG-120` (JWKS) → `YG-121` (secrets+E2E); ∥ co: `CO-389` finish, then `CO-383`/`CO-385` | The real launch critical path now that CO-384 is done. **YG-118 is the co-auto first task.** |
| **B — Corpus / Caderno** ✅ landed v2.6.0 | yggdrasil | `YG-111`(found) `YG-112` `YG-114` done; open: `YG-113` `YG-115` `YG-116` | Follow-ups parallel, not launch-blocking. |
| **C — CI debt** ✅ done | yggdrasil | `YG-104` (WASM repaired, CI honest) | Closed in v2.6.0. |
| **D — Per-universe versioning** ✅ done | yggdrasil | `YG-63`..`YG-67` | Closed in v2.6.0. |
| **E — Catalog expansion** ✅ landed | yggdrasil | `YG-68`/`70`/`72`/`69` (REGISTRY + 40 RPGs + Shandara) | v2.6.0; remaining seeds are content trickle. |
| **F — Godot POC** ✅ decided | yggdrasil-godot | `YG-35` → ADR hybrid; `YG-32/33/34` superseded | Closed — canvas is production. |
| **G — CO launch hardening** | co session | `CO-278`/`278-B` (public API), `CO-145` (assets), `CO-128` (conflict UI → CO-385) | CO-internal priority; outside this PM's grounding. |

**Concurrency now:** Lane A is the front (yggdrasil YG-118→121 via **co-auto**, ∥ co session on
CO-389/383/385 + Lane G). B's follow-ups (YG-113/115/116) and content trickle run in parallel as
capacity allows — none gate launch.

## Wave sequence (revised)

- **Wave 1 — Phase-2 wave (DONE, v2.6.0):** lanes B/C/D/E + Godot decision landed via 5 parallel
  agents + integration.
- **Wave 2 — Bridge go-live (NOW):** `YG-118` (co-auto, first) freezes the wire → `YG-119` dial →
  `YG-120` JWKS → `YG-121` E2E; co session runs `CO-389` finish + `CO-383`/`385` against YG-118's
  frozen contract. **This is the v3.0 full-federation gate.**
- **Wave 3:** `CO-374` e2e gate; CO launch hardening (CO-278/145); trailing content lanes D/E/`YG-115`
  as capacity allows. Godot (F) decided independently.

## v3.0 vs v3.1 (the scope decision)

- **v3.0 (launch):** read-only notes ingest (CO-383) + comunicação observable + public API/assets
  hardening. "Edit at source →" deep-links back to yggdrasil.
- **v3.1 (fast-follow, yggdrasil half already done):** editable round-trip (CO-385 + YG-97 ✅),
  messaging federation (CO-389 + YG-103 ✅), corpus Caderno federation (YG-112 → YG-114),
  time-rendering/calendar lens (CO-387).

Recommendation: **ship v3.0 lean, fast-follow v3.1** — most of v3.1's yggdrasil work is built; gating it
all into launch makes CO serially walk CO-384→383→389→385 before go-live.

## Open PM decisions

1. **Launch scope** — lean v3.0 + v3.1 fast-follow (recommended), or hold for full editable+messaging+corpus?
2. **Godot (YG-35)** — is canvas→Godot migration in release scope, or post-launch R&D?
3. **Lane staffing** — how many parallel agents/teams can you actually run? (yggdrasil lanes B/C/D/E I can drive; A/G need the co session.)
4. **CO backlog (103 open)** — the CO-internal launch program (API/assets/infra) needs the co team's
   prioritization; this PM owns the bridge edges + the yggdrasil lanes with confidence.
