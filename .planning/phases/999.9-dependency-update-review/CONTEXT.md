---
status: backlog
source: surfaced 2026-07-20 — GitHub Actions annotated `actions/checkout@v4` as targeting deprecated Node.js 20 during the first all-branch CI run (`234f080`)
---

# Backlog: Dependency Update Review

## Immediate trigger

Every CI job now emits:

> Node.js 20 is deprecated. The following actions target Node.js 20 but are
> being forced to run on Node.js 24: `actions/checkout@v4`.
> https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/

It is a **warning, not a failure** — runs are green (CI 1m21s, Devcontainer
1m47s at `234f080`). GitHub is force-running the action on Node 24. The risk is
that the forced upgrade is a transition measure; when it ends, or if `@v4`
behaves subtly differently under Node 24, the failure lands on every job at
once because all four use it. Bumping to `actions/checkout@v5` is the fix, but
it should be verified rather than assumed — the annotation appears on 4 job
definitions across 2 workflow files.

## Why this should be a review, not a one-line bump

Nothing else in the dependency surface has been audited in a while, and the
pins are inconsistent in kind — some exact, some floating, some implicitly
"latest". Doing a single bump leaves the rest unexamined. Current inventory as
of 2026-07-20:

| Surface | Pin | Notes |
|---|---|---|
| `actions/checkout` | `@v4` | **The trigger.** 4 usages across both workflows. `@v5` targets Node 24. |
| `dtolnay/rust-toolchain` | `@stable` | Floating tag — a moving target by design, no version pin at all. |
| `devcontainers/ci` | `@v0.3` | Pre-1.0; minor bumps may carry breaking changes. |
| devcontainer base image | `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` | Exact pin, verified against the registry tag list during Phase 15 (15-03). Likely stale by now. |
| `rust-toolchain.toml` | `channel = "stable"` | Floating. Local runs used cargo 1.97.1 on 2026-07-20. |
| Cargo deps | `serde 1`, `serde_json 1`, `toml 1`, `clap 4`, `thiserror 2`, `tracing 0.1`, `tracing-subscriber 0.3` | All major-only ranges; `Cargo.lock` is committed, so actual resolved versions drift only on explicit update. |

## Scope when promoted

- Bump `actions/checkout@v4` → `@v5` across both workflow files; confirm green
  on a branch before merging (all-branch CI makes this cheap now).
- Decide a policy on floating vs. pinned action tags. `dtolnay/rust-toolchain@stable`
  and `channel = "stable"` mean CI can break from an upstream change with no
  commit here — that is a real reproducibility gap for a project whose whole
  premise is trustworthy automated pipelines. Weigh against the maintenance cost
  of pinning.
- Re-verify the devcontainer base image tag against the registry (the Phase 15
  note documents the method).
- `cargo update` review with `Cargo.lock` diff; check for advisories. Consider
  whether `cargo audit` / `cargo deny` belongs in CI — currently neither runs.
  Note `cargo-outdated` is not installed locally.
- Update `DEPENDENCIES.md` if any documented minimum versions shift.

## Notes

Not urgent — nothing is failing. Best batched into one pass rather than
trickled, since each change wants its own CI verification and they share the
same verification cost. Deliberately **not** folded into Phase 18, which is
scoped to pipeline-reliability bug fixes; a dependency bump landing mid-phase
would confound Phase 18's own test signal.

Promote with `/gsd-review-backlog` when ready.
