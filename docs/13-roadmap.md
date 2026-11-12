# 13. 로드맵

## 1. 진행 방식
- **스코프를 먼저 고정한다**: `docs/scope.md`의 모든 행이 처음부터 존재한다.
- **계약을 먼저 만든다**: 레이어는 계약을 기준으로 병렬 구현한다.
- **E2E를 먼저 연결한다**: 1주차에 MCP → VMM 호출 경로가 전부 이어져 있고 응답만 비어 있는 상태를 만든다.
- **작업 단위 = 레이어 하나 + 계약 하나.** 사람이든 AI 에이전트든 이 단위로 맡긴다.

## 2. 1주차: 골격 (계약 우선)
- [ ] 모노레포 골격 (이 문서 세트 기준), Cargo workspace, `LAYER.toml`들
- [ ] `build/checklayers`, `tools/patch/lint.sh`, `build/codegen`
- [ ] `contracts/` 8종 초안 (machine, device, guest_link, keymint, instance, adp, node, spi) + `VERSION`
- [ ] 모든 레이어 crate·모듈 스켈레톤 (`Unimplemented` 반환)
- [ ] `base/trace` (Perfetto) + trace ID 전파
- [ ] `testing/e2e` 하네스 + MCP 기준 첫 E2E 시나리오들 (전부 빨간 상태)
- [ ] `versions.toml` TBD 확정 (wick commit, AOSP 태그, GKI, gfxstream, rutabaga)
- [ ] 인프라: 빌드 서버 + AOSP sync, C4A 부팅 하네스, 로컬 PC Linux 듀얼부팅

## 3. 2~4주차: 레이어 병렬 구현 (트랙)
| 트랙 | 범위 |
|---|---|
| A. VMM | wick hook 패치, 범용 vhost-user frontend, shm region, `vmm/boot`, `vmm/disk`, `vmm/vmgenid`, `vmm/scanout` |
| B. 디바이스 | `devices/gpu`(gfxstream), `devices/input`, `devices/net`(usernet), `devices/snd` |
| C. 게스트 | `wick_arm64`/`wick_x86_64` product, GKI 모듈, AVB, SELinux, frameworks hook 패치, inspector, adpd, agent, restore |
| D. 런타임 | instance(FrameChannel 포함), session, snapshot(MemorySource, uffd), identity, keymint, jail, resources, stores |
| E. 서비스·임베더 | inspect(트리 병합, frame sync), screen, input, app, adp_server, mcp, node |

## 4. 1개월 목표 (exit criteria)
**macOS arm64 (+ Linux arm64 on C4A)**에서 상용 품질 기준으로:
- [ ] user 빌드, AVB, SELinux enforcing으로 `boot_completed`
- [ ] gfxstream으로 화면 출력, virtio-input 입력
- [ ] ADP v1: 통합 트리(View + 접근성 + Compose), frameId 동기화, 안정적인 노드 ID, UI·Input·Screen·App 도메인
- [ ] MCP: observe / act / wait_for / app_*
- [ ] 스냅샷과 restore, 복제 고유성 테스트 통과
- [ ] ADP 골든 테스트(views, compose) 통과

**일정 판단 신호**: 첫 주 안에 트랙 C의 부팅이 되면 1개월 목표가 유효하다. 부팅에서 막혀 있으면 일정을 재조정한다.

## 5. 이후 마일스톤
| 단계 | 범위 |
|---|---|
| ADP v2 | WebView CDP 중계, Flutter/RN 시맨틱, 레이어 트리, Network, Log, Overlay, stream, record |
| 호환성 | berberis NativeBridge(arm32→arm64, arm64→x86_64), microG, HAL 보완, CTS 전체 목표 통과율 |
| Linux x86_64 GPU | 로컬 PC에서 gfxstream Vulkan 경로 |
| Windows | wick WHP 첫 부팅 이후: AppContainer, TPM, Vulkan |
| 노드·SaaS 준비 | `embedders/node`, SPI 안정화, Node API v1 |
| 배포 | 서명, SBOM, OTA(A/B 델타), upstream rebase 자동화 |

## 6. 남은 결정
| 항목 | 선택지 | 비고 |
|---|---|---|
| GMS | microG / Google 파트너십 | Play Integrity strong 필요 앱은 제품 범위에서 명시적으로 제외 |
| v1 플랫폼 | macOS arm64 (제안) | 데스크탑 AI 환경 목적, wick에서 가장 성숙, arm64 앱 네이티브 |
| AOSP 태그 | 최신 릴리스 태그 | `versions.toml` |
| TLS 가시성 기본값 | off (제안) | 정책으로만 켬 |
| OWNERS 실제 계정 | - | `OWNERS`의 `<owner>` 교체 |
