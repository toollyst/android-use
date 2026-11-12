//! keymint 계약: 게스트 KeyMint HAL ↔ `host/runtime/keymint`.
//!
//! 상세: `docs/03-contracts.md` keymint. 전용 vsock 포트를 쓴다.
//!
//! - 페이로드는 고정 AOSP 태그의 kmr wire 형식 그대로다. 그 정의는 upstream에 있고 여기서 다시 정의하지
//!   않는다.
//! - 우리가 정의하는 것은 프레이밍과 인증뿐이다. restore 후 재인증은 guest_link와 같다.
//!
//! 프레이밍(길이·종류 표시, 크기 제한)과 인증 메시지의 형태는 아직 문서에 없다. 그래서 이 crate는
//! 비어 있다. 형태가 정해지기 전에 타입을 만들지 않는다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
