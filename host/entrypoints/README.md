# host/entrypoints/ (L6)

호스트에서 runtime과 services를 묶어 실제로 실행되는 진입점과 제품 기본값. 코어 로직을 두지 않는다.

| 모듈 | 내용 |
|---|---|
| `adp_server/` | ADP WebSocket JSON-RPC 서버, 인증(`Authenticator`/`Authorizer` SPI) |
| `node/` | 노드 데몬 + `NodeBuilder`(SPI 구현 주입). 셀프호스팅 겸 SaaS 노드 기반 |
| `cli/` | 개발·운영 CLI |
| `desktop/` | 데스크탑 앱 임베딩 API |

ADP 클라이언트(SDK, MCP)는 여기가 아니라 `clients/`에 있다.
