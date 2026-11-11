## CONFIRMED

### 1. Proof Run Deadlock under D-46: Uncommitted Chain Flag Prevents Exit Criterion
* **Severity:** Critical
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:32-36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L32-L36) (D-02), [52.1-CONTEXT.md:91-97](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L91-L97) (D-15)
* **Code & Prior Context:**
  - [.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md:412-427](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md#L412-L427) (D-46):
    > "a Validate PASS counts only on a clean tree (amends D-45 rule 1 and rule 4). `agent_pass_on_head` holds only when the recorded PASS head equals HEAD **and** `git status --porcelain` over the evidence root is empty, `.planning/` included... a Validate whose own outcome reads a dirty tree records **no** PASS (`validate_pass_head` stays unset)"
  - [scripts/scratch-dogfood-repo.sh:1251-1255](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/scratch-dogfood-repo.sh#L1251-L1255):
    ```json
    cat > "$DEST_ABS/.planning/config.json" <<'EOF'
    {
      "workflow": {}
    }
    EOF
    ```
  - [crates/devflow-core/src/gsd_config.rs:168](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-core/src/gsd_config.rs#L168) (`set_auto_chain_active`) & [crates/devflow-cli/src/pipeline_launch.rs:806-860](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/pipeline_launch.rs#L806-L860):
    Before Code launches, `set_auto_chain_active(true)` writes `"_auto_chain_active": true`. When Code ends, `set_auto_chain_active(false)` writes `"_auto_chain_active": false` into `.planning/config.json`.
  - [crates/devflow-cli/src/pipeline_outcomes.rs:1125-1145, 1480-1496](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/pipeline_outcomes.rs#L1125-L1145):
    `read_tree` runs `git status --porcelain --untracked-files=all`. If any path is modified, `agent_pass_on_head` is forced to `false`.
* **Defect:**
  D-15 mandates closing the phase on a supervised run against `scripts/scratch-dogfood-repo.sh` with `"no hand-run stage, no edit under .devflow/, and no process killed by hand"`.
  However, `scripts/scratch-dogfood-repo.sh` scaffolds `.planning/config.json` without `_auto_chain_active`. When Code executes, DevFlow (and upstream GSD) sets and clears the flag, leaving `"_auto_chain_active": false` uncommitted in `.planning/config.json`.
  When Validate runs, `git status --porcelain` detects ` M .planning/config.json`. Under D-46, Validate records **no PASS on HEAD**. Under D-45, `gate approve` over `passed` without an agent PASS on HEAD unconditionally routes to: **run Validate again**.
  The proof run will enter an infinite loop at the Validate gate. It cannot finish without a manual commit or manual state intervention, directly violating D-15. Furthermore, D-02's assertion that 999.219 can be dropped because `"nothing else depends on it"` is false: the proof run in D-15 depends strictly on solving or committing the chain-flag discrepancy.

---

### 2. Proof Run Scaffold (`scripts/scratch-dogfood-repo.sh`) Fails v4 Preflight and Validation
* **Severity:** Critical
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:91-97](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L91-L97) (D-15), [52.1-CONTEXT.md:155-157](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L155-L157)
* **Code Citations:**
  - [scripts/scratch-dogfood-repo.sh:1137-1263](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/scratch-dogfood-repo.sh#L1137-L1263) (default mode):
    Only scaffolds `PROJECT.md`, `ROADMAP.md`, `STATE.md`, `01-01-PLAN.md`, and `config.json`. It does NOT scaffold `01-VALIDATION.md`.
  - [scripts/scratch-dogfood-repo.sh:915-942](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/scratch-dogfood-repo.sh#L915-L942) (`--live-payload` mode):
    Scaffolds `01-VALIDATION.md`, but `01-01-PLAN.md` declares `<task type="checkpoint:decision" gate="blocking-human">`.
  - [crates/devflow-cli/src/main.rs:48-50](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/main.rs#L48-L50):
    `phase: PhaseId` on `Command::Start` is a required argument with no default.
* **Defect:**
  1. D-15 assumes `scripts/scratch-dogfood-repo.sh` is a turnkey v4 proof target. In default mode, it omits `VALIDATION.md` entirely (scaffolded back in Phase 23a for Define→Ship). Without `VALIDATION.md`, GSD's `/gsd-validate-phase` reports `missing`, which D-45 routes to `re-verify Code` instead of finishing.
  2. If run with `--live-payload`, `01-01-PLAN.md` carries a `blocking-human` gate. DevFlow's one-shot launch cannot answer it, and preflight refuses it (Phase 52 D-01, 52-CONTEXT line 50), because the Question Channel is deferred to Phase 53.
  3. The verbatim invocation in D-15 (`devflow start --mode supervise`) omits `--phase 1` and crashes on clap validation (`error: the following required arguments were not provided: --phase <PHASE>`).

---

### 3. Flake Fix D-06 / 999.222 `rename` Fallacy: `ETXTBSY` Survives File Renaming
* **Severity:** Major
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:49-54](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L49-L54) (D-06)
* **Backlog Citation:**
  - [.planning/ROADMAP.md:848-850](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/ROADMAP.md#L848-L850) (999.222):
    > "write the stub to a temp name in the same directory, close it, and `rename` it into place (the exec'd inode was never open for writing), or retry the spawn on `ETXTBSY` in test builds only."
* **Code Citation:**
  - [crates/devflow-core/src/test_support.rs:414-416](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-core/src/test_support.rs#L414-L416):
    `plant_gsd_run_stub` writes the script with `std::fs::write(&launcher, script)` and sets mode `0o755`.
* **Defect:**
  Linux kernel VFS semantics return `ETXTBSY` whenever `execve(2)` is called on an inode that has an active write descriptor (`i_writecount > 0`). `rename(2)` updates directory entries; it does **not** allocate a new inode.
  If a child process is forked by another test thread while the temporary stub is open for writing, the child inherits that open write descriptor. Closing the file in the test thread and renaming it does not drop the child's reference; the inode remains open for writing in the child. When `execve` is called on the renamed path, the kernel inspects the same inode and still returns `ETXTBSY (os error 26)`.
  The premise in 999.222 that `"the exec'd inode was never open for writing"` is factually incorrect on Linux. D-06 discarded the working alternative (retrying `spawn` on `ETXTBSY` in test builds) and chose a no-op rename.

---

### 4. D-05 Flake Investigation Premise Contradicts Linux Fork/Exec Semantics
* **Severity:** Major
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:44-48](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L44-L48) (D-05)
* **Code Citations:**
  - [crates/devflow-core/src/workflow.rs:1066](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-core/src/workflow.rs#L1066):
    `// Wait out a fork in another test thread: between its fork and its exec a child holds a duplicate of the lock's open file description.`
  - [crates/devflow-core/src/agents/opencode.rs:1501-1504](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-core/src/agents/opencode.rs#L1501-L1504) (999.232):
    `assert!(!crate::agent::agent_running(pid), "the timed-out health probe's sleep descendant must be dead: pid {pid}");`
  - [crates/devflow-core/src/agent.rs:36-46](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-core/src/agent.rs#L36-L46):
    `agent_running(pid)` calls `libc::kill(signed, 0)` and parses `/proc/{pid}/status`.
* **Defect:**
  1. D-05 states: `"The research step checks it from source: which fds are opened without O_CLOEXEC, and which test helpers fork without exec."` On Linux, `O_CLOEXEC` does NOT prevent file descriptor duplication across `fork()`. Descriptors marked `O_CLOEXEC` remain open in child processes until `execve()` is called. In a multi-threaded test process, any standard `Command::spawn()` inherently creates a window where forked children hold duplicates of the parent's open files. Auditing for "missing `O_CLOEXEC`" or "fork without exec" will yield zero results and falsely discredit the hypothesis.
  2. 999.232 (`health_fails_closed_on_a_hung_probe`) does not involve file descriptors, locks, or `write_lock_status`. It failed because an asynchronous `sleep` process had not finished terminating when `kill(pid, 0)` ran. Grouping 999.232 into D-05's fd hypothesis is a root-cause misattribution.

---

### 5. D-13 Fails Open on Develop Checkout by Leaving Divergence Script Diffing HEAD
* **Severity:** Critical
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:82-86](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L82-L86) (D-13)
* **Code Citations:**
  - [scripts/check-workspace-divergence.sh:79](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/check-workspace-divergence.sh#L79):
    `git diff --name-status --no-renames "$BASE_REF" HEAD`
  - [scripts/check-workspace-divergence.sh:87](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/check-workspace-divergence.sh#L87):
    `git diff --quiet "$BASE_REF" HEAD -- "$entry"`
  - [scripts/check-workspace-divergence.sh:99](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/check-workspace-divergence.sh#L99):
    `git ls-files -s -- ':(glob)scripts/hooks/**' ':(glob)scripts/*.sh'`
  - [scripts/hooks/pre-push:123-126](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/scripts/hooks/pre-push#L123-L126):
    `read -r local_ref local_sha remote_ref _remote_sha <<< "$line"`
* **Defect:**
  D-13 modifies `scripts/hooks/pre-push` to trigger when pushing `refs/heads/workspace/*` and to read `.workspace-divergence` from `git show <sha>:.workspace-divergence`.
  However, D-13 leaves `scripts/check-workspace-divergence.sh` unchanged. `check-workspace-divergence.sh` hardcodes diffs against `HEAD` (lines 79, 87) and inspects permissions from the index of the active working directory (line 99).
  When testing the suspected fail-open case ("a workspace branch pushed from a develop-based checkout"), `check-workspace-divergence.sh` diffs `origin/develop` against `HEAD` (`develop`), finds zero differences, and exits 0 — completely failing to verify the pushed workspace commit `<sha>`.
  Additionally, on remote ref deletion (`git push origin :workspace/foo`), `local_sha` is all zeros (`0000000...`). Calling `git show <sha>:.workspace-divergence` without checking for deletion SHAs will crash `pre-push` with a fatal git error.

---

### 6. Contradictory Semantics in `--force` Removal: `start` "Continuing" vs `resume`
* **Severity:** Major
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:62-64](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L62-L64) (D-09.1)
* **Code Citations:**
  - [crates/devflow-cli/src/commands.rs:374](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/commands.rs#L374):
    `let mut state = fresh_state_carrying_phase_failures(project_root, phase, agent, mode);`
  - [crates/devflow-cli/src/pipeline_launch.rs:3159-3165](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/pipeline_launch.rs#L3159-L3165):
    `resume` explicitly preserves state and stage: `"unlike start, this does NOT call State::new, feature_start, or ensure_phase_worktree... the workflow is never reset to Plan."`
  - [crates/devflow-cli/src/commands.rs:4665-4670](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/commands.rs#L4665-L4670):
    `resume` refuses finished runs: `"resume refused: phase {phase} finished (its Validate passed), so there is nothing to resume... Run devflow start --phase {phase} to start a new run"`
* **Defect:**
  D-09.1 asserts that `start` over a finished or stopped phase `"continues on its worktree and branch instead of demanding --force; resume, advance and status name that verb"`.
  This creates an unresolvable state contradiction:
  1. If `start` reuses the existing state and continues execution from the saved stage, it is functionally identical to `resume`, making the separation between `start` and `resume` meaningless.
  2. If `start` initializes fresh state (`State::new`), it resets the stage to `Plan`. Running Plan over an already finished or stopped worktree does not "continue"; it re-plans over an existing feature branch.
  3. If a phase is `finished` (Validate passed), what does `start` "continue"? Validate already succeeded. Does it run Validate again? Does it immediately declare finish?
  4. If `start` silently continues existing worktrees and `--force` is removed, an operator has no way to re-run a phase from scratch without invoking `discard` (which nukes the branch and uncommitted files).

---

### 7. Concurrency Hazard and Checked-Out Branch Failures in `devflow discard`
* **Severity:** Major
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:65-67](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L65-L67) (D-09.2), [52.1-CONTEXT.md:73-75](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L73-L75) (D-10)
* **Code Citations:**
  - [crates/devflow-cli/src/commands.rs:1238-1259](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/src/commands.rs#L1238-L1259):
    `cleanup` refuses to delete worktrees if an agent is running or supervisor is alive.
  - [.planning/ROADMAP.md:811-812](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/ROADMAP.md#L811-L812) (999.224):
    `"the --no-worktree refusal's git branch -D hint fails if the main checkout has the branch checked out"`
* **Defect:**
  1. **Concurrency Hazard:** D-09 specifies `devflow discard --phase N` without any concurrency or liveness guards. If executed while a supervisor or agent is running, it deletes the worktree and state files out from under active processes, resulting in leaked background workers and repository corruption.
  2. **Checked-out branch in `--no-worktree`:** D-10 requires tests for discarding branches under `--no-worktree`. In `--no-worktree` mode, `feature/phase-NN` is checked out in the primary working tree. Git explicitly rejects branch deletion for the currently checked-out branch (`fatal: cannot delete branch '...' used by worktree`). D-09 and D-10 provide no specification for switching branches before deletion.

---

### 8. Scope Leak: Unsanctioned Promotion of Backlog Item 999.167
* **Severity:** Minor
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:8-9](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L8-L9), [52.1-CONTEXT.md:30-36](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L30-L36) (D-01..D-03), [52.1-CONTEXT.md:80-82](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L80-L82) (D-12)
* **Roadmap Citations:**
  - [.planning/ROADMAP.md:170-172](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/ROADMAP.md#L170-L172):
    Candidate scope lists 999.221, 999.222, 999.223, 999.232, 999.224, 999.230, 999.231. 999.167 is omitted.
  - [.planning/ROADMAP.md:2028](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/ROADMAP.md#L2028):
    `Phase 999.167` is in BACKLOG: `"(promote with /gsd-review-backlog when ready)"`.
* **Defect:**
  The operator accepted scope D-01..D-04. 999.167 is not listed in D-01, D-02, or D-03, nor in the roadmap candidate scope. D-12 quietly bundles 999.167 into the phase under the justification that 999.230 mentioned pairing with it. Furthermore, 999.167's acceptance criterion (`"(NN.1) scopes for other phases are not [replayed]"`) conflicts directly with Phase 52.1 itself, which is a dot phase (`52.1`).

---

### 9. False Semver Reversibility Claim for `devflow-core`
* **Severity:** Minor
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:70-72](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L70-L72) (D-09)
* **Code Citations:**
  - [Cargo.toml:4-7](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/Cargo.toml#L4-L7) & [crates/devflow-cli/Cargo.toml:1-10](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/Cargo.toml#L1-L10):
    `devflow-core` is the published library crate; `devflow` (`crates/devflow-cli`) is the binary CLI crate.
  - [crates/devflow-core/src/](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-core/src/):
    Contains no clap CLI definitions, no `--force` CLI flags, and no `discard` verb.
* **Defect:**
  D-09 argues that removing `--force` and adding `discard` is a one-way breaking change because `"--force and the new verb are CLI surface, and devflow-core is published on crates.io. Removing a flag or verb after 4.0.0 ships needs a 5.0.0"`.
  CLI flags and command verbs belong to the `devflow` binary crate (`crates/devflow-cli`), not the `devflow-core` library API. `devflow-cli` is not published to crates.io. Conflating binary CLI flags with `devflow-core` semver constraints misrepresents the crate boundary and falsely invokes `cargo semver-checks`.

---

## SUSPECTED

### 10. Flake 999.223 Root Cause Misattributed to Fork Inheritance
* **Severity:** Minor
* **Target:** [.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:45-48](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md#L45-L48) (D-05)
* **Code Citation:**
  - [crates/devflow-cli/tests/pending_supervisor_e2e.rs:429-435](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/tests/pending_supervisor_e2e.rs#L429-L435):
    ```rust
    let output = stop.wait_with_output().expect("wait for devflow stop");
    fixture.assert_stopped(&output, &pending);
    common::wait_for(
        || launcher.try_wait().expect("poll the launcher").is_some(),
        REACH_WITHIN,
        "the launcher to exit",
    );
    ```
  - [crates/devflow-cli/tests/pending_supervisor_e2e.rs:309-313](file:///tmp/claude-1000/-var-home-denniyahh-Github-devflow/e9cd38ce-6af0-4260-abf3-c56e7c22c72a/scratchpad/review-52.1-context/snap/crates/devflow-cli/tests/pending_supervisor_e2e.rs#L309-L313):
    `assert_stopped` asserts `liveness::probe(&pending.liveness_path) == Held::Missing`.
* **Analysis:**
  D-05 presumes 999.223 is caused by another test's child holding open file descriptors across fork.
  In `pending_supervisor_e2e.rs`, `assert_stopped` asserts that the liveness lock file is unlinked (`Held::Missing`) **before** `launcher` has been waited on. `launcher` is a live subprocess spawned by *this test*, which holds file handles and tears down state on exit.
  It is suspected that the flake was an internal race between `stop` exiting and `launcher` exiting/unlinking the liveness file within the test fixture itself, rather than an external fd leak from unrelated tests. Polling `Held::Missing` masks the fact that the test asserted on state cleanup before its own child process terminated.
