## CONFIRMED
### CR-45-01: Resumed Define preflight ignores the run’s persisted trunk
- **File:** `crates/devflow-cli/src/preflight.rs:648`
- **Severity:** BLOCKER
- **Defect:** The interactivity preflight re-resolves ambient config instead of using `state.base_branch`.
- **Trigger:** Start an auto Codex run with `DEVFLOW_BASE_BRANCH=workspace/denniyahh`, where CONTEXT exists only there; resume from a shell without that variable.
- **Consequence:** Resume checks `develop`, rejects the valid run as missing CONTEXT, and cannot relaunch.
- **Evidence:** `preflight.rs:648-653` resolves `base_branch(project_root)`; `pipeline_launch.rs:1355-1356` saves then calls `launch_stage`, whose `pipeline_launch.rs:1178-1180` invokes preflight.

### CR-45-02: Persisted state can redirect an unattended merge to `main`
- **File:** `crates/devflow-core/src/config.rs:435`
- **Severity:** BLOCKER
- **Defect:** `git_flow_for_run` copies any persisted `base_branch` into the merge target without validation or local-branch verification.
- **Trigger:** A valid-but-corrupt or manually edited `.devflow/state-NN.json` contains `"base_branch":"main"` (or `origin/main`, SHA-like data, etc.), then `advance` reaches Ship.
- **Consequence:** The phase can merge into `main`, bypassing the start-time production-branch refusal.
- **Evidence:** `workflow.rs:195-204` deserializes state directly; `config.rs:435-442` trusts `Some(base)`; `pipeline_outcomes.rs:1065-1067` supplies that value to checkout hooks, including merge.

### CR-45-03: `--no-worktree` re-resolves config after validating and persisting a different base
- **File:** `crates/devflow-cli/src/commands.rs:452`
- **Severity:** BLOCKER
- **Defect:** `start` resolves and persists `base`, then the no-worktree fork calls `GitFlow::for_project`, which reads configuration again.
- **Trigger:** `devflow.toml` changes between the initial resolution and `GitFlow::for_project`—for example from `workspace/denniyahh` to `develop`.
- **Consequence:** The feature branch forks from the new value while State retains the old value; later Ship merges the branch into the old value.
- **Evidence:** `commands.rs:325-341` resolves/persists once; `commands.rs:452-456` re-resolves through `GitFlow::for_project`; `config.rs:404-417` shows that constructor reads config anew.

### CR-45-04: Scoped staleness accepts non-member crates
- **File:** `crates/devflow-cli/src/staleness.rs:280`
- **Severity:** WARNING
- **Defect:** The predicate treats every `crates/**` Rust/build file as a workspace member, despite claiming to scope to actual members.
- **Trigger:** A tracked edit under `crates/scratch/src/main.rs` or `crates/third-party/Cargo.toml`.
- **Consequence:** Self-dogfood runs hard-block on code Cargo does not build, recreating the false-positive class AUTO-02 intended to remove.
- **Evidence:** `Cargo.toml:3-6` lists only `crates/devflow-core` and `crates/devflow-cli`; `staleness.rs:280-285` accepts any `crates/` path. The passing targeted test (`1 passed; 360 filtered`) only covers those two members at `staleness.rs:645-649`; it has no non-member-under-`crates/` negative control.

### CR-45-05: Scoped staleness misses root Cargo configuration
- **File:** `crates/devflow-cli/src/staleness.rs:271`
- **Severity:** WARNING
- **Defect:** `.cargo/config.toml` is excluded because only four exact root filenames are accepted.
- **Trigger:** Commit a root `.cargo/config.toml` changing build rustflags, linker, target configuration, or build target; run the previously compiled DevFlow binary.
- **Consequence:** The self-dogfood stale-build guard reports Fresh despite a build-affecting configuration change.
- **Evidence:** `staleness.rs:271-285` permits only `Cargo.toml`, `Cargo.lock`, `build.rs`, and `rust-toolchain.toml` at root or selected crate paths; `.cargo/config.toml` matches none.

### CR-45-06: Idle-timeout evidence still loses the persisted trunk
- **File:** `crates/devflow-core/src/monitor.rs:1259`
- **Severity:** WARNING
- **Defect:** The monitor computes timeout commit evidence from ambient project config, not the run’s persisted base.
- **Trigger:** Start with env-only `DEVFLOW_BASE_BRANCH=workspace/denniyahh`, then resume from a clean shell; the resumed monitor reaches its idle timeout.
- **Consequence:** The authoritative timeout record names commits against `develop`, potentially including base commits as agent work or omitting actual phase commits.
- **Evidence:** `monitor.rs:1254-1264` has no State/base parameter and calls `git_flow_for_project`; `agent_result.rs:2173-2200` surfaces that recorded commit list to the operator as timeout evidence.

## SUSPECTED
