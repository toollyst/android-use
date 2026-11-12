# 03. 계약 (contracts/)

레이어 경계는 **이 문서의 계약으로만** 넘는다. 계약은 Chromium의 mojom과 같은 역할이다.

## 공통 규칙
- 모든 계약은 `contracts/<name>/`에 스키마/트레이트로 존재하고 `VERSION`(semver)을 가진다.
- contracts는 다른 레이어에 의존하지 않는다. 공통 타입(에러 코드, trace ID 등)도 contracts 안에 둔다.
- 하위호환을 깨는 변경은 major 증가 + `contracts/compat/` 픽스처 + 마이그레이션 문서가 필요하다.
- 변경 PR에는 양쪽 레이어 OWNERS 승인이 필요하다.
- 직렬화 계약(guest_link, adp, node)은 `build/codegen`으로 Rust / Kotlin·Java / C++ / TypeScript / Python 코드를 생성한다.
- 모든 채널은 인증한다. 모든 메시지에 `trace_id`를 실을 수 있다.
- 비동기 수명주기 호출(VM 생성·시작·스냅샷·복원, 세션 acquire 등)이 도중에 취소되면(future drop) 그 인스턴스는 `Failed`로 끝나고 자원 정리는 runtime이 맡으며(반쯤 된 VM은 `Ready`로 보이지 않는다), 읽기 호출의 취소는 부작용이 없다.

## 계약 목록
| 계약 | 경계 | 형식 | 전송 |
|---|---|---|---|
| `machine` | runtime ↔ vmm | Rust trait + 확장 REST API | in-process / unix socket |
| `device` | vmm ↔ devices | vhost-user 표준 + 확장 | unix socket + shm |
| `guest_link` | runtime·services ↔ guest | IDL → codegen | gRPC over vsock |
| `keymint` | guest KeyMint HAL ↔ host/runtime/keymint | AOSP kmr wire 페이로드 + 우리 프레이밍·인증 | 전용 vsock 포트 |
| `instance` | services ↔ runtime | Rust trait | in-process |
| `adp` | host/entrypoints/adp_server ↔ clients·외부 클라이언트 | CDP 스타일 JSON-RPC 스키마 | WebSocket |
| `node` | control plane ↔ node 데몬 | IDL(gRPC) | TLS |
| `spi` | 코어 ↔ 확장 구현체 | Rust trait (+ 일부 gRPC 훅) | in-process / gRPC |

---

## machine
VM 하나의 정의와 제어. Android 개념 없음.
```rust
struct VmSpec {
    vcpus: u8,
    memory_mib: u32,
    cpu_template: CpuTemplateRef,          // 스냅샷 이식성을 위해 필수
    boot: BootSpec,                        // Linux{kernel,initrd,cmdline} | BootImageV4{boot,vendor_boot,bootconfig}
                                           // bootconfig는 불투명 key=value 목록. vmm은 내용을 해석하지 않는다
                                           // (androidboot.* 값은 host/runtime/instance가 만든다)
    disks: Vec<CompositeDiskSpec>,         // GPT + CoW 레이어(base ro / snapshot / profile)
    vsock: VsockSpec,
    net: Vec<NetSpec>,                     // vhost-user-net 백엔드 소켓
    vhost_user: Vec<VhostUserDeviceSpec>,  // {kind, socket, shm_regions}
    vmgenid: bool,
    labels: Labels,                        // 불투명 map, 코어는 해석하지 않음
}

// 생성 경로와 인스턴스 제어를 분리해 둘 다 trait object(dyn)로 쓸 수 있게 한다.
#[async_trait]
trait MachineFactory: Send + Sync {
    async fn create(&self, spec: VmSpec) -> Result<Box<dyn Machine>>;
    async fn restore(&self, spec: VmSpec, meta: SnapshotMeta, mem: Box<dyn MemorySource>) -> Result<Box<dyn Machine>>;
}

#[async_trait]
trait Machine: Send + Sync {
    async fn start(&self) -> Result<()>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    async fn snapshot(&self, out: SnapshotSink) -> Result<SnapshotMeta>;
    async fn bump_generation(&self) -> Result<()>;   // VMGenID 갱신
    async fn destroy(self: Box<Self>) -> Result<()>;
    fn events(&self) -> EventStream<VmEvent>;        // EventStream<T> = BoxStream<'static, Result<T>>
}
```
- VMM은 jail 안의 별도 프로세스다. runtime 쪽 구현은 `host/vmm/api`의 REST 클라이언트가 이 trait을 구현한다 (in-process 구현은 테스트용).

## device
- 기본은 **vhost-user 표준 프로토콜**이다. 디바이스는 vhost-user 백엔드로 구현한다.
- 확장:
  - `SET_DEVICE_STATE_FD` 계열 device state 전송 (스냅샷에 디바이스 상태 포함)
  - virtio shared memory region (gfxstream blob resource용)
  - **scanout export** (gpu 전용): 현재 scanout 버퍼의 shm fd + 프레임 커밋 알림 (virtio 수준 식별자 포함). `host/runtime/instance`가 받아 `FrameChannel`로 내보낸다.
- devices는 Android를 모른다. 프레임 메타데이터에 게스트 frameId를 넣지 않는다(그건 guest_link로 온다). 두 쪽은 `host/runtime/instance`의 `FrameChannel`이 virtio 수준 식별자로 **정확히** 매칭한다. 추정 매칭은 하지 않는다.

## guest_link
호스트와 게스트(adpd) 사이의 vsock 프로토콜.
```
Hello { protocol_version, build_fingerprint, capabilities[] }   // 버전 협상 필수
Auth  { session_token }                                         // 인스턴스별 토큰
Auth.Rebind { prev_token, vmgenid } → { session_token }       // restore 직후 1회, 복제본별 새 토큰

// 인스펙션
Inspect.Subscribe { sources[], displays[], throttle_ms }
Inspect.Snapshot  → TreeSnapshot { frame_id, windows[], nodes[] }
Inspect.Diff      ← TreeDiff { base_frame_id, frame_id, ops[] }     // 이벤트
Inspect.Node      { node_id } → Node

// 프레임
Frame.Committed   ← { frame_id{display, vsync_id, sf_frame}, scanout_ref, ts_ns, layers[] }  // 이벤트. scanout_ref = 매칭용 virtio 식별자

// 액션 (노드 기반, 좌표 입력은 host/devices/input 경유)
Action.Perform    { node_id, action, args } → ActionResult

// 앱·시스템
App.Install / Launch / Stop / ClearData / List / ActivityStack
App.Lifecycle     ← 이벤트 (activity, anr, crash)
Log.Stream        { filters } ← LogEntry 스트림
WebView.Targets   → [ { target_id, pkg, url } ]
WebView.Attach    { target_id } → CDP 프록시 스트림

// 디버그 (호스트 인가 전용, CTS/VTS용)
Debug.SetAdb      { enabled }

// 복원
Restore.Notify    ← { generation, completed_steps[] }   // VMGenID 변경 후 게스트 재초기화 완료 알림
```
- gRPC over vsock이다. 게스트가 연결 하나를 열고 listening 포트를 열지 않는다. 호스트 → 게스트 호출도 이 연결로 가며 호출마다 자기 상태·deadline·취소를 가진다. 토큰은 불투명 비밀(bytes), `vmgenid`는 16바이트다. Hello·Auth 말고는 모든 호출이 양방향 모두 세션 토큰을 메타데이터 `droiduse-session-token-bin`에 싣고, 없거나 틀리면 `UNAUTHENTICATED`다. trace는 W3C Trace Context(`traceparent`, `tracestate`) 메타데이터로 싣는다.
- KeyMint HAL 채널은 guest_link와 **분리된 전용 vsock 포트**를 쓰며 계약은 `keymint`다 (아래).

## keymint
게스트 KeyMint HAL과 호스트 `host/runtime/keymint` 사이 채널.
- 페이로드는 고정 AOSP 태그의 kmr wire 형식을 그대로 쓰고, 프레이밍·인증만 우리가 정의한다.
- restore 후 재인증은 guest_link와 같다.

## instance
services가 보는 "Android 인스턴스 하나".
```rust
trait Instance: Send + Sync {
    fn id(&self) -> InstanceId;
    fn labels(&self) -> &Labels;
    fn state(&self) -> InstanceState;          // Creating | Booting | Ready | Paused | Snapshotting | Restoring | Stopped | Failed
    fn guest(&self) -> &dyn GuestChannel;      // guest_link 클라이언트
    fn frames(&self) -> &dyn FrameChannel;     // frameId가 매칭된 scanout 프레임
    fn input(&self) -> &dyn InputChannel;      // host/devices/input 이벤트 주입
    fn net(&self) -> &dyn NetObserver;         // host/devices/net 플로우 관찰·정책 적용
    fn events(&self) -> EventStream<InstanceEvent>;
}

// guest 프레임 커밋 + device scanout 정확 매칭, 최근 N 프레임 보관
#[async_trait]
trait FrameChannel: Send + Sync {
    fn subscribe(&self, display: u32) -> EventStream<Frame>;
    async fn get(&self, id: &FrameId, deadline: Instant) -> Result<Frame>;
        // 매칭될 때까지 기다림. 이미 밀려났으면 FrameExpired, deadline이 지나면 Timeout. 추측해서 다른 프레임을 주지 않음
    fn latest(&self, display: u32) -> Result<Frame>;
}
struct Frame {
    frame_id: Option<FrameId>,   // 매칭되지 않으면 None (frameId 지정 요청에는 쓰지 않음)
    buffer: FrameBufferRef,      // shm 참조 (복사 없음)
}

#[async_trait]
trait SessionManager: Send + Sync {
    async fn acquire(&self, req: SessionRequest) -> Result<Session>;   // 스냅샷 풀에서 할당
    async fn reset(&self, s: &Session) -> Result<()>;
    async fn release(&self, s: Session) -> Result<()>;
    async fn snapshot(&self, s: &Session, name: &str) -> Result<SnapshotRef>;
}
```

## adp
외부 클라이언트용 프로토콜. 자세한 내용은 `docs/04-adp.md`.

## node
control plane(SaaS 또는 셀프호스팅 도구)이 노드 데몬을 제어하는 API.
```
Node.Info        → { node_id, host_os, arch, capacity{vcpu, mem, gpu}, versions }
Node.Health      → { status, instances, load }
Instance.Create  { image_ref, snapshot_ref?, profile_ref?, device_profile?, policy?, labels } → InstanceId
Instance.Destroy / Get / List(label_selector)
Session.Acquire / Reset / Release
Snapshot.Export  { instance_id } → SnapshotRef (SnapshotStore로)
Snapshot.Import  { snapshot_ref }
Events.Stream    ← 수명주기·리소스 사용량 이벤트 (미터링 근거)
Adp.Endpoint     { session_id } → { url, token }   // ADP 접속 정보 발급
Debug.SetAdb     { instance_id, enabled }         // CTS/VTS용. ADP·MCP에는 노출하지 않음
```

## spi
코어 확장 지점. 자세한 내용은 `docs/09-oss-and-saas.md`. semver로 **안정성을 약속**한다.
