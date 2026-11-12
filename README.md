# Droid Use

> **Android용 Playwright/CDP.**
> AI가 Android 앱을 웹처럼 구조적으로 읽고 정확하게 조작할 수 있게 하는, 가볍고 격리된 상용급 런타임.

- 런타임: [wick](https://github.com/toollyst/wick)(크로스플랫폼 Firecracker) microVM + Rust 디바이스 백엔드 + 직접 빌드한 AOSP
- 핵심 프로토콜: **ADP (Android DevTools Protocol)**: CDP 스타일. MCP는 ADP 위의 클라이언트 중 하나
- 호스트: Linux · macOS(Apple silicon) · Windows (wick 지원 범위를 따름)
- 라이선스: Apache-2.0 (오픈 코어). 클라우드 SaaS는 별도 비공개 레포에서 SPI로 확장

## 상태
설계 단계. 이 레포는 **문서와 골격**으로 시작하고, 코드는 `docs/13-roadmap.md`의 M0(골격, 계약 우선)부터 게이트 순서로 채운다.

## 차별점 (셋 다 v1 필수)
1. **microVM 밀도와 스냅샷**: 복제가 안전한 스냅샷 풀로 작업마다 깨끗한 Android를 즉시 제공
2. **CDP급 인스펙션**: View · 접근성 · Compose · WebView(CDP) · 렌더 레이어를 하나의 트리로 합치고, 프레임 동기화와 안정적인 노드 ID를 보장
3. **크로스플랫폼 로컬 실행**: 같은 바이너리, 같은 API

## 레포 구조
```
host/            호스트 코드 (위 → 아래 = 레이어 순서)
  entrypoints/   L6  ADP 서버 · node 데몬 · CLI · desktop
  services/      L5  도메인 기능 (트리 · 화면 · 입력 · 앱 · 네트워크 …)
  runtime/       L4  VM·디바이스·게스트를 인스턴스로 조립, 수명주기·스냅샷
  vmm/ devices/  L2  microVM · virtio 디바이스 (Android 모름)
  base/          L1  공통 유틸
guest/           L3  게스트 Android (AOSP overlay · HAL · 게스트 서비스)
clients/             ADP 클라이언트 (MCP · SDK)
contracts/           레이어 경계 계약 (의존 0)
third_party/     L0  upstream + 패치
testing/ build/ tools/ docs/
```
자세한 구조와 의존 규칙: [docs/02-architecture.md](docs/02-architecture.md)

## 문서 지도
| 문서 | 내용 |
|---|---|
| [CLAUDE.md](CLAUDE.md) | **코딩 에이전트 작업 규칙 (먼저 읽기)** |
| [docs/00-vision.md](docs/00-vision.md) | 비전, 사용 사례, 경쟁 구도 |
| [docs/01-principles.md](docs/01-principles.md) | 불변 원칙 |
| [docs/02-architecture.md](docs/02-architecture.md) | 레이어, 의존 규칙, 디렉터리 구조, 트레이싱 |
| [docs/03-contracts.md](docs/03-contracts.md) | 레이어 경계 계약 전체 |
| [docs/04-adp.md](docs/04-adp.md) | ADP 프로토콜 설계 (제품 핵심) |
| [docs/05-vmm-and-devices.md](docs/05-vmm-and-devices.md) | wick 패치·확장, virtio 디바이스 백엔드 |
| [docs/06-guest-aosp.md](docs/06-guest-aosp.md) | AOSP 패치·overlay, HAL, 커널 |
| [docs/07-runtime.md](docs/07-runtime.md) | 인스턴스 조립, 스냅샷, 고유성, 샌드박스 |
| [docs/08-upstream-and-patches.md](docs/08-upstream-and-patches.md) | upstream 추적과 패치 거버넌스 |
| [docs/09-oss-and-saas.md](docs/09-oss-and-saas.md) | 오픈 코어 경계, SPI, SaaS 확장 |
| [docs/10-security.md](docs/10-security.md) | 위협 모델과 보안 요건 |
| [docs/11-quality-and-testing.md](docs/11-quality-and-testing.md) | 품질 지표, 테스트 계층, CI |
| [docs/12-infra.md](docs/12-infra.md) | 빌드·테스트 장비 구성 |
| [docs/13-roadmap.md](docs/13-roadmap.md) | 실행 계획, 마일스톤, 남은 결정 |
| [docs/scope.md](docs/scope.md) | 전체 기능 × 레이어 매트릭스 (진척 추적 기준) |
| [docs/glossary.md](docs/glossary.md) | 용어집 |
