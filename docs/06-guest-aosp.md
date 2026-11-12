# 06. 게스트 AOSP (L3)

## 1. 구성 원칙
- **AOSP 릴리스 태그 고정** (`versions.toml [aosp]`) + **hook 패치**(`third_party/aosp/patches/`) + **overlay**(`guest/vendor/` → `$AOSP_ROOT/vendor/droiduse`)
- upstream manifest를 그대로 `repo init`한다. 별도 manifest 레포는 두지 않는다.
- 배포 빌드는 **user 빌드**: adbd 기본 off(호스트 인가로만 활성), AVB on, SELinux enforcing.
- 커널은 **Google 서명 GKI prebuilt + vendor 모듈 + config fragment**. 커널 패치 0이 목표. KMI 밖의 기능이 필요하면 우회하지 않고 설계를 다시 한다.

## 2. 패치보다 먼저 쓰는 확장 지점
아래로 해결되면 패치하지 않는다.
1. BoardConfig, product makefile, `AndroidProducts.mk`
2. RRO(런타임 리소스 오버레이)
3. vendor / product 파티션
4. `BOARD_SEPOLICY_DIRS`, init `.rc`
5. APEX, privileged 앱과 권한 XML
6. AIDL vendor HAL
7. 커널: vendor 모듈(KMI 준수), config fragment

## 3. 초기 패치 목록 (hook만)
| 프로젝트 | 패치 | hook 내용 | 로직 위치 |
|---|---|---|---|
| frameworks/base | `0001-droiduse-viewroot-inspector-hook` | `ViewRootImpl` traversal 지점에서 `DroidInspector` 호출 | `guest/vendor/services/inspector` |
| frameworks/base | `0002-droiduse-wm-activity-event-hook` | WindowManager / ActivityTaskManager 이벤트 통지 | 동상 |
| frameworks/base | `0003-droiduse-webview-debug-policy` | `WebViewFactory` 로드 후 디버깅 정책 적용 (정책은 시스템 속성) | 동상 |
| frameworks/native | `0001-droiduse-sf-frameid-layer-capture` | SurfaceFlinger 프레임 번호·레이어 정보 노출, 레이어별 캡처 진입점 | `guest/vendor/services/adpd` |
| frameworks/libs/binary_translation | (v1 이후) berberis 프런트엔드 | 신규 파일 패치와 기존 파일 수정 패치를 분리 | 해당 프로젝트 |

> 정확한 삽입 지점(파일·함수)은 고정한 AOSP 태그 기준으로 착수 시 확정한다.

### 패치를 피하는 설계 결정
- **WebView**: Chromium은 포크하지 않는다. 프레임워크 hook으로 디버깅만 켠다.
- **Flutter/RN 시맨틱**: agent를 접근성 서비스로 등록하면 노출된다. 패치 불필요.
- **커널**: GKI prebuilt. 불가피할 때만 android-common 태그 기준 패치(`third_party/kernel/patches/`).

## 4. overlay 구성 (`guest/vendor/`)
```
vendor/droiduse/
├─ AndroidProducts.mk
├─ products/
│  ├─ wick_arm64/      BoardConfig.mk · device.mk · fstab · init.wick.rc · *.idc
│  ├─ wick_x86_64/
│  └─ common/          공통 product 설정, RRO
├─ services/
│  ├─ inspector/       프레임워크 hook이 호출하는 실제 로직 (java + jni)
│  ├─ adpd/            게스트 ADP 데몬 (binder ↔ vsock guest_link)
│  ├─ agent/           platform 서명 system 서비스 (UiAutomation, 노드 액션, 접근성 서비스 등록)
│  └─ restore/         VMGenID 감지 → 재초기화
├─ hals/
│  ├─ graphics/        gfxstream guest(GLES/Vulkan), minigbm, HWC (프레임 매칭용 virtio 식별자 보고)
│  ├─ keymint/         호스트 KeyMint와 통신하는 HAL (전용 vsock 포트)
│  ├─ health/  sensors/  audio/
├─ nativebridge/       berberis 통합 설정 (v1 이후)
├─ apps/microg/        privileged 설치 + 권한 XML (signature spoofing 사용 안 함)
├─ sepolicy/           adpd · agent · restore · HAL 전용 도메인
├─ avb/                공개키만 (개인키는 CI 시크릿 / HSM)
└─ protocol/           contracts/guest_link codegen 결과 (CI가 동기화)
```

## 5. 부팅 경로
1. `host/runtime/instance`가 `androidboot.*` 값(하드웨어 이름, 슬롯, verifiedbootstate 등)을 만들어 `VmSpec.boot.bootconfig`로 넘기고, `host/vmm/boot`가 boot.img v4 + vendor_boot를 읽어 kernel, ramdisk(+vendor ramdisk)를 구성하고 bootconfig를 그대로 부착
2. first-stage init이 ramdisk의 fstab으로 `super`의 logical 파티션을 dm-linear로 마운트 (`host/vmm/disk`의 GPT 이름 → `/dev/block/by-name`)
3. AVB: 자체 키로 vbmeta 체인 검증, rollback index
4. second-stage init → vendor 모듈 로드(virtio-blk/net/vsock/gpu/input/snd/pci, vmgenid)
5. `adpd`, `agent`, `restore` 시작 → adpd가 VMM이 붙인 비밀 디스크에서 첫 세션 토큰을 읽는다 → adpd가 `guest_link` 호스트 채널(vsock)로 연결해 Hello → 게스트 채널 포트를 연다 (채널 두 개, `docs/03-contracts.md` guest_link)

## 6. 타깃
| product | 호스트 | 비고 |
|---|---|---|
| `wick_arm64` | macOS arm64, Linux arm64 | **arm64-only** (32비트 앱은 v1 이후 NativeBridge로) |
| `wick_x86_64` | Linux x86_64, Windows x86_64 | ARM 앱은 NativeBridge(berberis arm64→x86_64)로. 시점은 `docs/13-roadmap.md` 남은 결정 |

## 7. 보안 요소
- SELinux enforcing, 신규 컴포넌트마다 최소 권한 도메인
- AVB 자체 키 + rollback 보호
- KeyMint는 호스트 보안 저장소로 래핑 (게스트에 TEE 없음)
- restore hook: 커널 RNG reseed, ANDROID_ID와 기타 식별자 재생성, keystore 재초기화, 네트워크 재협상
- 제어는 인증된 guest_link만. adbd는 기본 off이며 호스트 인가로만 켜진다 (CTS/VTS 측정 전용)

## 8. GMS
- 기본은 **microG**(privileged 앱, 명시적 권한). signature spoofing은 쓰지 않는다.
- GMS 정식 탑재나 Play Integrity strong 등급은 라이선스·인증 영역. 사업 결정 사항 (`docs/13-roadmap.md` 남은 결정).
