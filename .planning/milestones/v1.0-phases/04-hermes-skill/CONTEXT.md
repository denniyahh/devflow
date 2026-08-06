# Phase 4: Hermes Skill

## Goal
Hermes currently drives devflow via raw `terminal()` calls. Create a proper skill so Hermes auto-detects devflow projects and provides structured commands.

## Tasks

### 4a — Create skill file
- [ ] Create `skills/hermes/devflow/SKILL.md`
- [ ] Follow standard Hermes skill format (YAML frontmatter + markdown body)
- [ ] Document: what devflow is, when to use the skill, all commands
- [ ] Document project detection: look for `.devflow.yaml` in repo root
- [ ] File: `skills/hermes/devflow/SKILL.md`

### 4b — Skill content
The skill should instruct Hermes to:
- [ ] Detect `.devflow.yaml` → project uses devflow
- [ ] `devflow start --phase N --agent <agent>` → start a phase
- [ ] `devflow status` → check current state
- [ ] `devflow check` → poll and advance if agent done
- [ ] Report phase transitions: "Phase 3 started (Claude, monitor PID 12345)"
- [ ] Report completion: "Phase 3 complete — X commits, Y files changed"

### 4c — Testing
- [ ] Create `skills/hermes/devflow/test.sh` — validation script
- [ ] Verify skill file is valid YAML
- [ ] Verify all commands work against installed devflow binary
- [ ] File: `skills/hermes/devflow/test.sh`

## Verification
```bash
# Skill file is valid
hermes skill validate skills/hermes/devflow/SKILL.md

# Devflow binary is functional
devflow status
devflow --version
```

## Success
Hermes detects devflow projects automatically and uses `devflow start/status/check` instead of raw terminal calls.
