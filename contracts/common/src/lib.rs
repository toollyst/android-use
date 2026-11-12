//! 계약 공통 타입.
//!
//! `contracts/`는 자기완결이다 (`contracts/LAYER.toml` `allow = []`). 여러 계약이 함께 쓰는
//! 타입(에러 코드, trace ID, labels, frameId 등)은 다른 레이어가 아니라 이 crate에 둔다.
//! 상세: `docs/03-contracts.md` 공통 규칙.
//!
//! 이 crate는 `machine`·`device` 계약을 통해 `host/vmm`, `host/devices`에서도 보인다.
//! 따라서 여기에는 Android 개념을 다루는 식별자를 두지 않는다 (`LAYERS.toml` `[forbidden]`).
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정했거나 표현을 정하지 않은 것이다.
//! 구조와 의미는 프로젝트 오너가 정한다. 임의로 채우지 않는다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;
use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// 에러
// ---------------------------------------------------------------------------

/// 계약 전반의 에러 코드.
///
/// 현재 값은 `docs/04-adp.md` §5 "에러 코드" 목록 그대로다.
/// `FrameChannel::get`의 `FrameExpired`(`docs/03-contracts.md` instance)도 이 목록의 값이다.
/// 추가는 하위호환, 제거·의미 변경은 major 증가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// 요청한 frameId의 프레임이 링 버퍼에 없다. 다른 프레임으로 대체하지 않는다. (`FRAME_EXPIRED`)
    FrameExpired,
    /// 대상 노드가 사라졌다. (`NODE_GONE`)
    NodeGone,
    /// 대상 타깃이 사라졌다. (`TARGET_GONE`)
    TargetGone,
    /// 대상에 액션을 수행할 수 없다. (`NOT_ACTIONABLE`)
    NotActionable,
    /// 시간 초과. (`TIMEOUT`)
    Timeout,
    /// 정책이 거부했다. (`POLICY_DENIED`)
    PolicyDenied,
    /// 인증되지 않았다. (`UNAUTHENTICATED`)
    Unauthenticated,
    /// 협상할 수 없는 프로토콜 버전이다. (`UNSUPPORTED_VERSION`)
    UnsupportedVersion,
}

/// 계약 trait들이 돌려주는 에러.
///
/// PLACEHOLDER: 문서는 trait 시그니처에 `Result<T>`만 쓰고 에러 타입을 정의하지 않았다.
/// 문서에 있는 에러 코드는 ADP 목록뿐이라 VM·디바이스·저장소 실패처럼 목록에 없는 실패가 있다.
/// 그런 실패는 `code: None`으로 둔다. 에러 모델(코드 체계, 원인 체인, 전송 시 매핑)은 미정이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// 문서화된 에러 코드에 해당하면 그 코드. 해당하는 코드가 없으면 `None`.
    pub code: Option<ErrorCode>,
    /// 사람이 읽는 설명.
    pub message: String,
}

/// 계약 trait들의 결과 타입.
pub type Result<T, E = Error> = std::result::Result<T, E>;

// ---------------------------------------------------------------------------
// 트레이스
// ---------------------------------------------------------------------------

/// 레이어를 관통하는 단일 트레이스의 ID (`docs/02-architecture.md` §5).
///
/// ADP 요청에서 시작해 guest_link 메시지 헤더로 전파된다. 모든 메시지에 실을 수 있다.
///
/// PLACEHOLDER: 표현(길이, 형식, Perfetto·OTel과의 대응)은 미정이다.
/// 문자열은 "불투명 값"이라는 뜻일 뿐 표현을 정한 것이 아니다.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraceId(pub String);

// ---------------------------------------------------------------------------
// labels
// ---------------------------------------------------------------------------

/// 인스턴스·세션·스냅샷의 불투명 key-value (`docs/glossary.md` labels).
///
/// 코어는 내용을 해석하지 않는다. 테넌트·플랜 같은 개념은 코어가 아니라
/// labels와 SPI 구현체 쪽에 있다 (ADR-0007).
///
/// `BTreeMap`은 순서를 결정적으로 만들기 위한 선택이다 (스냅샷 메타데이터 해시 등).
/// 키·값 제약(문자 집합, 길이, 예약 접두사)은 미정이다.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Labels(pub BTreeMap<String, String>);

// ---------------------------------------------------------------------------
// 프레임
// ---------------------------------------------------------------------------

/// 트리·스크린샷·이벤트의 기준 프레임 (`docs/04-adp.md` §3.1).
///
/// `FrameId = { display: u32, vsyncId: i64, sfFrame: u64 }`
///
/// 게스트에서 오는 값이다. `host/devices`는 이 값을 다루지 않는다 (scanout은 virtio 수준
/// 식별자로 내보내고, `host/runtime/instance`의 `FrameChannel`이 둘을 정확히 매칭한다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameId {
    /// 디스플레이 번호.
    pub display: u32,
    /// 앱 쪽 Choreographer vsync ID.
    pub vsync_id: i64,
    /// 합성 쪽 SurfaceFlinger 프레임 번호.
    pub sf_frame: u64,
}

// ---------------------------------------------------------------------------
// 이벤트 스트림
// ---------------------------------------------------------------------------

/// 이벤트 스트림 (`Machine::events`, `Instance::events`, `FrameChannel::subscribe`).
///
/// PLACEHOLDER: 문서는 이름만 정했다. 동기/비동기, 배압(backpressure), 지연 구독자 처리,
/// 종료 의미는 미정이다. 지금은 이벤트 타입만 실어 나르는 표지(marker)다.
#[derive(Debug)]
pub struct EventStream<T>(pub PhantomData<T>);
