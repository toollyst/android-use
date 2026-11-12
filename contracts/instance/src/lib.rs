//! instance 계약: services ↔ runtime.
//!
//! services가 보는 "Android 인스턴스 하나". 상세: `docs/03-contracts.md` instance,
//! `docs/07-runtime.md`.
//!
//! 비동기 메서드는 `async-trait`으로 선언한다. trait object(dyn)로 쓸 수 있고, 특정 async
//! 런타임을 가정하지 않으며, `Send` future를 돌려준다. future를 drop하면 그 호출은 취소된다.
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::time::Instant;

use async_trait::async_trait;
use droiduse_contracts_common::{EventStream, FrameId, Labels, Result};
use droiduse_contracts_spi::SnapshotRef;

// ---------------------------------------------------------------------------
// 인스턴스
// ---------------------------------------------------------------------------

/// Android 인스턴스 하나.
pub trait Instance: Send + Sync {
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
    /// 인스턴스 이벤트 스트림. 구독만 하고 기다리지 않는다. 전달은 스트림 쪽에서 일어난다.
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
    /// 스냅샷 생성 중 (`Ready`에서 들어가 끝나면 `Ready`로, `docs/07-runtime.md` §2).
    Snapshotting,
    /// 스냅샷에서 복원 중 (`Restore.Notify` 수신 전).
    Restoring,
    /// 정지.
    Stopped,
    /// 실패. 수명주기 호출이 도중에 취소된 인스턴스도 여기로 온다. 자원 정리는 runtime이 맡는다.
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
/// 비동기 메서드가 생기면 `#[async_trait]`으로 선언해 `&dyn`을 유지한다.
pub trait GuestChannel: Send + Sync {}

/// `host/devices/input` 이벤트 주입. PLACEHOLDER: 메서드 미정.
pub trait InputChannel: Send + Sync {}

/// `host/devices/net` 플로우 관찰·정책 적용. PLACEHOLDER: 메서드 미정.
pub trait NetObserver: Send + Sync {}

// ---------------------------------------------------------------------------
// 프레임
// ---------------------------------------------------------------------------

/// 게스트 프레임 커밋과 device scanout을 정확히 매칭한 결과. 최근 N 프레임을 보관한다.
#[async_trait]
pub trait FrameChannel: Send + Sync {
    /// 디스플레이의 프레임 스트림. 구독만 하고 기다리지 않는다. 전달은 스트림 쪽에서 일어난다.
    fn subscribe(&self, display: u32) -> EventStream<Frame>;
    /// 정확히 그 프레임. 아직 scanout과 매칭되지 않았으면 매칭되거나 `deadline`이 지날 때까지
    /// 기다린다. 이미 링 버퍼에서 밀려났으면 `FrameExpired`, `deadline`이 지나면 `Timeout`.
    /// 추측해서 다른 프레임을 주지 않는다. 읽기 호출이라 drop(취소)에 부작용이 없다.
    async fn get(&self, id: &FrameId, deadline: Instant) -> Result<Frame>;
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
///
/// **취소**: 비동기 메서드(수명주기 호출)가 도중에 취소되면(future drop) 대상 인스턴스는
/// `Failed`로 끝나고 자원 정리는 runtime이 맡는다. 반쯤 된 인스턴스가 `Ready`로 보이거나
/// 세션으로 넘겨지는 일은 없다.
#[async_trait]
pub trait SessionManager: Send + Sync {
    /// 스냅샷 풀에서 할당한다.
    async fn acquire(&self, req: SessionRequest) -> Result<Session>;
    /// 세션을 리셋한다 (세션 쓰기 CoW 레이어 폐기, `docs/07-runtime.md` §5).
    async fn reset(&self, s: &Session) -> Result<()>;
    /// 세션을 반납한다.
    async fn release(&self, s: Session) -> Result<()>;
    /// 세션의 현재 상태를 `name`으로 스냅샷한다.
    async fn snapshot(&self, s: &Session, name: &str) -> Result<SnapshotRef>;
}

/// 세션 요청. PLACEHOLDER: 구조 미정.
#[derive(Debug, Clone)]
pub struct SessionRequest {}

/// 할당된 세션. PLACEHOLDER: 구조 미정.
#[derive(Debug)]
pub struct Session {}

/// 컴파일 전용 검사: 비동기 trait을 더미 타입으로 구현할 수 있고, `Box<dyn …>`·`&dyn …`로 쓸 수
/// 있으며, future가 `Send`다.
#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_common::Error;

    fn assert_send<T: Send>(_: T) {}
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    fn unimplemented_err() -> Error {
        Error {
            code: None,
            message: String::new(),
        }
    }

    struct DummySessions;
    #[async_trait]
    impl SessionManager for DummySessions {
        async fn acquire(&self, _req: SessionRequest) -> Result<Session> {
            Err(unimplemented_err())
        }
        async fn reset(&self, _s: &Session) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn release(&self, _s: Session) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn snapshot(&self, _s: &Session, _name: &str) -> Result<SnapshotRef> {
            Err(unimplemented_err())
        }
    }

    struct DummyFrames;
    #[async_trait]
    impl FrameChannel for DummyFrames {
        fn subscribe(&self, _display: u32) -> EventStream<Frame> {
            unimplemented!("컴파일 전용 더미")
        }
        async fn get(&self, _id: &FrameId, _deadline: Instant) -> Result<Frame> {
            Err(unimplemented_err())
        }
        fn latest(&self, _display: u32) -> Result<Frame> {
            Err(unimplemented_err())
        }
    }

    /// trait object로 호출해도 모든 future가 `Send`다.
    fn dyn_futures_are_send(m: &dyn SessionManager, f: &dyn FrameChannel) {
        let s = Session {};
        assert_send(m.acquire(SessionRequest {}));
        assert_send(m.reset(&s));
        assert_send(m.snapshot(&s, "name"));
        assert_send(m.release(Session {}));
        let id = FrameId {
            display: 0,
            vsync_id: 0,
            sf_frame: 0,
        };
        assert_send(f.get(&id, Instant::now()));
    }

    #[test]
    fn dummy_implements_contract_as_dyn() {
        assert_send_sync::<dyn Instance>();
        assert_send_sync::<dyn FrameChannel>();
        assert_send_sync::<dyn SessionManager>();
        assert_send_sync::<dyn GuestChannel>();
        assert_send_sync::<dyn InputChannel>();
        assert_send_sync::<dyn NetObserver>();
        let m: Box<dyn SessionManager> = Box::new(DummySessions);
        let f: Box<dyn FrameChannel> = Box::new(DummyFrames);
        dyn_futures_are_send(m.as_ref(), f.as_ref());
    }
}
