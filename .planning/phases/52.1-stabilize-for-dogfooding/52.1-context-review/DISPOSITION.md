# 52.1-CONTEXT adversarial review — disposition

**Date:** 2026-11-11. **Reviewed:** `52.1-CONTEXT.md` at `a05ebf4`, from a `git archive` snapshot.
**Lanes:**
- codex `gpt-6.1-sol` (high): 7 findings
- agy `gemini-3.8-flash-high` (high): 10 findings
- DeepSeek `deepseek-flash` via `pi`: 17 findings

All three lanes returned output; none dropped.
**Method:** each finding was checked against the cited file and line before it changed the CONTEXT. Accepted findings
are folded into the revised D-02 and D-05..D-17.

## Accepted (CONTEXT changed)

| Finding | Lanes | Checked | Change |
|---|---|---|---|
| A temp file plus `rename` does not prevent `ETXTBSY`: the inode is the same | codex 1, agy 3 | from kernel semantics; `test_support.rs` writes with `fs::write` | D-06: retry the spawn on `ETXTBSY` |
| An `O_CLOEXEC` audit cannot test a fork→exec race | codex 3, agy 4, DeepSeek 4 | `liveness.rs` `in_own_process` comment | D-05: reproduce with a child held between fork and exec |
| nextest isolates tests in processes, so it hides the cross-test race | DeepSeek 1 | 999.221/222/232 failed under `cargo test`, 999.223 under nextest | D-05/D-08: run under the runner that failed; 999.223's cause re-examined |
| `check-in-container.sh` cannot run one test | DeepSeek 2 | `check.sh` targets are `all\|fmt\|clippy\|test\|build\|deps` | D-08: the plan names a repeat harness |
| 999.221's test has three post-release reads; `wait_for_free_write_lock` already exists | DeepSeek 3 | `commands.rs` ~L19060-19077; `workflow.rs` ~L1066 | D-06 |
| 999.232 is not a descriptor race | agy 4 | the test asserts that a pid is not running | D-05 |
| 999.223 may be the test's own ordering | agy 10 (speculation) | not checked; for research | D-05/D-06 |
| D-09 discard-with-name vs D-10 "never delete unmerged" | codex 2 | text of both | D-10: one named exception |
| `start` "continues" is ambiguous; `start` builds fresh state; 52 D-35 says it replaces state | DeepSeek 7, agy 6 | `commands.rs` ~L374; 52-CONTEXT D-35 | D-09.1 defined as a new run on the kept worktree |
| `cleanup` keeps every stopped worktree, so a "kept" result proves nothing | codex 4, DeepSeek 8 | `commands.rs` ~L1227-1231 | D-09.5, D-16: check effects, with a merged control |
| `cleanup`/`discard` can race `start` | codex 5 (speculation) | not reproduced | D-09.2: hold the phase holder across the check and the removal |
| `discard` needs liveness and checked-out-branch handling | agy 7 | 999.224 notes the `branch -D` hint failure | D-09.2 |
| The message list for `--force` was incomplete | DeepSeek 6 | `kept_stopped_worktree_line`, `phase_worktree.rs` messages | D-09, Integration Points |
| Removing a clap flag cannot name the new verb | DeepSeek 9 | `main.rs` clap bools | D-11: a hidden refusing `--force` |
| Divergence script reads `HEAD` and the index, not the pushed commit; delete SHA; ref-name fail-open; drop-in stdin | codex 6, agy 5, DeepSeek 5 | `check-workspace-divergence.sh` L79/L87/L99; `pre-push` L85, L208 | D-13 rewritten |
| The 999.167 pairing is a scope leak; `SCOPE_RE` cannot match `(52.1-review)` | agy 8, DeepSeek 11 | `cut-pr-branch.sh` L134 | D-12: `--include` + listing, 999.167 not folded |
| `pre-push` already has tests | DeepSeek 10 | `pre_push_signing_policy.rs`, `script_modes.rs` exist | D-14 |
| The default scaffold lacks the chain-flag key, so Validate reads a dirty tree | codex 7, agy 1 | `scratch-dogfood-repo.sh` ~L1251; Plan prompt runs `/gsd-plan-phase N` without `--auto`; the guard restores only its own write (`pipeline_launch.rs` ~L774) | D-15: fixture commits the key; dirty-config control |
| The fixture mode is not pinned; `--live-payload` has a `blocking-human` checkpoint; the command omitted `--phase` | agy 2, DeepSeek 13 | `scratch-dogfood-repo.sh` ~L915 | D-15 |
| No evidence source for "no hand steps"; a later commit voids the proof | DeepSeek 15, 16 | — | D-15 evidence list; D-17 re-run rule |
| ROADMAP still places 999.219 on a separate branch | DeepSeek 12 | ROADMAP 999.219 disposition | ROADMAP updated (operator accepted, 2026-11-11) |
| `stop` is untested for `Stale`/`ProbeFailed` | DeepSeek 14 (speculation) | — | D-09.3 adds tests |
| The listing logic could drift from the filter | DeepSeek 17 (speculation) | `cut-pr-branch.sh` computes it inside the loop | D-12: one function, agreement test |

## Rejected

| Finding | Lane | Why |
|---|---|---|
| A missing `_auto_chain_active` does not dirty the tree, because `AutoChainGuard` restores the bytes | DeepSeek (method note) | The guard restores only **its own** write. GSD's `/gsd-plan-phase` (run without `--auto`) writes the flag itself during the stage. That counts as "the file changed during the stage", so the guard keeps it, as 52-CONTEXT D-46's amendment records. |
| `devflow-cli` is not published, so the semver reasoning is false | agy 9 | `scripts/cut-release.sh` runs `cargo publish -p devflow`. The wording was still imprecise: the CLI break is the `devflow` crate's, not `devflow-core`'s. D-09 was reworded. |
| No `VALIDATION.md` routes Validate to re-verify Code | agy 2.1 | Unverified, not rejected outright. D-15 item 3 makes the research step establish what a v4 Validate needs. |

## Not established by this review

- None of the lanes ran code; every finding is read from source.
- The ordering cause of 999.223 and the `VALIDATION.md` question are open for research.
