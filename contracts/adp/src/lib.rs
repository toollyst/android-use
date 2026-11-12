//! adp 계약: `host/entrypoints/adp_server` ↔ clients·외부 클라이언트.
//!
//! 상세: `docs/04-adp.md`. CDP와 같은 JSON-RPC 스타일 프로토콜이다 (WebSocket, 인증).
//!
//! - 정의: `protocol.json`. CDP 프로토콜 정의와 같은 구조다: `domains[]`마다 `domain`, `types`,
//!   `commands`(`parameters`, `returns`), `events`(`parameters`), 그리고 `description`, `experimental`,
//!   `deprecated`, `optional`, `$ref`, `enum`, `items`.
//!   CDP에 없는 확장은 두 가지다: 최상위 `errors`(ADP 에러 코드), 정수 속성의 `format`
//!   (`uint32`, `int64`, `uint64`; 문서가 폭을 정한 경우에만).
//! - 버전은 `VERSION` 하나가 원천이다. `protocol.json`에 버전을 두지 않는다.
//! - 문서가 형태까지 정한 것만 정의에 있다. 이름만 정해진 커맨드·이벤트와 메시지 봉투는 넣지 않았다.
//! - 생성 코드: `build/codegen`(`cargo xtask codegen`)이 `src/generated/`에 만들고 커밋한다.
//!   생성기가 생기기 전까지 이 crate에는 생성 코드가 없다. 생성 코드를 손으로 쓰지 않는다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
