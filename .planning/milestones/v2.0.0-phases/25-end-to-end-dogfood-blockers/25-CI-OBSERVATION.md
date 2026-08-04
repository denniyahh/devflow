---
phase: 25-end-to-end-dogfood-blockers
unit: 25e
backlog: 999.47 / DEN-72
observed: 2026-07-28T05:10:00Z
head_sha: 4f65cdb7c5736b19bbf4949b4212159275af7119
run_id: null
trials: 0
image: mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm
status: reproduced
---

# 25e / 999.47 — CI observation record

**Outcome: `999.47 reproduced`. Truth 7 is NOT closed. Plan 25-10 halted at Task 1 Step E.**

No CI trials were run. The plan's Step A gate — `git push origin feature/phase-25`, whose
`pre-push` hook runs `scripts/check-in-container.sh all` inside the pinned image — rejected
the push **twice out of two attempts**, before any commit reached `origin`. The race the
trials were meant to observe reproduced locally, inside the same pinned container CI uses,
so the trials became unnecessary: the question they were designed to answer is already
answered, negatively.

`origin/feature/phase-25` remains at `a5a068f`. Nothing was pushed.

## What failed

```
---- commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling stdout ----
thread 'commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling' (1480)
  panicked at crates/devflow-cli/src/commands.rs:3678:9:
the fixture must be part of the real discovery census gate_sweep would use

failures:
    commands::tests::gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling

test result: FAILED. 218 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.43s
error: test failed, to rerun pass `-p devflow --bin devflow`
```

## Classification: `999.47 reproduced`

Task 1 Step E defines `999.47 reproduced` as a failure "in either of the two named
retargeted tests, **or in the `stop`/identity/`/proc` area**." This failure is not in
either retargeted test, but it is squarely in the `/proc` identity area, and it is the
*same defect mechanism* 999.47 names.

The failing assertion (`commands.rs:3669-3682`):

```rust
let mut child = std::process::Command::new("sh")
    .arg("-c")
    .arg("trap cleanup TERM INT; sleep 30")
    .spawn()
    .expect("spawn monitor-wrapper-shaped fixture");
let pid = child.id();
let dir = tempfile::tempdir().unwrap();

assert!(
    agent::discover_stray_devflow_processes()
        .iter()
        .any(|p| p.pid == pid),
    "the fixture must be part of the real discovery census gate_sweep would use"
);
```

`discover_stray_devflow_processes()` (`agent.rs:308-345`) identifies candidates by reading
`/proc/{pid}/cmdline` and classifying the resulting argv via `classify_stray_layer`.

`Command::spawn()` returns once `fork()` has completed — **not** once the child has finished
`execve()`. Inside that window the child's `/proc/<pid>/cmdline` still holds the *parent's*
argv (the test binary), not `sh -c "trap cleanup TERM INT; sleep 30"`. `classify_stray_layer`
therefore does not recognise it, the fixture pid is absent from the census, and the assertion
fails. That is the cmdline-inheritance race — 999.47's confirmed mechanism — verbatim.

## Why the phase's "structurally removed" claim does not hold

`25-VERIFICATION.md` records truth 7 as `PRESENT_BEHAVIOR_UNVERIFIED` on the grounds that
the race is *structurally* removed: `looks_like_devflow_process` is `#[deprecated]` and both
historically-flaky tests were retargeted onto `lock::holder_identity`'s `(pid, starttime)`
guard, "with no `spawn()` and therefore no `execve` window."

That is true of those two tests. It is not true of the codebase. The same
`spawn()`-then-immediately-read-the-`/proc`-census shape survives at five other sites:

| File | Line | Test |
|------|------|------|
| `crates/devflow-cli/src/commands.rs` | 3669 | `gate_sweep_reap_strays_dry_run_discovers_a_real_stray_without_signalling` **(observed failing)** |
| `crates/devflow-cli/src/commands.rs` | 3706 | (same fixture shape) |
| `crates/devflow-core/src/agent.rs` | 631 | `discover_stray_devflow_processes_finds_a_monitor_wrapper` |
| `crates/devflow-core/src/agent.rs` | 664 | (same fixture shape) |
| `crates/devflow-core/src/agent.rs` | 687 | (same fixture shape) |

The two flaky tests were fixed. The defect class was not.

## Reproduction recipe

Deterministic enough to act on — 2 failures in 2 attempts via the push path:

```bash
git push origin feature/phase-25     # pre-push -> scripts/check-in-container.sh all
```

It does **not** reproduce under a warm standalone container run. Observed counts at
`head_sha` `4f65cdb`:

| Shape | Runs | Failures |
|-------|------|----------|
| `git push` → `check-in-container.sh all` (fmt+clippy, then test) | 2 | **2** |
| `check-in-container.sh test`, warm volume | 1 | 0 |
| `cargo test --workspace --no-fail-fast` in pinned container, warm, 2-core pinned | 4 | 0 |
| `cargo test -p devflow --bin devflow` in pinned container, warm, 2-core pinned | 12 | 0 |

The distinguishing factor is load, not the image: the failing shape runs `cargo fmt` and a
full `cargo clippy --workspace --all-targets` immediately before the test phase, on
`taskset -c 0,1`. The extra compile pressure widens the `fork()`→`execve()` window enough
for the race to land. This is consistent with 999.47's original signature — host-green,
CI-red — and with `check-in-container.sh`'s 2-core pinning rationale ("a 4-core host hides
races that CI sees").

## Trials

None. The push gate never passed, so no commit reached `origin` and no CI run exists for
this head SHA. A trials table would be fabrication.

| trial | run id | attempt | head sha | Test job | conclusion | url |
|-------|--------|---------|----------|----------|------------|-----|
| — | — | — | — | — | — | — |

## Discarded runs

none — no CI run was ever started.

## Retargeted test execution proof

Not obtainable. Step D requires extracting the two retargeted tests' log lines from a CI
`Test` job; no CI job ran. For the record, both tests passed in every local container run
performed above — but that is not the evidence Step D asks for, and it is not offered as a
substitute.

## Limits of this evidence

- This is a **positive** reproduction, so it does not need the five-trial argument. One
  reproduction falsifies "closed"; the streak was only ever needed to argue the negative.
- The reproduction is load-sensitive, not deterministic. It reproduced 2/2 via the push
  path and 0/17 via warm standalone runs. Treat "green" from a warm local run as
  uninformative for this defect class.
- The failing test is a *test-side* race: the fixture is not observable at the instant it
  is looked for. Whether the same window is reachable in production `gate_sweep` /
  `reap-strays` operation against a real monitor wrapper is **not** established here and
  should not be assumed either way. What is established is that truth 7's premise — no
  remaining `spawn()`/`execve` window in this area — is false.
- No source file was changed by this plan. `git status --porcelain crates/` is empty.
