# 스코프 매트릭스

> 모든 기능은 여기 먼저 존재한다. 개발은 칸 채우기다.
> 상태: ☐ 미착수 · ◐ 진행 · ☑ 완료 · — 해당 없음
> 작업을 마치면 해당 칸을 갱신한다 (`CLAUDE.md` §3).

## 1. 플랫폼·실행
| 기능 | 계약 | guest | vmm / devices | runtime | services | entrypoints · clients | 단계 |
|---|---|---|---|---|---|---|---|
| Android 부팅 (boot.img/vendor_boot/bootconfig) | machine | ☐ products, fstab, init | ☐ host/vmm/boot | ☐ instance | — | ☐ cli | M1 |
| GPT 컴포지트 디스크 + CoW 레이어 | machine | ☐ fstab | ☐ host/vmm/disk | ☐ stores | — | — | M1 |
| AVB · SELinux enforcing · user 빌드 | — | ☐ avb, sepolicy | — | — | — | — | M1 |
| 화면 출력 (gfxstream) | device | ☐ hals/graphics | ☐ host/devices/gpu, vhost-user, shm | ☐ instance | — | — | M1 |
| 좌표 입력 | device | ☐ idc | ☐ host/devices/input | ☐ InputChannel | ☐ input | ☐ adp, mcp | M1 |
| 오디오 | device | ☐ hals/audio | ☐ host/devices/snd | ☐ instance | — | — | v1 이후 |
| 네트워크 (usernet) | device | — | ☐ host/devices/net | ☐ instance | ☐ network | — | M1 |
| egress 정책 | spi | — | ☐ host/devices/net 훅 | ☐ resources | ☐ network | ☐ adp | v1 이후 |
| KeyMint (호스트) | keymint | ☐ hals/keymint | — | ☐ keymint, jail | — | — | M1 |
| 샌드박스 (VMM + 디바이스별) | — | — | — | ☐ jail | — | — | M1 |
| 호스트: macOS arm64 · Linux arm64 | machine, device | ☐ products/wick_arm64 | ☐ wick HVF·KVM, host/devices/gpu(MoltenVK·Vulkan) | ☐ jail(seatbelt·jailer), keymint(Secure Enclave·TPM) | — | — | M1 |
| 호스트: Linux x86_64 · Windows x86_64 | machine, device | ☐ products/wick_x86_64 | ☐ wick KVM·WHP, host/devices/gpu(Vulkan) | ☐ jail(jailer·AppContainer), keymint(TPM) | — | — | M5 |

## 2. 인스펙션 (ADP)
| 기능 | 계약 | guest | vmm / devices | runtime | services | entrypoints · clients | 단계 |
|---|---|---|---|---|---|---|---|
| View 트리 | guest_link, adp | ☐ patch 0001 + inspector | — | ☐ GuestChannel | ☐ inspect | ☐ adp, mcp | M2 |
| 접근성 트리 | guest_link, adp | ☐ agent | — | — | ☐ inspect | ☐ adp | M2 |
| Compose 시맨틱 | guest_link, adp | ☐ inspector | — | — | ☐ inspect | ☐ adp | M2 |
| 통합 트리 병합 | adp | — | — | — | ☐ inspect | ☐ adp | M2 |
| frameId 동기화 | guest_link, device, instance | ☐ patch native 0001, hals/graphics | ☐ gpu scanout export, host/vmm/scanout | ☐ FrameChannel | ☐ inspect, screen | ☐ adp | M2 |
| 안정적인 노드 ID | adp | ☐ inspector | — | — | ☐ inspect | ☐ adp | M2 |
| 트리 diff 이벤트 | guest_link, adp | ☐ inspector, adpd | — | — | ☐ inspect | ☐ adp | M2 |
| 윈도우·태스크·IME 상태 | guest_link, adp | ☐ patch 0002 | — | — | ☐ inspect, app | ☐ adp | M2 |
| 노드 기반 액션 | guest_link, adp | ☐ agent | — | — | ☐ input | ☐ adp, mcp | M2 |
| 스크린샷 (frameId 지정) | device, adp | — | ☐ host/vmm/scanout | ☐ FrameChannel | ☐ screen | ☐ adp, mcp | M2 |
| WebView CDP 중계 | guest_link, adp | ☐ patch 0003 | — | — | ☐ webview | ☐ adp | M4 |
| Flutter / RN 시맨틱 | adp | ☐ agent(접근성 등록) | — | — | ☐ inspect | ☐ adp | M4 |
| 레이어 트리 (게임 등) | guest_link, adp | ☐ patch native 0001 | — | — | ☐ inspect | ☐ adp | M4 |
| 스크린캐스트 | adp, spi | — | ☐ host/vmm/scanout | — | ☐ stream | ☐ adp | v1 이후 |
| Overlay 하이라이트 | adp | ☐ agent | — | — | ☐ overlay | ☐ adp | v1 이후 |
| 네트워크 관찰 | adp | ☐ (TLS 가시성 CA, 정책) | ☐ host/devices/net | — | ☐ network | ☐ adp | v1 이후 |
| 로그·ANR·크래시 | guest_link, adp | ☐ adpd | — | — | ☐ log, app | ☐ adp | v1 이후 |
| 세션 녹화 | spi | — | — | — | ☐ record | ☐ adp | v1 이후 |

## 3. 앱·세션
| 기능 | 계약 | guest | vmm / devices | runtime | services | entrypoints · clients | 단계 |
|---|---|---|---|---|---|---|---|
| 앱 설치·실행·종료·목록 | guest_link, adp | ☐ adpd | — | — | ☐ app | ☐ adp, mcp | M2 |
| 파일 push/pull | guest_link, adp | ☐ adpd | — | — | ☐ app | ☐ adp | v1 이후 |
| 세션 acquire/reset/release | instance, adp | — | — | ☐ session | — | ☐ adp, mcp | M3 |
| 스냅샷 생성·restore | machine, device | — | ☐ device state, vmgenid | ☐ snapshot | — | ☐ adp | M3 |
| 복제 고유성 | guest_link | ☐ services/restore | ☐ host/vmm/vmgenid | ☐ identity | — | — | M3 |
| 디바이스 프로필 | spi | ☐ services/restore | — | ☐ identity | — | ☐ node | v1 이후 |
| 프로필(사용자 데이터) 보존 | spi | — | ☐ host/vmm/disk | ☐ stores | — | ☐ node | v1 이후 |

## 4. 호환성
| 기능 | 계약 | guest | vmm / devices | runtime | services | entrypoints · clients | 단계 |
|---|---|---|---|---|---|---|---|
| NativeBridge (berberis) | — | ☐ nativebridge + patch | — | — | — | — | v1 이후 |
| microG | — | ☐ apps/microg | — | — | — | — | v1 이후 |
| CTS용 adb 활성화 경로 | guest_link, node | ☐ adbd, adpd | — | ☐ instance | — | ☐ node, cli | M1 |
| CTS 목표 통과율 | — | ☐ | ☐ | ☐ | — | — | v1 이후 |

## 5. 운영·확장
| 기능 | 계약 | guest | vmm / devices | runtime | services | entrypoints · clients | 단계 |
|---|---|---|---|---|---|---|---|
| 단일 트레이스 (Perfetto) | 모든 계약 헤더 | ☐ adpd 전파 | ☐ | ☐ | ☐ | ☐ | M0 |
| 채널 인증 | spi | ☐ guest_link·keymint 토큰 | — | ☐ | — | ☐ adp_server, node | M1 |
| 복제 후 재인증 | guest_link, keymint | ☐ adpd, services/restore | — | ☐ identity | — | — | M3 |
| 리소스 회계 이벤트 | spi | — | — | ☐ resources | — | ☐ node | v1 이후 |
| Node API | node | — | — | ☐ | — | ☐ node | v1 이후 |
| 원격 MemorySource | spi | — | — | ☐ snapshot | — | ☐ node 빌더 | v1 이후 |
| 릴리스 번들·서명·SBOM | — | ☐ | ☐ | — | — | ☐ | M6 |

단계: `M0`~`M6` = v1 마일스톤, `v1 이후` = v1 다음 단계 (`docs/13-roadmap.md`)
