# embedders/ (L6)

외부 노출 형태와 제품 기본값. 코어 로직을 두지 않는다.

| 모듈 | 내용 |
|---|---|
| `adp_server/` | ADP WebSocket JSON-RPC 서버, 인증(`Authenticator`/`Authorizer` SPI) |
| `mcp/` | ADP 위 MCP 어댑터 (observe / act / wait_for …, `docs/04-adp.md` §8) |
| `cli/` | 개발·운영 CLI |
| `node/` | 노드 데몬 + `NodeBuilder`(SPI 구현 주입). 셀프호스팅 겸 SaaS 노드 기반 |
| `desktop/` | 데스크탑 앱 임베딩 API |
| `sdk/` | `rust/`, `ts/`, `python/` ADP 클라이언트 |
