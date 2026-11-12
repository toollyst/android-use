//! instance 계약: services ↔ runtime.
//!
//! services가 보는 "Android 인스턴스 하나". 상세: `docs/03-contracts.md` instance,
//! `docs/07-runtime.md`.
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use droiduse_contracts_common::{EventStream, FrameId, Labels, Result};
use droiduse_contracts_spi::SnapshotRef;

// ---------------------------------------------------------------------------
// 인스턴스
// ---------------------------------------------------------------------------

/// Android 인스턴스 하나.
pub trait Instance {
    /// 인스턴스 ID.
    fn id(&self) -> InstanceId;
    /// 불투명 labels. 코어는 해석하지 않는다.
    fn labels(&self) -> &Labels;
    /// 현재 상태.
    fn state(&self) -> InstanceState;
    /// guest_link 클라이언트.
    fn guest(&self) -> &dyn GuestChannel;
    /// frameId가 매칭된 scanout 프레임.
    fn frames(&self) -> &dyn FrameChannel;
    /// `host/devices/input` 이벤트 주입.
    fn input(&self) -> &dyn InputChannel;
    /// `host/devices/net` 플로우 관찰·정책 적용.
    fn net(&self) -> &dyn NetObserver;
    /// 인스턴스 이벤트 스트림.
    fn events(&self) -> EventStream<InstanceEvent>;
}

/// 인스턴스 상태 (`docs/03-contracts.md` instance 목록 그대로).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstanceState {
    /// 생성 중.
    Creating,
    /// 부팅 중.
    Booting,
    /// 사용 가능.
    Ready,
    /// 일시정지.
    Paused,
    /// 스냅샷에서 복원 중 (`Restore.Notify` 수신 전).
    Restoring,
    /// 정지.
    Stopped,
    /// 실패.
    Failed,
}

/// 인스턴스 ID. PLACEHOLDER: 표현 미정 (문자열은 "불투명 값"이라는 뜻일 뿐).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceId(pub String);

/// 인스턴스 이벤트. PLACEHOLDER: 값 집합 미정.
#[derive(Debug, Clone)]
pub struct InstanceEvent {}

/// guest_link 클라이언트.
///
/// PLACEHOLDER: 메서드 미정. guest_link 메시지 정의(IDL·codegen, 형식 미정)를 따른다.
pub trait GuestChannel {}

/// `host/devices/input` 이벤트 주입. PLACEHOLDER: 메서드 미정.
pub trait InputChannel {}

/// `host/devices/net` 플로우 관찰·정책 적용. PLACEHOLDER: 메서드 미정.
pub trait NetObserver {}

// ---------------------------------------------------------------------------
// 프레임
// ---------------------------------------------------------------------------

/// 게스트 프레임 커밋과 device scanout을 정확히 매칭한 결과. 최근 N 프레임을 보관한다.
pub trait FrameChannel {
    /// 디스플레이의 프레임 스트림.
    fn subscribe(&self, display: u32) -> EventStream<Frame>;
    /// 정확히 그 프레임. 링 버퍼에 없으면 `FrameExpired`. 추측해서 다른 프레임을 주지 않는다.
    fn get(&self, id: &FrameId) -> Result<Frame>;
    /// 디스플레이의 최신 프레임.
    fn latest(&self, display: u32) -> Result<Frame>;
}

/// scanout 프레임 하나.
#[derive(Debug)]
pub struct Frame {
    /// 매칭되지 않으면 `None` (frameId 지정 요청에는 쓰지 않는다).
    pub frame_id: Option<FrameId>,
    /// shm 참조 (복사 없음).
    pub buffer: FrameBufferRef,
}

/// 프레임 버퍼의 shm 참조 (복사 없음).
///
/// PLACEHOLDER: 표현과 수명(링 버퍼 슬롯이 재사용될 때 이 참조가 어떻게 되는지) 미정.
#[derive(Debug)]
pub struct FrameBufferRef {}

// ---------------------------------------------------------------------------
// 세션
// ---------------------------------------------------------------------------

/// 세션 풀.
pub trait SessionManager {
    /// 스냅샷 풀에서 할당한다.
    fn acquire(&self, req: SessionRequest) -> Result<Session>;
    /// 세션을 리셋한다 (세션 쓰기 CoW 레이어 폐기, `docs/07-runtime.md` §5).
    fn reset(&self, s: &Session) -> Result<()>;
    /// 세션을 반납한다.
    fn release(&self, s: Session) -> Result<()>;
    /// 세션의 현재 상태를 `name`으로 스냅샷한다.
    fn snapshot(&self, s: &Session, name: &str) -> Result<SnapshotRef>;
}

/// 세션 요청. PLACEHOLDER: 구조 미정.
#[derive(Debug, Clone)]
pub struct SessionRequest {}

/// 할당된 세션. PLACEHOLDER: 구조 미정.
#[derive(Debug)]
pub struct Session {}
