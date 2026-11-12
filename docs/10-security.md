# 10. 보안

## 1. 위협 모델
- **신뢰하지 않음**: 게스트 전체(앱, AI가 실행한 행동, 게스트 커널 포함), 외부 ADP 클라이언트(인증 전)
- **보호 대상**: 호스트, 다른 세션, 호스트 키와 시크릿, 다른 테넌트(SaaS)
- 가정: 게스트는 언제든 악의적인 virtio 입력과 guest_link 메시지를 보낼 수 있다.

## 2. 격리
| 대상 | 방법 |
|---|---|
| 게스트 ↔ 호스트 | microVM (KVM / HVF / WHP) |
| VMM 프로세스 | `host/runtime/jail`: Linux jailer, macOS seatbelt/App Sandbox, Windows AppContainer |
| 디바이스 백엔드 | **각각 별도 프로세스, 각각 최소 권한 샌드박스.** 공격면이 가장 큰 gpu는 가장 강하게 |
| 호스트 KeyMint | 별도 프로세스, 전용 샌드박스, 호스트 보안 저장소 외 접근 없음 (`host/runtime/keymint`) |
| 세션 ↔ 세션 | 인스턴스별 프로세스 집합, 별도 디스크 CoW 레이어, 별도 토큰 |
| 네트워크 | usernet 기본 차단 + allowlist 정책, 세션별 egress 로깅 |

## 3. 게스트 보안
- user 빌드, adbd 기본 off (호스트 인가로만 활성, CTS/VTS 전용)
- AVB 자체 키, rollback index
- SELinux enforcing, 신규 컴포넌트마다 전용 도메인
- KeyMint는 호스트(`host/runtime/keymint`, 별도 jailed 프로세스)에서 실행, 키는 호스트 보안 저장소로 래핑 (Secure Enclave / TPM)
- 복제 고유성: VMGenID + restore hook (`docs/07-runtime.md` §4)

## 4. 채널 보안
- 모든 채널 인증 (ADP, Node API, 스트림, guest_link 세션 토큰)
- guest_link는 버전 협상과 메시지 크기 제한, 스키마 검증 후 처리. 크기 제한은 호스트·게스트 설정이고 기본값은 gRPC 기본값(4 MiB)이다
- guest_link 인증: Hello와 Auth 말고는 모든 호출이 양방향 모두 세션 토큰을 바이너리 메타데이터(`droiduse-session-token-bin`)에 싣는다. 유효한 토큰이 없으면 gRPC `UNAUTHENTICATED`
- ADP 인증: 업그레이드의 `Authorization: Bearer` 헤더 또는 첫 메시지 `Connection.auth`. 인증 전 메시지는 모두 거부한다. URL 쿼리 문자열의 토큰은 받지 않는다
- TLS 가시성(세션별 CA 주입)은 정책으로만 켜고, 켜진 세션은 메타데이터에 표시
- 복제 후 재인증: restore 직후 복제본마다 새 토큰을 발급하고 이전 토큰은 폐기한다

## 4.1 게스트 내부 노출면
- 게스트 안의 다른 앱도 신뢰하지 않는다.
- 인스펙션·디버그용으로 연 경로(WebView 디버깅, inspector hook, agent, adbd, TLS 가시성 CA)는 adpd와 호스트만 쓸 수 있어야 하고, 정책으로만 켜며, AVB를 깨지 않는다.

## 5. fuzzing (CI 상시)
- virtio 큐 파서 (모든 devices)
- `host/vmm/boot` 이미지 파서, `host/vmm/disk` GPT/레이어 파서
- usernet 패킷 처리
- guest_link 디코더, keymint 프레이밍 디코더, ADP 디코더
- 스냅샷 메타데이터 파서
- 프레임 매칭 입력 (게스트가 보낸 식별자는 신뢰하지 않고 검증)

## 6. 키와 시크릿
- AVB 서명키, 플랫폼 서명키: CI 시크릿 / HSM. 레포에는 공개키만
- 키 순환 runbook (`docs/runbooks/`)
- 릴리스 번들: 서명, SBOM, provenance

## 7. 공급망
- 의존성 고정 (`Cargo.lock`, `versions.toml`)
- 재현 가능한 빌드 목표
- Renovate 업데이트는 테스트 통과 후 머지
