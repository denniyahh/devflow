---
phase: 50-addressable-monitor-liveness
plan: 29
subsystem: testing
tags: [live-run, evidence, sanitizer, identity-scan, verdict, validation-map, review-disposition, macos]

requires:
  - phase: 50-28
    provides: live attempt 2 and fault-7 on the fixed binary, raw under $HOME/.cache/devflow-phase50
  - phase: 50-27
    provides: check-supervisor-evidence.sh --attempt-gate --current-source
  - phase: 50-26
    provides: the WR-03 fix, its tests and revert check
provides:
  - "check-supervisor-evidence.sh --identity-scan, --published --attempt <N>, --live-run --attempt <N> (append-only)"
  - "50-evidence/attempt-2 and 50-evidence/fault-7: sanitized, curated, verdicts saved raw first"
  - "50-LIVE-RUN.md around attempt 2 (attempt 1 kept as pre-fix history); 50-VALIDATION.md V-58..V-68, V-28/V-29 on attempt 2"
  - "WR-03 recorded fixed; 999.166 item 3 fixed, item 6 names the exit-file limit"
affects: [phase-50-verification, 999.166, deferred-items]

actuals:
  tokens: 938949    # chars/4 over the whole realized diff 56bef8d..dd1c381; 918627 of it is the 94 published evidence files, about 20300 is hand-written
  tasks: 3
  commits: 5        # measured before the SUMMARY commit: git rev-list --count 56bef8d..HEAD
plan_head_before: 56bef8de2ed494b567c948ff243546f164523203
plan_head_after: dd1c381ebb310a42b7dd9387aec4c9f5af4e2db3

tech-stack:
  added: []
  patterns:
    - "Attempt-scoped judges: a check that names the attempt it is about, with a selftest that fails a tree or record holding only an earlier attempt"

key-files:
  created:
    - .planning/phases/50-addressable-monitor-liveness/50-evidence/attempt-2/ (82 files)
    - .planning/phases/50-addressable-monitor-liveness/50-evidence/fault-7/ (12 files)
  modified:
    - .planning/phases/50-addressable-monitor-liveness/tools/check-supervisor-evidence.sh
    - .planning/phases/50-addressable-monitor-liveness/50-LIVE-RUN.md
    - .planning/phases/50-addressable-monitor-liveness/50-VALIDATION.md
    - .planning/phases/50-addressable-monitor-liveness/50-REVIEW-DISPOSITION.md
    - .planning/phases/50-addressable-monitor-liveness/deferred-items.md
    - .planning/ROADMAP.md

key-decisions:
  - "Three planted control credentials in attempt 2's stream transcript were redacted in the published copy only (REDACTED-PLANTED-CONTROL-SECRET), by the operator's decision of 2026-10-21; the raw tree is unchanged"
  - "Only fault-7 is published with attempt 2: it is the directory the gate pairs by bin_sha256; fault-6 stays raw"
  - "--live-run keeps its two-argument parsing byte-for-byte and adds only the four-argument `<file> --attempt <N>` form, so no existing invocation changes behaviour"
  - "No non-blocking defect from attempt 2; no backlog entry added"

patterns-established: []

requirements-completed: [SUPV-01]

coverage:
  - id: D1
    description: "Attempt 2 and fault-7 published sanitized and curated, tied to attempt 2's current-source verdict, with no identity fragment and no credential"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "Task 1 gate 2: --published 50-evidence && --published 50-evidence --attempt 2 && --identity-scan 50-evidence && sanitize-evidence.py scan 50-evidence (GATE ok attempt=2)"
        status: pass
      - kind: other
        ref: "Task 1 gate 3: raw attempt-1 capture fails --identity-scan with account hit lines, published tree passes (GATE ok)"
        status: pass
    human_judgment: false
  - id: D2
    description: "--live-run --attempt <N> with two selftest cases"
    verification:
      - kind: unit
        ref: "check-supervisor-evidence.sh --selftest#live run fails a record quoting only an earlier attempt verdict"
        status: pass
    human_judgment: false
  - id: D3
    description: "50-LIVE-RUN.md and 50-VALIDATION.md rewritten around attempt 2"
    requirement: SUPV-01
    verification:
      - kind: other
        ref: "Task 2 gates 2 and 3 (--live-run --attempt 2, --handoff, the validation-map gate)"
        status: pass
    human_judgment: true
    rationale: "The judges check that fragments are present, not that the criterion table, limits and behaviour cells say the right thing; those are written by hand against the evidence."
  - id: D4
    description: "WR-03 fixed in the review ledger, 999.166 items 3 and 6, attempt 2's defect disposition, macOS compile gate"
    verification:
      - kind: other
        ref: "Task 3 gates 1-5 (--defects, attribution, ledger, item 6, cargo check aarch64-apple-darwin x2)"
        status: pass
    human_judgment: false

duration: 30min (this continuation, 17:46-18:16 EDT; Task 1 step 1 and step 2 were done before the checkpoint)
completed: 2026-10-21
status: complete
---

# Phase 50 Plan 29: Judge and Publish Live Attempt 2 Summary

**Attempt 2 (fixed binary `182ab8b5…`, built from `bca7c35`) and its paired fault record `fault-7` are published under `50-evidence/` with their raw-tree verdicts (`ATTEMPT_GATE current_source=yes`, `ATTEMPT_GATE verdict=shipped attempt=2`, `FAULT verdict=pass`). Three made-up credentials the live agent planted as a gitleaks control were redacted in the published copy by operator decision. 50-LIVE-RUN.md now takes its verdict from attempt 2 and keeps attempt 1 as pre-fix history. 50-VALIDATION.md has V-58..V-68, and V-28/V-29 quote attempt 2. WR-03 is recorded fixed, and the macOS compile gate passes on the final tree.**

## What this does NOT establish (read first)

- **One shipped attempt is one sample.** It shows the fixed binary *can* carry this payload to Ship. It bounds no rate.
- **Neither fix's path ran live.** No stop, kill or signal was sent in attempt 2. Its raw `events.jsonl` has **0** `stream_wait_bounded` events, counted two independent ways. So the CR-01 and WR-03 stop paths rest on their tests, on 50-26's revert check (not re-run here, 10-15 min) and on the fault arm.
- **The judges check presence, not meaning.** `--live-run`, `--defects` and the validation-map gate check that fragments are there. The criterion table, the limits and the V-58..V-68 behaviour cells were written by hand against the evidence.
- **`--identity-scan` covers two identifiers only:** the host's Claude account email and its git `user.email`. The GitHub user name appears by design.
- **The planted `ghp_` value** was checked against the host's `gh auth token` by equality only, and they differ. Whether it, or the two AWS values, is valid anywhere else is unknown. They were never printed.
- **V-56 is compile only.** `devflow-core`'s check was a cached no-op (0.09 s) on a crate tree unchanged since `bca7c35`. `devflow` recompiled because its build.rs embeds the commit.

## Performance

- **Duration:** about 30 min for this continuation (Task 1 steps 3-5, Tasks 2 and 3)
- **Completed:** 2026-10-21T22:16Z
- **Tasks:** 3/3
- **Files:** 94 evidence files created; 6 planning or tool files modified

## Accomplishments

### Task 1: judge, check identity, sanitize and publish (tracer)

- **Step 1 (before the checkpoint, `c36f0d9`):** added `--identity-scan` and `--published --attempt`, with 13 planned selftest cases and 2 extra ones (a token in a file name; a paired fault dir with another `bin_sha256`). RED: 58 passed and 14 failed, on usage exits. GREEN: `SELFTEST passed=73 failed=0`. Gates 1 and 3 printed `GATE ok`.
- **Step 2 (before the checkpoint):** wrote the raw verdicts to new files only: `attempt-2/verdict-attempt.txt`, `attempt-2/verdict-attempt-gate.txt`, `fault-7/verdict-attempt-gate.txt`, `fault-7/verdict-fault.txt`. Not re-run.
- **Step 3:** staged under `target/p50-publish-2/` with 50-20's curation (no `project/`, `collected/sessions/`, `prelaunch/agent-env-names.txt`, `ledger.lock` or `collected/devflow/.gitignore`). That staged 82 files of attempt 2 and 12 of fault 7.
- **Redaction, under the operator's 2026-10-21 decision ("Redact in published copy"):** the three planted values (AWS access key id, AWS secret access key, `ghp_` GitHub token) came from line 171 of the raw `collected/devflow/phase-01-stdout`. They were replaced with `REDACTED-PLANTED-CONTROL-SECRET` in the **staged** copy only:

  | File (staged attempt-2) | aws_access_key_id | aws_secret_access_key | github_token | markers after |
  |---|---|---|---|---|
  | `collected/devflow/phase-01-stdout` | 1 | 1 | 1 | 3 |
  | `gates/003-ship/devflow/phase-01-stdout` | 1 | 1 | 1 | 3 |

  A count over every staged file found the values in those two files only. After redaction a recount found none. The raw file has 0 markers (`rg` exit 1), and the values still derive from it, so the raw tree is unchanged. `HOSTTOKEN gh_auth_token_rc=0 host_token_present=yes ghp_equals_host_token=no`.
- **Redaction proved both ways:**
  - The sanitizer refused the unredacted staged copy: exit 1, four `refused:` lines (`github_token_prefix`, `credential_named_assignment` × 2 files), and it left no destination. It accepted the redacted copy (exit 0), and accepted `fault-7` (exit 0).
  - gitleaks on the raw file: rc 1, 2 findings (`aws-access-token`, `github-pat`). On both sanitized copies and on the published tree: rc 0, 0 findings. The pre-commit gitleaks hook also printed `no leaks found`.
- **Step 4, scans of the sanitized copies:** the sanitizer `scan` exited 0 on both. `--identity-scan` printed `IDENTITY tokens=9`, `IDENTITY verdict=pass` on both. There was **no identity hit, so no `REDACTED-EMAIL-*` replacement**, and no `source=git` or `part=address` hit. Raw attempt 2 holds 16 files with account-address hits (tokens 1-5), all under the curated-out `collected/sessions/`.
- **Step 5:** moved the copies into `50-evidence/attempt-2` and `50-evidence/fault-7` and removed `target/p50-publish-2` by exact path. Then ran the gates:
  - Gate 1: `GATE ok`.
  - Gate 2: `PUBLISHED verdict=pass`; `PUBLISHED attempt=2 verdict_line=shipped`, `PUBLISHED attempt_current_source=yes`, `PUBLISHED attempt_fault=fault-7`, `PUBLISHED verdict=pass`; `IDENTITY tokens=9`, `IDENTITY verdict=pass`; `GATE ok attempt=2`.
  - Gate 3: the raw attempt-1 capture printed `IDENTITY tokens=9`, `IDENTITY hit token=3 source=account part=local file=phase-01-stdout`, `IDENTITY hit token=5 source=account part=domain file=phase-01-stdout`, `IDENTITY verdict=fail`, then `GATE ok`.

  `git status --ignored` showed no ignored published file. Committed as `c6ea5d6`.
- **Tracer gate (interactive mode, `end-of-phase`, automated verify):** after the commit, `--published --attempt 2` was re-run and printed `PUBLISHED verdict=pass`, so Task 2 went ahead.

### Task 2: `--live-run --attempt`, the record and the validation map

- **TDD:** wrote the two cases first. RED: `SELFTEST passed=73 failed=2`, both on usage. GREEN: `SELFTEST passed=75 failed=0`.
  - Real-data control: the pre-edit 50-LIVE-RUN.md under `--attempt 2` printed `LIVE_RUN missing=attempt-2-verdict`, `LIVE_RUN verdict=fail` (rc 1), while the plain `--live-run` passed.
  - Mutation control: dropping the `([^0-9]|$)` boundary makes `live run fails a record quoting only an earlier attempt verdict` fail (`attempt=20` then stands in for 2).
  - Committed as `2096b85`.
- **Exact-name re-runs** at `2096b85`, whose crate tree is unchanged since `bca7c35` (`git diff --stat bca7c35 HEAD -- crates Cargo.toml Cargo.lock rust-toolchain.toml` is empty, while against `1deaa4d` it lists 5 files):

  ```
  RESULT A rc=0          1 test run: 1 passed, 942 skipped
  RESULT B rc=0          1 test run: 1 passed, 942 skipped
  RESULT C rc=0          1 test run: 1 passed, 942 skipped
  RESULT F rc=0          1 test run: 1 passed, 942 skipped
  RESULT G rc=0          1 test run: 1 passed, 942 skipped
  RESULT doc_check rc=0  1 test run: 1 passed, 942 skipped
  RESULT D rc=0          1 test run: 1 passed, 942 skipped
  RESULT E1 rc=0         1 test run: 1 passed, 8 skipped
  RESULT E2 rc=0         1 test run: 1 passed, 8 skipped
  RESULT E3 rc=0         1 test run: 1 passed, 8 skipped
  RESULT CR01_stop_sp1 rc=0    1 test run: 1 passed, 469 skipped
  RESULT CR01_ship_sp1 rc=0    1 test run: 1 passed, 469 skipped
  RESULT CR01_stop_final rc=0  1 test run: 1 passed, 469 skipped
  RESULT CR01_ship_final rc=0  1 test run: 1 passed, 469 skipped
  RESULT CONTROL_misspelled rc=4  0 tests run: 0 passed, 943 skipped
  ```

- **50-LIVE-RUN.md** now has:
  - an update note in the header;
  - the verdict, quoted from attempt 2, with why attempt 2 exists, and attempt 1's verdict kept as history;
  - a new "Attempt 2's publication" subsection: the judging, curation, the refusal, the redaction, the identity result and the scans;
  - the attempts row, with the delegation of 2026-10-21 and the deletion left to the operator;
  - the `fault-7` quote and its per-case facts;
  - attempt 2's `stage_launched` table (each path 77 bytes raw, under `$HOME/.cache/devflow/sock`);
  - criterion row 4 naming the CR-01 tests and WR-03 tests A, C, D, F, E1, E3, controls B and E2, and the revert check. It names the clauses that hold (E3, D) and the two that do not (no group kill; the exit file keeps `0` while `stopped` says 143);
  - seven new limits, with the `stream_wait_bounded` count of 0.
- **50-VALIDATION.md:**
  - Rows V-58..V-68 have the commands the plan names.
  - V-28 and V-29 are re-pointed at attempt 2 and `fault-7`. Their commands were re-run read-only and exited 0, and no new raw file was written.
  - The Source line is extended.
  - `verification.append-audit` printed `"appended": true` and added `## Validation Audit 2026-10-21` (11/11/0). The audit notes follow it.
  - The frontmatter is still `validated` and `nyquist_compliant: true`.
- **Gates 1-3:** `GATE ok`, `GATE ok attempt=2`, `GATE ok`. Committed as `ae5c853`.

### Task 3: ledger, backlog, defects, macOS

- **`50-REVIEW-DISPOSITION.md`:** WR-03 is `disposition: fixed`, and its row lists `07e237b`, `493156b`, `5b1c417` and `60b1538`. `open: 0` and the other rows are unchanged.
- **ROADMAP 999.166:**
  - Item 3 is prefixed `**Fixed in Phase 50 by 50-26** (`07e237b`, `493156b`; …)`.
  - Item 6 adds the exit-file sentence, with `exit file keeps the code the agent exited with` on one line.
  - One sentence was added after the deferral paragraph.
  - No item and no heading were added.
- **50-LIVE-RUN.md "Defects":**
  - A WR-03 bullet was added.
  - The next line now reads "other two warnings and three infos", with item 6 widened.
  - The statement now reads `**No non-blocking defect found** by 50-18, 50-19 or 50-28 (live attempt 2), so no backlog entry was added.`
  - An attempt-2 paragraph says the planted credentials and the empty `surv-watch` tail are not defects. Its `analysis.txt` reads `interleaved_windows=0`, `overflow_windows=0`.
- **`deferred-items.md`:** the "Detection (50-29)" line was added and the entry stays `open`.
- **Gates, run before the commit:**
  - `--defects`: `DEFECTS added=0`, `DEFECTS verdict=pass`, and `roadmap.analyze` exit 0.
  - Attribution gate: `GATE ok`.
  - Ledger gate: rc 0. Item-6 gate: rc 0.
  - Negative controls on the HEAD versions: the attribution check rc 1, the item-6 check rc 1.
- **V-56:** `cargo check --target aarch64-apple-darwin` exited 0 for `-p devflow-core --all-targets` and `-p devflow --all-targets`, with no `warning` or `error` line (rustc 1.97.1). Recorded in 50-LIVE-RUN.md.
- Committed as `dd1c381`.

## Task Commits

1. **Task 1 step 1:** `c36f0d9` feat(50-29): judge identity fragments and one attempt's publication (before the checkpoint)
2. **Task 1 step 5:** `c6ea5d6` docs(50-29): publish live attempt 2 and its fault record
3. **Task 2 step 1:** `2096b85` feat(50-29): require one attempt's verdict in the live-run record
4. **Task 2 step 4:** `ae5c853` docs(50-29): record live attempt 2 in the live-run record and validation map
5. **Task 3:** `dd1c381` docs(50-29): record WR-03 fixed and attempt 2's defects

## Deviations from Plan

### Operator-decided (the checkpoint)

**1. Planted control credentials redacted in the published copy.** The sanitizer refused attempt 2 at Task 1 step 3. The plan's rule (a refusal is a blocker) stopped the run, and the operator chose "Redact in published copy" on 2026-10-21. The replacements are in the table above, recorded in 50-LIVE-RUN.md beside attempt 1's email note.

### Auto-handled

**2. [Rule 1 - Bug] The first version of my redaction regex for the AWS secret expected two backslashes before the quote. The file has one.** I caught this before any write. The script exits when a pattern matches 0 or 2+ distinct values, and the regex was fixed before it ran.

**3. [Rule 3] I removed the shellcheck SC2016 hits in the new selftest lines** by dropping backticks from the planted record lines. The case names and assertions are unchanged.

**4. Plan-fixed commit subject over 72 characters.** The subject of `ae5c853` is 76 characters, and the commit-msg hook printed an advisory warning. I kept the plan's exact text, as 50-26 did.

**5. Extra selftest cases beyond the plan's lists** (from step 1): `identity fails a token in a file name` and `published fails an attempt whose paired fault directory records another bin_sha256`. V-67 and V-68 name them.

No command-safety refusal occurred, and every plan gate ran verbatim.

## Issues Encountered

None blocking.

`fault-6`'s `stop-races-completion` record (raw, not published, not judged by this plan) reads `advance_ran: true` with `next_stage_launched: false`, `state_stopped: true` and `expected_met: true`. `fault-7`'s reads `advance_ran: false`. I recorded the difference and did not investigate it. It is outside this plan.

## Known Stubs

None.

## Cleanup

These are removed, each checked absent afterwards:
- the staging directory `target/p50-publish-2`;
- every scratch file this continuation created in the session scratchpad: `stage2.sh`, `redact_planted.py`, `rerun.sh`, `rerun-logs/`, `rerun-out.txt`, `red.txt`, `mac.log`, `st.txt`, `val-before.md`, `mut.sh` and `gl.json`.

The other files in that scratchpad (`pin.sh` and the earlier sessions' files) are not this plan's, and I left them.

No background process is left (the re-run task completed).

Raw tree: the only files written since the plan began are the four step-2 verdict files. `attempt-1` and `fault-1`..`fault-6` have 0 newer files.

## Self-Check: PASSED

- FOUND: `50-evidence/attempt-2/verdict-attempt-gate.txt` (holds `ATTEMPT_GATE current_source=yes`), `50-evidence/fault-7/verdict-fault.txt`
- FOUND commits: c36f0d9, c6ea5d6, 2096b85, ae5c853, dd1c381 (each an ancestor of HEAD)
- `git rev-list --count 56bef8d..HEAD` = 5 before this SUMMARY's commit
