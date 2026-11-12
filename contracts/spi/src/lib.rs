//! spi 계약: 코어 확장 지점.
//!
//! 상세: `docs/09-oss-and-saas.md` §2. semver로 **안정성을 약속**한다.
//! 코어는 각 SPI의 기본 구현을 갖고(`host/runtime/stores` 등), SaaS는 코어를 포크하지 않고
//! 이 trait들의 구현체를 주입해 자기 node 바이너리를 조립한다.
//!
//! 이 crate는 `host/devices/net`(`EgressUpstream`)과 `machine` 계약(`MemorySource`,
//! `SnapshotMeta`)을 통해 `host/devices`, `host/vmm`에서도 보인다. 따라서 Android 개념을
//! 다루는 식별자를 두지 않는다 (`LAYERS.toml` `[forbidden]`). 테넌트·과금 개념도 없다.
//!
//! 주입 방식(문서): `MemorySource`, `SnapshotStore`, `StreamTransport`, `EgressUpstream`은
//! in-process(Rust trait), `Authorizer`, `PolicyProvider`는 out-of-process(gRPC 훅)도 가진다.
//! 이 crate는 Rust trait 쪽만 정의한다. gRPC 훅의 직렬화 형식은 미정이다.
//!
//! 비동기 메서드는 `async-trait`으로 선언한다. 모든 SPI trait은 trait object(dyn)로 주입할 수
//! 있고, 특정 async 런타임을 가정하지 않으며, `Send` future를 돌려준다.
//! future를 drop하면 그 호출은 취소된다.
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use bytes::Bytes;
use droiduse_contracts_common::{Labels, Result};
use futures_io::{AsyncRead, AsyncWrite};

// ---------------------------------------------------------------------------
// 스냅샷·이미지 저장소
// ---------------------------------------------------------------------------

/// 스냅샷 저장소. 코어 기본: 로컬 파일시스템. 사용 위치: `host/runtime/snapshot`.
#[async_trait]
pub trait SnapshotStore: Send + Sync {
    /// 스냅샷을 저장하고 참조를 돌려준다.
    async fn put(&self, meta: &SnapshotMeta, parts: SnapshotParts) -> Result<SnapshotRef>;
    /// 스냅샷을 연다. 메타 + 디바이스 state + `MemorySource` 팩토리.
    async fn open(&self, r: &SnapshotRef) -> Result<SnapshotHandle>;
    /// selector에 맞는 스냅샷 목록.
    async fn list(&self, selector: &LabelSelector) -> Result<Vec<SnapshotRef>>;
    /// 스냅샷을 지운다.
    async fn delete(&self, r: &SnapshotRef) -> Result<()>;
}

/// 이미지 저장소 (디스크 base 레이어). 코어 기본: 로컬 파일시스템.
///
/// PLACEHOLDER: `docs/09-oss-and-saas.md` §2 표에 이름만 있고 시그니처 초안이 없다.
pub trait ImageStore: Send + Sync {}

/// 스냅샷 메모리 페이지 공급원. 로컬 파일 또는 원격 페이지 서버(lazy restore).
/// 사용 위치: `host/runtime/snapshot` (uffd), `machine` 계약의 `MachineFactory::restore`.
// 문서 초안에 `is_empty`가 없다. 메서드를 추가하는 것은 계약 변경이므로 lint만 기대값으로 표시한다.
#[expect(
    clippy::len_without_is_empty,
    reason = "docs/09-oss-and-saas.md §2 초안 그대로. is_empty 추가는 계약 변경이라 오너 결정"
)]
#[async_trait]
pub trait MemorySource: Send + Sync {
    /// 전체 길이(바이트).
    fn len(&self) -> u64;
    /// `offset`부터 정확히 `len` 바이트를 읽어 소유 버퍼로 돌려준다.
    ///
    /// 버퍼를 호출자가 빌려주지 않으므로 완료 기반 I/O(io_uring, IOCP)에서도 future를 drop해
    /// 취소할 수 있다.
    async fn read_at(&self, offset: u64, len: usize) -> Result<Bytes>;
    /// 워킹셋 힌트.
    async fn prefetch(&self, ranges: &[std::ops::Range<u64>]);
}

/// 디스크 profile 레이어 저장소 (사용자 데이터 보존). 사용 위치: runtime.
#[async_trait]
pub trait ProfileStore: Send + Sync {
    /// profile 레이어를 연다.
    async fn open(&self, id: &ProfileId) -> Result<DiskLayer>;
    /// profile 레이어를 반영한다.
    async fn commit(&self, id: &ProfileId, layer: DiskLayer) -> Result<()>;
}

// ---------------------------------------------------------------------------
// 인증·인가·정책
// ---------------------------------------------------------------------------

/// 인증. 사용 위치: `host/entrypoints/adp_server`, `host/entrypoints/node`. 코어 기본: 로컬 토큰.
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// 자격 증명을 검증하고 주체를 돌려준다.
    async fn authenticate(&self, cred: &Credential, ch: &ChannelCtx) -> Result<Principal>;
}

/// 인가. 사용 위치: `host/entrypoints/adp_server`, `host/entrypoints/node`.
#[async_trait]
pub trait Authorizer: Send + Sync {
    /// `who`가 `on`에 대해 `action`을 해도 되는지 판단한다.
    ///
    /// **fail closed**: 호출자는 `Err`와 취소된 호출(future drop)을 모두 거부로 다룬다.
    async fn authorize(&self, who: &Principal, action: &Action, on: &Resource) -> Result<Decision>;
}

/// 정책 공급. 사용 위치: `host/services/network`, `host/runtime/resources`. 코어 기본: 정적 설정.
#[async_trait]
pub trait PolicyProvider: Send + Sync {
    /// 세션 정책 (egress, 리소스 한도, TLS 가시성 등).
    async fn session_policy(&self, labels: &Labels) -> Result<SessionPolicy>;
}

// ---------------------------------------------------------------------------
// 네트워크·프로필·스트림
// ---------------------------------------------------------------------------

/// egress 업스트림. 사용 위치: `host/devices/net` (usernet). 코어 기본: 직접 연결.
#[async_trait]
pub trait EgressUpstream: Send + Sync {
    /// 목적지로 연결을 연다.
    async fn connect(&self, dst: &Destination, flow: &FlowCtx) -> Result<Box<dyn Conn>>;
}

/// 기기 프로필 공급 (기종·로케일·시간대·위치). 사용 위치: `host/runtime/identity`.
#[async_trait]
pub trait DeviceProfileProvider: Send + Sync {
    /// labels에 맞는 프로필.
    async fn profile(&self, labels: &Labels) -> Result<DeviceProfile>;
}

/// 화면 스트림 전송. 사용 위치: `host/services/stream`. 코어 기본: WebSocket.
#[async_trait]
pub trait StreamTransport: Send + Sync {
    /// 인코딩된 트랙을 내보내고 접속 지점을 돌려준다.
    async fn publish(&self, session: &SessionId, track: EncodedTrack) -> Result<StreamEndpoint>;
}

// ---------------------------------------------------------------------------
// 이벤트·텔레메트리·기록
// ---------------------------------------------------------------------------

/// 이벤트 출력 (수명주기, 리소스 사용량 등). 사용 위치: runtime, services. 코어 기본: 로그.
#[async_trait]
pub trait EventSink: Send + Sync {
    /// 이벤트 하나를 내보낸다.
    async fn emit(&self, ev: &Event) -> Result<()>;
}

/// 텔레메트리 내보내기. 사용 위치: `host/base/trace`. 코어 기본: Perfetto 파일.
///
/// PLACEHOLDER: `docs/09-oss-and-saas.md` §2 표에 이름만 있고 시그니처 초안이 없다.
pub trait TelemetryExporter: Send + Sync {}

/// 세션 기록. 사용 위치: `host/services/record`. 코어 기본: 로컬 파일.
#[async_trait]
pub trait Recorder: Send + Sync {
    /// 기록을 시작한다.
    async fn begin(&self, session: &SessionId) -> Result<RecordingId>;
    /// 항목을 덧붙인다.
    async fn append(&self, id: &RecordingId, item: RecordItem) -> Result<()>;
    /// 기록을 마친다.
    async fn finish(&self, id: &RecordingId) -> Result<RecordingRef>;
}

// ---------------------------------------------------------------------------
// 자리표시 타입 (문서가 이름만 정한 것)
// ---------------------------------------------------------------------------

/// 스냅샷 메타데이터.
///
/// PLACEHOLDER: `docs/03-contracts.md` machine(`Machine::snapshot` 반환)과
/// `docs/09-oss-and-saas.md` §2(`SnapshotStore::put` 인자)가 같은 이름을 쓴다.
/// 하나의 타입으로 두었다. 버전 있는 포맷의 필드 후보는 `docs/07-runtime.md` §3에 있다.
#[derive(Debug, Clone)]
pub struct SnapshotMeta {}

/// 스냅샷 구성 요소 (메모리, vCPU 상태, 디바이스 state 등). PLACEHOLDER.
#[derive(Debug)]
pub struct SnapshotParts {}

/// 저장된 스냅샷 참조. PLACEHOLDER: 표현 미정 (문자열은 "불투명 값"이라는 뜻일 뿐).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotRef(pub String);

/// 열린 스냅샷: 메타 + 디바이스 state + `MemorySource` 팩토리. PLACEHOLDER.
#[derive(Debug)]
pub struct SnapshotHandle {}

/// labels selector. PLACEHOLDER: 문법 미정.
#[derive(Debug, Clone)]
pub struct LabelSelector {}

/// profile 식별자. PLACEHOLDER: 표현 미정.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileId(pub String);

/// 디스크 CoW 레이어 하나. PLACEHOLDER.
#[derive(Debug)]
pub struct DiskLayer {}

/// 자격 증명. PLACEHOLDER.
#[derive(Debug)]
pub struct Credential {}

/// 자격 증명이 들어온 채널의 맥락. PLACEHOLDER.
#[derive(Debug)]
pub struct ChannelCtx {}

/// 인증된 주체. PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct Principal {}

/// 인가 대상 행위. PLACEHOLDER.
#[derive(Debug)]
pub struct Action {}

/// 인가 대상 자원. PLACEHOLDER.
#[derive(Debug)]
pub struct Resource {}

/// 인가 결정. PLACEHOLDER: 값 집합 미정.
#[derive(Debug)]
pub struct Decision {}

/// 세션 정책 (egress, 리소스 한도, TLS 가시성 등). PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct SessionPolicy {}

/// egress 목적지. PLACEHOLDER.
#[derive(Debug)]
pub struct Destination {}

/// egress 플로우 맥락. PLACEHOLDER.
#[derive(Debug)]
pub struct FlowCtx {}

/// 업스트림 연결: 양방향 비동기 바이트 스트림. 닫기는 `AsyncWrite::poll_close`.
pub trait Conn: AsyncRead + AsyncWrite + Send {}

/// 기기 프로필 (기종·로케일·시간대·위치). PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct DeviceProfile {}

/// 세션 식별자. PLACEHOLDER: 표현 미정.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

/// 인코딩된 미디어 트랙. PLACEHOLDER.
#[derive(Debug)]
pub struct EncodedTrack {}

/// 스트림 접속 지점. PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct StreamEndpoint {}

/// `EventSink`로 내보내는 이벤트. PLACEHOLDER.
#[derive(Debug)]
pub struct Event {}

/// 기록 식별자. PLACEHOLDER: 표현 미정.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordingId(pub String);

/// 기록 항목 하나. PLACEHOLDER.
#[derive(Debug)]
pub struct RecordItem {}

/// 완료된 기록 참조. PLACEHOLDER: 표현 미정.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordingRef(pub String);

/// 컴파일 전용 검사: 비동기 SPI trait을 더미 타입으로 구현할 수 있고, `Box<dyn …>`·`&dyn …`로
/// 쓸 수 있으며, future가 `Send`다.
#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_common::Error;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    fn assert_send<T: Send>(_: T) {}

    fn unimplemented_err() -> Error {
        Error {
            code: None,
            message: String::new(),
        }
    }

    struct Dummy;

    #[async_trait]
    impl SnapshotStore for Dummy {
        async fn put(&self, _meta: &SnapshotMeta, _parts: SnapshotParts) -> Result<SnapshotRef> {
            Err(unimplemented_err())
        }
        async fn open(&self, _r: &SnapshotRef) -> Result<SnapshotHandle> {
            Err(unimplemented_err())
        }
        async fn list(&self, _selector: &LabelSelector) -> Result<Vec<SnapshotRef>> {
            Err(unimplemented_err())
        }
        async fn delete(&self, _r: &SnapshotRef) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl MemorySource for Dummy {
        fn len(&self) -> u64 {
            0
        }
        async fn read_at(&self, _offset: u64, _len: usize) -> Result<Bytes> {
            Err(unimplemented_err())
        }
        async fn prefetch(&self, _ranges: &[std::ops::Range<u64>]) {}
    }
    #[async_trait]
    impl ProfileStore for Dummy {
        async fn open(&self, _id: &ProfileId) -> Result<DiskLayer> {
            Err(unimplemented_err())
        }
        async fn commit(&self, _id: &ProfileId, _layer: DiskLayer) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl Authenticator for Dummy {
        async fn authenticate(&self, _cred: &Credential, _ch: &ChannelCtx) -> Result<Principal> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl Authorizer for Dummy {
        async fn authorize(
            &self,
            _who: &Principal,
            _action: &Action,
            _on: &Resource,
        ) -> Result<Decision> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl PolicyProvider for Dummy {
        async fn session_policy(&self, _labels: &Labels) -> Result<SessionPolicy> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl EgressUpstream for Dummy {
        async fn connect(&self, _dst: &Destination, _flow: &FlowCtx) -> Result<Box<dyn Conn>> {
            Ok(Box::new(DummyConn))
        }
    }
    #[async_trait]
    impl DeviceProfileProvider for Dummy {
        async fn profile(&self, _labels: &Labels) -> Result<DeviceProfile> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl StreamTransport for Dummy {
        async fn publish(
            &self,
            _session: &SessionId,
            _track: EncodedTrack,
        ) -> Result<StreamEndpoint> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl EventSink for Dummy {
        async fn emit(&self, _ev: &Event) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl Recorder for Dummy {
        async fn begin(&self, _session: &SessionId) -> Result<RecordingId> {
            Err(unimplemented_err())
        }
        async fn append(&self, _id: &RecordingId, _item: RecordItem) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn finish(&self, _id: &RecordingId) -> Result<RecordingRef> {
            Err(unimplemented_err())
        }
    }

    /// 테스트 전용: 항상 EOF인 연결.
    struct DummyConn;
    impl AsyncRead for DummyConn {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Ok(0))
        }
    }
    impl AsyncWrite for DummyConn {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Ok(0))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }
    impl Conn for DummyConn {}

    /// trait object로 주입·호출해도 모든 future가 `Send`다.
    #[allow(clippy::too_many_arguments)]
    fn dyn_futures_are_send(
        store: &dyn SnapshotStore,
        mem: &dyn MemorySource,
        profiles: &dyn ProfileStore,
        authn: &dyn Authenticator,
        authz: &dyn Authorizer,
        policy: &dyn PolicyProvider,
        egress: &dyn EgressUpstream,
        device: &dyn DeviceProfileProvider,
        transport: &dyn StreamTransport,
        sink: &dyn EventSink,
        recorder: &dyn Recorder,
    ) {
        let snap = SnapshotRef(String::new());
        let profile = ProfileId(String::new());
        let session = SessionId(String::new());
        let rec = RecordingId(String::new());
        let labels = Labels::default();
        let ranges: [std::ops::Range<u64>; 0] = [];

        assert_send(store.put(&SnapshotMeta {}, SnapshotParts {}));
        assert_send(store.open(&snap));
        assert_send(store.list(&LabelSelector {}));
        assert_send(store.delete(&snap));
        assert_send(mem.read_at(0, 1));
        assert_send(mem.prefetch(&ranges));
        assert_send(profiles.open(&profile));
        assert_send(profiles.commit(&profile, DiskLayer {}));
        assert_send(authn.authenticate(&Credential {}, &ChannelCtx {}));
        assert_send(authz.authorize(&Principal {}, &Action {}, &Resource {}));
        assert_send(policy.session_policy(&labels));
        assert_send(egress.connect(&Destination {}, &FlowCtx {}));
        assert_send(device.profile(&labels));
        assert_send(transport.publish(&session, EncodedTrack {}));
        assert_send(sink.emit(&Event {}));
        assert_send(recorder.begin(&session));
        assert_send(recorder.append(&rec, RecordItem {}));
        assert_send(recorder.finish(&rec));
    }

    #[test]
    fn dummy_implements_contract_as_dyn() {
        let d = Dummy;
        dyn_futures_are_send(&d, &d, &d, &d, &d, &d, &d, &d, &d, &d, &d);
        let boxed: Vec<Box<dyn SnapshotStore>> = vec![Box::new(Dummy)];
        assert_send(boxed);
        let conn: Box<dyn Conn> = Box::new(DummyConn);
        assert_send(conn);
    }
}
