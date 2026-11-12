# clients/

ADP clients. They depend only on `contracts/adp` and know nothing of host internals (`host/*`). They attach the same way to a local node and to remote SaaS.

| Module | Content |
|---|---|
| `mcp/` | MCP adapter on top of ADP (observe / act / wait_for …, `docs/04-adp.md` §8) |
| `sdk/` | `rust/`, `ts/`, `python/` ADP client SDKs |

**Debug-only adb tools.** `mcp/` built with the cargo feature `debug` (off by default) adds
`debug_screenshot` and `debug_tap`, which control a device through `adb` (`--adb-path`,
`--adb-serial`, `--adb-timeout`). They exist only for bring-up before the ADP path reaches a
device, are never part of a default or release build, and say so in a warning at startup
(`CLAUDE.md` §1.2). Build: `cargo build -p droiduse-mcp --features debug`.
