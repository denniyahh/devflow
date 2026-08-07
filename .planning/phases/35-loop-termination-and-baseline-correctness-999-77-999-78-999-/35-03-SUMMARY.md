---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
plan: 03
subsystem: infra
tags: [ssh, ssh-keygen, sshsig, signing, release-preflight, rust, process-timeout]

# Dependency graph
requires:
  - phase: 20d
    provides: "SigningViability, public_key_fingerprint, inline_signing_key_blob and git's user.signingKey prefix precedence"
  - phase: 24-01
    provides: "inline key:: / raw ssh- classification at the CLI boundary"
provides:
  - "check_ssh_signing_viability establishes tag-signing viability by performing the operation — a bounded, non-interactive `ssh-keygen -Y sign` over throwaway bytes — instead of predicting it from `ssh-add -l`"
  - "SSH_SIGN_NAMESPACE, decoded byte-for-byte from this repository's own v2.4.0 SSHSIG blob"
  - "a per-call probe workspace name (pid + atomic counter + sub-millisecond time), std-only"
  - "NC-9 and a calibrated NC-10 as committed tests, plus a leak-assertion helper over the rendered verdict"
  - "removal of pub SigningStatus and pub classify_ssh_add_status from devflow_core::git — a breaking change to a published crate (D-04/D-08)"
affects: [35-06, release, ship, DEN-50]

actuals:
  tokens: 32592
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "probe-not-predictor: establish a capability by performing it on disposable input rather than inferring it from a proxy signal"
    - "calibrated negative control: measure the positive arm first, derive the control's observation window from that measurement, and fail 'control uncalibrated' when the relation breaks"

key-files:
  created: []
  modified:
    - crates/devflow-core/src/git.rs
    - crates/devflow-cli/tests/release_check.rs

key-decisions:
  - "The probe's exit code is the sole verdict; ssh-keygen's stderr is read by nobody and reproduced nowhere, because it embeds the configured key path verbatim (D-02, D-08)"
  - "Probe helpers were extracted into run_ssh_sign_probe / sign_probe_within rather than inlined, which moves the create_dir call outside the region one acceptance criterion scoped its grep to — recorded as a deviation, re-verified over the wider region with the control intact"
  - "Both NC-10 arms drop the controlling terminal via setsid; without it ssh-keygen prompts on /dev/tty regardless of SSH_ASKPASS_REQUIRE and both arms block for a reason unrelated to the variable"
  - "The NC-10 arms deliberately do NOT clear SSH_AUTH_SOCK — the fixture key is generated microseconds earlier into a fresh tempdir, so reaching for agent state near a signing assertion would reproduce the premise that caused 999.86"

patterns-established:
  - "Probe-not-predictor: a viability check with no independent behaviour cannot drift out of sync with the thing it reports on"
  - "Calibrated control: an observation window derived from a measured baseline, with both numbers recorded and an explicit 'control uncalibrated' failure mode"

requirements-completed: [HARDEN-05]

coverage:
  - id: D1
    description: "release --check reports Viable/NotViable from a real ssh-keygen -Y sign probe over a throwaway payload, never from an ssh-add -l fingerprint comparison"
    requirement: HARDEN-05
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#ssh_signing_probe_reports_viable_with_on_disk_private_key"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#ssh_signing_probe_reports_not_viable_without_a_private_key"
        status: pass
    human_judgment: false
  - id: D2
    description: "A configured key whose private sibling is on disk and which no agent holds reports Viable — the live false negative 999.86 was filed for twice"
    requirement: HARDEN-05
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#ssh_signing_probe_reports_viable_with_on_disk_private_key"
        status: pass
    human_judgment: false
  - id: D3
    description: "The probe cannot hang an unattended preflight: SSH_ASKPASS_REQUIRE=never plus a wall-clock ceiling that kills and reaps"
    requirement: HARDEN-05
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#ssh_signing_probe_does_not_block_on_an_encrypted_key"
        status: pass
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#encrypted_key_blocks_without_the_askpass_require_env_var"
        status: pass
    human_judgment: false
  - id: D4
    description: "Inline key:: / raw ssh- values return Unknown with a fixed reason and are never probed"
    requirement: HARDEN-05
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#inline_signing_key_returns_unknown_without_probing"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_inline_signingkey_warns_without_probing"
        status: pass
    human_judgment: false
  - id: D5
    description: "No reason string carries the configured signingkey, a filesystem path, or any fragment of ssh-keygen's stderr"
    requirement: HARDEN-05
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#assert_no_leak (asserted in all four probe fixtures)"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/release_check.rs#release_check_signing_output_leaks_no_key_material_or_path"
        status: pass
    human_judgment: false
  - id: D6
    description: "classify_ssh_add_status, SigningStatus and inline_key_fingerprint removed with their tests; workspace builds clean"
    requirement: HARDEN-05
    verification:
      - kind: other
        ref: "cargo build --workspace --all-targets && rg -c 'classify_ssh_add_status|SigningStatus|inline_key_fingerprint' crates/ (exit 1)"
        status: pass
    human_judgment: false
  - id: D7
    description: "The per-call probe workspace name is unique across concurrent threads (F-8)"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/git.rs#probe_workspace_name_is_unique_per_call"
        status: pass
    human_judgment: false

duration: 25min
completed: 2026-08-07
status: complete
---

# Phase 35 Plan 03: Signing-Viability Probe Summary

**`release --check`'s tag-signing preflight now signs throwaway bytes with `ssh-keygen -Y sign` and reports the exit code, replacing the `ssh-add -l` predictor that false-negatived live on two release cuts with the correct key present.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-08-07T11:03:00Z
- **Completed:** 2026-08-07T11:26:00Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments

- The SSH branch establishes viability by **doing the operation** rather than inferring it. A probe has no independent behaviour, so it cannot drift out of sync with what `git tag -s` actually does — which is the structural defect the predictor had, not a circumstantial one.
- The predictor's public surface is gone: `SigningStatus` and `classify_ssh_add_status` removed from `devflow_core::git` (breaking, D-04/D-08), along with the private `inline_key_fingerprint` orphaned by D-03, and all three of their tests.
- Both mandatory negative controls are committed tests that were **observed failing when sabotaged**, not merely observed passing.

## Task Commits

1. **Task 1 (RED): failing per-call probe workspace name test (F-8)** — `ae1dee8` (test)
2. **Task 1 (GREEN): probe tag-signing viability instead of predicting it** — `6fe8862` (feat)
3. **Task 2: probe fixtures with their two mandatory negative controls** — `d7527b8` (test)
4. **Task 3: retire every test and comment the deletion falsified** — `a5ed1e9` (test)

## Files Created/Modified

- `crates/devflow-core/src/git.rs` — `check_ssh_signing_viability` rewritten as a bounded probe; `SSH_SIGN_NAMESPACE` / `SSH_SIGN_PROBE_TIMEOUT` / `SSH_SIGN_PROBE_POLL`; `probe_workspace_name`, `run_ssh_sign_probe`, `sign_probe_within`, `SignProbeOutcome`; predictor and orphan deleted; five new tests plus three restated ones.
- `crates/devflow-cli/tests/release_check.rs` — absent-tooling and inline-key surface tests rewritten; two further tests corrected (see Deviations).

## Verified Measurements

### The namespace was extracted, not assumed

`SSH_SIGN_NAMESPACE = "git"` was decoded from this repository's own `v2.4.0` signed tag, not from documentation:

```
S S H S I G \0\0\0 001 \0\0\0 3 \0\0\0 \v s s h - e d 2 5 5 1 9 ... \0\0\0 003 g i t \0\0\0\0 \0\0\0 006 s h a 5 1 2
```

**Its own control:** the reading is by offsets, so a misread would desynchronise everything after it. The namespace's length prefix (3) is followed by exactly `git`, and the next length prefix (6) is followed by exactly `sha512`. A wrong offset could not produce two consecutive well-formed fields.

### NC-9 — the negative control that stops the positive case being vacuous

| arm | fixture | result |
|---|---|---|
| positive | fresh ed25519 keypair, unencrypted private half on disk, held by no agent | `Viable { fingerprint: Some("SHA256:…") }` |
| NC-9 | same fixture, private half deleted, only `.pub` remains | `NotViable` |

The verdict flips. Both arms are committed tests. The positive arm reads, sets and depends on no agent state at all — that is the point: agent membership is not a necessary condition for signing, and assuming it was is what produced the original defect.

### NC-10 — calibrated, with both numbers recorded (F-9)

Both arms drive the **raw** `ssh-keygen` command, not the probe, so the interval measured is the environment variable's effect rather than `SSH_SIGN_PROBE_TIMEOUT`.

| quantity | value |
|---|---|
| measured non-blocking exit (`SSH_ASKPASS_REQUIRE=never`) | **10.63 ms** (standalone run of the same arm: 5.08 ms) |
| calibration multiple | 8x, floored at 1000 ms |
| **observation window used for the blocking arm** | **1000 ms** |
| ratio window : measured baseline | **≈94x** (assertion requires ≥4x) |
| blocking arm (variable omitted, same fixture) | **still alive when the window closed** (`None`) |
| window vs. probe ceiling | 1000 ms against a 10 s ceiling — 10% |

Both arms were observed in both directions. The test fails with an explicit `control uncalibrated` message if the window is not ≥4x the measured baseline, or is not comfortably under the probe's own ceiling.

**The control was verified capable of failing.** Sabotaging arm 2 to also set `SSH_ASKPASS_REQUIRE=never` produced:

```
NC-10 control FAILED: with SSH_ASKPASS_REQUIRE omitted the child still exited in
Some(5.160077ms), inside the 1s window. A control that agrees with its positive case
is a broken measurement, not evidence
```

The sabotage was reverted before commit.

### Leak assertions

`assert_no_leak` renders the returned `SigningViability` and asserts it contains neither the fixture key directory, nor `PRIVATE KEY`, nor any of four `ssh-keygen` stderr fragments (`No private key found`, `Couldn't load public key`, `Enter passphrase`, `incorrect passphrase`). All four probe fixtures pass it.

**It is not vacuous.** Feeding it a known-positive — a `NotViable` whose reason was ssh-keygen's real stderr — tripped it:

```
signing viability leaked a filesystem path:
NotViable { reason: "No private key found for public key \"/tmp/.tmpaOqFfE\"" }
```

The scratch probe was removed before commit.

### F-8 uniqueness

`probe_workspace_name_is_unique_per_call` passes: `test result: ok. 1 passed; 0 failed; 552 filtered out`. The non-zero `filtered out` count confirms the filter matched a real test (this repo has already paid for `cargo test --exact` exiting 0 on a name matching nothing). The two-thread half generates 128 names across two concurrently spawned threads and asserts zero duplicates.

### No residue

Zero `/tmp/devflow-sign-probe-*` directories remain after roughly ten probe invocations across the test runs. **Control:** creating one such directory made the same check report 1, so the glob genuinely matches — an empty glob and a broken glob are otherwise indistinguishable. The probe touches no repository refs and signs nothing but bytes it generated inside its own private directory.

## What these fixtures do NOT establish

Carried forward from `35-VALIDATION.md` and `35-CONTEXT.md`, and stated here rather than discovered at verification time:

- **n=1 on every axis.** One host, one OpenSSH build (`OpenSSH_10.4p1, OpenSSL 3.6.3`), one key type (ed25519). Nothing here establishes behaviour for RSA, ECDSA or PKCS11-backed keys, or for other OpenSSH builds. The operator's measured 8-row table from the discussion is explicitly **not** coverage and is not cited as such.
- **NC-10 is a single observation of each arm**, even calibrated. That is a weak bound on the environment variable's effect, not a reliability claim. It supports "the variable changed the outcome in this run"; it does not support "the variable prevents hangs reliably".
- **HARDEN-05 is met for the path form only.** Under D-03 inline `key::` and raw `ssh-` values return `Unknown` without being probed, so those operators get *no* viability verdict at all, where the predictor gave them a fingerprint comparison. Deliberate and recorded (A-10), not an oversight.
- **The timeout, not the environment variable, is what covers a host with a controlling terminal.** Measured this session: with a controlling terminal available, `ssh-keygen` prompts on `/dev/tty` regardless of `SSH_ASKPASS_REQUIRE`, so an interactive `release --check` against an encrypted key would block until the 10 s ceiling. Unattended runs have no controlling terminal, which is the case D-01 targets — but the env var's protection is narrower than its name suggests. See Deferred Items.
- **Two claimed timeout justifications remain unmeasured** (A-16): a wedged `ssh-agent` and a stalled PKCS11 provider are reasoned defence-in-depth, not tested scenarios.

## Decisions Made

- **Probe helpers extracted rather than inlined.** `run_ssh_sign_probe` / `sign_probe_within` keep `check_ssh_signing_viability` readable as a five-arm classification. Consequence recorded under Deviations.
- **`SignProbeOutcome` has five variants, not four.** The plan named four failure classes; an unpollable child is a sixth possibility that establishes nothing about the key, so it maps to the same fail-soft `NotRun` class as a workspace-creation failure rather than being asserted as a `NotViable` verdict. Two `Unknown` classes total, as the plan specified.
- **`setsid` on both NC-10 arms** (via `pre_exec`, using the already-present `libc` dependency — no dependency added). Without it the control measures the tty-prompt route rather than the askpass route, and a `SIGKILL`ed child can leave an operator's terminal with echo disabled.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Two further `release_check.rs` tests broke behaviourally, beyond the two the plan enumerated**

- **Found during:** Task 3
- **Issue:** The plan's `<planner_note>` correctly warned that the symbol search missed tests referencing the predictor through its *reason strings*, and named three in `git.rs`. The same class exists in `release_check.rs` and was not enumerated: `release_check_inline_signingkey_is_not_reported_missing_and_leaks_no_key_material` asserted `stdout.contains("no ssh-agent reachable")` **twice**, a string the code can no longer emit. It would have failed at run time, not compile time — the exact failure mode this phase exists to eliminate. Separately, `release_check_signing_output_leaks_no_key_material_or_path`'s doc comment claimed the fixture "resolves deterministically to the no ssh-agent reachable branch"; that fixture now reaches `Viable`, so the comment was falsified even though its assertions still hold.
- **Fix:** Both assertions re-derived from D-03's contract (the unprobed-inline reason), and both doc comments restated in terms of the mechanism that now runs. No test was deleted to make the suite green.
- **Files modified:** `crates/devflow-cli/tests/release_check.rs`
- **Verification:** `cargo test -p devflow --test release_check` → `10 passed; 0 failed`, matching the pre-change baseline of 10 exactly (two renamed, none added or dropped).
- **Committed in:** `a5ed1e9`

**2. [Rule 3 - Blocking] One acceptance criterion's region-scoped grep no longer contains the call it greps for**

- **Found during:** Task 1
- **Issue:** The criterion runs `awk '/^fn check_ssh_signing_viability/,/^fn check_gpg_signing_viability/'` and requires `create_dir_all` to count 0 **and** `create_dir` to count ≥1 — the second half being the control against a region that lost the creation call entirely. Extracting the workspace setup into `run_ssh_sign_probe` (which the plan's own action text asked for: "composed by one small private helper") moved that call above the region's start line. The literal criterion therefore reports **0 and 0**, and its control correctly flags that.
- **Fix:** Not papered over. Re-run over a wider region anchored at `run_ssh_sign_probe`, which is a strict superset spanning all probe code including `check_ssh_signing_viability`: `create_dir_all` → **0**, `create_dir` → **1**. The control is intact — the two numbers still disagree, which is what the criterion exists to establish.
- **Files modified:** none (verification-only)
- **Verification:** both greps run and both numbers recorded above.
- **Committed in:** n/a

**3. [Rule 2 - Correctness] The NC-10 fixture initially cleared `SSH_AUTH_SOCK` / `SSH_AGENT_PID`**

- **Found during:** Task 2
- **Issue:** Copied reflexively from the existing surface-test idiom. It violated the plan's explicit criterion (`! rg -q 'SSH_AUTH_SOCK' crates/devflow-core/src/git.rs`) and, more importantly, its intent: a test that reaches for agent state near a signing assertion has reproduced the premise that caused 999.86. It was also redundant — the fixture key is generated microseconds earlier into a fresh tempdir, so no agent can hold it.
- **Fix:** Both `env_remove` calls dropped, replaced with a comment stating why agent state is deliberately neither read nor cleared.
- **Files modified:** `crates/devflow-core/src/git.rs`
- **Verification:** `rg -n 'SSH_AUTH_SOCK' crates/devflow-core/src/git.rs` exits 1.
- **Committed in:** `d7527b8`

**4. [Rule 3 - Blocking] `use std::io::Write` orphaned by the deletion**

- **Found during:** Task 1
- **Issue:** `inline_key_fingerprint` was the import's only consumer; leaving it would fail `clippy -D warnings`.
- **Fix:** Import removed; `std::time::{Duration, Instant}` added for the ceiling.
- **Files modified:** `crates/devflow-core/src/git.rs`
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **Committed in:** `6fe8862`

---

**Total deviations:** 4 auto-fixed (2 x Rule 3, 1 x Rule 1, 1 x Rule 2)
**Impact on plan:** No scope creep. Deviation 1 is the plan's own `<planner_note>` class extended to the second file, which Task 3's mandate ("in both files") already covered. Deviation 2 is a verification-method adjustment with the control preserved, recorded rather than silently substituted. The GPG branch and both pre-probe early returns are untouched, as required.

## Issues Encountered

- **`ssh-keygen -Y sign` blocks on a controlling terminal regardless of `SSH_ASKPASS_REQUIRE`.** Discovered while designing NC-10; resolved for the tests with `setsid`. Not resolved in production code — out of scope, and the timeout already bounds it. Logged to Deferred Items.
- Nothing else. No fix-attempt limits were approached; no task needed more than one corrective pass.

## Deferred Items

- **The production probe does not detach from a controlling terminal.** On an interactive host with an encrypted key, `release --check` would prompt on `/dev/tty` and then eat the full 10 s ceiling, returning `NotViable`. D-01 scopes the mechanism to the env var plus the timeout, and "do not widen the scope" is explicit in the plan, so this was **not** implemented. If interactive `release --check` latency ever bites, a `pre_exec(setsid)` on the probe is the recorded way to close it — the same one-line change the NC-10 fixtures already use.

## Threat Flags

None. No new network endpoint, auth path or schema change. The three mitigations this plan owned are implemented as specified: T-35-11 (fixed reasons, stderr discarded), T-35-12 (non-recursive `create_dir` under a per-call name), T-35-13 (signs only self-generated bytes in its own directory, removed on every exit path including timeout), T-35-14 (env var plus kill-and-reap ceiling), T-35-15 (predictor replaced by the operation).

## Known Stubs

None.

## User Setup Required

None.

## Next Phase Readiness

- The public-API removals are complete and `cargo build --workspace --all-targets` is clean, so **35-06**'s enumeration of every changed/removed `pub` item can record `devflow_core::git::classify_ssh_add_status` and `devflow_core::git::SigningStatus` as removed.
- `scripts/check.sh all` is green end to end: fmt clean, clippy clean under `-D warnings`, and `0 failed` across all binaries (558 in `devflow-core`, 279 + integration suites in `devflow-cli`).
- **DEN-50 is unaffected and must stay that way** — `devflow release`'s real signing executor must still run the real signed `git tag`. This probe is a preflight and must never be substituted for it.

---
*Phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-*
*Completed: 2026-08-07*
