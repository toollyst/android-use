//! guest_link 계약: 호스트(runtime·services) ↔ 게스트(adpd) vsock 프로토콜.
//!
//! 상세: `docs/03-contracts.md` guest_link. 형식은 Protocol Buffers, 전송은 gRPC over vsock이다.
//!
//! - IDL: `proto/droiduse/guest_link/guest_link.proto` (proto 루트 `proto/`, 패키지 `droiduse.guest_link`)
//! - 생성 코드: `build/codegen`(`cargo xtask codegen`)이 `src/generated/`에 메시지 타입만 만들고 커밋한다.
//!   서비스 바인딩(tonic 등)은 이 계약을 쓰는 레이어에서 생성한다. 생성 코드를 손으로 쓰지 않는다.
//! - gRPC 메타데이터 키는 proto가 상수를 만들지 않으므로 아래에 손으로 둔다 (proto 파일 머리 주석과 같다).
//!
//! 이 계약은 게스트를 향하므로 Android 개념을 다룰 수 있다. `host/vmm`, `host/devices`는 이 crate에
//! 의존하지 않는다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// 생성 코드에만 적용한다.
mod generated {
    include!("generated/droiduse.guest_link.rs");
}

pub use generated::*;

/// gRPC 메타데이터 키: W3C Trace Context `traceparent`. 모든 호출에 실을 수 있다.
pub const TRACEPARENT_METADATA_KEY: &str = "traceparent";

/// gRPC 메타데이터 키: W3C Trace Context `tracestate`. 모든 호출에 실을 수 있다.
pub const TRACESTATE_METADATA_KEY: &str = "tracestate";

/// gRPC 메타데이터 키: 세션 토큰 (불투명 비밀, 바이너리 메타데이터). `HelloService.Hello`와
/// `AuthService.Auth`를 뺀 모든 RPC가 양방향 모두 싣는다.
pub const SESSION_TOKEN_METADATA_KEY: &str = "droiduse-session-token-bin";
