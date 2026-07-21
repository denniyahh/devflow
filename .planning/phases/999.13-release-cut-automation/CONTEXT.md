---
status: backlog
source: v1.5.0 release session, 2026-07-21 — three distinct manual-process failures in one release cut
---

# Backlog: Release-Cut Automation / `devflow release --check`

## Problem

DevFlow automates the *phase* pipeline thoroughly (Define→Plan→Code→Validate→Ship),
but the *release-cut* step — version-bump PR → merge to `main` → tag → sync `develop`
→ publish to crates.io — is a fully manual, hand-run checklist
(`CONTRIBUTING.md` § "Cutting a Release"). Cutting v1.5.0 hit three separate
failures from this gap, in one release:

1. **`devflow-core` version pin drift.** `crates/devflow-cli/Cargo.toml` hard-codes
   `devflow-core = { path = "../devflow-core", version = "X" }` instead of inheriting
   from `[workspace.package]`. This is the **second** time this has bitten — PR #10
   (`c9aff7f`) fixed the identical drift once already, by hand, going into a prior
   release. Nothing prevents a third occurrence except remembering. (Partially
   addressed directly in this session — see 2026-07-21 Decision entry moving
   `devflow-core` into `[workspace.dependencies]` — but there is still no CI
   enforcement that the pinned version and the workspace version can never diverge
   again undetected.)
2. **`main`/`develop` divergence.** Because `main` only accepts squash merges,
   `develop` silently fell behind by a full release cycle before this was caught,
   producing 11 file conflicts on the next release PR. Fixed with
   `scripts/sync-main-to-develop.sh`, but running it is still an unenforced manual
   step 5 in the checklist — nothing stops the next release from skipping it again.
3. **crates.io publish ordering was undocumented.** `cargo publish --dry-run -p devflow`
   fails to compile until `devflow-core` is live on the registry at a satisfying
   version, since dry-run/verify resolves the path dependency against the
   *published* registry version, not local source. Discovered by trial and error
   during this release; now documented in `CONTRIBUTING.md`, but only as prose.
4. **Tag-signing had no preflight.** The official signed tag failed repeatedly with
   opaque `ssh_askpass`/agent errors before the underlying environment issue was
   diagnosed. DevFlow's own automated version-bump tags already scope off signing
   entirely (`git.rs::tag()` forces `tag.gpgsign=false` per-invocation, confirmed
   at HEAD) — this is specifically about the *manual*, human-run official release
   tag, which has no equivalent safety net.

## Proposed shape

`devflow release --check` (name TBD) as a preflight command, run before attempting
the actual tag:

- Verify `crates/devflow-cli/Cargo.toml`'s `devflow-core` version matches
  `[workspace.package].version` (or: verify the workspace-inherited version once
  999.13's Cargo.toml fix lands — see 2026-07-21 Decision).
- Verify `develop`'s tip is reachable as an ancestor check against `origin/main`
  (i.e. `scripts/sync-main-to-develop.sh` would be a no-op) before allowing a new
  release PR to be described as ready.
- State the crates.io publish order as a structured check, not just prose: confirm
  `devflow-core`'s target version isn't already the tip of what's live if publishing
  is next.
- Check tag-signing viability: if `tag.gpgsign` is `true`, confirm a signing key is
  actually reachable (`ssh-add -l` / `gpg-connect-agent` succeeds) *before*
  attempting `git tag -s`, with an actionable error instead of the opaque
  `ssh_askpass: exec(...): No such file or directory` failure this session hit.

A full `devflow release` command that executes the whole checklist is a larger,
separate design question (open: should it also cut the tag itself, or stop at
"here's what to run next"?) — `--check` alone would already have caught 3 of the
4 issues above before they cost debugging time.

## Notes

Distinct from 999.7 (Manual Ship Override), which is about recovering a stuck
*phase* mid-pipeline. This is about the top-level version-cut process that happens
outside any single phase's Ship stage.

Promote with `/gsd-review-backlog` when ready.
