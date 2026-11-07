---
phase: 51-rate-limit-agent-failover
plan: 14
subsystem: config
tags: [rust, config, devflow-toml, fallback-agent, failover, doc-check]
status: complete

requires:
  - phase: 51-rate-limit-agent-failover
    provides: "51-02: doc_check::strip_rust_comments; 51-04: --fallback-agent, validate_fallback_agent, print_fallback_notice, State.fallback_agent"
provides:
  - "pub fn devflow_core::config::fallback_agent(project_root) -> Result<Option<AgentKind>, String> (additive) and crate-private FallbackAgentKey; DevflowConfig unchanged"
  - "devflow.toml key fallback_agent, read by devflow start only"
  - "commands::start resolves the key unconditionally, flag wins the value, notice names devflow.toml for a key-only fallback, key-supplied refusals name the key and how to override it"
  - "doc_check::fallback_env_name_findings and no_environment_variable_selects_the_fallback_agent"
  - "new suite crates/devflow-cli/tests/fallback_agent_config.rs (7 tests)"
affects: [51-11]

actuals:
  tokens: 6921
  tasks: 2
  commits: 4
plan_head_before: ebc1f3b39302051db6fe05d95bb7aeefaf9f7acf
plan_head_after: 804886d3616912958509ba6eb84960db36b75c1a

tech-stack:
  added: []
  patterns:
    - "Additive single-key resolver over a crate-private serde struct, so a published struct with pub fields keeps its shape"
    - "Source scan over string literals (not call sites) so a const-held name is seen; the scan's own needle and planted names are concat!-split inside the word"

key-files:
  created:
    - crates/devflow-cli/tests/fallback_agent_config.rs
  modified:
    - crates/devflow-core/src/config.rs
    - crates/devflow-cli/src/commands.rs
    - crates/devflow-core/src/doc_check.rs

key-decisions:
  - "devflow.toml fallback_agent is resolved at start before the flag-first choice, so an unparseable file or a value naming no agent refuses start even with a valid --fallback-agent (the flag wins the value, it does not excuse a broken file)"
  - "A key-supplied fallback refused by validate_fallback_agent is wrapped as `devflow.toml's fallback_agent (<agent>) is refused: <51-04 text>` plus the override hint; a flag-supplied refusal keeps 51-04's text unchanged"
  - "The env-name scan's FALLBACK needle is itself concat!-split (FALLBACK_WORD); the R2-11 self-check caught the unsplit needle on first GREEN run"

patterns-established:
  - "R2-11 self-check: a source scan that reads its own file asserts it reports nothing there before asserting the real tree"

requirements-completed: [SUPV-02]

coverage:
  - id: D1
    description: "config::fallback_agent: no file or no key is Ok(None); \"codex\" is Some(Codex); \"nosuch\" is an Err naming fallback_agent and devflow.toml; invalid TOML is an Err"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/config.rs#config::tests::fallback_agent_key_resolves_from_devflow_toml"
        status: pass
    human_judgment: false
  - id: D2
    description: "start combines the flag and the key: key alone arms the fallback with a devflow.toml source notice; the flag wins; neither arms nothing"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#config_key_alone_arms_the_fallback_and_names_its_source"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#fallback_flag_wins_over_the_config_key"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#no_flag_and_no_key_arms_no_fallback"
        status: pass
    human_judgment: false
  - id: D3
    description: "A real key-only start saves fallback_agent \"codex\", unused, in state-5157.json"
    requirement: "SUPV-02"
    verification:
      - kind: e2e
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#config_key_alone_is_persisted_in_the_run_state"
        status: pass
    human_judgment: false
  - id: D4
    description: "Key refusals name their source and write nothing; a valid flag does not mask a broken key"
    requirement: "SUPV-02"
    verification:
      - kind: integration
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#config_key_equal_to_the_agent_refuses_start_and_names_the_key"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#config_key_with_an_invalid_value_refuses_start"
        status: pass
      - kind: integration
        ref: "crates/devflow-cli/tests/fallback_agent_config.rs#a_valid_flag_does_not_mask_an_invalid_config_key"
        status: pass
    human_judgment: false
  - id: D5
    description: "No env-shaped string literal containing FALLBACK exists in comment-stripped crate source (literal or const-held)"
    requirement: "SUPV-02"
    verification:
      - kind: unit
        ref: "crates/devflow-core/src/doc_check.rs#doc_check::no_environment_variable_selects_the_fallback_agent"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-11-07
---

# Phase 51 Plan 14: devflow.toml fallback_agent key Summary

**A standing `fallback_agent = "<agent>"` in `devflow.toml` now arms the rate-limit fallback at `devflow start`. A typed `--fallback-agent` wins the value. A key-only fallback prints a notice naming `devflow.toml`. A broken key refuses `start` even when a valid flag is given. No environment variable can select the fallback. `DevflowConfig` is unchanged.**

## Performance

- **Duration:** about 12 min (2026-11-07T22:09:21Z to 2026-11-07T22:21:30Z)
- **Tasks:** 2 of 2
- **Files:** 4 (1 created, 3 modified)

## Accomplishments

- `config::fallback_agent` (`config.rs` L519) reads the key through the crate-private `FallbackAgentKey` (L495). A missing file or a missing key gives `Ok(None)`. An unreadable file logs a warning and gives `Ok(None)`. A file that does not parse, or a value that names no agent, gives an `Err` that starts `devflow.toml: cannot read \`fallback_agent\``.
- `commands::start` resolves the key unconditionally (L410), before the flag-first choice. The key-only notice is `note: fallback agent <agent> set by devflow.toml (fallback_agent) — pass --fallback-agent to choose another`. When the key's value is refused, the error reads `devflow.toml's fallback_agent (<agent>) is refused: <51-04 text>` and is followed by the override hint. All of this runs before `if dry_run`.
- `doc_check::fallback_env_name_findings` and the `no_environment_variable_selects_the_fallback_agent` scan. The scan checks string literals, not call sites, so a name held in a `const` is also seen.
- A new CLI suite with 7 tests. It drives the real binary only through `common::hermetic_devflow`. Each child that must not launch an agent gets a `PATH` with only the stubs it needs plus `git` and `sh`.

## Task Commits

1. **Task 1 RED** `bd6c26e` test(51-14): add failing tests for the devflow.toml fallback_agent key
2. **Task 1 GREEN** `38042fe` feat(51-14): arm the fallback from devflow.toml's fallback_agent key
3. **Task 2 RED** `1cdac77` test(51-14): add failing tests for key refusals and the env-name scan
4. **Task 2 GREEN** `804886d` feat(51-14): name devflow.toml in key refusals and scan for env names

No REFACTOR commit was needed.

## TDD Gate Compliance

RED → GREEN: `bd6c26e` → `38042fe`, then `1cdac77` → `804886d`. Each GREEN commit descends from its RED commit. RED evidence came from nextest JUnit reports, written with a scratch `red` profile and checked with `gsd-tools check tdd-red-evidence`. The records are in `target/red-evidence/51-14-*.json` (gitignored).

The core RED commit includes a stub `config::fallback_agent` that always returns `Ok(None)`, and the Task 2 RED commit includes a stub `fallback_env_name_findings` that reports nothing. The stubs keep the RED tests compiling, so each fails on its assertion and not on a build error.

| Test | RED verdict | Failed on | Semantic assessment |
|------|-------------|-----------|---------------------|
| `config::tests::fallback_agent_key_resolves_from_devflow_toml` | RED_EVIDENCE_OK | `an agent name resolves to that agent`: left `Ok(None)`, right `Ok(Some(Codex))` | Intended: the stub ignores the key |
| `config_key_alone_arms_the_fallback_and_names_its_source` | RED_EVIDENCE_OK | `the key arms codex as the fallback`. The dry-run stdout had no fallback line | Intended: `start` ignored the key |
| `config_key_alone_is_persisted_in_the_run_state` | RED_EVIDENCE_OK | `the real start names the key as the source` | Intended. The launch succeeded, but the key was ignored |
| `fallback_flag_wins_over_the_config_key` | INVALID_RED (unexpected_green) | n/a | Expected. The flag path already existed (51-04). The mutation below shows the test can fail |
| `no_flag_and_no_key_arms_no_fallback` | INVALID_RED (unexpected_green) | n/a | Expected: this is the zero-arming control. The mutation below shows it can fail |
| `doc_check::no_environment_variable_selects_the_fallback_agent` | RED_EVIDENCE_OK | `a literal read of a FALLBACK name is reported`: left 0, right 1 | Intended: the stub finder reports nothing |
| `config_key_equal_to_the_agent_refuses_start_and_names_the_key` | RED_EVIDENCE_OK | `the refusal names the key, its file and its value`. stderr was 51-04's flag text only | Intended: Task 1 passed the refusal through unwrapped |
| `config_key_with_an_invalid_value_refuses_start` | passed on Task 1 code | n/a | Expected: the Task 1 resolver already names its source. See the mutation below |
| `a_valid_flag_does_not_mask_an_invalid_config_key` | passed on Task 1 code | n/a | Expected: Task 1 already resolves the key unconditionally. See the mutation below |

**Mutation runs (each applied, run, then restored from a saved copy and confirmed with `cmp`; each mutation record is RED_EVIDENCE_OK):**
- **R2-5 (required):** after the `if dry_run` return in `start`, I inserted `if from_config { state.fallback_agent = None; }`.
  - `config_key_alone_is_persisted_in_the_run_state` FAILED (exit 100) with `the saved run carries the key's fallback`: left `Null`, right `"codex"`. The saved state otherwise showed a live launch: a `supervisor` record, `"stop_until": "define"`, and `"fallback_consumed": false`.
  - `config_key_alone_arms_the_fallback_and_names_its_source` still PASSED (exit 0, `1 passed`). A dry-run test alone could not see this mutation.
- **Flag wins:** I swapped the choice to `config_fallback.or(fallback_agent)`. `fallback_flag_wins_over_the_config_key` failed with `the flag's agent is the fallback` (stdout showed codex).
- **Zero arming:** I appended `.or(Some(AgentKind::Codex))` to the choice. `no_flag_and_no_key_arms_no_fallback` failed with `nothing arms a fallback`.
- **Source naming:** I changed the resolver's error to `cannot read the fallback: {error}`. `config_key_with_an_invalid_value_refuses_start` failed with `the refusal names the key and the file`. Limit: in that test, only the `devflow.toml` half of the assertion discriminates. The TOML error snippet itself contains `fallback_agent`.
- **Flag short-circuit:** I moved `config::fallback_agent` inside a `fallback_agent.is_none()` branch. `a_valid_flag_does_not_mask_an_invalid_config_key` failed on case (a): `a key naming no agent refuses start even with a valid flag`. Case (b) never ran in that test, so I ran it separately against the built binary from a scratch repo, using the same `--fallback-agent codex --dry-run` and an unparseable `devflow.toml`. The restored code exits 1, the mutated code exits 0, and the restored code exits 1 again. So no other reader refuses an unparseable file before the dry-run return. The only earlier reader is `config::yes_ship`, which only logs a warning (`failed to parse devflow config; using defaults`).
- **Scan, real tree:** I planted `const PLANTED_REAL_TREE: &str = "DEVFLOW_FALLBACK_AGENT";` in `config.rs`. The scan failed and named `config.rs:1105: "DEVFLOW_FALLBACK_AGENT"`.

## Verification

| Gate | Command | Result |
|------|---------|--------|
| Task 1 verify | the plan's 5 chained `cargo nextest run … --no-tests=fail -E 'test(=…)'` commands, then `cargo semver-checks check-release -p devflow-core` | exit 0. semver-checks: `229 checks: 229 pass, 31 skip`, `no semver update required` |
| Tracer feedback gate | re-ran the Task 1 verify before Task 2 (automated-only verify, `end-of-phase`) | exit 0 |
| Task 2 verify | the plan's 7 chained nextest commands, then `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Plan verification | `cargo test -p devflow-core --lib` | `1134 passed; 0 failed; 2 ignored` |
| Plan verification | `cargo test -p devflow --test fallback_agent_config` | `7 passed; 0 failed` |
| Regression | `cargo nextest run -p devflow --bin devflow --test yes_ship_config --no-fail-fast` | `671 passed (2 leaky), 2 skipped`. The two leaky tests are existing gate-sweep and reachable-pid tests in `commands::tests`, which this plan did not touch |
| Acceptance | the `pub struct DevflowConfig` block extracted at `ce08a9d` and at HEAD | byte-identical (42 lines, `cmp` exit 0) |
| Acceptance | `rg -n "config::fallback_agent" crates/devflow-cli/src/` | one hit, `commands.rs:410` (`start`). `resume_loaded` (`pipeline_launch.rs`) does not call it |
| Acceptance | `/proc` scan for `__monitor … --phase 5157` or the `sleep 120` stub after each run of the persisted-run test | none left running |

**What these checks do not establish:**
- The CLI tests watch `start` through `--dry-run`, through refusals, and through one real launch whose saved state carries the key's fallback. They do not show that the saved value then drives 51-01's failover at a rate limit. That is covered by 51-01's e2e test, which uses a flag-supplied value. It was not re-run here with a key-supplied value, but the saved field is the same either way.
- No test drives the D-03 auto-mode refusal for a key-supplied fallback. That refusal reaches the same wrap as the D-02 one, but only the D-02 case is pinned (`config_key_equal_to_the_agent_refuses_start_and_names_the_key`).
- The scan cannot see a name assembled at runtime (`format!`, `concat!`). This limit is stated in the test's doc comment.
- The persisted-run test passed every time it ran: 4 runs on the GREEN code (Task 1 suite run, Task 1 gate, Task 2 suite run, `cargo test` suite run). That is a weak bound and does not measure flakiness.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The scan's own needle tripped the R2-11 self-check**
- **Found during:** Task 2 GREEN
- **Issue:** The first finder used `content.contains("FALLBACK")`. The literal `"FALLBACK"` is itself an env-shaped literal containing FALLBACK, so the self-check reported `doc_check.rs:1630: "FALLBACK"`. The self-check worked as designed.
- **Fix:** I split the needle as `const FALLBACK_WORD: &str = concat!("FALL", "BACK");`, the same way the planted names are split.
- **Files modified:** `crates/devflow-core/src/doc_check.rs`
- **Commit:** `804886d`

**2. [Rule 2 - Missing critical] Char literals are skipped whole in the scan**
- **Found during:** Task 2 GREEN
- **Issue:** If the walker skipped a char literal one character at a time, a `'"'` would open a false string and misread the rest of the line.
- **Fix:** The walker now skips any span `literal_at` recognises, and reports only those that start with `"` or `r`.
- **Commit:** `804886d`

**Total deviations:** 2 auto-fixed. **Impact:** none on the plan's contract.

## Decisions Made

- Task 1's RED commit includes a stub resolver, so the core RED failure is an assertion and not a build error.
- The refusal tests give the child a `PATH` with only the stubs it needs plus `git` and `sh`. They also skip `--dry-run`, as the plan requires, so a lost refusal ends at the missing `claude` binary instead of a real agent launch.
- Wording of the key refusal: it keeps 51-04's text inside the wrap. That text still begins `--fallback-agent claude is the run's own agent (claude)`, although no flag was typed. The `devflow.toml's fallback_agent (claude) is refused:` prefix names the real source. Changing 51-04's shared message would have changed the flag refusal, which the plan keeps unchanged.

## Issues Encountered

- My first manual check of case (b) used a scratch repo under `.git/worktrees/…/gsd-orch-scratch/`. With the path inside a `.git` directory, `devflow` did not read that repo's `devflow.toml`, so both the restored and mutated code exited 0. I re-ran the check from a repo outside `.git`, and the result above comes from that run. I did not investigate further: the path cannot occur in real use.

## Known Stubs

None. Both RED stubs were replaced in their GREEN commits.

## Next Phase Readiness

The key exists and is tested. 51-11's configuration-guide sentence about `fallback_agent` should match the behaviour here: the flag wins the value, a broken key refuses `start` even with a flag, and `resume` does not read the key.

## Self-Check: PASSED

- FOUND: crates/devflow-cli/tests/fallback_agent_config.rs
- FOUND: crates/devflow-core/src/config.rs, crates/devflow-cli/src/commands.rs, crates/devflow-core/src/doc_check.rs
- FOUND commits: bd6c26e, 38042fe, 1cdac77, 804886d (each `git merge-base --is-ancestor` HEAD)
