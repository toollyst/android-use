# 13. 로드맵

## 1. 진행 원칙
- **마일스톤은 게이트로만 정의한다.** 각 마일스톤에는 목표와 검증 가능한 완료 기준만 있고 기간은 없다. 게이트를 통과해야 다음 마일스톤으로 간다.
- **품질 기준을 낮춰서 게이트를 통과시키지 않는다.** 상용 품질(P6)은 첫 마일스톤부터 기본값이다. user 빌드, AVB, SELinux enforcing, 프로세스별 샌드박스, 채널 인증은 나중에 붙이는 기능이 아니다.
- **스코프 → 계약 → E2E 순서.** 기능은 `docs/scope.md`에 먼저 있고, 계약을 먼저 만들고, E2E 테스트를 먼저 붙인다. 빨간 테스트가 남은 일이다.
- **레이어는 계약을 기준으로 병렬로 진행한다.** 구체적인 할 일은 코드와 지시로 정한다.

## 2. v1의 정의
v1은 README의 세 차별점을 모두 상용 품질로 충족한 첫 릴리스다.
1. **microVM 밀도와 복제 안전 스냅샷**: 깨끗한 세션을 즉시 제공하고, 복제본끼리 식별자와 키가 겹치지 않는다.
2. **CDP급 인스펙션**: View, 접근성, Compose, WebView(CDP), 렌더 레이어, Flutter/RN 시맨틱을 하나의 트리로 합친다. frameId 정확 동기화와 안정적인 노드 ID를 보장한다.
3. **크로스플랫폼 로컬 실행**: Linux(x86_64·arm64), macOS(arm64), Windows(x86_64)에서 같은 바이너리, 같은 API로 동작한다.

v1 = 아래 M0~M6 게이트를 모두 통과한 상태.

## 3. 마일스톤

### M0. 골격
레이어와 계약이 코드로 존재하고, 위반은 CI가 잡으며, 클라이언트에서 VMM까지 호출 경로가 끝까지 이어져 있다.

게이트:
- 계약 8종(machine, device, guest_link, keymint, instance, adp, node, spi)이 버전과 함께 존재한다
- 레이어 의존, 금지 개념, 패치 규칙, codegen 최신 여부 검사가 CI에서 위반 사례를 실제로 실패시킨다
- `clients/mcp` → `host/entrypoints/adp_server` → … → `host/vmm` 경로가 연결되어 있다(응답은 미구현). 하나의 trace ID가 이 경로 전체를 관통한다
- v1의 E2E 시나리오가 모두 존재하고, 전부 빨간 상태다
- `versions.toml`의 upstream 조합이 고정되어 있다

검증 환경: 로컬 Mac + GitHub Actions. AOSP 태그를 고정하면 빌드 서버를 만들고 AOSP sync를 시작한다.

### M1. 상용 품질 부팅: macOS arm64 + Linux arm64
같은 arm64 게스트 이미지가 두 호스트(HVF, KVM)에서 상용 보안 구성으로 부팅하고, 화면·입력·네트워크가 동작한다.

게이트:
- user 빌드, AVB, SELinux enforcing으로 `boot_completed`에 도달하고 guest_link 인증이 성립한다
- gfxstream 화면 출력, virtio-input 입력, usernet 네트워크, 호스트 KeyMint가 동작한다
- VMM과 각 디바이스 프로세스, KeyMint가 각자의 샌드박스에서 돈다
- 두 호스트에서 부팅 테스트를 통과하고, 부팅 지표를 측정해 기록한다
- 디바이스 conformance와 fuzz가 CI에서 상시로 돈다. CTS/VTS를 배포 이미지에서 측정해 기준선을 기록한다

검증 환경: 빌드 서버(이미지 빌드), 평소 부팅 반복은 Mac(HVF), 게이트 검증은 Mac + C4A bare metal(KVM).

### M2. ADP 코어
AI와 클라이언트가 앱 화면을 프레임 단위로 정확히 읽고 조작한다.

게이트:
- View, 접근성, Compose를 합친 통합 트리. 트리, 스크린샷, 이벤트의 frameId가 정확히 일치한다
- 노드 ID가 노드 수명 동안 유지되고, 트리 diff 이벤트가 나온다
- 윈도우·태스크·IME 상태, 노드 기반 액션, 좌표 입력, frameId 지정 스크린샷, 앱 관리가 동작한다
- MCP `observe` / `act` / `wait_for` / `app_*`가 동작한다
- views·compose 테스트 앱의 ADP 골든 테스트(트리, bounds, frameId 일치, 노드 ID 안정성, 액션 결과)를 통과한다
- 트리 갱신, 스크린샷, 입력→화면 지연을 측정해 기록한다

검증 환경: M1과 같음. 골든 테스트는 평소 Mac에서 돌리고, 게이트 검증은 두 호스트 모두에서 한다.

### M3. 스냅샷과 세션
작업마다 깨끗한 Android를 즉시 제공하고, 복제본은 서로 독립적이다.

게이트:
- 골든 스냅샷 생성, lazy restore, 디바이스 상태 복원, 디스크 CoW 레이어가 동작한다
- 세션 acquire / reset / release가 동작한다
- 복제 세션 간 식별자·키·토큰 중복이 0이다
- 호환되지 않는 스냅샷의 restore는 거부한다
- restore 시간과 밀도를 측정해 기록한다

검증 환경: M1과 같음. 밀도 지표는 C4A bare metal에서 측정한다.

### M4. 인스펙션 완성
v1 인스펙션 범위 전체가 하나의 트리로 들어온다.

게이트:
- WebView CDP 중계, 렌더 레이어 트리(게임·커스텀 렌더러), Flutter/RN 시맨틱이 통합 트리에 들어간다
- 모든 테스트 앱(views, compose, webview, flutter, rn, gl)의 ADP 골든 테스트를 통과한다
- 게스트 내부 노출면(WebView 디버깅, inspector, agent) 보안 회귀 테스트를 통과한다

검증 환경: M1과 같음.

### M5. 플랫폼 완성
v1의 모든 호스트에서 같은 바이너리, 같은 API로 M1~M4가 성립한다.

게이트:
- Linux x86_64(gfxstream Vulkan 경로)와 Windows x86_64(WHP)에서 상용 보안 구성으로 부팅한다
- Windows의 AppContainer 샌드박스와 TPM 키 래핑이 동작한다
- wick WHP 게스트 부팅처럼 upstream이 필요한 부분은 upstream-first로 기여해서 해결한다
- 부팅 매트릭스의 모든 호스트에서 같은 E2E와 ADP 골든 테스트를 통과한다

검증 환경: M1 환경에 더해 Linux x86_64(GCE x86, GPU 경로는 로컬 PC Linux)와 Windows(로컬 PC). 추가 장비 비용은 착수할 때 산정한다.

### M6. v1 릴리스
배포할 수 있는 상용 1.0.

게이트:
- 릴리스 번들에 서명, SBOM, provenance가 붙고, 재현 가능한 빌드가 된다
- Apache-2.0 코어만으로 셀프호스팅 단일 노드가 완결된다(P9)
- 보안 검토를 마치고, 운영 runbook이 갖춰져 있다
- CTS/VTS가 기준선 대비 회귀 0이다

검증 환경: 부팅 매트릭스 전체 (`docs/11-quality-and-testing.md` §4).

## 4. 검증 환경과 비용 기준
### 운영 방식
| 장비 | 역할 | 켜는 시점 |
|---|---|---|
| 로컬 Mac (Apple silicon) | 평소 개발, arm64 게스트 부팅·테스트 반복(HVF) | 상시 |
| 빌드 서버: GCE n2-standard-96 **Spot** (서울) + Local SSD | AOSP·커널·호스트 빌드 | 빌드할 때만. 작업이 끝나면 끈다 |
| C4A bare metal: c4a-highmem-96-metal (도쿄) | Linux arm64(KVM) 게이트 검증, 밀도 지표 | **게이트 검증할 때만**. 끝나면 바로 끈다 |

- 게이트 검증은 비용 때문에 생략하거나 한쪽 호스트만으로 대신하지 않는다. 비용은 켜 두는 시간으로 관리한다.

### 비용 기준 (2026-11 GCP Billing Catalog 단가, 1달러 = 1,400원 가정)
| 항목 | 단가 | 원화 |
|---|---|---|
| 빌드 서버 (Spot, Local SSD 포함) | $1.55/h | 약 2,200원/h |
| C4A bare metal (온디맨드) | $7.27/h | 약 1만 원/h |
| 디스크 (소스 500GB + C4A 부트 200GB) | 약 $3/일 | 약 4,000원/일 (VM을 꺼도 나감) |

| 하루 사용 패턴 | 하루 비용 |
|---|---|
| 빌드 서버 6~8시간 | **약 2만 원** |
| 위 + C4A 게이트 검증 1시간 | 약 3만 원 |

- 단가는 바뀔 수 있다. 장비 구성을 바꿀 때 다시 조회한다.
- Spot 빌드 서버는 GCP가 회수할 수 있다. 회수되면 다시 띄워 빌드를 이어간다(소스 디스크는 남는다).

## 5. v1 이후
| 단계 | 범위 |
|---|---|
| ADP 확장 | Network(관찰·정책), Log·ANR·크래시, Overlay, 스크린캐스트, 세션 녹화, 파일 push/pull |
| 호환성 | NativeBridge(berberis), microG, 오디오, CTS/VTS 목표 통과율 |
| 노드·SaaS 준비 | Node API v1, SPI 안정화, 리소스 회계 이벤트, 원격 MemorySource, 디바이스·사용자 프로필 |
| 배포 운영 | OTA(A/B 델타), upstream rebase 자동화 |

## 6. 남은 결정
| 항목 | 선택지 | 비고 |
|---|---|---|
| GMS | microG / Google 파트너십 | Play Integrity strong이 필요한 앱은 제품 범위에서 명시적으로 제외 |
| TLS 가시성 기본값 | off (제안) | 정책으로만 켬 |
| x86_64 호스트에서 ARM 앱 실행 | v1 포함 / v1 이후 | x86_64 호스트에서 arm64 전용 앱을 돌리려면 NativeBridge(berberis)가 필요하다. 현재는 v1 이후로 분류 |
| OWNERS 실제 계정 | - | `OWNERS`의 `<owner>` 교체 |
