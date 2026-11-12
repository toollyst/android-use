# testing/

| Directory | Content |
|---|---|
| `testapps/` | Apps for the ADP golden tests: `views/`, `compose/`, `webview/`, `flutter/`, `rn/`, `gl/` |
| `golden/` | Expected tree, bounds, frameId and node ID stability per screen |
| `e2e/` | E2E at the MCP and ADP level. **Added before the implementation** |
| `conformance/` | virtio device conformance |
| `fuzz/` | cargo-fuzz targets |
| `perf/` | Metric measurement and trends |
| `cts/` | CTS/VTS run configuration, result baseline |

Details: `docs/11-quality-and-testing.md`

## e2e

`testing/e2e/run.sh <target>...` builds the runner and runs the scenarios the targets select:
`all`, a milestone (`m0`..`m6`), a tag (`adp`, `mcp`, `trace`, `security`, ...) or a scenario id
prefix (`adp.auth`).

- The inventory is data: `testing/e2e/scenarios.toml`. Each scenario has an id, a title, a
  milestone, the `docs/scope.md` row (or roadmap gate) it covers, tags, what it needs and its
  expected outcome. `docs/scope.md` v1 rows that no scenario can verify are listed under
  `not_e2e` with the reason; the runner rejects a manifest that leaves a v1 row uncovered.
- Expected outcomes are PASS, or RED (error code `UNIMPLEMENTED`) with a named reason:
  `missing:<component>`, `missing:<component>#<surface>` or `<component>#<operation>`.
  Matching is strict: a RED for another reason, an unexpected PASS, any other error, a panic
  or a timeout is FAIL. Nothing is skipped: an unmet need is a RED named after it.
- Components run as separate processes, located only through `DROIDUSE_E2E_<COMPONENT PATH>`
  (`host/entrypoints/adp_server` → `DROIDUSE_E2E_HOST_ENTRYPOINTS_ADP_SERVER`).
- Exit status: 0 = no FAIL, 1 = at least one FAIL, 2 = the run could not start. The report of
  the RED scenarios (the remaining work, with every unmet need) is written to
  `target/e2e/report.json` (`--report <file>` to change it).
