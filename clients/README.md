# clients/

ADP clients. They depend only on `contracts/adp` and know nothing of host internals (`host/*`). They attach the same way to a local node and to remote SaaS.

| Module | Content |
|---|---|
| `mcp/` | MCP adapter on top of ADP (observe / act / wait_for …, `docs/04-adp.md` §8) |
| `sdk/` | `rust/`, `ts/`, `python/` ADP client SDKs |
