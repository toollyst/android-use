# CLAUDE.md: 코딩 에이전트 작업 규칙

이 파일은 이 레포에서 작업하는 모든 코딩 에이전트(와 사람)를 위한 규칙이다.
규칙과 다른 판단이 필요하거나 작업 중 새로 할 일이 생기면 **코드를 쓰기 전에** 멈추고 사용자에게 직접 질문해 허락을 받는다.
선택지를 제시할 때는 최종 제품의 품질·정확성·보안만 기준으로 추천한다. 구현 비용·일정·우회 편의는 추천 근거가 아니다.

---

## 0. 먼저 읽을 것 (순서대로)
1. `docs/01-principles.md`: 불변 원칙
2. `docs/02-architecture.md`: 레이어와 의존 규칙
3. `docs/03-contracts.md`: 경계 계약
4. 작업 대상 레이어의 `README.md`와 `LAYER.toml`
5. `docs/scope.md`: 지금 채우는 칸이 어디인지
6. ADP 관련 작업이면 `docs/04-adp.md`

## 1. 절대 규칙
1. **QEMU 금지.** QEMU 코드, 바이너리, 런타임 의존을 어떤 형태로도 들이지 않는다.
2. **우회 금지.** "일단 되게" 하는 임시 경로(AVB 끄기, SELinux permissive, adb 의존 제어, 소프트웨어 렌더링 고정 등)를 기본 경로로 만들지 않는다. 디버그 전용이면 `debug` 빌드 플래그 뒤에 두고 문서에 명시한다.
3. **레이어 의존은 위에서 아래로만.** 같은 레이어의 형제 모듈끼리는 직접 의존하지 않는다. 경계는 `contracts/`로만 넘는다. (`LAYERS.toml`, `build/checklayers`)
4. **`host/vmm/`와 `host/devices/`는 Android를 모른다.** Android 개념(패키지, Activity, View 등)이 L2에 들어오면 위반이다.
5. **코어는 테넌트를 모른다.** 테넌트, 과금, 플랜 같은 개념은 코어에 없다. 필요하면 `labels`(불투명 map)와 SPI로 해결한다. SaaS 코드는 이 레포에 넣지 않는다.
6. **모든 채널은 인증한다.** "localhost라서 신뢰"를 가정하지 않는다.
7. **upstream 코드는 직접 수정하지 않는다.** `third_party/<x>/patches/`의 패치로만 바꾸고, 패치에는 hook 지점만 넣는다. 로직은 우리 레이어(ext/overlay)에 둔다.

## 2. 코드를 어디에 둘지 결정하기
```
upstream 동작을 바꿔야 하나?
 ├─ 예 → hook 지점만 third_party/<x>/patches/ 에 (예산·헤더 규칙 준수)
 │        └─ 실제 로직은 아래 중 하나에
 └─ 아니오 ↓
VM/virtio 디바이스 수준인가?          → host/vmm/ 또는 host/devices/   (Android 지식 금지)
게스트(Android) 안에서 돌아가나?      → guest/
인스턴스 조립·수명주기·스냅샷인가?    → host/runtime/
도메인 기능(트리, 화면, 입력, 앱…)인가? → host/services/<domain>/
호스트 실행 진입점(ADP 서버, node, CLI, desktop)인가? → host/entrypoints/
ADP 클라이언트(MCP, SDK)인가?          → clients/   (contracts/adp만 의존)
여러 레이어가 공유하는 인터페이스인가? → contracts/
도메인 지식 없는 공통 유틸인가?       → host/base/
```
두 곳 이상이 맞아 보이면 계약이 빠진 것이다. 계약부터 제안한다.

## 3. 작업 단위
- 문서(`docs/`)에는 로드맵·아키텍처·지향점처럼 추상적이고 명확한 것만 적는다. 구체적인 할 일은 코드나 사용자 지시로 정한다.
- 한 작업 = **레이어 하나 + 계약 하나**. 여러 레이어를 한 번에 바꿔야 하면 계약 변경 → 각 레이어 구현 순으로 쪼갠다.
- 작업을 마치면 `docs/scope.md`의 해당 칸 상태를 갱신한다.
- 구현 전에 E2E 테스트(`testing/e2e/`)에 기대 동작을 먼저 추가한다. 빨간 테스트가 곧 남은 일이다.

## 4. 계약 변경 절차
1. `contracts/<name>/`의 스키마/트레이트를 수정하고 semver를 올린다.
2. 하위호환이 깨지면 `contracts/compat/`에 이전 버전 픽스처를 추가하고 마이그레이션 방침을 적는다.
3. 계약 양쪽 레이어의 OWNERS 승인이 필요하다. (`OWNERS`)
4. codegen 결과를 갱신한다 (`build/codegen`).

## 5. 패치 작성 절차
1. `docs/08-upstream-and-patches.md`의 헤더 템플릿을 따른다.
2. 수정 hunk마다 `droiduse:` 마커 주석을 단다.
3. 패치 예산(줄 수)을 넘기면 사용자 승인이 필요하다.
4. 범용 기능이면 upstream PR을 먼저 연다(upstream-first). 패치 헤더에 링크를 남긴다.
5. `tools/patch/lint.sh`를 통과시킨다.

## 6. 검증 (작업 완료 전에 반드시)
> 아래 명령은 M0(골격)에서 만들어진다. 없으면 만드는 것부터가 작업이다.
```sh
cargo xtask check-layers        # 레이어 의존 규칙
cargo fmt --check && cargo clippy --workspace -- -D warnings
cargo test --workspace
tools/patch/lint.sh             # 패치 헤더·마커·예산
cargo xtask codegen --check     # 계약 codegen 최신 여부
testing/e2e/run.sh <target>     # 영향 범위 E2E
```
게스트 변경은 `tools/sync/aosp-sync.sh` → AOSP 빌드 → `tools/infra/boot-test.sh`까지 확인한다.

## 7. 커밋 / PR
- 커밋 제목: `<layer>/<module>: <요약>` (예: `host/services/inspect: merge compose semantics into unified tree`)
- 계약 변경 PR과 구현 PR은 분리한다.
- PR 본문: 바뀐 `scope.md` 칸, 영향 레이어, 검증 결과, 트레이스 스크린샷(성능 관련 시).

## 8. 막혔을 때
- 문제 위치를 모르겠으면 Perfetto 트레이스부터 본다 (`docs/02-architecture.md` §5 증상 → 레이어 표).
- 결정되지 않은 사항은 `docs/13-roadmap.md`의 "남은 결정"을 확인하고, 임의로 정하지 않고 사용자에게 묻는다.
