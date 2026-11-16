# Graphify review triage: PR #237 (Phase 52.1, plans 01 to 07)

Head reviewed: `2180823fe36e1b010fc715d9a2a6dc655c36c7f9`, the head that merged (merge commit `c13178a`). The bot
reviewed once, on that head, and marked the review partial (the diff was larger than one pass covers).

Verdict line: "No issues found by static checks". No finding was marked "Worth a look".

| Finding | Count | Ruling | Reason |
|---|---|---|---|
| Health regression, efferent coupling, on test functions in `auto_chain_leak_repair_e2e.rs`, `cut_pr_branch_e2e.rs`, `phase7_cli.rs` | 17 inline | dismissed | each is a test calling six or seven fixture helpers |
| Health regression, afferent coupling, on test helpers `run_discard()` (32 callers) and `add_bystanders()` (7) | 2 inline | dismissed | shared test helpers; the callers are the discard tests |
| Health regression, `discard()` fans out to 9 callees (`commands.rs`) | 1 inline | dismissed | the command's top-level sequence of checks; moot in part once Phase 52.2 removes the liveness arms |
| Risky pattern `no-stderr-to-stdout` in `scripts/scratch-dogfood-repo.sh` (5 lines) | advisory | dismissed | `jq -e ... >/dev/null 2>&1` used as a condition, and one self-test capture |
| Docs that may be stale (`ARCHITECTURE.md`, `CHANGELOG.md`, 20 sections) | advisory | dismissed | matched on the words `state`, `git` and `push`, not on changed behaviour |
| Formal verification: 0 of 10 functions verified | advisory | no action | the tool reports each as unsupported |

Not read: 5 findings on lines outside the diff and 16 the bot did not show (its cap).

CI on the same head: all checks passed (`gh pr checks 237`).
