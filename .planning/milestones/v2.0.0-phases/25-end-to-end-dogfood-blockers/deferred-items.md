# Deferred Items — Phase 25

Items discovered during plan execution and deliberately not fixed by the
discovering plan. Each entry records what was resolved at wave close and what
genuinely remains open.

## RESOLVED — `doc_check::doc_referenced_identifiers_exist_in_source` failed on `--stat`

**Status: fixed by the orchestrator at Wave 3 post-merge.** Retained here because
the original entry recorded an incorrect attribution that would mislead a later
phase.

- **Discovered by:** Plan 25-07, running `cargo test --workspace --no-fail-fast`
  as part of its own `<verification>` step.
- **Original claim (INCORRECT):** "pre-existing … predates every Wave 3 plan
  (25-06, 25-07) and was already present when the wave started."
- **Correction:** the test **passed at `d2b6865`**, the Wave 3 shared base —
  verified from the orchestrator's Wave 2 post-merge gate log, in which all
  `doc_check` tests are green and the sole workspace failure is `pipeline_gate`'s.
  The regression was introduced during Wave 3, not inherited.
- **Why the original analysis missed it:** 25-07 diffed its commits against
  `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `docs/guides/`,
  `doc-check-allowlist.toml` and `doc_check.rs`, found the diff empty, and
  concluded it was not responsible. The coupling ran through a file it *did*
  edit and did not include in that check:
  `crates/devflow-cli/src/commands.rs`.
- **Actual mechanism:** `doc_check` scrapes `--stat` from `README.md:43`'s
  `git diff --stat origin/main..HEAD` example and asserts it exists in Rust
  source. The only thing satisfying that assertion was an incidental substring
  in a *code comment* at `commands.rs:3303`
  (`// /proc/<pid>/stat: field 3 = state, 14 = utime, 15 = stime, 22 = starttime.`),
  which matched the extractor's `source.contains("stat:")` fallback. 25-07
  legitimately rewrote that region as part of its `looks_like_devflow_process`
  migration at :3308, the comment changed, and the coincidental match vanished.
- **Fix applied:** added `--stat` to `doc-check-allowlist.toml`'s
  `docs_to_source` exceptions, with a reason, matching the established pattern
  for `--no-ff`, `--no-deps`, `--path`, `--release` and `--workspace-folder` —
  all third-party flags appearing in documentation examples.
- **Credit where due:** 25-07's root-cause analysis of the extractor was correct
  and identified the applied fix. Only the attribution was wrong.
  status: acknowledged

## OPEN — `doc_check::documented_flags` has no command context

The underlying brittleness is real and outlived the fix above. `documented_flags`
extracts any bare `--lowercase-with-dashes` token from the scoped docs with no
awareness of which command it belongs to, then asserts it is a DevFlow CLI flag.
Consequences:

1. Every third-party flag appearing in a documentation example must be
   allowlisted by hand; the allowlist is now six entries of pure false positive.
2. More seriously, the assertion can be satisfied by **arbitrary unrelated
   substrings anywhere in Rust source, including comments**. A documented flag
   can therefore read as "verified" with no corresponding CLI surface at all,
   and an unrelated comment edit can flip the result — as it just did. That is a
   false-evidence shape of the same class D-18 exists to prevent: the check
   reports agreement between docs and code where none was established.

**Fix direction:** make `documented_flags` context-aware (skip tokens inside a
fenced example whose leading command is not `devflow`), and tighten the source
side from a bare `contains` to a match against the actual clap flag surface.

**Not filed to the backlog** — no owning plan in Phase 25. Worth a backlog entry
before this check is relied on as release evidence.
