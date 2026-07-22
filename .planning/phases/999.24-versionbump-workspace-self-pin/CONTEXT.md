---
status: backlog
source: v1.6.0 release (2026-07-22); same defect previously patched by 7ad260c for v1.5.0
---

# Backlog: VersionBump Must Rewrite Workspace Member Self-Pins

## Goal

Make the `VersionBump` ship hook bump **every** place a workspace states its
version, not just `[workspace.package] version`.

`version::write_version` rewrites exactly one dotted field — `field_for()`
(`version.rs:59`) returns `"workspace.package.version"` for a workspace
`Cargo.toml`, and `replace_version_in_contents` rewrites that field alone. But
a published Cargo workspace states its version twice:

```toml
[workspace.package]
version = "1.6.0"                                            # VersionBump writes this

[workspace.dependencies]
devflow-core = { path = "crates/devflow-core", version = "1.6.0" }   # nothing writes this
```

The self-pin cannot use `version.workspace = true` — Cargo has no
interpolation for dependency versions — and cannot be omitted, because a path
dependency of a *published* crate requires an explicit version.

## Why fix it rather than document it

It has shipped broken **two for two**:

- **v1.5.0** — patched after the fact by `7ad260c`
- **v1.6.0** — patched by release-prep PR #15

The failure mode is invisible until the last step of a release. Everything
builds, every test passes, clippy is clean — a `path` dependency resolves
locally and ignores the `version` field entirely. It only detonates at
`cargo publish`, where the registry rejects the upload as a duplicate of the
already-published version. On release day. After `main` is tagged.

This is a **product** bug, not a repo chore: any DevFlow user with a published
Cargo workspace hits it identically and gets the same opaque duplicate-version
error with no hint about the cause.

## Proposed fix

After writing `workspace.package.version`, also rewrite every
`[workspace.dependencies]` entry whose `path` points at a workspace member.
That is a general rule, not a special case: *a dependency on a crate in this
workspace carries this workspace's version.*

Care needed:
- Only entries with a local `path`. A third-party dep like
  `serde = { version = "1" }` has a version but no path and must not be touched.
- Extend the existing hand-rolled TOML handling rather than pulling in a parser
  dependency — `version.rs` is deliberately hand-rolled.

## Already landed

`crates/devflow-cli/tests/workspace_version_pin.rs` asserts every
workspace-member pin equals `[workspace.package] version`, RED-proven against
the real defect. It converts a silent release-day rejection into a loud
pre-merge failure — but it is a **guard, not the fix**. The manual bump is
still required every release until `VersionBump` handles it.

## Rejected alternatives

- **Loosen the pin to `version = "1"`** — the symptom vanishes across 1.x, but
  `devflow 1.6.0` could then resolve against `devflow-core 1.5.0`. These crates
  release in lockstep and the CLI is tightly coupled to core, so that skew would
  surface as baffling runtime behavior. It also just defers the problem to 2.0.
- **Adopt `cargo-release` / `cargo-workspaces`** — solves it, but partly
  duplicates the tool whose entire purpose is automating releases.

Promote with `/gsd-review-backlog`.
