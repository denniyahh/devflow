---
phase: 49-live-unattended-run-the-milestone-s-instrument
plan: 05
subsystem: testing
tags: [python, redaction, secrets, evidence, stream-json, tdd]

requires:
  - phase: 30-keep-the-session-alive-past-turn-end
    provides: "30c-monitor-env-harness.py redaction analog (token shapes, home variants, names-only scan, atomic publish)"
provides:
  - "tools/sanitize-evidence.py: sanitize <src> <dest>, scan <path>..., --selftest [--real-captures DIR]"
  - "Pattern names: openai_key_prefix, github_token_prefix, bearer_token, credential_named_assignment, credential_named_field (credentials, refuse); home_path, home_path_escaped, email_address (leftover redaction targets, scan only)"
  - "Redaction tokens: ~, -HOME-, <email>, uuid-<n>"
affects: [49-12]

actuals:
  tokens: 7859
  tasks: 3
  commits: 6
plan_head_before: e47379087908bdcefc7a254f15b03dafcb8fa53e

tech-stack:
  added: []
  patterns:
    - "Credential patterns run on JSON-decoded strings (whole file, else per line, else plain text), never on raw JSON"
    - "Publish = temp sibling of dest, post-redaction scan of every output, then rename; any refusal publishes nothing"
    - "Regexes over long runs are anchored with lookbehinds at the start of the run so they stay linear"

key-files:
  created:
    - .planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/sanitize-evidence.py
  modified: []

key-decisions:
  - "scan also reports leftover email addresses (email_address) and escaped home forms (home_path_escaped), not only the path spelling: the output scan then confirms every redaction class, not just one"
  - "A refusal must be a deliberate `refused: <file>: <pattern>` line, never a traceback; the selftest controls check for that, not only for a non-zero exit"
  - "Collisions between redacted names (two sources mapping to one output path) and non-regular files are refused too"

patterns-established:
  - "Selftest real-data cases sit behind an opt-in flag, print only counts, and fail as ABSENT on an empty sample so they cannot pass vacuously"

requirements-completed: [VERIFY-01]

coverage:
  - id: D1
    description: "sanitize redacts both home spellings, Claude escaped project paths (in contents and path names) and emails, and replaces UUIDs with uuid-<n> from one map shared across files; the published copy scans clean"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "tools/sanitize-evidence.py --selftest (Task 1 gate: 4 cases ok, gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Fails closed: symlink, non-UTF-8 file, existing destination and any JSON-decoded credential (named assignment inside an escaped string, named field) are refused naming file and pattern only, publishing nothing; usage counters and key names still publish"
    requirement: VERIFY-01
    verification:
      - kind: unit
        ref: "tools/sanitize-evidence.py --selftest (Task 2 gate: 12 cases ok, real_home_sanitize_exit=0 home_left=0, gate_rc=0)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The 12 real phase-43 Code-stage captures (7 of which 30c's raw pattern refuses) publish and scan clean with the host's real home and user; a credential appended to the largest one is still refused"
    requirement: VERIFY-01
    verification:
      - kind: integration
        ref: "tools/sanitize-evidence.py --selftest --real-captures <main checkout>/.devflow/history/phase-43 (Task 3 gate: 15 ok, empty-dir run 3 ABSENT fails, gate_rc=0)"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-09-29
status: complete
---

# Phase 49 Plan 05: Evidence Sanitizer Summary

**`sanitize-evidence.py` publishes a copy of an evidence tree with home spellings, Claude escaped paths, emails and UUIDs redacted, and refuses the whole publish on anything it cannot vouch for. Its credential scan runs on JSON-decoded strings. That is what lets the real phase-43 Code captures publish, which 30c's raw pattern would have refused, while a credential hidden in a JSON-escaped string is still caught.**

## Performance

- **Duration:** 12 min
- **Started:** 2026-09-29T20:14:50Z
- **Completed:** 2026-09-29T20:26:30Z
- **Tasks:** 3
- **Files modified:** 1 (created, 845 lines)

## Accomplishments

- `sanitize <src> <dest>`: redacts the home path, its realpath and its `/home` ↔ `/var/home` twin to `~`, and the escaped forms (`-var-home-<user>`, `-home-<user>`, plus the escaped form of every home spelling) to `-HOME`. The result is `-HOME-…` wherever the form continues. Emails become `<email>` and UUIDs become `uuid-<n>` (case-insensitive, one map across contents and relative path names). The copy is written to a temp sibling and renamed into place only after every output file scans clean.
- Fails closed before writing anything if the destination exists or the source holds a symlink, a non-regular file, a non-UTF-8 file, a redacted-name collision or any credential shape. Refusal lines name the relative file and the pattern name, never the value.
- Credential patterns (D-26): 30c's three prefix shapes unchanged, `credential_named_assignment` and `credential_named_field` (value ≥20 chars of `[A-Za-z0-9_+/=.-]` holding a letter and a digit). All five run on JSON-decoded strings. `LEGACY_30C_NAMED_ASSIGNMENT` is kept as a selftest reference only.
- `--selftest` has 12 host-independent cases, planted with `/var/home/tester` and `tester`. `--real-captures DIR` adds 3 cases that run on real DevFlow captures in a temp copy and print only counts.

## Task Commits

1. **Task 1: Tracer, planted tree sanitized end to end.** RED `6006a6b` (test), GREEN `0b75d85` (feat)
2. **Task 2: Fail-closed controls and JSON-decoded credential scan.** RED `ca3c86d` (test), GREEN `df741c2` (feat)
3. **Task 3: Publish proven on real DevFlow captures.** RED `dd2336c` (test), GREEN `8d6cd77` (feat)

No REFACTOR commits.

## Gate results (all run on the final code)

| Gate | Result |
|------|--------|
| Task 1 | `selftest_exit=0 passed=12 failed=0`, all 4 `case[...]=1`, ruff clean, `gate_rc=0` |
| Task 2 | all 8 `case[...]=1`, `real_home_sanitize_exit=0 home_left=0`, ruff clean, `gate_rc=0` |
| Task 3 | `real_capture_files=12`, `selftest_exit=0 passed=15 failed=0`, all 3 real `case[...]=1`, `home_in_selftest_output=0`, `empty_dir_selftest_exit=1 absent_fails=3`, ruff clean, `gate_rc=0` (wall time about 11 s against the 120 s bound) |

Stdlib only: an AST import check found no non-stdlib module. `git status --porcelain` after Task 3 showed only `sanitize-evidence.py` besides the two orchestrator-owned files. No capture, raw or sanitized, was added.

## TDD Gate Compliance

For each task, the RED selftest output was transcoded line by line into TAP and checked with `gsd-tools check tdd-red-evidence`. As a control on the checker, a target name that does not exist returned `INVALID_RED / no_target_test_failure`.

| Task | RED commit | Failing targets (`RED_EVIDENCE_OK`) | Passed already in RED |
|------|-----------|--------------------------------------|------------------------|
| 1 | `6006a6b` | all 4 (`SELFTEST passed=0 failed=4`) | none |
| 2 | `ca3c86d` | symlink, existing destination, non-UTF-8, escaped credential | surviving secret, bare username, email, usage counters |
| 3 | `dd2336c` | all 3 (stubbed `ABSENT`) | none |

The four Task 2 cases that passed in RED are the ones the plan anticipated ("where it does not yet refuse"). Task 1 already redacted emails and left bare usernames alone. Its post-redaction scan already refused a GitHub-token shape, and it had no named-credential pattern to over-refuse the usage counters. A scratch-copy mutation sweep shows each of those four catches the defect it guards against:

| Mutation | Case that fails |
|----------|-----------------|
| Username redacted | `bare username kept` |
| Email redaction removed | `email address redacted` |
| 30c raw pattern substituted | `usage counters and key names publish`, `real: every capture…` |
| Credential scan on neither source nor output | `surviving secret`, `escaped credential` |
| Named pattern run on JSON-encoded text | `escaped credential` |
| Named-assignment pattern removed | `escaped credential`, `credential planted in a real capture` |
| Home redaction removed | `home paths…`, `scan: sanitized output…`, `real: every capture…` |
| Symlink check removed | `symlink in source refused` |
| UUID pseudonym removed | `session ids pseudonymized consistently` |
| Twin spelling removed | `home paths in both spellings redacted` |

## What these results do not establish

- **Refusing before any write is not independently tested.** With the source pre-check removed, all 15 cases still pass, because the post-redaction scan catches the same credentials in the temp copy and deletes it. Either way nothing is published, but no test can observe whether the temp copy was ever written.
- **Leftover UUIDs are not scanned for.** Neither `scan` nor the real-capture case looks for them (the spec lists secrets and home spellings only). A regression in UUID pseudonymization is caught only by the planted `session ids` case.
- **The real-data proof covers 17 captures**: 12 from phase 43 plus a supplementary run on the 5 from phase 900 (15 ok, 3 of 5 trip 30c's raw pattern). This shows the pattern set does not over-refuse captures of this kind. It does not guarantee that 49-12's own captures publish. If one is refused, 49-12 must stop and report, as its plan already says.
- **Accepted risk T-49-05-05:** a named value under 20 characters, or one without both a letter and a digit, is not matched.
- **Email redaction may over-redact** strings shaped like `x@host.tld`. That is safe, but it can alter quoted text.

## Files Created/Modified

- `.planning/phases/49-live-unattended-run-the-milestone-s-instrument/tools/sanitize-evidence.py`: the redactor (`sanitize`), the committed-evidence scanner (`scan`) and `--selftest [--real-captures DIR]`

## Decisions Made

See `key-decisions` in the frontmatter. None of them changes the interface 49-12 consumes.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] `scan` also checks for leftover emails and escaped home forms**
- **Found during:** Task 1
- **Issue:** The spec names only secret shapes and home spellings for `scan`. A leftover email or escaped `-var-home-<user>` would have passed the post-redaction check.
- **Fix:** Added `email_address` and `home_path_escaped` findings, checked in the raw text and in every JSON-decoded string.
- **Verification:** `scan: sanitized output passes the secret scan`. The real case's independent substring count found 0 leftovers across 12 captures (8 of which contain an email and all 12 an escaped home before sanitizing).
- **Committed in:** `0b75d85`, `df741c2`

**2. [Rule 2 - Missing critical] Refuse name collisions and non-regular files**
- **Found during:** Task 2
- **Issue:** Two source names that redact to the same output path could merge silently. A FIFO would block the read.
- **Fix:** Added `name_collision` and `not_regular_file` refusals.
- **Committed in:** `df741c2`

**3. [Rule 1 - Test strength] Controls require a deliberate refusal line**
- **Found during:** Tasks 2 and 3
- **Issue:** Task 1's code already exited non-zero on an existing destination (an `OSError` traceback on rename) and on non-UTF-8 input (a `UnicodeDecodeError` traceback). A bare "exits non-zero" check would have counted a crash as a refusal.
- **Fix:** The control cases require a `refused: ` line with no traceback. Since Task 3, one refusal line must name both the file and the pattern.
- **Committed in:** `ca3c86d`, `8d6cd77`

**4. [Format] Selftest prints `  info …` count lines for the real cases**
- The plan asks the real case to print file and byte counts. They appear on separate `  info` lines, so the `  ok   <name>` lines stay exact for the gates.

---

**Total deviations:** 4 (2 missing-critical, 1 test-strength, 1 format)
**Impact on plan:** Each change tightens fail-closed behaviour or test strength. The interface and pattern names are as the plan specifies.

## Issues Encountered

- The first `ruff check` flagged a blind `except Exception` in the selftest runner. The runner now catches `OSError`, `ValueError` and `SubprocessError`, and case names come from the case table, so a crashing case still prints its exact name.

## User Setup Required

None. No external service configuration is required.

## Next Phase Readiness

49-12 can call `sanitize <staging>/src <staging>/out` and `scan`. The refusal contract is `refused: <relative file>: <pattern>` on stderr with exit 1. The sanitizer refuses an existing destination by design, which matches 49-12's re-execution sequence.

---
*Phase: 49-live-unattended-run-the-milestone-s-instrument*
*Completed: 2026-09-29*

## Self-Check: PASSED

- FOUND: `tools/sanitize-evidence.py`
- FOUND commits: `6006a6b`, `0b75d85`, `ca3c86d`, `df741c2`, `dd2336c`, `8d6cd77`
