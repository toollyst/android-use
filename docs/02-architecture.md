# 02. 아키텍처

## 1. 레이어
```
 clients/           mcp · sdk{rust,ts,python}     ADP 클라이언트 (contracts/adp만 의존)
                       │ ADP (WebSocket JSON-RPC, 인증)
 host/
   L6 entrypoints/  adp_server · node · cli · desktop                      (= Chromium chrome/)
   L5 services/     inspect · screen · input · app · network · log ·
                    overlay · webview · stream · record                     (= Chromium services/, components/)
   L4 runtime/      ★ 조립 계층: Instance 수명주기 · 세션 풀 · 스냅샷 · 고유성 ·
                    프레임 매칭 · 호스트 KeyMint · 샌드박스 · 리소스       (= content/)
   L2 vmm/ devices/ microVM (wick ext) · virtio 디바이스 백엔드 (Android 모름)
   L1 base/         IPC · Perfetto 트레이싱 · 설정 · 에러 · 플랫폼 추상화
 guest/        L3   AOSP overlay · HAL · 게스트 서비스 · 커널 설정
 third_party/  L0   wick · AOSP · GKI · gfxstream · rutabaga (+ 패치)
 contracts/         machine · device · guest_link · keymint · instance · adp · node · spi   (= mojom, 의존 0)
```

| 레이어 | 책임 (이것만 한다) | 하지 않는 것 |
|---|---|---|
| **L0 third_party** | upstream 고정과 패치 | 우리 로직 |
| **L1 base** | 전송(vsock, unix, shm), 트레이싱, 설정, 에러 타입, OS 추상화 | 도메인 지식 |
| **L2 vmm** | VM 생성·시작·정지·스냅샷, boot image v4 컨테이너 로딩(bootconfig는 불투명), 디스크 레이어링, VMGenID, scanout 공유 | Android 의미(`androidboot.*` 해석 포함), UI |
| **L2 devices** | virtio 디바이스를 정확히 구현 (gpu, input, snd, net). 각각 별도 프로세스 | Android 지식 |
| **L3 guest** | 부팅, HAL, 보안(AVB, SELinux), 게스트 서비스(inspector, adpd, agent, restore) | 호스트 정책 |
| **L4 runtime** | vmm + devices + 게스트 이미지를 `Instance`로 조립. 수명주기, 세션 풀, 스냅샷, 고유성, 프레임 매칭(`FrameChannel`), 호스트 KeyMint, 샌드박스, 리소스 회계, SPI 기본 구현 | 도메인 기능 |
| **L5 services** | 도메인별 기능을 끝까지 책임진다 | 전송 방식, 제품 UX |
| **L6 entrypoints** | 호스트 실행 진입점(ADP 서버, node 데몬, CLI, desktop)과 제품 기본값 | 코어 로직 |
| **clients** | ADP 클라이언트(MCP, SDK). `contracts/adp`만 쓴다 | 호스트 내부 접근 |
| **contracts** | 레이어 경계 계약. 자기완결 | 다른 레이어 의존 |

## 2. 의존 규칙
1. 상위 → 하위로만 의존한다. (`LAYERS.toml`)
2. 같은 레벨 형제끼리 직접 의존 금지. 같은 레이어 안의 형제 모듈끼리도 직접 의존하지 않는다. `host/services/screen`은 `host/services/inspect`를 직접 호출하지 않고 runtime의 채널(계약)을 거친다. 예외는 레이어 루트 `LAYER.toml`에 선언한 단방향 간선뿐이다.
3. `contracts/`는 모두가 참조하고, `contracts/` 자신은 다른 레이어에 의존하지 않는다. 외부 crate는 `LAYERS.toml`의 허용 목록에 있는 것만 쓴다.
4. `testing/`은 모든 것을 참조할 수 있지만 제품 코드는 `testing/`에 의존하지 않는다.
5. `clients/`는 `contracts/`에만 의존하고, 제품 코드는 `clients/`에 의존하지 않는다.

### 강제 방법
| 대상 | 도구 |
|---|---|
| Rust crate | `build/checklayers`: `cargo metadata`로 의존 그래프를 검사. 디렉터리별 `LAYER.toml`로 허용 목록을 좁힘 (Chromium DEPS와 같은 역할). 형제 모듈 예외는 레이어 루트 `LAYER.toml`의 단방향 간선, contracts의 외부 crate는 `LAYERS.toml` 허용 목록으로 검사 |
| AOSP vendor 모듈 | Soong `visibility` 속성 |
| 금지 개념 | `LAYERS.toml [forbidden]` 기반 lint (예: devices에 android 개념 금지) |
| 소유권 | `OWNERS`: 계약·패치는 별도 승인 그룹 |

모든 검사는 CI에서 실패로 처리한다.

## 3. 디렉터리 구조 (모노레포)
```
droid-use/
├─ README.md  CLAUDE.md  AGENTS.md  versions.toml  LAYERS.toml  OWNERS  LICENSE  NOTICE
│
├─ host/                           호스트 코드 (위 → 아래 = 레이어 순서)
│  ├─ entrypoints/                 L6  adp_server/  node/  cli/  desktop/
│  ├─ services/                    L5  inspect/  screen/  input/  app/  network/  log/  overlay/  webview/  stream/  record/
│  ├─ runtime/                     L4  instance/  session/  snapshot/  identity/  keymint/  jail/  resources/  stores/
│  ├─ vmm/                         L2  boot/  disk/  vmgenid/  scanout/  api/   (wick ext crates)
│  ├─ devices/                     L2  gpu/  input/  snd/  net/   (vhost-user 백엔드, 각각 바이너리)
│  └─ base/                        L1  ipc/  trace/  config/  error/  platform/
│
├─ guest/                          L3  게스트 Android
│  ├─ vendor/                      → $AOSP_ROOT/vendor/droiduse 로 sync
│  │  ├─ products/{wick_arm64,wick_x86_64,common}/
│  │  ├─ hals/{graphics,keymint,health,sensors,audio}/
│  │  ├─ services/{inspector,adpd,agent,restore}/
│  │  ├─ sepolicy/  avb/  nativebridge/  apps/microg/
│  └─ kernel/{fragments,modules}/
│
├─ clients/                        ADP 클라이언트 (contracts/adp만 의존)
│  ├─ mcp/
│  └─ sdk/{rust,ts,python}/
│
├─ contracts/                      경계 계약 (의존 0)
│  ├─ machine/  device/  guest_link/  keymint/  instance/  adp/  node/  spi/
│  └─ codegen/  compat/
│
├─ third_party/                    L0  upstream + 패치
│  ├─ wick/        src/(submodule) patches/
│  ├─ aosp/        manifest pin(versions.toml) · patches/ · security/   (소스는 외부 체크아웃 $AOSP_ROOT)
│  ├─ kernel/      Kleaf manifest pin · patches/(목표: 비어 있음)
│  ├─ gfxstream/   (패치가 필요할 때만)
│  └─ rutabaga/    (패치가 필요할 때만)
│
├─ testing/        testapps/{views,compose,webview,flutter,rn,gl}/  e2e/  conformance/  fuzz/  perf/  cts/  golden/
├─ build/          checklayers/  codegen/  toolchains/
├─ tools/          setup/  sync/  patch/  rebase/  bundle/  infra/
└─ docs/
```
- **모노레포로 시작한다.** 초기에는 프로토콜이 자주 바뀌고 호스트·게스트를 함께 바꿔야 하므로 계약 변경을 한 번에 반영한다.
- **최상위 디렉터리와 `host/` 아래 레이어 디렉터리 = 미래의 레포 경계.** 외부 기여자가 일부만 다룰 때, 접근 권한 분리가 필요할 때, 프로토콜을 독립 배포할 때 `git filter-repo --subdirectory-filter`로 히스토리째 뗀다.
- `guest/vendor/`는 AOSP 트리에 **rsync 또는 bind mount**로 넣는다. Soong은 기본적으로 symlink된 디렉터리 아래 `Android.bp`를 허용하지 않으므로 symlink를 쓰지 않는다.

## 4. 런타임 구성도
```
  clients/ (MCP · SDK · 디버거 UI)           control plane (SaaS · 셀프호스팅 도구)
        │ ADP: WebSocket JSON-RPC, 인증                │ Node API: TLS, 인증
┌───────┼────────────────── Host ──────────────────────┼────────────────────┐
│ entrypoints: adp_server        CLI        node 데몬 ─┘                     │
│                 │                                                         │
│ services: inspect · screen · input · app · network · log · stream · record│
│                 │  (contracts/instance)                                   │
│ runtime: Instance(FrameChannel) ─ Session pool ─ Snapshot(MemorySource) ─ │
│          Identity ─ KeyMint[jailed] ─ Jail                                │
│      │ (contracts/machine)        │ (contracts/guest_link over vsock)     │
│ vmm: wick + ext [jailed]          │                                       │
│  ├ boot(boot.img/vendor_boot/bootconfig) ├ disk(GPT, CoW 레이어)          │
│  ├ vmgenid ├ scanout ├ vhost-user frontend(범용) + shm region             │
│      │ (contracts/device = vhost-user)                                    │
│ devices: gpu(rutabaga+gfxstream) · input · snd · net(usernet)             │
│          [각각 별도 프로세스, 각각 jailed]                                │
└───────────────────────────────────────────────────────────────────────────┘
┌─────────────────────────── Guest (AOSP user 빌드) ───────────────────────┐
│ GKI + vendor 모듈 · AVB · SELinux enforcing                               │
│ 앱 프로세스(프레임워크 hook) ─┐                                           │
│ system_server hook ──────────┼ binder ─ adpd ─ vsock ─▶ host             │
│ WebView devtools 소켓 ───────┘                                            │
│ SurfaceFlinger(frameId, 레이어) · gfxstream guest · minigbm · HWC         │
│ restore hook(VMGenID) · NativeBridge(berberis) · microG                   │
└───────────────────────────────────────────────────────────────────────────┘
```

## 5. 관측성: 레이어를 관통하는 단일 트레이스
- 호스트는 Perfetto SDK, 게스트는 Android 기본 Perfetto. 둘을 **같은 trace ID와 시간축**으로 병합한다.
- MCP `act` 한 번이 하나의 타임라인으로 보인다:
  `clients/mcp → host/entrypoints/adp_server → host/services/input → host/devices/input → 게스트 InputDispatcher → 앱 렌더링 → SurfaceFlinger → host/devices/gpu scanout → host/services/screen`
- trace ID는 ADP 요청 ID에서 시작해 guest_link 메시지 헤더로 전파한다.

### 증상 → 책임 레이어
| 증상 | 책임 레이어 |
|---|---|
| 노드 bounds가 틀림 | guest/vendor/services/inspector |
| 트리와 스크린샷이 어긋남 | host/runtime/instance (`FrameChannel` 매칭) 또는 host/services/inspect (트리에 frameId 부착) |
| 검은 화면 | host/devices/gpu 또는 guest/vendor/hals/graphics (트레이스로 구분) |
| 입력이 무시됨 | host/devices/input 또는 guest idc 설정 |
| restore가 느림 | host/runtime/snapshot (MemorySource) 또는 vmm |
| 복제된 세션 간 ID·키 중복 | host/runtime/identity + guest/vendor/services/restore |
| 네트워크 차단 | host/services/network (정책) 또는 host/devices/net (전송) |
| MCP 응답 형식 문제 | clients/mcp |
| 부팅 실패 | host/vmm/boot · host/vmm/disk · guest/vendor/products (fstab, init) |
