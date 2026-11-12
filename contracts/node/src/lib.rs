//! node 계약: control plane ↔ node 데몬.
//!
//! 상세: `docs/03-contracts.md` node. 형식은 Protocol Buffers, 전송은 gRPC over TLS(인증)다.
//!
//! - IDL: `proto/droiduse/node/node.proto` (proto 루트 `proto/`, 패키지 `droiduse.node`)
//! - 생성 코드: `build/codegen`(`cargo xtask codegen`)이 `src/generated/`에 메시지 타입만 만들고 커밋한다.
//!   gRPC 서비스 바인딩을 이 crate에 둘지는 미정이다.
//!   생성기가 생기기 전까지 이 crate에는 생성 코드가 없다. 생성 코드를 손으로 쓰지 않는다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
