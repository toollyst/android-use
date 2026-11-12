# 용어집

| 용어 | 뜻 |
|---|---|
| **ADP** | Android DevTools Protocol. CDP 스타일의 Droid Use 외부 프로토콜 |
| **adpd** | 게스트 안의 ADP 데몬. binder ↔ vsock(guest_link) 중계 |
| **agent (게스트)** | platform 서명 system 서비스. UiAutomation, 노드 액션, 접근성 서비스 등록 |
| **inspector** | 프레임워크 hook이 호출하는 게스트 로직. View·Compose 트리 수집 |
| **guest_link** | 호스트 ↔ 게스트 vsock 계약 |
| **frameId** | `{display, vsyncId, sfFrame}`. 트리·스크린샷·이벤트의 기준 프레임 |
| **FrameChannel** | runtime이 제공하는 프레임 계약. 게스트 frameId와 scanout 정확 매칭, 최근 프레임 보관 |
| **NodeId** | 노드 수명 동안 유지되는 불투명 ID |
| **통합 트리** | View·접근성·Compose·WebView·레이어를 병합한 하나의 트리 |
| **wick** | 크로스플랫폼 Firecracker (Linux KVM, macOS HVF, Windows WHP) |
| **ext** | wick의 확장 방식(`vmm::ext`). 우리 VMM 기능은 `host/vmm/` crate로 같은 방식 |
| **overlay** | AOSP 트리에 추가하는 우리 프로젝트 (`guest/vendor/` → `vendor/droiduse`) |
| **hook 패치** | upstream 코드에 호출 지점만 넣는 패치. 로직은 overlay/ext |
| **vhost-user** | virtio 디바이스를 별도 프로세스로 구현하는 표준 프로토콜 |
| **gfxstream** | 렌더링 API 스트리밍 라이브러리. 게스트 GLES/Vulkan → 호스트 GPU |
| **rutabaga** | virtio-gpu 백엔드 추상화 (gfxstream 등 연결) |
| **usernet** | 우리 user-mode 네트워크 스택 (smoltcp 기반 NAT) |
| **VMGenID** | VM 세대 ID. 스냅샷 복제를 게스트가 감지하게 함 |
| **restore hook** | VMGenID 변경 시 게스트 재초기화 (RNG, 식별자, keystore, 네트워크) |
| **MemorySource** | 스냅샷 메모리 페이지 공급 SPI. 로컬 파일 또는 원격 |
| **CPU 템플릿** | 게스트에 보이는 CPU 기능 고정. 스냅샷 이식성 |
| **GKI** | Generic Kernel Image. Google 서명 커널 + vendor 모듈 |
| **KMI** | Kernel Module Interface. vendor 모듈이 지켜야 하는 커널 ABI |
| **berberis** | AOSP의 오픈소스 바이너리 번역 프레임워크 (NativeBridge) |
| **ASB** | Android Security Bulletin. 월간 보안 패치 |
| **SPI** | 코어 확장 지점 트레이트 (`contracts/spi`) |
| **labels** | 인스턴스·세션·스냅샷의 불투명 key-value. 코어는 해석하지 않음 |
| **node** | 머신 한 대에서 여러 인스턴스를 운영하는 데몬 (`host/entrypoints/node`) |
| **control plane** | 여러 노드를 관리하는 상위 계층 (SaaS, 이 레포 밖) |
