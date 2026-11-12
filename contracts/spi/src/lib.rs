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
//! 비동기 메서드는 특정 async 런타임을 가정하지 않고 `Send` future를 돌려준다.
//! future를 drop하면 그 호출은 취소된다.
//!
//! OPEN(crate): 비동기 메서드가 있는 SPI trait은 언어 기본 기능만으로는 dyn과 호환되지 않는다.
//! 구현체를 `Box<dyn …>`/`Arc<dyn …>`로 주입하려면 외부 crate(`async-trait` 또는 `dynosaur`)가
//! 필요하다. 오너 결정.
//!
//! `PLACEHOLDER`로 표시한 타입은 문서가 이름만 정한 것이다. 구조와 의미는 미정이다.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use droiduse_contracts_common::{Labels, Result};

// ---------------------------------------------------------------------------
// 스냅샷·이미지 저장소
// ---------------------------------------------------------------------------

/// 스냅샷 저장소. 코어 기본: 로컬 파일시스템. 사용 위치: `host/runtime/snapshot`.
pub trait SnapshotStore: Send + Sync {
    /// 스냅샷을 저장하고 참조를 돌려준다.
    fn put(
        &self,
        meta: &SnapshotMeta,
        parts: SnapshotParts,
    ) -> impl Future<Output = Result<SnapshotRef>> + Send;
    /// 스냅샷을 연다. 메타 + 디바이스 state + `MemorySource` 팩토리.
    fn open(&self, r: &SnapshotRef) -> impl Future<Output = Result<SnapshotHandle>> + Send;
    /// selector에 맞는 스냅샷 목록.
    fn list(
        &self,
        selector: &LabelSelector,
    ) -> impl Future<Output = Result<Vec<SnapshotRef>>> + Send;
    /// 스냅샷을 지운다.
    fn delete(&self, r: &SnapshotRef) -> impl Future<Output = Result<()>> + Send;
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
pub trait MemorySource: Send + Sync {
    /// 전체 길이(바이트).
    fn len(&self) -> u64;
    /// `offset`부터 `buf.len()` 바이트를 읽는다.
    // OPEN(crate): 빌린 `&mut [u8]`는 완료 기반 I/O(io_uring, IOCP)에서 취소 시 복사가 필요하다.
    // 소유 버퍼(`bytes`의 `BytesMut`)로 바꿀지는 오너 결정.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> impl Future<Output = Result<()>> + Send;
    /// 워킹셋 힌트.
    fn prefetch(&self, ranges: &[std::ops::Range<u64>]) -> impl Future<Output = ()> + Send;
}

/// 디스크 profile 레이어 저장소 (사용자 데이터 보존). 사용 위치: runtime.
pub trait ProfileStore: Send + Sync {
    /// profile 레이어를 연다.
    fn open(&self, id: &ProfileId) -> impl Future<Output = Result<DiskLayer>> + Send;
    /// profile 레이어를 반영한다.
    fn commit(&self, id: &ProfileId, layer: DiskLayer) -> impl Future<Output = Result<()>> + Send;
}

// ---------------------------------------------------------------------------
// 인증·인가·정책
// ---------------------------------------------------------------------------

/// 인증. 사용 위치: `host/entrypoints/adp_server`, `host/entrypoints/node`. 코어 기본: 로컬 토큰.
pub trait Authenticator: Send + Sync {
    /// 자격 증명을 검증하고 주체를 돌려준다.
    fn authenticate(
        &self,
        cred: &Credential,
        ch: &ChannelCtx,
    ) -> impl Future<Output = Result<Principal>> + Send;
}

/// 인가. 사용 위치: `host/entrypoints/adp_server`, `host/entrypoints/node`.
pub trait Authorizer: Send + Sync {
    /// `who`가 `on`에 대해 `action`을 해도 되는지 판단한다.
    fn authorize(
        &self,
        who: &Principal,
        action: &Action,
        on: &Resource,
    ) -> impl Future<Output = Decision> + Send;
}

/// 정책 공급. 사용 위치: `host/services/network`, `host/runtime/resources`. 코어 기본: 정적 설정.
pub trait PolicyProvider: Send + Sync {
    /// 세션 정책 (egress, 리소스 한도, TLS 가시성 등).
    fn session_policy(&self, labels: &Labels)
    -> impl Future<Output = Result<SessionPolicy>> + Send;
}

// ---------------------------------------------------------------------------
// 네트워크·프로필·스트림
// ---------------------------------------------------------------------------

/// egress 업스트림. 사용 위치: `host/devices/net` (usernet). 코어 기본: 직접 연결.
pub trait EgressUpstream: Send + Sync {
    /// 목적지로 연결을 연다.
    fn connect(
        &self,
        dst: &Destination,
        flow: &FlowCtx,
    ) -> impl Future<Output = Result<Box<dyn Conn>>> + Send;
}

/// 기기 프로필 공급 (기종·로케일·시간대·위치). 사용 위치: `host/runtime/identity`.
pub trait DeviceProfileProvider: Send + Sync {
    /// labels에 맞는 프로필.
    fn profile(&self, labels: &Labels) -> impl Future<Output = Result<DeviceProfile>> + Send;
}

/// 화면 스트림 전송. 사용 위치: `host/services/stream`. 코어 기본: WebSocket.
pub trait StreamTransport: Send + Sync {
    /// 인코딩된 트랙을 내보내고 접속 지점을 돌려준다.
    fn publish(
        &self,
        session: &SessionId,
        track: EncodedTrack,
    ) -> impl Future<Output = Result<StreamEndpoint>> + Send;
}

// ---------------------------------------------------------------------------
// 이벤트·텔레메트리·기록
// ---------------------------------------------------------------------------

/// 이벤트 출력 (수명주기, 리소스 사용량 등). 사용 위치: runtime, services. 코어 기본: 로그.
pub trait EventSink: Send + Sync {
    /// 이벤트 하나를 내보낸다.
    fn emit(&self, ev: &Event) -> impl Future<Output = ()> + Send;
}

/// 텔레메트리 내보내기. 사용 위치: `host/base/trace`. 코어 기본: Perfetto 파일.
///
/// PLACEHOLDER: `docs/09-oss-and-saas.md` §2 표에 이름만 있고 시그니처 초안이 없다.
pub trait TelemetryExporter: Send + Sync {}

/// 세션 기록. 사용 위치: `host/services/record`. 코어 기본: 로컬 파일.
pub trait Recorder: Send + Sync {
    /// 기록을 시작한다.
    fn begin(&self, session: &SessionId) -> impl Future<Output = Result<RecordingId>> + Send;
    /// 항목을 덧붙인다.
    fn append(&self, id: &RecordingId, item: RecordItem)
    -> impl Future<Output = Result<()>> + Send;
    /// 기록을 마친다.
    fn finish(&self, id: &RecordingId) -> impl Future<Output = Result<RecordingRef>> + Send;
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

/// 업스트림 연결. PLACEHOLDER: 메서드 미정.
// OPEN(crate): 비동기 바이트 스트림 trait(`futures-io`의 `AsyncRead`/`AsyncWrite`)을 쓸지,
// `Send` bound를 줄지는 오너 결정.
pub trait Conn {}

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

/// 컴파일 전용 검사: 비동기 메서드를 더미 타입으로 구현할 수 있고, 모든 구현의 future가 `Send`다.
#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_common::Error;

    fn assert_send<T: Send>(_: T) {}

    fn unimplemented_err() -> Error {
        Error {
            code: None,
            message: String::new(),
        }
    }

    struct Dummy;

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
    impl MemorySource for Dummy {
        fn len(&self) -> u64 {
            0
        }
        async fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn prefetch(&self, _ranges: &[std::ops::Range<u64>]) {}
    }
    impl ProfileStore for Dummy {
        async fn open(&self, _id: &ProfileId) -> Result<DiskLayer> {
            Err(unimplemented_err())
        }
        async fn commit(&self, _id: &ProfileId, _layer: DiskLayer) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    impl Authenticator for Dummy {
        async fn authenticate(&self, _cred: &Credential, _ch: &ChannelCtx) -> Result<Principal> {
            Err(unimplemented_err())
        }
    }
    impl Authorizer for Dummy {
        async fn authorize(&self, _who: &Principal, _action: &Action, _on: &Resource) -> Decision {
            Decision {}
        }
    }
    impl PolicyProvider for Dummy {
        async fn session_policy(&self, _labels: &Labels) -> Result<SessionPolicy> {
            Err(unimplemented_err())
        }
    }
    impl EgressUpstream for Dummy {
        async fn connect(&self, _dst: &Destination, _flow: &FlowCtx) -> Result<Box<dyn Conn>> {
            Err(unimplemented_err())
        }
    }
    impl DeviceProfileProvider for Dummy {
        async fn profile(&self, _labels: &Labels) -> Result<DeviceProfile> {
            Err(unimplemented_err())
        }
    }
    impl StreamTransport for Dummy {
        async fn publish(
            &self,
            _session: &SessionId,
            _track: EncodedTrack,
        ) -> Result<StreamEndpoint> {
            Err(unimplemented_err())
        }
    }
    impl EventSink for Dummy {
        async fn emit(&self, _ev: &Event) {}
    }
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

    /// 계약 시그니처만으로 모든 구현의 future가 `Send`임을 확인한다 (구현 타입과 무관).
    fn futures_are_send<T>(t: &T)
    where
        T: SnapshotStore
            + MemorySource
            + ProfileStore
            + Authenticator
            + Authorizer
            + PolicyProvider
            + EgressUpstream
            + DeviceProfileProvider
            + StreamTransport
            + EventSink
            + Recorder,
    {
        let snap = SnapshotRef(String::new());
        let profile = ProfileId(String::new());
        let session = SessionId(String::new());
        let rec = RecordingId(String::new());
        let labels = Labels::default();
        let mut buf = [0u8; 1];

        assert_send(SnapshotStore::put(t, &SnapshotMeta {}, SnapshotParts {}));
        assert_send(SnapshotStore::open(t, &snap));
        assert_send(SnapshotStore::list(t, &LabelSelector {}));
        assert_send(SnapshotStore::delete(t, &snap));
        assert_send(t.read_at(0, &mut buf));
        let ranges: [std::ops::Range<u64>; 0] = [];
        assert_send(t.prefetch(&ranges));
        assert_send(ProfileStore::open(t, &profile));
        assert_send(t.commit(&profile, DiskLayer {}));
        assert_send(t.authenticate(&Credential {}, &ChannelCtx {}));
        assert_send(t.authorize(&Principal {}, &Action {}, &Resource {}));
        assert_send(t.session_policy(&labels));
        assert_send(t.connect(&Destination {}, &FlowCtx {}));
        assert_send(t.profile(&labels));
        assert_send(t.publish(&session, EncodedTrack {}));
        assert_send(t.emit(&Event {}));
        assert_send(t.begin(&session));
        assert_send(t.append(&rec, RecordItem {}));
        assert_send(t.finish(&rec));
    }

    #[test]
    fn dummy_implements_contract() {
        futures_are_send(&Dummy);
    }
}
