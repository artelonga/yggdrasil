# CHANGELOG-PENDING

Per-task changelog fragments, consumed by a single **release commit** that bumps
`Cargo.toml` + consolidates `CHANGELOG.md`. Adopted 2026-06-06 (YG-94 epic) so that
**parallel agents don't conflict** on `Cargo.toml`/`CHANGELOG.md`.

## Rule for agents (waves with parallel YG tasks)

- **Do NOT** edit `Cargo.toml` (workspace version) or `CHANGELOG.md` in a task PR.
- Instead write `CHANGELOG-PENDING/YG-<n>.md` describing what changed.
- The release commit (one agent, after the wave merges) bumps the version and folds
  every `CHANGELOG-PENDING/*.md` into `CHANGELOG.md`, then deletes them.

This mirrors `co`'s proven wave pattern (`CHANGELOG-PENDING/` + `release-commit`).

## Fragment format

```markdown
## YG-<n> — <title>

<what changed and why, user-facing>
```
