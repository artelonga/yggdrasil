# Release Execution Pipeline — Yggdrasil × co

> PM view as of 2026-06-08. Spans two repos (`yggdrasil`, `co`). The bridge is the
> seam; this doc sequences the whole portfolio into parallel lanes with one critical path.

## TL;DR

- **The bottleneck is `CO-384` (federated event-bus hub).** It is the single serialization
  point. Nothing federates live until it lands; after it, three consumers fan out in parallel.
- **The yggdrasil bridge side is DONE** — producer (YG-93), inbound apply (YG-97), comunicação
  federation (YG-103), curation (YG-101) all shipped + CI-green. yggdrasil is *waiting*, not blocking.
- **CO's EDA spine is DONE** — CO-380 (event bus) + CO-381 (/agora) shipped. CO-384 builds on them.
- **CO-389's only YG dependency (YG-101) is satisfied** → it unblocks the instant CO-384 exists.
- Recommended shape: **lean v3.0 read-only launch + v3.1 fast-follow** (whose yggdrasil half is
  already built). Blocking launch on "all" makes CO the long pole on ~5 serial consumer tasks.

## Critical path (live federation → launch)

```
CO-380 ✅ ──┐
CO-381 ✅ ──┼──► CO-384 (hub) ──┬──► CO-383 (notes ingest, read-only)  ──► v3.0 live notes at co
            │   ⟵ THE bottleneck │                                        │
            │                    ├──► CO-389 (messaging live)  ──────────►│  (YG-101 ✅ satisfied)
[YG-93/97/  │                    └──► CO-385 (UPSERT modal) ──► editable  │  v3.1
 103/101 ✅]┘                                                  round-trip │
                                                                          ▼
                                              CO-374 (staging Playwright e2e gate) ──► launch
            launch gates (CO-internal): CO-278/278-B (public API + rate limits),
                                        CO-145 (encrypted assets)
```

`YG-93/97/103/101` sit at the left edge **already complete** — they feed CO-383/389/385 the instant
those land. The instance-qualified note path `instances/<id>/notes/<slug>.md` (YG-97) is the wire
contract CO-383/384/385 must match.

## Parallel lanes (what can run concurrently)

| Lane | Owner | Tasks | Notes |
|---|---|---|---|
| **A — Bridge CO-side** (critical) | co session | `CO-384` → { `CO-383` ∥ `CO-389` ∥ `CO-385` } → `CO-374` | CO-384 serializes; then 3-wide fan-out. The launch critical path. |
| **B — Corpus / Caderno** | yggdrasil | `YG-111` (finish surface) ∥ `YG-112` (Caderno persist) → `YG-114` (federation, trivial) ∥ `YG-113` (suggestions→curation) → `YG-115` (etymology) | Independent of A until YG-114 e2e needs CO-389. YG-112 is the real unblock (YG-114 rides YG-97's path for free). |
| **C — CI debt** | yggdrasil | `YG-104` (repair WASM crates + Godot lint; remove band-aids) | Fully independent. Restores honest CI gating. Run anytime. |
| **D — Per-universe versioning** | yggdrasil | `YG-63`..`YG-67` | Independent tooling. Parallelizable. |
| **E — Catalog expansion** | yggdrasil | `YG-68`..`YG-72` (Shandara SRD, REGISTRY.yaml, ~40 RPG seeds) | Independent content. **Explicitly post-1.0** per YG-68. |
| **F — Godot POC** | yggdrasil-godot | `YG-32`..`YG-35` | Separate stack. **YG-35 = migration DECISION** (canvas vs Godot) — a strategic gate, timebox it; not release-blocking filler. |
| **G — CO launch hardening** | co session | `CO-278`/`278-B` (public API+limits, *critical*), `CO-145` (assets, *critical*), `CO-128` (conflict UI → feeds CO-385) | CO-internal priority; outside this PM's grounding. |

**Concurrency ceiling:** Lanes A/G are co-session work; B/C/D/E are yggdrasil and can run as separate
agents/worktrees; F is a different toolchain. Realistic simultaneous front: **A + (B,C) + one of D/E**.

## Wave sequence

- **Wave 1 (now):** A starts `CO-384` (critical). B starts `YG-111` + `YG-112`. C does `YG-104`.
  → 3 lanes live; CO-384 is the gating item to watch.
- **Wave 2 (CO-384 lands):** `CO-383` ∥ `CO-389` ∥ `CO-385` fan out. `YG-114` lights up (path already
  contracted by YG-97 → a confirmation test + AYVU_INSTANCE const). `YG-113` after YG-112.
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
