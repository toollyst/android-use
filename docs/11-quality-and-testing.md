# 11. 품질과 테스트

## 1. 품질 지표
| 지표 | 측정 |
|---|---|
| 호환성 | CTS/VTS 모듈별 통과율. 회귀 0 유지. **배포 이미지 그대로** 측정 |
| 콜드 부팅 | VM 생성 → `sys.boot_completed` → guest_link Ready |
| restore | 요청 → 세션 Ready (`Restore.Notify` 포함) |
| 입력→화면 | `Input.*` → 결과 프레임 `frameCommitted` |
| 트리 갱신 | 게스트 프레임 커밋 → `UI.treeMutated` 수신 |
| 스크린샷 | 요청 → 응답 |
| 밀도 | 호스트당 동시 세션 수, 세션당 메모리 |
| 고유성 | 복제 세션 간 식별자·키 중복 수 (항상 0) |

모든 지표는 `testing/perf/`에서 측정하고 추세를 저장한다.

## 2. 테스트 계층
| 계층 | 위치 | 주기 |
|---|---|---|
| 단위 | 각 crate | PR마다 |
| 레이어 규칙·패치 lint·codegen 검사 | `build/`, `tools/patch/` | PR마다 |
| 계약 하위호환 | `contracts/compat/` | PR마다 |
| virtio conformance | `testing/conformance/` | PR마다 (해당 경로 변경 시) |
| fuzz | `testing/fuzz/` | 야간 상시 |
| 부팅 매트릭스 | `tools/infra/boot-test.sh` | 게스트·vmm 변경 시 + 야간 |
| **ADP 골든** | `testing/golden/` + `testing/testapps/` | PR마다 (게스트·services 변경 시) |
| E2E (MCP·ADP 기준) | `testing/e2e/` | PR마다 |
| CTS/VTS (배포 이미지) | `testing/cts/` | 야간 서브셋, 릴리스 전체 |
| 보안 회귀 | `testing/e2e/security/` | 야간 |

## 3. ADP 골든 테스트 (제품 핵심이므로 가장 촘촘하게)
- `testing/testapps/`: `views`, `compose`, `webview`, `flutter`, `rn`, `gl`(커스텀 렌더러)
- 화면마다 기대값을 저장한다:
  - 통합 트리 (정규화된 JSON)
  - 주요 노드 bounds (허용 오차)
  - frameId 일치: 트리와 스크린샷이 같은 프레임인가
  - 노드 ID 안정성: 스크롤·애니메이션 후에도 같은 노드가 같은 ID인가
  - 액션 결과: 노드 클릭 → 기대 화면 전이

## 4. 부팅 매트릭스
| 호스트 | 게스트 | 장비 |
|---|---|---|
| Linux x86_64 | wick_x86_64 | GCE (nested 또는 bare metal), 로컬 PC(Linux) |
| Linux arm64 | wick_arm64 | GCE C4A bare metal |
| macOS arm64 | wick_arm64 | Mac 실기기 |
| Windows x86_64 | wick_x86_64 | 로컬 PC(Windows), wick Windows 부팅 이후 |

## 5. CI 구성
- **무료 GitHub Actions**: Rust 빌드·테스트, lint, 패치 lint, 레이어 검사, codegen 검사, 짧은 fuzz
- **self-hosted**: AOSP 빌드, KVM 부팅 테스트, macOS 부팅 테스트(Mac 실기기), GPU 경로 테스트, CTS
- AOSP 빌드는 `guest/`, `third_party/aosp/`, `versions.toml` 변경 시에만. 산출 이미지를 캐시하고 호스트 쪽 CI가 재사용

## 6. 측정 이미지
모든 테스트는 배포 이미지에서 돌린다. CTS/VTS만 호스트 인가로 adbd를 일시적으로 켠다.

## 7. E2E 우선 원칙
기능을 구현하기 전에 `testing/e2e/`에 기대 동작을 먼저 추가한다. 1주차부터 MCP → VMM 전체 경로가 연결된 상태에서, 빨간 테스트가 남은 일의 목록이 된다.
