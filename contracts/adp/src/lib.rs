//! adp 계약: `host/entrypoints/adp_server` ↔ clients·외부 클라이언트.
//!
//! 상세: `docs/04-adp.md`. CDP와 같은 JSON-RPC 스타일 프로토콜이다 (WebSocket, 인증).
//!
//! - 정의: `protocol.json`. CDP 프로토콜 정의와 같은 구조다: `domains[]`마다 `domain`, `types`,
//!   `commands`(`parameters`, `returns`), `events`(`parameters`), 그리고 `description`, `experimental`,
//!   `deprecated`, `optional`, `$ref`, `enum`, `items`, CDP의 `binary` 타입(base64, RFC 4648 §4, 패딩).
//!   CDP에 없는 확장: 최상위 `errors`(ADP 에러 코드), 최상위 `envelope`(Request·Response·Event·Error
//!   메시지 타입과 바이너리 메시지 `binary`), 커맨드·이벤트의 `scope`(`connection` | `session` | `both`),
//!   `format`(`integer`에 `int32`·`uint32`, 폭이 정해진 경우에만; `string`에 `int64`·`uint64`, 64비트
//!   정수는 JSON 10진 문자열), 태그 유니온의 `discriminator`·`variants`, optional 속성의 `nullable`
//!   (명시적 `null`이 값을 지운다: 없음·`null`·값의 세 상태).
//! - 버전은 `VERSION` 하나가 원천이다. `protocol.json`에 버전을 두지 않는다.
//! - 문서가 형태까지 정한 것만 정의에 있다.
//! - 생성 코드: `build/codegen`(`cargo xtask codegen`)이 `src/generated/`에 만들고 커밋한다.
//!   생성 코드를 손으로 쓰지 않는다. 아래 항목은 모두 생성 코드다 (도메인마다 모듈 하나,
//!   `errors`, `envelope`, `wire`, [`PROTOCOL_VERSION`], [`Scope`], [`Nullable`], [`Command`], [`Event`]).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// 생성 코드에만 적용한다. missing_docs: 정의에 description이 없는 필드·타입이 있다.
#[allow(missing_docs)]
mod generated {
    include!("generated/droiduse.adp.rs");
}

pub use generated::*;
