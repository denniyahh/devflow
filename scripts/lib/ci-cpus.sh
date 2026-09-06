#!/usr/bin/env bash
# The single definition site for the CPU pin CI and the local gate share.
#
# Two consumers read this file: scripts/check-in-container.sh (the pre-push
# gate, which pins the container) and .github/workflows/ci.yml's
# `Sequential 2-CPU check` job (which pins the suite via taskset). Re-typing
# the value into either one is exactly the drift this file prevents — the
# local gate would then measure a different load shape than CI while both
# looked green, which is the class the container image tag already needed
# scripts/assert-image-parity.sh for. A "keep in sync" comment is not a
# mechanism; cpu_pin_has_exactly_one_definition_site in
# crates/devflow-cli/tests/ci_parity_guards.rs is.
#
# This file is SOURCED, never executed, which is why it deliberately omits
# `set -euo pipefail` despite the repo's shell-header house style — those
# options would leak into both callers' shells.
#
# Override with DEVFLOW_CI_CPUS=all to use every core (faster, less faithful).
CPUS="${DEVFLOW_CI_CPUS:-0,1}"

# cpu_pin_prefix — the ONE place the `all` special-case is decided.
#
# Sets the global array CPU_PIN to the argv PREFIX a caller puts in front of
# the command it wants pinned: `(taskset -c "$CPUS")` normally, and an EMPTY
# array when $CPUS is the literal `all`.
#
# It BUILDS an array rather than running the command, and that is a hard
# constraint rather than a style choice. scripts/check-in-container.sh needs
# the value as an argv prefix INSIDE a `docker run ... "${CPU_PIN[@]}"
# scripts/check.sh "$TARGET"`, where a function defined in the host shell does
# not exist. A `run_pinned "$@"` wrapper could not be used there at all.
#
# Before this existed the `all` case was special-cased only in the local gate,
# so DEVFLOW_CI_CPUS=all was absorbed locally and killed CI with
# `taskset: failed to parse CPU list: all` (measured; exit 1) — a parity break
# inside the very pair of files whose only purpose is parity
# (46-REVIEWS.md C-06). Three consumers now share this one implementation: the
# local gate, and BOTH pin sites in .github/workflows/ci.yml. Copying the
# conditional into either caller re-opens C-06 in the same shape D-03 already
# closed for the CPU list itself.
#
# A shell function cannot cross a GitHub Actions step boundary, so each CI step
# that needs it sources this file again. That re-source is this single
# definition site being READ twice, not a second definition.
cpu_pin_prefix() {
    if [ "$CPUS" = "all" ]; then
        CPU_PIN=()
    else
        CPU_PIN=(taskset -c "$CPUS")
    fi
}
