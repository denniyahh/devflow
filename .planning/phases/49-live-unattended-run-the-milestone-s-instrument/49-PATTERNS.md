# Phase 49: Live Unattended Run — The Milestone's Instrument - Pattern Map

**Mapped:** 2026-09-25
**Worktree:** `.worktrees/phase-49` on `feature/phase-49` at `071e2eb`
**Files analyzed:** 13 new or modified files and artifact groups
**Analogs found:** 11 / 13. The launch wrapper and the inotify watcher core have no code analog; see "No Analog Found".

Every analog path below was checked with `git ls-files -- <path>`, and each one printed one line.
Negative control: the new `tools/snapshot.sh` path printed 0 lines. Line numbers were read at `071e2eb`.

**Read this first: three analog lines the planner must NOT copy.** `scripts/unattended-drill.sh` is
the best structural analog, but three of its choices contradict RESEARCH:

| Drill line | What it does | Why Phase 49 must not copy it |
|---|---|---|
| `:538` | `exec setsid "$DEVFLOW_BIN" start …` | RESEARCH Pattern 2 and Anti-Patterns say to use `systemd-run --user -p RemainAfterExit=yes -p KillMode=process` |
| `:714` | `DEVFLOW_BIN="$REPO_ROOT/target/debug/devflow"` | Pattern 1 requires an immutable pinned copy at `~/.cache/devflow-phase49/bin/devflow-<sha12>` |
| `:738` | `export DEVFLOW_GATE_TIMEOUT_SECS` | Pattern 3: every `DEVFLOW_*` variable leaks into the agent and its test children, so the launch env stays minimal |

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `.planning/phases/49-…/tools/snapshot.sh` (new) | utility | file-I/O (read-only copy) | `scripts/unattended-drill.sh` evidence helpers `:451-529` | role-match |
| `.planning/phases/49-…/tools/gate-watch.sh` (new) | utility (watcher) | event-driven (poll) | `scripts/unattended-drill.sh` `poll_arm` `:564-585`; self-test from `scripts/test-phase-worktree-guard.sh` | role-match |
| `.planning/phases/49-…/tools/surv-watch.py` (new) | utility (watcher) | event-driven (inotify stream) | `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30c-monitor-env-harness.py` (structure only) | partial |
| `.planning/phases/49-…/tools/negctl.sh` (new) | test driver | request-response (launch → refusal → assert) | `scripts/scratch-dogfood-repo.sh` + `scripts/unattended-drill.sh` | exact |
| `.planning/phases/49-…/tools/undo-ship-hooks.sh` (new) | utility (guarded git mutation) | batch | `scripts/cut-release.sh` `require_*` guards `:50-58`; test harness `scripts/test-phase-worktree-guard.sh` | role-match |
| `.planning/phases/49-…/tools/check-evidence.sh` (new) | test (assertion) | transform | `scripts/unattended-drill.sh` `FAILURES` block `:784-800`, `:944-957`; `scripts/test-phase-worktree-guard.sh` `check()` `:53-61` | exact |
| `.planning/phases/49.1-<slug>/49.1-01-PLAN.md` (new, pre-authored) | config (plan) | — | `44-04-PLAN.md` checkpoint task `:168-260`; `35.1-01-PLAN.md` frontmatter `:1-14`; `scripts/scratch-dogfood-repo.sh` plan body `:133-185` | exact |
| `.planning/ROADMAP.md`: Phase 49.1 heading, table rows, criteria 1–2 amendment, 999.55 PROMOTED | config (docs) | — | `.planning/milestones/v2.5.0-ROADMAP.md:107-` (35.1 block); ROADMAP `:790-792` (promoted marker) | exact |
| `.planning/REQUIREMENTS.md`: VERIFY-01 amendment, new 49.1 ID, traceability, coverage | config (docs) | — | REQUIREMENTS `:96-103` (TEST-01 shape), `:185-209` | exact |
| `crates/devflow-cli/tests/phase7_cli.rs` (the 49.1 workload; **analog only**, 49.1's Code agent edits it) | test | polling | `crates/devflow-cli/src/config_parse.rs:16-44`, `:185-208` | role-match |
| Phase 49 `PLAN.md` files (operator-run live tasks) | config (plan) | — | `44-04-PLAN.md` (`autonomous: false`, operator-run live dogfood) | exact |
| `49-evidence/README.md` plus the run write-up (name is the planner's call) | docs (evidence) | — | `35.3-evidence/README.md`; `44-CODEX-E2E.md` headings; the standard is `47-PHASE49-OBSERVATION.md` | exact |
| systemd launch/resume wrapper (optional tool, or inline plan commands) | utility | process supervision | none | no analog |

---

## Pattern Assignments

### `tools/negctl.sh` (test driver, request-response)

**Analogs:** `scripts/scratch-dogfood-repo.sh` (fixture) and `scripts/unattended-drill.sh` (the arm driver).

**Invoke the fixture as-is. Do not fork it.** It writes the file the negative arm needs to be absent,
at `scripts/scratch-dogfood-repo.sh:187-191`:
```bash
cat > "$DEST_ABS/.planning/config.json" <<'EOF'
{
  "workflow": {}
}
EOF
```
It ends on `develop` (`:193-196`). negctl.sh then adds the D-14 removal commit on `develop`, following
RESEARCH Pattern 6 step 2.

**Destination guard: copy the widened drill version.** The fixture's own guard (`:44-58`) checks only
`--show-toplevel`, which inside a worktree is the worktree and not the main checkout. Use
`scripts/unattended-drill.sh:65-70` together with `:122-141`:
```bash
GIT_COMMON_DIR="$(git -C "$SCRIPT_DIR" rev-parse --path-format=absolute --git-common-dir)"
MAIN_CHECKOUT="$(dirname "$GIT_COMMON_DIR")"
...
guard_destination() {
    local dest="$1"
    local dest_abs
    dest_abs="$(realpath -m -- "$dest")"
    local forbidden
    for forbidden in "$REPO_ROOT" "$MAIN_CHECKOUT"; do
        if [ "$dest_abs" = "$forbidden" ] || case "$dest_abs" in "$forbidden"/*) true ;; *) false ;; esac; then
            echo "ERROR: refusing to scaffold inside this checkout ($forbidden)." >&2
            ...
            exit 1
        fi
    done
    if [ -e "$dest_abs" ]; then ... exit 1; fi
    printf '%s\n' "$dest_abs"
}
```
The fixture also refuses an existing destination. Pass it a path that does not exist yet, under
`~/.cache/devflow-phase49/attempt-N/negctl/`.

**Script skeleton and helpers** (`scripts/unattended-drill.sh:61-86`):
```bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
die() { echo "ERROR: $*" >&2; exit 1; }
note() { echo "[drill] $*"; }
```

**State reads must distinguish absent from false** (`:454-466`). Copy this with `state-01.json`:
```bash
state_field() {
    local fixture="$1" field="$2"
    local sf
    sf="$(state_file "$fixture")"
    [ -f "$sf" ] || { printf 'ABSENT\n'; return 0; }
    jq -r "(.${field} // \"ABSENT\") | tostring" "$sf" 2>/dev/null || printf 'UNREADABLE\n'
}
```

**Launch:** do not copy `launch_arm` (`:534-546`, `setsid`). Use RESEARCH Pattern 6 step 4 and the
Code Example "Positive-arm launch": `systemd-run --user … -- env -u DEVFLOW_BASE_BRANCH sh -c 'exec "$BIN" start --phase 1 --agent claude --mode auto "$D" > … 2> …'`.

**Assertions:** use fixed-string text, never exit codes. The exact strings are in RESEARCH Code
Examples, "Positive / negative assertion core". Accumulate failures in the drill's `FAILURES` style
(see check-evidence.sh below). The abort verb is
`"$BIN" gate reject 1 --stage define --note "abort: negative control complete" --project "$D"`.

---

### `tools/check-evidence.sh` (test / assertion, transform)

**Analog:** `scripts/unattended-drill.sh:784-800` (named failures) and `:944-957` (a loud exit).

**Named-failure accumulation.** Each message names which assertion fired and why, so a green exit
can never mean "nothing happened":
```bash
FAILURES=()
if [ "${OBS[A.plan_has_checkpoint]}" != "yes" ] || [ "${OBS[B.plan_has_checkpoint]}" != "yes" ]; then
    FAILURES+=("FAILURE 1/3 — NO CHECKPOINT IN THE GENERATED PLAN. ...")
fi
...
if [ "$OBS_A" = "$OBS_B" ]; then
    FAILURES+=("FAILURE 3/3 — BOTH ARMS OBSERVED THE SAME THING at the checkpoint: '$OBS_A'. The MEASUREMENT is broken, not the subject: ...")
fi
```
```bash
if [ "${#FAILURES[@]}" -gt 0 ]; then
    echo "DRILL FAILED — ${#FAILURES[@]} assertion(s):" >&2
    for f in "${FAILURES[@]}"; do echo "  * $f" >&2; done
    exit 1
fi
```
**Arms-must-disagree rule** (drill `:672-693`): compare only the discriminating observation, not
downstream consequences. The comment at `:675-686` records how an extra field once let the arms
"differ" on noise. For Phase 49 the compared strings are the config-line text and the base-source
note: positive `[holds]` + `(from devflow.toml (base_branch))`, negative `[DOES NOT HOLD] …` and no
note. Also assert that `sha256sum` matches across the arms (criterion 3).

**Self-test shape** (so the checker can be shown to fail): `scripts/test-phase-worktree-guard.sh:21`, `:53-61`, `:196-198`:
```bash
pass=0; fail=0
check() { # check <name> <expected: refuse|allow> <actual_exit>
    local name="$1" expected="$2" rc="$3"
    local actual; if [ "$rc" -eq 0 ]; then actual=allow; else actual=refuse; fi
    if [ "$actual" = "$expected" ]; then
        printf '  ok   %-58s (%s, exit %d)\n' "$name" "$actual" "$rc"; pass=$((pass+1))
    else
        printf '  FAIL %-58s expected %s, got %s (exit %d)\n' "$name" "$expected" "$actual" "$rc"; fail=$((fail+1))
    fi
}
...
echo "passed=$pass failed=$fail"
[ "$fail" -eq 0 ]
```
Feed it a planted evidence dir that satisfies the checks (must pass) and one with the positive and
negative stdouts swapped (must fail). The swapped dir is the control.

---

### `tools/snapshot.sh` (utility, read-only file-I/O)

**Analog:** `scripts/unattended-drill.sh:451-529` (evidence helpers) and `scripts/drain-drill.sh:209-210` (provenance lines).

**Enumerate capture generations, newest last** (`:470-478`). Substitute `phase-49.1`:
```bash
capture_files() {
    local fixture="$1"
    local f
    for f in "$fixture"/.devflow/history/phase-01/*-stdout; do
        [ -f "$f" ] && printf '%s\n' "$f"
    done
    [ -f "$fixture/.devflow/phase-01-stdout" ] && printf '%s\n' "$fixture/.devflow/phase-01-stdout"
    return 0
}
```
**Truncate huge stream-json lines** before they enter an evidence file (`:482-494`: `printf '%s\n' "${line:0:600}"`).

**Provenance lines** (`scripts/drain-drill.sh:209-210`):
```bash
echo "claude_cli_version : $(claude --version 2>&1 | head -1)"
echo "devflow build commit : $(git -C "$REPO_ROOT" rev-parse --short HEAD)"
```
Phase 49 extends this with the pinned binary's `sha256sum`, `"$BIN" --version`, git and systemd
versions, and the D-16 fork-point triple (RESEARCH Pattern 4).

**Constraints the analog does not have:**
- Strictly read-only against `$ROOT/.devflow/`: `cp`, `jq`, `git rev-parse` only. Never `devflow advance`.
- Snapshot at every gate and at the Code launch, because of overwrites and pruning (RESEARCH Pitfall 6).
- Paths come from RESEARCH Pattern 4 and 5: `state-49.1.json`, `gates/49.1-*`, `lock-49.1`, `phase-49.1-{prompt,stdout,…}`, `history/phase-49.1/`.

---

### `tools/gate-watch.sh` (utility watcher, event-driven poll)

**Analog:** `scripts/unattended-drill.sh:564-585` (a bounded poll loop with periodic progress notes):
```bash
poll_arm() {
    local name="$1" fixture="$2" pid="$3"
    local waited=0
    while [ "$waited" -lt "$CEILING_SECS" ]; do
        if arm_finished "$fixture" "$pid"; then ... return 0; fi
        sleep "$POLL_SECS"
        waited=$((waited + POLL_SECS))
        if [ $((waited % 300)) -eq 0 ]; then
            note "$name: ${waited}s elapsed — stage=$(state_field "$fixture" stage) ..."
        fi
    done
    ...
}
```
**What differs:**
- The watcher is not the process owner. It must never signal anything, unlike the drill's
  `poll_arm`, which kills at `:579-583`.
- Its loop body lists `"$ROOT"/.devflow/gates/49.1-*.json` that have no sibling `.response.json`.
  For each new one it calls `notify-send` and appends a ledger line.
- It filters to the `49.1-` prefix, because test fixtures fire gates for phases 7 and 11 (RESEARCH Pattern 3).

**Self-test:** use the planted-gate fixture in `test-phase-worktree-guard.sh` style. A planted
`49.1-define.json` must notify and write a ledger line. A planted `07-validate.json` must not. A
planted gate that has a `.response.json` must not notify either.

---

### `tools/undo-ship-hooks.sh` (guarded git mutation, batch)

**Analog:** `scripts/cut-release.sh:24-58` ("every step re-verifies its precondition and refuses with a fix hint"):
```bash
die() { echo "ERROR: $*" >&2; exit 1; }
require_clean() {
    [ -z "$(git status --porcelain)" ] || die "working tree is not clean"
}
require_develop() {
    [ "$(git rev-parse --abbrev-ref HEAD)" = "develop" ] || \
        die "must be on develop (currently on '$(git rev-parse --abbrev-ref HEAD)')"
}
```
The version read without a TOML parser (`:60-64`, `workspace_version`) serves the post-undo
`2.12.0` check. Use it or `rg -n '^version = "2.12.0"' Cargo.toml` (RESEARCH Pattern 9).

The guard list and order are fixed by RESEARCH Pattern 9, recipe steps 1–4:
1. The archive ref comes first.
2. Assert the full `M`/`B`/`C` layout: `HEAD~2 == M`, `M^1 == pre-hook HEAD`, the merge subject, `v3.0.0^{commit} == HEAD~1`, and the diff is exactly `CHANGELOG.md` plus `Cargo.toml`.
3. Only then run `tag -d` and `reset --hard M`.
4. Verify afterwards.
Take M and the pre-hook HEAD as **arguments recorded by snapshot.sh**. Never re-derive them at undo time.

**Test harness:** `scripts/test-phase-worktree-guard.sh:12-16`, `:49-52` (hermetic fixture) and `:63-78` (fixture init):
```bash
mapfile -t git_local_env_vars < <(git rev-parse --local-env-vars)
unset "${git_local_env_vars[@]}"
...
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
```
```bash
init_fixture_repo() {
    local dir="$1"
    mkdir -p "$dir" "$TMP/no-hooks"
    git -C "$dir" init -q -b workspace/tester .
    git -C "$dir" config core.hooksPath "$TMP/no-hooks"
    git -C "$dir" config commit.gpgsign false
    git -C "$dir" config user.email t@example.com; git -C "$dir" config user.name Tester
    ...
}
```
Build the correct `M→B(tag)→C` layout in the scratch repo. That case must be allowed and must leave
`HEAD == M` with no tag. Then build one wrong layout per guard: an extra commit, the tag on the wrong
commit, a dirty tree, and a fast-forward rather than a merge. Each must refuse, and must refuse on
the guard's **text**, not only on a non-zero exit (memory: control must fail for the right reason).

---

### `tools/surv-watch.py` (utility watcher, event-driven inotify)

**Analog, structural only:** `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/30c-monitor-env-harness.py`.
No code in this repo uses inotify. The inotify core comes from the RESEARCH "SURV watcher skeleton" (`[ASSUMED]`).

**Copy these conventions:**
- **The module docstring states the one question and what the tool does not report.** See `:1-57`.
  It ends with: "This module reports OBSERVATIONS ONLY. It deliberately prints no verdict … Safe to
  import — nothing runs at import time." For Phase 49 the equivalent is "no interleaving observed
  confirms nothing" (REQUIREMENTS.md:220-223).
- **Stdlib-only imports** (`:59-69`), with a `dataclass` for records, and `main(argv) -> int` plus
  `sys.exit(main())` (`:995-1021`).
- **Atomic publish** (`:713-726`), for any file the watcher writes that other steps read:
  ```python
  def _atomic_publish(text: str, dest_path: str) -> None:
      os.makedirs(os.path.dirname(dest_path), exist_ok=True)
      handle, tmp_path = tempfile.mkstemp(dir=os.path.dirname(dest_path), prefix=".publish-")
      try:
          with os.fdopen(handle, "w", encoding="utf-8") as out:
              out.write(text)
          os.replace(tmp_path, dest_path)
      except BaseException:
          if os.path.exists(tmp_path):
              os.unlink(tmp_path)
          raise
  ```
- **Secret or home-path scan before anything is committed** (`:694-710`, `scan_for_secrets` returns
  pattern names, never values). Apply it to the curated `49-evidence/` excerpts (RESEARCH Security, V8).

**`--selftest` must ship with two controls** (RESEARCH Pattern 7). A synthetic two-writer overlap of
`.state-49.1.json.<pid>.<seq>.tmp` in a scratch dir must be flagged. One sequential writer must not
be. Log `IN_Q_OVERFLOW` as a void window. Code style is the operator's Python rules: type hints,
dataclasses rather than dicts, and `pathlib`.

---

### `.planning/phases/49.1-<slug>/49.1-01-PLAN.md` (pre-authored plan, lands on `workspace/denniyahh` before launch)

**Frontmatter analog:** `.planning/milestones/v2.5.0-phases/35.1-unattended-launch-prerequisites/35.1-01-PLAN.md:1-14`:
```yaml
---
phase: 35.1-unattended-launch-prerequisites
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - ...
autonomous: true
requirements: [HARDEN-07]
```
For 49.1:
- `phase: 49.1-<slug>`.
- `autonomous: false`, because the plan carries a checkpoint (RESEARCH Pattern 10).
- `requirements: [<new 49.1 ID>]`, per ND-3.
- `files_modified: [crates/devflow-cli/tests/phase7_cli.rs]`.
- `type`: `tdd_mode: true` is set in `.planning/config.json:14`. A `type: tdd` plan must satisfy
  RED→GREEN commit order (RESEARCH Pattern 10). The planner chooses `tdd` or `execute` deliberately.

**Seeded checkpoint analog:** `.planning/milestones/v2.8.0-phases/44-codex-end-to-end-verification/44-04-PLAN.md:168-260`.
This is the only tracked plan using `checkpoint:decision gate="blocking-human"`:
```xml
<task type="checkpoint:decision" gate="blocking-human">
  <name>Task 2: Operator chooses the dogfood target and runs it</name>
  <files>...</files>
  <read_first> ... </read_first>
  <precondition> ... </precondition>
  <decision>
    Which phase the live Codex dogfood run drives, and which working tree it runs against.
  </decision>
  <context> ... </context>
  <options>
    <option id="target-phase-45">
      <name>...</name>
      <pros>...</pros>
      <cons>...</cons>
    </option>
    ...
  </options>
  <resume-signal> ... </resume-signal>
  <acceptance_criteria> ... </acceptance_criteria>
  <reversibility rating="one-way"> ... </reversibility>
  <done> ... </done>
</task>
```
**49.1-specific constraints:**
- The decision is 999.55's CI default budget value (D-03).
- The literal `gate="blocking-human"` is what `verify.rs:131` matches. Keep it byte-exact.
- The whole task element is compared at resume (`verify.rs:171-183`), so nothing in the plan may
  tell the Code agent to edit it.
- Do not list the merit-best option first. This is planner's call A7. Without it, a positional pick
  cannot be told apart from "Followed" (RESEARCH Pitfall 7).

**Plan-body analog for a pre-authored plan:** `scripts/scratch-dogfood-repo.sh:145-184`. It has an
`<objective>`, and `<task type="auto">` blocks with `<files>`, `<action>`, `<acceptance_criteria>`,
`<verify><automated>`, `<done>`, `<verification>`, `<success_criteria>` and `<output>`.

**Acceptance must be structural** (D-01):
- All six helpers draw on one budget.
- The panic names the budget it used.
- No claim that the flake is gone.

**`<automated>` gates.** They pass the pre-commit bashism scanner (`scripts/lint-plan-bashisms.sh`;
`scripts/hooks/pre-commit:100-122`) only if bash-only syntax sits inside `bash -c '…'`. Use a
module-qualified test name, assert on `1 passed`, and never use a bare `--exact` name (memory:
cargo test --exact false green).

**Discovery constraints** (RESEARCH Pattern 10):
- The directory is `.planning/phases/49.1-<slug>/`.
- The file prefix is `49.1-` and the suffix is `-PLAN.md`.

---

### `crates/devflow-cli/tests/phase7_cli.rs` (the 49.1 workload: analog only, not edited in Phase 49)

**Current shape** (six helpers, `:200-255` and `:1302-1325`; for example `:200-207`):
```rust
fn wait_for(path: &Path) {
    for _ in 0..200 {
        if path.exists() { return; }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {}", path.display());
}
```
Two independent counts agree at 6 fixed loops: `0..200|0..400` gives 6 matches, and
`Duration::from_millis(25)` gives 6. Loops are at `:201`, `:216`, `:234`, `:247`, `:1303` and
`:1315`. Approximate call sites, from an `rg -o` word-boundary count minus the definition:
- `wait_for` 6;
- `wait_for_pid` 4;
- `wait_for_state_cleared` 1;
- `wait_for_stopped` 2;
- `wait_for_settled` 3;
- `wait_for_gate` 8.

The 999.55 entry's "7 call sites" figure is out of date.

**Pattern the 49.1 plan should direct the agent to:** `crates/devflow-cli/src/config_parse.rs:16-22`,
`:40-43` and tests `:203-208`. A pure parser takes the raw env value and applies the default, so it
is unit-testable **without mutating process env**, which the operator rules and TEST-01 forbid:
```rust
/// Parse `DEVFLOW_GATE_TIMEOUT_SECS`'s raw value, falling back to 3 days on
/// an absent or unparsable value. Pure (no env access) so it's unit-testable
/// without mutating process-global env.
fn parse_gate_timeout(raw: Option<String>) -> u64 {
    const THREE_DAYS: u64 = 3 * 24 * 60 * 60;
    raw.and_then(|s| s.parse().ok()).unwrap_or(THREE_DAYS)
}
pub(crate) fn gate_timeout_secs() -> u64 {
    parse_gate_timeout(std::env::var("DEVFLOW_GATE_TIMEOUT_SECS").ok())
}
```
```rust
fn parse_gate_timeout_env_override() {
    const THREE_DAYS: u64 = 3 * 24 * 60 * 60;
    assert_eq!(parse_gate_timeout(Some("42".into())), 42);
    assert_eq!(parse_gate_timeout(Some("bad".into())), THREE_DAYS);
    assert_eq!(parse_gate_timeout(None), THREE_DAYS);
}
```
For 49.1 the pure function takes two inputs, the raw budget variable and the `CI` presence, so both
defaults are testable.

**Child-env precedent in the same file** (`:130-157`, `run_devflow_inner`). The file already sets
env on the **child** `Command` (`.env("DEVFLOW_GATE_TIMEOUT_SECS", "60")`) and never on the test
process. The comment at `:146-150` ("the wait_for_* helpers cap at 10s") is budget-coupled prose
that 49.1 must update.

**Carry-forward (RESEARCH Pitfall 9).** `CI` is not visible inside the devcontainer job or the local
container gate. Changing that touches `scripts/check-in-container.sh` or
`.github/workflows/devcontainer.yml`, which means `.planning/user/DEV-SETUP-CHECKLIST.md` must be
updated in the same commit (CLAUDE.md).

---

### `.planning/ROADMAP.md` (modified: four edits)

**1. Phase 49.1 block.** Insert it between the end of Phase 49 (`**Plans**: TBD`, `:279`) and
`### Phase 50:` (`:281`). The analog is `.planning/milestones/v2.5.0-ROADMAP.md:107-`:
```markdown
### Phase 35.1: Unattended-Launch Prerequisites (999.93 + 999.98 + 999.99 + 999.100)

**Goal**: ...
**Depends on**: Phase 35 (...). Sequenced **before** Phase 35.3, ...
**Requirements**: HARDEN-07
**Success Criteria** (what must be TRUE):

  1. ...
```
- The heading must be exactly `### Phase 49.1: …`, with three hashes and a colon. That is the
  reachability-guard match (RESEARCH Pattern 10; preflight.rs:176-181).
- The name can carry `(999.55)`, following the 35.1 convention.

**2. Milestone table row** (`:25-32` format). Insert after the `:31` row:
```markdown
| 49 | Live Unattended Run — The Milestone's Instrument | VERIFY-01 | 3 | Not started |
```
The 49.1 row carries the new requirement ID. The wave column is the planner's call; 49.1 runs inside Phase 49's wave 3.

**3. Progress table row** (`:484-491` format, e.g. `| 49 | — | Not started | — |`). Insert after `:490`.

**4. Phase 49 criteria 1 and 2** (`:260-268`):
- Criterion 1 (`:260-262`) is rewritten per D-09 to cite `050ef7f` and keep the persisted
  `State::base_branch` + `ConfigFile` observation.
- Criterion 2 (`:263-266`), "unattended stage progression", becomes "between the operator-approved
  preflight gates and the Ship gate", per D-03 and D-07.
- The research found a **fourth** gate, the Ship-preflight major bump (RESEARCH Pitfall 3). The
  amendment wording should name all four human gates, not the three CONTEXT lists.
- CONTEXT makes this the **first plan task**.

**5. 999.55 PROMOTED marker** (`:4039`). Analog `:790-792`:
```markdown
### Phase 999.140: `resume` Launches With No Live-Run Check (PROMOTED — Phase 48 gap closure)

**Promoted 2026-09-23 by operator decision:** ...
```
- Retitle `(BACKLOG)` to `(PROMOTED — Phase 49.1)`.
- Add a `**Promoted 2026-09-2x by operator decision:**` paragraph citing D-01 and D-02.
- Replace `- [ ] TBD (promote with /gsd-review-backlog when ready)` (`:4070`).

**Warning (memory: GSD workflow quirks):** `roadmap.update-plan-progress` corrupts ROADMAP on every
call. Keep the milestone-window rule: the 49.1 heading must stay inside the v3.0.0 window, `:5` to
`:341`.

---

### `.planning/REQUIREMENTS.md` (modified)

**VERIFY-01 amendment** (`:107-116`). Replace `:116`, "**Carries a setup step:** this repository has
no committed `devflow.toml`.", with the D-09 wording. The configuration is satisfied by `050ef7f` on
`workspace/denniyahh`, and the run must still observe the persisted base and its `ConfigFile` source.

**New 49.1 requirement (ND-3).** The shape analog is TEST-01 (`:96-103`):
```markdown
### Test Isolation (TEST)

- [x] **TEST-01**: Tests that replace process-global `PATH` run in a child process with `PATH` set
      on the child `Command` only; ...
      *Traces to 999.38 with 999.80 folded in (D-10).* **Not established:** the full-suite run
      pinned to two CPUs is a sanity check and does not prove the flakes cannot recur.
```
- The new ID sits under the matching category. `### Test Isolation (TEST)` makes `TEST-02` the
  natural slot, but the ID and wording are the planner's call.
- Use `*Traces to 999.55 (D-01).*`.
- It **must** carry a `**Not established:**` clause: no claim that the flake is gone, and a note that
  CI run history is needed.

**Traceability and coverage** (`:185-209`):
- Add a row such as `| TEST-0x | Phase 49.1 — <name> | 3 | Pending |`.
- Change coverage from `13 total` / `Mapped to phases: 13` to 14.
- Update the footer `*Last updated:*` line (`:227-228`). That line is already stale: it says "10/10".

---

### Phase 49 `PLAN.md` files (operator-run live tasks)

**Analog:** `44-04-PLAN.md`, frontmatter `:1-14` (`autonomous: false`, and `files_modified` listing
the evidence dir, `REQUIREMENTS.md` and `ROADMAP.md`) plus its operator-owned live-run task `:168-260`.

**The rationale to reuse verbatim in spirit** (`44-04-PLAN.md`, `<context>`):
> a supervised run outlives an executor agent's turn. An agent that launches it does not own it —
> the process is orphaned rather than supervised … The operator runs it directly, then a fresh
> executor resumes … to classify the captures.

Also reuse the `<context>` rules "Capture, do not narrate" and "every attempt keeps its own capture;
a retry never replaces the record" (P-02). These map directly onto D-12's archive-ref-before-`--force`
recipe (RESEARCH Pattern 8).

The `<reversibility rating="one-way">` element is the precedent for tasks that push, open a PR, or
tag.

---

### `49-evidence/README.md` and the run write-up

**Analogs:**
- `35.3-evidence/README.md:1-45`: the answer first, a "What was measured and how strong it is"
  table, and explicit "What this evidence can support" and "What it cannot support" paragraphs.
- `44-CODEX-E2E.md`: its headings include "Verdict", "The record", "Every captured turn, in order",
  "What this run does not establish" and "Gap disposition".
- The drill's generated "What this run does NOT establish" section (`scripts/unattended-drill.sh:922-938`).

**Standard to classify against:** `.planning/phases/47-unattended-decision-policy-consistency/47-PHASE49-OBSERVATION.md`.
It defines Followed, Not followed and Void, sets a claude-only scope, and requires recording the
position effect. RESEARCH Pitfall 7 adds a fourth outcome, "not exercised".

---

## Shared Patterns

### Script header and fail-fast
**Source:** `scripts/unattended-drill.sh:1-61`, `scripts/test-phase-worktree-guard.sh:1-10`.
**Apply to:** all `tools/*.sh`.
- `#!/usr/bin/env bash`, then `set -euo pipefail`.
- A header that says **WHAT THIS ESTABLISHES** and **WHAT THIS DOES NOT ESTABLISH** (drill `:5-34`).
- `die`/`note` helpers.
- `bash -n` and `shellcheck` before commit.

### Negative controls asserted on text
**Source:** drill `:672-693`, `:798-800`; `test-phase-worktree-guard.sh` cases 2, 5 and 8a–8e (each control proves its own condition is present, `:125-131`).
**Apply to:** every tool's self-test and both arms. Assert refusal text with `rg -F`; an exit code alone is never enough.

### Hermetic git in scratch fixtures
**Source:** `scripts/test-phase-worktree-guard.sh:12-16`, `:47-52`, `:66-72`. The repo-local
identity comes from `scripts/scratch-dogfood-repo.sh:68-71` ("never --global, and no GIT_* env export (999.37 class)").
**Apply to:** the self-tests of undo-ship-hooks.sh, gate-watch.sh and check-evidence.sh.

### Process ownership and cleanup
**Source:** drill `:369-449` (`trap cleanup EXIT INT TERM`, with the reaper documented at `:433-437`).
**Apply to:** tools that spawn anything, which means self-tests only. The live run is owned by
systemd units, not by a script trap. The drill's reaper kills by process group, which is exactly
what must **not** happen to the live run's cgroup.

### Placement and mode
**Source:** RESEARCH "Project Constraints" and `.workspace-divergence` (`.planning/` is built in).
**Apply to:** all tools.
- Put them under `.planning/phases/49-…/tools/`, never top-level `scripts/`.
- `core.fileMode=false` applies. If a tool must be executable, set `git update-index --chmod=+x`
  and verify with `git ls-files --stage`, following `test-phase-worktree-guard.sh:23-45`. Otherwise
  invoke it as `bash tools/x.sh`.

### Pinned binary for every verb
**Source:** RESEARCH Pattern 1. There is no in-repo analog; the drill's `:710-718` is the opposite.
**Apply to:** negctl.sh, check-evidence.sh (sha256 equality), snapshot.sh (identity), and every
`gate approve/reject` command in the plans.

## No Analog Found

| File | Role | Data Flow | Reason | Use instead |
|---|---|---|---|---|
| systemd launch/resume wrapper, or the inline plan commands | utility | process supervision | No tracked script uses `systemd-run`. A search with `--hidden` found only prose in `.planning/` docs and 30d evidence logs | RESEARCH Pattern 2, plus the Code Example "Positive-arm launch" |
| inotify core of `surv-watch.py` | utility | event stream | No tracked code uses inotify (same search) | RESEARCH "SURV watcher skeleton". It is `[ASSUMED]` and is proven only by its own `--selftest` |

## Metadata

**Analog search scope:**
- `scripts/`, `scripts/hooks/`;
- `crates/devflow-cli/{src,tests}`;
- `.planning/milestones/*-phases/`, `.planning/phases/47-*`;
- `.planning/{ROADMAP,REQUIREMENTS}.md`, `.planning/milestones/v2.5.0-ROADMAP.md`.

**Files read:** 15.

**Pattern extraction date:** 2026-09-25, at `071e2eb`.

**What this map does not establish:**
- Line numbers are exact only at `071e2eb`.
- The helper call-site counts come from `rg -o` word-boundary matches and may include mentions in
  comments. They are approximate.
- No analog excerpt was executed. The drill and guard harness are cited for structure, not
  re-verified as passing here.
