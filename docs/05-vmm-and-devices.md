# 05. VMM과 디바이스 (L2)

## 1. wick과의 관계
- wick = upstream Firecracker를 submodule로 두고, 작은 패치 시리즈와 OS별 플랫폼 백엔드로 Linux·macOS·Windows를 지원하는 프로젝트 (Apache-2.0).
  - Linux: upstream Firecracker (KVM)
  - macOS (Apple silicon, 15+): Hypervisor.framework. SMP, virtio-blk, vsock, virtio-net(vmnet), PCI/MSI-X, 스냅샷, userfaultfd, vhost-user 동작. 게스트는 aarch64만
  - Windows (x86_64): WHP. 빌드와 CI는 되지만 작성 시점에 실기기 게스트 부팅 전
- wick은 자체 기능을 `crates/ext`(`vmm::ext`)로 분리한다. **우리는 같은 방식으로 wick 위에 한 층을 더 얹는다.**
```
Firecracker tag ─(wick patches: wick 소관)─▶ wick commit ─(third_party/wick/patches: 우리)─▶ + vmm/ crates ─▶ VMM 바이너리
```

## 2. wick 패치 (보수적)
`third_party/wick/patches/`. 원칙: **확장 지점과 범용 기능만**. 범용 기능은 wick에 upstream PR을 먼저 올린다.

| 패치 | 내용 | upstream 후보 |
|---|---|---|
| ext 디바이스 등록 hook | 우리 crate가 디바이스·API를 등록할 지점 (wick ext API로 충분하면 불필요) | wick |
| 부트 페이로드 hook | 커널 로딩 전에 ext 로더가 kernel/initrd/cmdline을 공급할 지점 | wick |
| 범용 vhost-user frontend | upstream의 block 전용 구현을 device type 파라미터화. feature/protocol 협상, device state 전송 | wick / Firecracker |
| virtio shared memory region | PCI BAR로 shm region 노출 (gfxstream blob) | wick / Firecracker |
| VMGenID 플랫폼 지원 | upstream VMGenID를 macOS·Windows 백엔드에서 동작하게 | wick |

> 착수 전 wick의 `vmm::ext` 실제 API를 확인하고, ext로 해결되는 항목은 패치 목록에서 뺀다.

## 3. vmm/ ext crates (적극적)
| crate | 책임 |
|---|---|
| `vmm/boot` | boot image v4 + vendor_boot **컨테이너 포맷** 파싱(헤더, kernel, ramdisk, vendor ramdisk), ramdisk 결합, **bootconfig** 섹션 부착. bootconfig 내용은 불투명 key=value로 받기만 한다 (`androidboot.*` 값을 만드는 것은 `runtime/instance`). x86_64(cmdline/boot params)와 aarch64(FDT) 모두 지원 |
| `vmm/disk` | **GPT 컴포지트 디스크**(super, userdata, metadata, misc 등 이름 있는 파티션 → `/dev/block/by-name`) + **CoW 레이어링**(base ro / snapshot / profile) |
| `vmm/vmgenid` | 세대 ID 갱신 API, restore 시 자동 증가 |
| `vmm/scanout` | gpu 백엔드의 scanout export를 호스트로 전달하는 fast path (링 버퍼와 frameId 매칭은 `runtime/instance`의 `FrameChannel`) |
| `vmm/api` | Firecracker REST API 확장 (boot image v4 부팅 스펙, 디스크 레이어, 세대 ID 등) + runtime용 `Machine` REST 클라이언트. Android 의미는 없음 (스펙 이름도 포맷 이름 `BootImageV4`) |

## 4. 디바이스 백엔드 (devices/)
모두 **vhost-user 백엔드 별도 바이너리**로 만들고, 각각 `runtime/jail`의 샌드박스에서 실행한다. Android 지식 금지.

| 디바이스 | 기반 | 비고 |
|---|---|---|
| `devices/gpu` | rust-vmm `vhost-device-gpu` 계열 + rutabaga + **gfxstream** | 호스트 렌더러: Linux Vulkan / macOS MoltenVK(Metal) / Windows Vulkan. blob resource(shm region), scanout export, device state 직렬화. SwiftShader는 GPU 없는 호스트용 fallback일 뿐 기본 경로가 아님 |
| `devices/input` | virtio-input | 멀티터치(MT protocol B), 키보드. 호스트가 이벤트를 직접 주입 |
| `devices/snd` | rust-vmm `vhost-device-sound` 계열 | 오디오가 필요한 앱 호환 |
| `devices/net` | vhost-user-net + **usernet**(smoltcp 기반 NAT) | root·entitlement 없이 동작. 플로우 관찰 훅, egress 정책 적용 지점(정책 결정은 services/network, SPI `EgressUpstream`으로 업스트림 교체) |

> 호스트 KeyMint는 virtio 디바이스가 아니고 Android 개념을 다루므로 `devices/`가 아니라 `runtime/keymint`에 둔다 (`docs/07-runtime.md`).

### 공통 요건
- device state 저장·복원 지원 (스냅샷에 포함)
- 신뢰할 수 없는 게스트 입력에 대한 fuzzing 타깃 제공 (`testing/fuzz/`)
- conformance 테스트 (`testing/conformance/`)
- 트레이스 이벤트 발행 (`base/trace`)

## 5. 플랫폼별 메모
| 호스트 | 하이퍼바이저 | 게스트 | 그래픽 | 샌드박스 |
|---|---|---|---|---|
| Linux x86_64 / arm64 | KVM | 같은 아키텍처 | Vulkan | Firecracker jailer |
| macOS arm64 | Hypervisor.framework | arm64 (**arm64-only 이미지**, Apple silicon은 AArch32 미지원) | MoltenVK → Metal | seatbelt / App Sandbox |
| Windows x86_64 | WHP | x86_64 | Vulkan | AppContainer |
