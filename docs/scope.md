# 스코프 매트릭스

> 모든 기능은 여기 먼저 존재한다. 개발은 칸 채우기다.
> 상태: ☐ 미착수 · ◐ 진행 · ☑ 완료 · — 해당 없음
> 작업을 마치면 해당 칸을 갱신한다 (`CLAUDE.md` §3).

## 1. 플랫폼·실행
| 기능 | 계약 | guest | vmm / devices | runtime | services | embedders | 단계 |
|---|---|---|---|---|---|---|---|
| Android 부팅 (boot.img/vendor_boot/bootconfig) | machine | ☐ products, fstab, init | ☐ vmm/boot | ☐ instance | — | ☐ cli | 1M |
| GPT 컴포지트 디스크 + CoW 레이어 | machine | ☐ fstab | ☐ vmm/disk | ☐ stores | — | — | 1M |
| AVB · SELinux enforcing · user 빌드 | — | ☐ avb, sepolicy | — | — | — | — | 1M |
| 화면 출력 (gfxstream) | device | ☐ hals/graphics | ☐ devices/gpu, vhost-user, shm | ☐ instance | — | — | 1M |
| 좌표 입력 | device | ☐ idc | ☐ devices/input | ☐ InputChannel | ☐ input | ☐ adp, mcp | 1M |
| 오디오 | device | ☐ hals/audio | ☐ devices/snd | ☐ instance | — | — | v2 |
| 네트워크 (usernet) | device | — | ☐ devices/net | ☐ instance | ☐ network | — | 1M |
| egress 정책 | spi | — | ☐ devices/net 훅 | ☐ resources | ☐ network | ☐ adp | v2 |
| KeyMint (호스트) | keymint | ☐ hals/keymint | — | ☐ keymint, jail | — | — | 1M |
| 샌드박스 (VMM + 디바이스별) | — | — | — | ☐ jail | — | — | 1M |

## 2. 인스펙션 (ADP)
| 기능 | 계약 | guest | vmm / devices | runtime | services | embedders | 단계 |
|---|---|---|---|---|---|---|---|
| View 트리 | guest_link, adp | ☐ patch 0001 + inspector | — | ☐ GuestChannel | ☐ inspect | ☐ adp, mcp | 1M |
| 접근성 트리 | guest_link, adp | ☐ agent | — | — | ☐ inspect | ☐ adp | 1M |
| Compose 시맨틱 | guest_link, adp | ☐ inspector | — | — | ☐ inspect | ☐ adp | 1M |
| 통합 트리 병합 | adp | — | — | — | ☐ inspect | ☐ adp | 1M |
| frameId 동기화 | guest_link, device, instance | ☐ patch native 0001, hals/graphics | ☐ gpu scanout export, vmm/scanout | ☐ FrameChannel | ☐ inspect, screen | ☐ adp | 1M |
| 안정적인 노드 ID | adp | ☐ inspector | — | — | ☐ inspect | ☐ adp | 1M |
| 트리 diff 이벤트 | guest_link, adp | ☐ inspector, adpd | — | — | ☐ inspect | ☐ adp | 1M |
| 윈도우·태스크·IME 상태 | guest_link, adp | ☐ patch 0002 | — | — | ☐ inspect, app | ☐ adp | 1M |
| 노드 기반 액션 | guest_link, adp | ☐ agent | — | — | ☐ input | ☐ adp, mcp | 1M |
| 스크린샷 (frameId 지정) | device, adp | — | ☐ vmm/scanout | ☐ FrameChannel | ☐ screen | ☐ adp, mcp | 1M |
| WebView CDP 중계 | guest_link, adp | ☐ patch 0003 | — | — | ☐ webview | ☐ adp | v2 |
| Flutter / RN 시맨틱 | adp | ☐ agent(접근성 등록) | — | — | ☐ inspect | ☐ adp | v2 |
| 레이어 트리 (게임 등) | guest_link, adp | ☐ patch native 0001 | — | — | ☐ inspect | ☐ adp | v2 |
| 스크린캐스트 | adp, spi | — | ☐ vmm/scanout | — | ☐ stream | ☐ adp | v2 |
| Overlay 하이라이트 | adp | ☐ agent | — | — | ☐ overlay | ☐ adp | v2 |
| 네트워크 관찰 | adp | ☐ (TLS 가시성 CA, 정책) | ☐ devices/net | — | ☐ network | ☐ adp | v2 |
| 로그·ANR·크래시 | guest_link, adp | ☐ adpd | — | — | ☐ log, app | ☐ adp | v2 |
| 세션 녹화 | spi | — | — | — | ☐ record | ☐ adp | v2 |

## 3. 앱·세션
| 기능 | 계약 | guest | vmm / devices | runtime | services | embedders | 단계 |
|---|---|---|---|---|---|---|---|
| 앱 설치·실행·종료·목록 | guest_link, adp | ☐ adpd | — | — | ☐ app | ☐ adp, mcp | 1M |
| 파일 push/pull | guest_link, adp | ☐ adpd | — | — | ☐ app | ☐ adp | v2 |
| 세션 acquire/reset/release | instance, adp | — | — | ☐ session | — | ☐ adp, mcp | 1M |
| 스냅샷 생성·restore | machine, device | — | ☐ device state, vmgenid | ☐ snapshot | — | ☐ adp | 1M |
| 복제 고유성 | guest_link | ☐ services/restore | ☐ vmm/vmgenid | ☐ identity | — | — | 1M |
| 디바이스 프로필 | spi | ☐ services/restore | — | ☐ identity | — | ☐ node | v2 |
| 프로필(사용자 데이터) 보존 | spi | — | ☐ vmm/disk | ☐ stores | — | ☐ node | v2 |

## 4. 호환성
| 기능 | 계약 | guest | vmm / devices | runtime | services | embedders | 단계 |
|---|---|---|---|---|---|---|---|
| NativeBridge (berberis) | — | ☐ nativebridge + patch | — | — | — | — | v3 |
| microG | — | ☐ apps/microg | — | — | — | — | v2 |
| CTS용 adb 활성화 경로 | guest_link, node | ☐ adbd, adpd | — | ☐ instance | — | ☐ node, cli | 1M |
| CTS 목표 통과율 | — | ☐ | ☐ | ☐ | — | — | v3 |

## 5. 운영·확장
| 기능 | 계약 | guest | vmm / devices | runtime | services | embedders | 단계 |
|---|---|---|---|---|---|---|---|
| 단일 트레이스 (Perfetto) | 모든 계약 헤더 | ☐ adpd 전파 | ☐ | ☐ | ☐ | ☐ | 1M |
| 채널 인증 | spi | ☐ guest_link·keymint 토큰 | — | ☐ | — | ☐ adp_server, node | 1M |
| 복제 후 재인증 | guest_link, keymint | ☐ adpd, services/restore | — | ☐ identity | — | — | 1M |
| 리소스 회계 이벤트 | spi | — | — | ☐ resources | — | ☐ node | v2 |
| Node API | node | — | — | ☐ | — | ☐ node | v2 |
| 원격 MemorySource | spi | — | — | ☐ snapshot | — | ☐ node 빌더 | v2 |
| 릴리스 번들·서명·SBOM | — | ☐ | ☐ | — | — | ☐ | v2 |

단계: `1M` = 1개월 목표, `v2`/`v3` = 이후 마일스톤 (`docs/13-roadmap.md`)
