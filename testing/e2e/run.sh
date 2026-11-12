#!/usr/bin/env bash
# E2E entry point (CLAUDE.md §6, docs/11-quality-and-testing.md §7). Builds the runner and
# runs the scenarios a target selects. Thin entry point: the runner is testing/e2e (bin e2e).
#
# usage: testing/e2e/run.sh [--root <dir>] [--manifest <file>] [--report <file>] <target>...
#        target: all | m0..m6 | <tag> | <scenario id prefix>
#        Components are located through DROIDUSE_E2E_<COMPONENT PATH> environment variables
#        (host/entrypoints/adp_server -> DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER).
# exit:  0 = no FAIL, 1 = at least one FAIL, 2 = the run could not start
#        (including cargo failing to build the runner, or Cargo.lock being out of date)
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 2
cargo run --locked --quiet --package droiduse-testing-e2e --bin e2e -- "$@"
status=$?
case "$status" in
  0 | 1 | 2) exit "$status" ;;
  *) exit 2 ;;
esac
