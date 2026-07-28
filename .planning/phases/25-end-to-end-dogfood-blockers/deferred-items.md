# Deferred Items — Phase 25 (out of scope for the discovering plan)

Items discovered during plan execution that are pre-existing, unrelated to the
discovering plan's own diff, and therefore NOT auto-fixed per the executor's
scope-boundary rule ("only auto-fix issues directly caused by the current
task's changes").

## `doc_check::doc_referenced_identifiers_exist_in_source` fails on `--stat`

- **Discovered by:** Plan 25-07, running `cargo test --workspace --no-fail-fast`
  as part of this plan's own `<verification>` step.
- **Confirmed pre-existing and unrelated to 25-07's diff:** `git diff --stat
  d2b6865 HEAD -- README.md ARCHITECTURE.md CONTRIBUTING.md docs/guides/
  doc-check-allowlist.toml crates/devflow-core/src/doc_check.rs` is empty —
  none of plan 25-07's three task commits touch any of these files.
  `d2b6865` is the wave's shared base commit, so this failure predates every
  Wave 3 plan (25-06, 25-07) and was already present when the wave started.
- **Failure:** `documented CLI flag \`--stat\` does not exist in Rust source`,
  panicking at `crates/devflow-core/src/doc_check.rs:376`.
- **Root cause:** `doc_check::documented_flags` extracts ANY bare
  `--lowercase-with-dashes` token from the scoped docs (README.md,
  ARCHITECTURE.md, CONTRIBUTING.md, `docs/guides/*.md`) with no context
  filtering for what command it belongs to, then asserts every such token
  exists somewhere in Rust source as a devflow CLI flag. `README.md:43`
  contains an example `git diff --stat origin/main..HEAD` line — `--stat` is
  `git`'s own flag, not a devflow flag, but the extractor cannot tell the
  difference and treats it as a claimed devflow flag anyway.
- **Fix direction (not applied here):** either add `--stat` to
  `doc-check-allowlist.toml`'s `docs_to_source` exceptions (mirroring the
  existing allowlist mechanism `doc_check.rs` already reads via
  `load_allowlist`), or make `documented_flags` context-aware enough to skip
  tokens inside a `git ...` example line. Scoped to whichever future plan
  owns `doc_check.rs`/the scoped docs — not in any Wave 3 plan's
  `files_modified`.
- **Impact on 25-07:** none. This plan's own success criteria, `<verify>`
  blocks, and acceptance criteria are all scoped to `commands.rs`/`main.rs`/
  `tests/reap_strays_e2e.rs`, none of which this failure touches. Recorded
  here per the scope-boundary rule rather than fixed, since fixing it would
  mean editing files entirely outside this plan's declared scope.
