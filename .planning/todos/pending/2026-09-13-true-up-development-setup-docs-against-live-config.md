---
created: 2026-09-13T18:37:15.901Z
title: True up development setup docs against live config
area: docs
severity: minor
files:
  - .planning/user/DEV-SETUP-CHECKLIST.md
  - CONTRIBUTING.md
  - scripts/hooks/post-commit
---

## Problem

`.planning/user/DEV-SETUP-CHECKLIST.md` exists so this repo's development setup can be replicated elsewhere. It drifts silently, and parts of it cannot be applied as written. Found 2026-09-13:

- **Two entries were factually wrong, now fixed in `a6524bf`.**
  - It named the maintainer's key (`github_ed25519`) as the agent's signing key. Live config is the reverse: the agent key is `devflow_signing_ed25519`, set in `.git/config`.
  - It listed `admin:ssh_signing_key` among the `gh` token scopes. The token has `repo`, `workflow`, `gist` and `read:org`.
- **§2 "GitHub repository settings" describes the wrong mechanism.** It covers classic branch protection and says `develop` has "no required status checks or review count set". The rules that actually apply are repository rulesets:
  - `develop-merge-or-squash` (id 19616771) requires a PR, 0 approvals, and the checks `Test`, `Clippy`, `Format` and `Build + test in devcontainer`, with a strict up-to-date policy and no bypass actors (verified 2026-09-13).
  - `main-squash-only` (id 19616766) has pull-request and required-status-check rules with 0 approvals. Its check list was not re-read.
  - Both rulesets were created and last edited 2026-07-23 by `denniyahh`. The classic API reports `main` required reviews 0 and `develop` none.
- **Only visible on the personal branch.** The checklist lives in `.planning/user/`, which only `workspace/denniyahh` tracks, so no one cloning the public repo can see it. `CONTRIBUTING.md` also differs between workspace and develop (41 changed lines).
- **Drift is only warned about.** `scripts/hooks/post-commit` warns when a setup file changes without the checklist; nothing asserts the checklist against reality.
- **Tags mixed with machine values.** Entries mix `[GLOBAL]` / `[PROJECT]` / `[PATTERN]` tags with machine-specific values (paths, key files, account names), so a reader cannot tell what to apply on a new machine.

## Solution

TBD. Starting points:

- **Re-verify every entry against live state:**
  - `gh api repos/denniyahh/devflow/rulesets` and each ruleset id;
  - `git config --show-origin --get-regexp`;
  - `gh auth status`;
  - `scripts/hooks/*`;
  - the jobs in `.github/workflows/*.yml`;
  - the devcontainer pins.
- **Split by audience:** public, replicable steps go in `CONTRIBUTING.md` or `docs/`; machine-specific notes stay separate.
- **Automate the checkable entries:** consider `scripts/check-dev-setup.sh`, so drift fails loudly instead of depending on the post-commit warning.
- **Decide D1 first.** The retrospective's open decision D1 (workspace branch structure) determines where the public half belongs: `.planning/audits/2026-09-13-phase-47-retrospective.md`.
