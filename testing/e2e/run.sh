#!/usr/bin/env bash
# E2E entry point (CLAUDE.md §6, docs/11-quality-and-testing.md §7). Builds the runner and the
# component binaries that exist, and runs the scenarios a target selects.
#
# usage: testing/e2e/run.sh [--root <dir>] [--manifest <file>] [--report <file>] <target>...
#        target: all | m0..m6 | <tag> | <scenario id prefix>
#        Components are located through DROIDUSE_E2E_<COMPONENT PATH> environment variables
#        (host/entrypoints/adp_server -> DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER). A variable
#        that is already set is kept; otherwise it points at the binary built here.
# exit:  0 = no FAIL, 1 = at least one FAIL, 2 = the run could not start
#        (including cargo failing to build, or Cargo.lock being out of date)
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 2

cargo build --locked --quiet \
  --package droiduse-testing-e2e --bin e2e \
  --package droiduse-entrypoints-adp-server --bin droiduse-adp-server \
  --package droiduse-mcp --bin droiduse-mcp || exit 2

bin="${CARGO_TARGET_DIR:-${CARGO_BUILD_TARGET_DIR:-target}}/debug"
exe=""
case "${OS:-}" in Windows_NT) exe=".exe" ;; esac

: "${DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER:=$bin/droiduse-adp-server$exe}"
: "${DROIDUSE_E2E_CLIENTS_MCP:=$bin/droiduse-mcp$exe}"
export DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER DROIDUSE_E2E_CLIENTS_MCP

"$bin/e2e$exe" "$@"
status=$?
case "$status" in
  0 | 1 | 2) exit "$status" ;;
  *) exit 2 ;;
esac
