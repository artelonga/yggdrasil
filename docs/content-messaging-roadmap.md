# Roadmap — Content + Messaging to GA (Yggdrasil)

> The two subsystems Yuri framed as "Yggdrasil will have a **content / messaging** system."
> Both shipped to prod but are **untracked** and (for notes) **uncommitted**. This roadmap turns
> that into tracked work toward **v2.2.0 "Content + Messaging GA"**, with the editable round-trip
> *and* messaging federation deferred to **v2.3.0** (= CO v3.1). Epic: **YG-94**.

## State today (v2.1.0)

| Subsystem | What shipped | Gap |
|---|---|---|
| **Content** — notes / jardim | NoteStore + `[[wikilinks]]` + backlinks + graph + editor (Phase 0, prod 2026-06-06) | **Uncommitted + untracked**; no jardim UX; read-only at CO only |
| **Messaging** — comunicação | Salas de léxico, write-back to disk, spaced review, yoruba/mbya templates (v1.2.0) | **Untracked**; not in the lobby; contributions not committed; no curation flow; no e2e |
| **Bridge** — notes → CO | — | **YG-93** open (federated producer; CO v3.0 Theme 7 gate) |

## The plan (YG-94 epic → 9 tasks)

### Theme A — Content (notes / jardim)
| Task | Title | Milestone |
|---|---|---|
| **YG-95** (A0) | Commit + track Phase 0 notes (NoteStore/wikilinks/graph) | v2.2.0 — **gate** |
| **YG-93** (A1) | Federate notes → CO bus (read-only ingest) | v2.2.0 (= CO v3.0 gate) |
| **YG-96** (A2) | Notes/jardim UX — `jardim` template + search + notes-first view | v2.2.0 |
| **YG-97** (A3) | P-B bidirectional apply — notes editable at CO flow back | **v2.3.0** / CO v3.1 |

### Theme B — Messaging (comunicação) — the documented pendências, now tracked
| Task | Title | Milestone |
|---|---|---|
| **YG-98** (B0) | Track + commit comunicação | v2.2.0 — **gate** |
| **YG-99** (B1) | Lobby portal for comunicação (discoverable) | v2.2.0 |
| **YG-100** (B2) | Write-back commit/sync — `_users/` → git commit/PR | v2.2.0 |
| **YG-101** (B3) | Curation review — promote `stub` → `reviewed` | v2.2.0 |
| **YG-102** (B4) | `e2e-comunicacao.sh` | v2.2.0 |

### Theme C — Cross-cutting
| Task | Title | Milestone |
|---|---|---|
| **YG-103** (C1) | Extend the YG-93 producer to emit comunicação events → CO (read-only) | **v2.3.0** / CO v3.1 (+ CO-389) |

## Sequencing + critical path

1. **Gates first:** YG-95 (commit notes) + YG-98 (track/commit comunicação) — nothing is "releasable" until the code is under version control with a bump + CHANGELOG.
2. **Cross-repo gate:** YG-93 stays the hard prerequisite for CO's v3.0 Theme 7 (Batch C).
3. **Messaging reachability + durability:** YG-99 (portal) → YG-100 (write-back commit) → YG-101 (curation) → YG-102 (e2e).
4. **Messaging in v2.2.0 stays unfederated** — "tracked + in-lobby + persisted." Federation (YG-103) moves to v2.3.0 to **decouple from the CO-side consumer** (CO-389), so neither repo's release waits on the other.
5. **v2.3.0 (= CO v3.1, deep integration):** YG-97 (editable notes round-trip, with CO-385) + YG-103 (messaging federation, with CO-389 + CO-386). One coherent cut.

## Release cut
- **v2.2.0 "Content + Messaging GA"** = both subsystems committed, tracked, reachable from the lobby, persisted (write-back committed), e2e-tested. **Notes** federated to CO read-only (YG-93). **Messaging** = "tracked + in-lobby + persisted, **not yet federated**."
- **v2.3.0 (= CO v3.1)** = editable notes round-trip (YG-97 + CO-385) **and** messaging federation (YG-103 + CO-389 + CO-386).

## Out of scope (other open YG tasks, not content/messaging)
YG-28 (poker WS), YG-32–35 (Godot POC), YG-63–67 (per-universe versioning), YG-68–72 (RPG catalog).
