#!/usr/bin/env bash
# Patch lint: third_party/<x>/patches/ (docs/08-upstream-and-patches.md §2, CLAUDE.md §5).
# Checks the header template, a droiduse: marker on every hunk and the Budget: line count.
# Thin entry point: the rules live in build/patchlint (cargo xtask patch-lint).
#
# usage: tools/patch/lint.sh [--root <dir>]   (a relative <dir> is taken from the repo root)
# exit:  0 = clean, 1 = violations found, 2 = the lint could not run
#        (including cargo failing to build it, or Cargo.lock being out of date)
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 2
cargo --locked xtask patch-lint "$@"
status=$?
case "$status" in
  0 | 1 | 2) exit "$status" ;;
  *) exit 2 ;;
esac
