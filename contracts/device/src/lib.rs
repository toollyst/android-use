//! device 계약: vmm ↔ devices.
//!
//! 상세: `docs/03-contracts.md` device, `docs/05-vmm-and-devices.md` §4.
//!
//! 기본은 **vhost-user 표준 프로토콜**이다 (unix socket + shm). 디바이스는 vhost-user 백엔드
//! 별도 프로세스로 구현한다. 표준 메시지의 정의는 vhost-user 명세에 있고 이 crate가 다시 정의하지
//! 않는다. 이 crate는 문서가 "확장"으로 적은 것 중 우리가 정의해야 하는 것의 Rust 쪽 타입만 둔다.
//!
//! 문서의 확장 목록과 이 crate의 범위:
//! - device state 전송 (`SET_DEVICE_STATE_FD` 계열): vhost-user 명세의 메시지다. 여기서 타입을
//!   정의하지 않는다.
//! - virtio shared memory region (gfxstream blob resource용): virtio 명세의 개념이다. VM 쪽 설정은
//!   `machine` 계약의 `VhostUserDeviceSpec::shm_regions`. 여기서 타입을 정의하지 않는다.
//! - **scanout export** (gpu 전용): 우리 확장. 아래 타입들.
//!
//! **Android를 모른다.** 이 crate는 `host/vmm`, `host/devices`가 쓰므로 Android 개념을 다루는
//! 식별자·문자열을 두지 않는다 (`LAYERS.toml` `[forbidden]`). 프레임 메타데이터에 게스트 frameId를
//! 넣지 않는다. 게스트 frameId는 guest_link로 오고, `host/runtime/instance`의 `FrameChannel`이
//! virtio 수준 식별자(`ScanoutRef`)로 둘을 **정확히** 매칭한다. 추정 매칭은 하지 않는다.
//!
//! 이 타입들의 전송 형식(어느 채널로, 어떤 바이트 배치로 오가는지)은 미정이다.
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 개념만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// scanout 프레임을 식별하는 virtio 수준 식별자.
///
/// guest_link `Frame.Committed`의 `scanout_ref`와 같은 값이어서 정확 매칭의 키가 된다.
///
/// PLACEHOLDER: 어떤 virtio 수준 값들로 이루어지는지, 동등성 정의는 미정이다.
/// 정확 매칭이 이 타입의 동등성에 달려 있으므로 구조가 정해지기 전에는 `PartialEq`를 주지 않는다.
#[derive(Debug, Clone)]
pub struct ScanoutRef {}

/// 공유 메모리 핸들 (scanout 버퍼).
///
/// PLACEHOLDER: 플랫폼별 표현 미정. 문서는 "shm fd"라고 적었지만 Windows 호스트에는 fd 전달이 없다
/// (HANDLE). 소유권(누가 닫는지)도 미정이다.
#[derive(Debug)]
pub struct ShmHandle {}

/// scanout export: 현재 scanout 버퍼.
#[derive(Debug)]
pub struct ScanoutBuffer {
    /// 현재 scanout 버퍼의 공유 메모리.
    pub shm: ShmHandle,
}

/// scanout export: 프레임 커밋 알림.
#[derive(Debug, Clone)]
pub struct ScanoutCommit {
    /// 커밋된 프레임의 virtio 수준 식별자.
    pub scanout_ref: ScanoutRef,
}
