# Phase 20 — API Coverage Declaration

**Determined:** 2026-07-22 (during `/gsd-plan-phase 20`)

## No external API integration

This phase adds release-preflight and operator-recovery CLI commands that read
local git/version state; it wraps no external SDK or service.

Specifically:

- **20a** rewrites fields in a local `Cargo.toml` (`devflow-core::version`).
- **20b** guards local worktree removal and hardens test git fixtures.
- **20c** intercepts an in-process state-machine transition (`pipeline_gate`).
- **20d** `devflow release --check` *reads* local `git config`, shells out to
  local `git`/`ssh-add`/`gpg-connect-agent`, and states the crates.io publish
  ORDER as a structured, read-only check — it does **not** call the crates.io
  registry API, publish, or authenticate against any remote service.
- **20e** consumes an on-disk gate-response record and calls an existing
  in-process terminal hook batch (`pipeline_gate::finish_workflow`).

No capability matrix is produced because no external API/SDK/service is
integrated. This reasoned declaration is the seal-time gate artifact for the
API-coverage checkpoint.
