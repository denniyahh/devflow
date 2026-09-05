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
