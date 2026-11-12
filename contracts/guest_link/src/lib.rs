//! guest_link 계약: 호스트(runtime·services) ↔ 게스트(adpd) vsock 프로토콜.
//!
//! 상세: `docs/03-contracts.md` guest_link. 형식은 Protocol Buffers다.
//!
//! - IDL: `proto/droiduse/guest_link/guest_link.proto` (proto 루트 `proto/`, 패키지 `droiduse.guest_link`)
//! - 생성 코드: `build/codegen`(`cargo xtask codegen`)이 `src/generated/`에 만들고 커밋한다.
//!   생성기가 생기기 전까지 이 crate에는 생성 코드가 없다. 생성 코드를 손으로 쓰지 않는다.
//!
//! 이 계약은 게스트를 향하므로 Android 개념을 다룰 수 있다. `host/vmm`, `host/devices`는 이 crate에
//! 의존하지 않는다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
