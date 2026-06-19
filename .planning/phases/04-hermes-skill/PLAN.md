# Phase 4: Hermes Skill

> Parent: ROADMAP.md | Status: In Progress (2026-06-18)

## Goal

Ship a production-quality Hermes skill that auto-detects DevFlow projects, provides structured commands, and handles the v0.5.0 architecture (direct process spawning, PID-based monitoring).

## Context

- **Repo skill exists:** `skills/hermes/devflow/SKILL.md` (62 lines, tmux-era)
- **Installed skill:** `~/.hermes/skills/software-development/devflow/SKILL.md` (already updated for v0.5.0)
- **Gap:** Repo skill is stale. Need to sync from installed version + add test script.
- **Reference:** Installed skill at `~/.hermes/skills/software-development/devflow/`

## Tasks

### 4a — Sync repo skill with v0.5.0 reality
- [ ] Update `skills/hermes/devflow/SKILL.md` to match installed version
  - Direct process spawning (no tmux)
  - PID-based monitoring
  - Rich prompt generation
  - Manual launch workaround for Claude (the `--monitor` pitfall)
  - Dogfooding patterns
  - All pitfalls from real usage
  - References to linked files
- [ ] Ensure YAML frontmatter is valid

### 4b — Create test script
- [ ] Create `skills/hermes/devflow/test.sh`
  - Validate SKILL.md YAML frontmatter
  - Verify devflow binary is functional
  - Check all documented commands work
- [ ] Test script passes

### 4c — Verify integration
- [ ] Hermes can detect `.devflow.yaml` projects
- [ ] `devflow status` works from any project
- [ ] Skill loads cleanly via `skill_view`

## Verification

```bash
# Skill file valid YAML
head -15 skills/hermes/devflow/SKILL.md | grep -E "^(---|name:|description:|category:|triggers:)"

# Devflow binary functional
devflow status
devflow --version

# Test script passes
bash skills/hermes/devflow/test.sh
```

## Success

Repo skill matches installed version. Test script validates skill + binary. Hermes auto-detects devflow projects.
