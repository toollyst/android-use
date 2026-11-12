# clients/

ADP 클라이언트. `contracts/adp`에만 의존하고 호스트 내부(`host/*`)를 모른다. 로컬 노드와 원격 SaaS에 똑같이 붙는다.

| 모듈 | 내용 |
|---|---|
| `mcp/` | ADP 위 MCP 어댑터 (observe / act / wait_for …, `docs/04-adp.md` §8) |
| `sdk/` | `rust/`, `ts/`, `python/` ADP 클라이언트 SDK |
