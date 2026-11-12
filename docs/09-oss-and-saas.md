# 09. 오픈 코어와 SaaS 확장

## 1. 경계
```
[droid-use-cloud: 비공개 별도 레포]
   control plane: 테넌트 · 인증 · 과금 · 스케줄러 · 플릿 · 대시보드
   스토리지(GCS 등) · WebRTC 게이트웨이 · 프록시 · 녹화 보관
           │  contracts/node (Node API)  +  contracts/spi 구현체 주입
[droid-use: 이 레포, Apache-2.0]
   host/entrypoints/node: 머신 한 대에서 여러 인스턴스를 운영하는 노드 데몬
   runtime / services / devices / vmm / guest
```
- **코어**: 머신 한 대 안에서 일어나는 모든 일. 셀프호스팅 단일 노드로 완결된다.
- **SaaS**: 여러 머신과 여러 테넌트에 걸친 일. 코어를 포크하지 않고 **교체·추가**만 한다.
- 이 레포에는 SaaS 코드를 넣지 않는다. 코어는 테넌트·과금 개념을 모른다.
- 라이선스: **Apache-2.0** (wick, Firecracker와 동일). 경쟁력은 운영 계층(플릿, 스냅샷 배포 속도, 프로필, 관측성, 대시보드)에서 만든다.

## 2. SPI (contracts/spi)
| SPI | 사용 위치 | 코어 기본 구현 | SaaS 구현 예 |
|---|---|---|---|
| `SnapshotStore`, `ImageStore` | host/runtime/snapshot | 로컬 파일시스템 | 오브젝트 스토리지 + 지역 캐시 |
| `MemorySource` | host/runtime/snapshot (uffd) | 로컬 파일 | 원격 페이지 서버 (lazy restore) |
| `ProfileStore` | runtime (디스크 profile 레이어) | 로컬 | 사용자별 로그인 상태 보존 |
| `Authenticator`, `Authorizer` | host/entrypoints/adp_server, host/entrypoints/node | 로컬 토큰 | API 키, OIDC, 테넌트 권한 |
| `PolicyProvider` | host/services/network, host/runtime/resources | 정적 설정 | 테넌트·플랜별 동적 정책 |
| `EgressUpstream` | host/devices/net (usernet) | 직접 연결 | 프록시 체인, 지역 선택 |
| `DeviceProfileProvider` | host/runtime/identity | 기본 프로필 | 기종·로케일·시간대·위치 프로필 |
| `StreamTransport` | host/services/stream | WebSocket | WebRTC |
| `EventSink` | runtime, services | 로그 | 미터링, 감사 로그 |
| `TelemetryExporter` | host/base/trace | Perfetto 파일 | OTel 중앙 수집 |
| `Recorder` | host/services/record | 로컬 파일 | 보관 + 리플레이 |

### 시그니처 초안
```rust
pub trait SnapshotStore: Send + Sync {
    fn put(&self, meta: &SnapshotMeta, parts: SnapshotParts) -> impl Future<Output = Result<SnapshotRef>> + Send;
    fn open(&self, r: &SnapshotRef) -> impl Future<Output = Result<SnapshotHandle>> + Send;   // 메타 + 디바이스 state + MemorySource 팩토리
    fn list(&self, selector: &LabelSelector) -> impl Future<Output = Result<Vec<SnapshotRef>>> + Send;
    fn delete(&self, r: &SnapshotRef) -> impl Future<Output = Result<()>> + Send;
}
pub trait MemorySource: Send + Sync {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> impl Future<Output = Result<()>> + Send;
    fn prefetch(&self, ranges: &[std::ops::Range<u64>]) -> impl Future<Output = ()> + Send;        // 워킹셋 힌트
}
pub trait ProfileStore: Send + Sync {
    fn open(&self, id: &ProfileId) -> impl Future<Output = Result<DiskLayer>> + Send;
    fn commit(&self, id: &ProfileId, layer: DiskLayer) -> impl Future<Output = Result<()>> + Send;
}
pub trait Authenticator: Send + Sync {
    fn authenticate(&self, cred: &Credential, ch: &ChannelCtx) -> impl Future<Output = Result<Principal>> + Send;
}
pub trait Authorizer: Send + Sync {
    fn authorize(&self, who: &Principal, action: &Action, on: &Resource) -> impl Future<Output = Decision> + Send;
}
pub trait PolicyProvider: Send + Sync {
    fn session_policy(&self, labels: &Labels) -> impl Future<Output = Result<SessionPolicy>> + Send;   // egress, 리소스 한도, TLS 가시성 등
}
pub trait EgressUpstream: Send + Sync {
    fn connect(&self, dst: &Destination, flow: &FlowCtx) -> impl Future<Output = Result<Box<dyn Conn>>> + Send;
}
pub trait DeviceProfileProvider: Send + Sync {
    fn profile(&self, labels: &Labels) -> impl Future<Output = Result<DeviceProfile>> + Send;
}
pub trait StreamTransport: Send + Sync {
    fn publish(&self, session: &SessionId, track: EncodedTrack) -> impl Future<Output = Result<StreamEndpoint>> + Send;
}
pub trait EventSink: Send + Sync { fn emit(&self, ev: &Event) -> impl Future<Output = ()> + Send; }
pub trait Recorder: Send + Sync {
    fn begin(&self, session: &SessionId) -> impl Future<Output = Result<RecordingId>> + Send;
    fn append(&self, id: &RecordingId, item: RecordItem) -> impl Future<Output = Result<()>> + Send;
    fn finish(&self, id: &RecordingId) -> impl Future<Output = Result<RecordingRef>> + Send;
}
```

### 주입 방식
- **in-process (Rust trait)**: 성능이 중요한 것 (`MemorySource`, `SnapshotStore`, `StreamTransport`, `EgressUpstream`). SaaS 레포가 코어 crate를 의존성으로 가져와 **자기 node 바이너리를 조립**한다. 동적 로딩 없음.
- **out-of-process (gRPC 훅)**: 제어성 판단 (`Authorizer`, `PolicyProvider`). 언어 제약 없음.
- 코어의 `host/entrypoints/node`는 SPI 구현체를 받는 **빌더 API**를 공개한다.
```rust
NodeBuilder::new(config)
    .snapshot_store(my_store)
    .memory_source_factory(my_remote_pages)
    .authenticator(my_auth)
    .event_sink(my_metering)
    .build()?
    .run()
```

## 3. 지금 설계에 넣어야 하는 것 (나중에 고치기 비싼 것)
1. **디스크 레이어링**: base / snapshot / profile CoW (`host/vmm/disk`)
2. **`MemorySource` 추상화**: restore가 로컬 파일을 직접 읽지 않음
3. **CPU 템플릿 고정**과 스냅샷 메타데이터의 템플릿·이미지 해시
4. **모든 채널 인증**: ADP, Node API, 스트림, guest_link
5. **불투명 `labels`**: 인스턴스·세션·스냅샷 전부
6. **리소스 회계 이벤트**: 미터링 근거
7. **버전 있는 스냅샷 포맷**: 호환되지 않는 restore 거부

## 4. SaaS 레포 구조 (참고, 이 레포에 만들지 않음)
```
droid-use-cloud/
├─ node/            코어 crate + SPI 구현으로 조립한 노드 바이너리
├─ control-plane/   테넌트 · 인증 · 스케줄러 · 과금 · 공개 API
├─ gateway/         WebRTC · ADP 원격 프록시
├─ storage/         스냅샷 · 이미지 · 프로필 · 녹화
└─ dashboard/       라이브 뷰 · 리플레이 · 트레이스 뷰어
```
