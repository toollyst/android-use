//! node 계약: control plane ↔ node 데몬.
//!
//! 상세: `docs/03-contracts.md` node. 형식은 Protocol Buffers, 전송은 gRPC over TLS(인증)다.
//!
//! - IDL: `proto/droiduse/node/node.proto` (proto 루트 `proto/`, 패키지 `droiduse.node`)
//! - 생성 코드: `build/codegen`(`cargo xtask codegen`)이 `src/generated/`에 메시지 타입만 만들고 커밋한다.
//!   서비스 바인딩(tonic 등)은 이 계약을 쓰는 레이어에서 생성한다. 생성 코드를 손으로 쓰지 않는다.
//! - gRPC 메타데이터 키는 proto가 상수를 만들지 않으므로 아래에 손으로 둔다 (proto 파일 머리 주석과 같다).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// 생성 코드에만 적용한다.
mod generated {
    include!("generated/droiduse.node.rs");
}

pub use generated::*;

/// gRPC 메타데이터 키: W3C Trace Context `traceparent`. 모든 호출에 실을 수 있다.
pub const TRACEPARENT_METADATA_KEY: &str = "traceparent";

/// gRPC 메타데이터 키: W3C Trace Context `tracestate`. 모든 호출에 실을 수 있다.
pub const TRACESTATE_METADATA_KEY: &str = "tracestate";
