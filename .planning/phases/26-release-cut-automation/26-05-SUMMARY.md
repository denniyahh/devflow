---
phase: 26-release-cut-automation
plan: 05
subsystem: infra
tags: [cargo, crates.io, publish, subprocess, git.rs, devflow-core]

# Dependency graph
requires:
  - phase: 26-release-cut-automation
    provides: "publish_order (git.rs:740, existing) — the package-sequence primitive this plan composes with but never recomputes"
provides:
  - "PublishCheck enum (AlreadyPublished/NotPublished/Ambiguous) — typed verdict from a cargo info existence check"
  - "classify_cargo_info_result — pure classifier, exit code + stderr in, PublishCheck out"
  - "cargo_info_args — private, pure argv builder for cargo info <name>@<version> --registry crates-io"
  - "PublishError enum (Io/Ambiguous/Failed)"
  - "crate_already_published — spawns cargo info, maps the verdict to Ok(bool)/Err"
  - "cargo_publish — spawns cargo publish -p <package>, the one-way primitive (D-04)"
affects: ["26-06 (release executor's publish step, the intended caller of crate_already_published/cargo_publish)", "26-07 (CLI --yes-release surface)"]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure classifier + thin I/O wrapper (mirrors inline_signing_key_blob / AncestorStatus): classify_cargo_info_result takes only Option<i32> + &str so all branch logic is hermetically unit-testable without a subprocess."
    - "Untrusted subprocess stderr is passed through version::sanitize_changelog_subject before it is stored in any error type, bounding length and neutralizing control characters at the point of capture, not at the point of display."

key-files:
  created: []
  modified:
    - "crates/devflow-core/src/git.rs — added PublishCheck, PublishError, classify_cargo_info_result, cargo_info_args, crate_already_published, cargo_publish, and 4 new tests"

key-decisions:
  - "Classification order is exit-code-first, then a two-fragment stderr match (both 'could not find' AND 'registry' must be present) before returning NotPublished; anything else — including a missing-manifest error that only names one fragment — is Ambiguous. This is what discriminates the missing-Cargo.toml case from a genuine not-published case, per RESEARCH.md Pitfall 3."
  - "No PATH shim and no live-registry test, per the plan's own resolved hermeticity question — classification logic lives entirely in the pure classify_cargo_info_result, tested with captured fixtures; the live tool's real behavior was independently re-confirmed once via the tracer's oracle probe."
  - "cargo_publish never accepts --dry-run and has no retry loop, matching D-04/D-05 exactly as specified — verified absent via `rg -n '\"--dry-run\"'`."

requirements-completed: ["999.25"]

coverage:
  - id: D1
    description: "classify_cargo_info_result correctly distinguishes AlreadyPublished (exit 0) / NotPublished (both stderr fragments present) / Ambiguous (everything else, including the missing-manifest discriminating case, absent exit code, and empty stderr) — six documented cases, each asserting the exact variant."
    requirement: "999.25"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::publish_check_classifies_exit_codes"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::cargo_info_args_targets_the_exact_version_on_crates_io"
        status: pass
    human_judgment: false
  - id: D2
    description: "crate_already_published spawns cargo info via the fixed argv and maps AlreadyPublished/NotPublished/Ambiguous to Ok(true)/Ok(false)/Err — the ambiguous case never degrades into a boolean, and the stored detail is bounded and control-character-free."
    requirement: "999.25"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::crate_already_published_surfaces_an_ambiguous_check_as_an_error"
        status: pass
    human_judgment: false
  - id: D3
    description: "cargo_publish spawns cargo publish -p <package> with no --dry-run and no retry, returning Err(PublishError::Failed(detail)) on failure (bounded, control-character-free) and Ok(()) on success; the failure path is proven via a fixture directory containing no Cargo.toml, so the test structurally cannot reach the registry."
    requirement: "999.25"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#git::tests::cargo_publish_reports_a_failure_without_publishing_anything"
        status: pass
    human_judgment: false
  - id: D4
    description: "The real cargo info tool's real behavior against the live crates.io registry still matches RESEARCH.md's Pattern 4 transcripts (the tracer's live oracle probe, not a #[test])."
    verification:
      - kind: manual_procedural
        ref: "cargo info devflow-core@2.1.0 --registry crates-io (exit 0); cargo info devflow-core@0.0.1 --registry crates-io (exit 101, stderr: could not find `devflow-core@0.0.1` in registry `https://github.com/rust-lang/crates.io-index`)"
        status: pass
    human_judgment: false
  - id: D5
    description: "A real cargo publish of devflow-core then devflow completes against the live registry in the correct order with the operator's own credentials (backstop truth — this plan does not and cannot close it; deferred to 26-06's actual release run)."
    verification: []
    human_judgment: true
    rationale: "This is 26-VALIDATION.md's Manual-Only Verifications row 2: a real, irreversible cargo publish. No test in this plan performs or can perform it (D-04/D-05). It remains manual-pending until an operator runs the actual release via 26-06."

# Metrics
duration: 35min
completed: 2026-07-29
status: complete
---

# Phase 26 Plan 05: cargo publish primitives (existence oracle + publish) Summary

**Added `PublishCheck`/`classify_cargo_info_result` (pure `cargo info` classifier), `crate_already_published`, and `cargo_publish` to `devflow_core::git` — the two primitives the release executor's final step needs and DevFlow has never had.**

## Performance

- **Duration:** ~35 min
- **Completed:** 2026-07-29T20:53:06-04:00
- **Tasks:** 3/3
- **Files modified:** 1 (`crates/devflow-core/src/git.rs`)

## Accomplishments
- `PublishCheck` (`AlreadyPublished`/`NotPublished`/`Ambiguous { detail }`) and the pure `classify_cargo_info_result(exit_code, stderr)` classifier, table-tested against all six documented cases (exit 0, both-fragments-present, network/index failure, the missing-manifest discriminating case, absent exit code, empty stderr) — every case asserts the exact variant, never merely "not `NotPublished`".
- `cargo_info_args` pins the exact `info <name>@<version> --registry crates-io` argv without spawning anything.
- `PublishError` (`Io`/`Ambiguous`/`Failed`) and `crate_already_published(project_root, name, version)`, which spawns `cargo info` and maps the classifier's verdict to `Ok(bool)`/`Err` — the ambiguous case is structurally incapable of degrading into either boolean.
- `cargo_publish(project_root, package)`, the one-way `cargo publish -p <package>` primitive (D-04), with no `--dry-run` and no retry loop, doc-citing the operator's `automate-publish` authorization (`26-01-SUMMARY.md` Decision 2 / `D2`) rather than re-asking it.
- Live oracle probe re-run against the real crates.io registry from this checkout: `cargo info devflow-core@2.1.0 --registry crates-io` exits 0; `cargo info devflow-core@0.0.1 --registry crates-io` exits 101 with `error: could not find \`devflow-core@0.0.1\` in registry \`https://github.com/rust-lang/crates.io-index\`` — matches `classify_cargo_info_result`'s assumptions exactly.

## Task Commits

Each task was committed atomically:

1. **Task 1: The existence oracle, end to end — argv in, typed verdict out** - `9788837` (feat)
2. **Task 2: `crate_already_published` — the wrapper that turns the verdict into a decision** - `861b1c9` (feat)
3. **Task 3: `cargo_publish` — the one-way step (D-04)** - `44d4516` (feat)

_No plan-metadata commit in this worktree — STATE.md/ROADMAP.md are updated centrally by the orchestrator after all wave agents merge._

## Files Created/Modified
- `crates/devflow-core/src/git.rs` - `PublishCheck`, `PublishError`, `classify_cargo_info_result`, `cargo_info_args` (private), `crate_already_published`, `cargo_publish`, and 4 new tests (`publish_check_classifies_exit_codes`, `cargo_info_args_targets_the_exact_version_on_crates_io`, `crate_already_published_surfaces_an_ambiguous_check_as_an_error`, `cargo_publish_reports_a_failure_without_publishing_anything`)

## Decisions Made
- Classification requires BOTH stderr fragments ("could not find" AND "registry") before returning `NotPublished` — a stderr naming only one (e.g. a missing-`Cargo.toml` error) classifies `Ambiguous`, per RESEARCH.md Pitfall 3's explicit discriminating-case requirement.
- No `PATH` shim, no live-registry unit test — the hermeticity question `26-VALIDATION.md` left open is resolved by keeping all decision logic in the pure `classify_cargo_info_result`, per the plan's own rationale (a shim mutates process-global state and only covers `Command::spawn`, which is `std`; a live test breaks offline/CI runs).
- `cargo_publish` carries no `--dry-run` option and no retry loop, matching D-04/D-05 literally — confirmed absent via `rg -n '"--dry-run"' crates/devflow-core/src/git.rs` (no match).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Interim `#[allow(dead_code)]` on `cargo_info_args` during Task 1's own commit, removed in Task 2**
- **Found during:** Task 1, while committing tasks atomically per-task (as required) rather than as one combined edit.
- **Issue:** The plan's own Task 1 acceptance criteria requires `cargo clippy --workspace --all-targets -- -D warnings` to be clean immediately after Task 1's commit. `cargo_info_args` is a private helper with no production caller until Task 2 adds `crate_already_published`; in the non-test library compilation profile (which drops `#[cfg(test)] mod tests`), `cargo_info_args` has zero callers at that intermediate point, so clippy's `dead_code` lint correctly fires and blocks the `-D warnings` gate.
- **Fix:** Added a scoped `#[allow(dead_code)]` with a doc comment stating the real caller lands in the very next task, then removed the attribute in Task 2's commit once `crate_already_published` became the real production caller (confirmed via a second clean clippy run in Task 2).
- **Files modified:** `crates/devflow-core/src/git.rs` (attribute added in Task 1's commit `9788837`, removed in Task 2's commit `861b1c9`)
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` ran clean after every one of the 3 task commits; the final cumulative diff (`git diff <base> HEAD -- crates/devflow-core/src/git.rs`) is byte-identical to a single-pass implementation validated end-to-end before being split into per-task commits.
- **Committed in:** `9788837` (added), `861b1c9` (removed)

---

**Total deviations:** 1 auto-fixed (Rule 3 — blocking, self-resolving within the plan's own task sequence)
**Impact on plan:** Purely mechanical, an artifact of atomizing a single cohesive feature into 3 task-scoped commits; no scope creep, no behavior change, and the interim attribute is gone by the plan's second commit.

## Issues Encountered
None beyond the deviation above.

## User Setup Required
None - no external service configuration required. (`cargo_publish`'s real success path requires the operator's own crates.io credentials at actual release time — that is 26-06's concern, not this plan's.)

## Next Phase Readiness
- `26-06` can now call `crate_already_published` before `cargo_publish` per `publish_order`'s existing sequence — all three primitives named in `26-VERIFICATION.md` Truth 11 and `26-CONTEXT.md` D-04 now exist and are tested.
- `26-VALIDATION.md` § "Manual-Only Verifications" row 2 (a real, operator-run `cargo publish` of both crates in order) remains manual-pending by design — this plan cannot and does not close it; it is recorded as coverage id `D5` above with `human_judgment: true`.
- No blockers for `26-06`/`26-07`.

---
*Phase: 26-release-cut-automation*
*Completed: 2026-07-29*

## Self-Check: PASSED

- FOUND: `crates/devflow-core/src/git.rs`
- FOUND: `.planning/phases/26-release-cut-automation/26-05-SUMMARY.md`
- FOUND commit `9788837` (Task 1)
- FOUND commit `861b1c9` (Task 2)
- FOUND commit `44d4516` (Task 3)
