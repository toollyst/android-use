# 08. upstream 추적과 패치 거버넌스

## 1. 세 축
```
Firecracker tag ─(wick patches: wick 소관)─▶ wick ─(third_party/wick/patches)─▶ + vmm/ ext ─▶ VMM
AOSP release tag ─(third_party/aosp/patches + security/)─▶ + guest/vendor overlay ─▶ 게스트 이미지
GKI prebuilt ─(guest/kernel/fragments + modules)─────────────────────────────────▶ 커널 + vendor 모듈
```
조합은 `versions.toml` 하나로 고정한다.

## 2. 패치 규칙
1. **hook만 넣는다.** 예: `ViewRootImpl`에는 `DroidInspector.onTraversal(this)` 같은 호출 한 줄. 구현은 overlay/ext에 둔다.
2. **관심사 하나에 패치 하나.**
3. **마커**: 수정 hunk마다 `droiduse:` 주석 (wick의 `wick:` 관례와 같은 방식).
4. **헤더 필수 필드** (아래 템플릿)
5. **패치 예산**: hook 패치는 수십 줄 수준. 예산 초과는 프로젝트 오너 승인 필요. `tools/patch/lint.sh`가 검사.
6. **upstream-first**: 범용 기능은 wick, Firecracker, rust-vmm, AOSP Gerrit에 먼저 제출. 머지되면 패치 삭제.
7. **ext/overlay는 hook 인터페이스만 사용**: upstream 내부 구조에 직접 의존하지 않아야 rebase 때 깨지지 않는다.

### 패치 헤더 템플릿
```
From: <author>
Subject: [PATCH] droiduse: <한 줄 요약>

Purpose:      <이 패치가 존재하는 이유>
Hook-for:     <호출하는 우리 쪽 모듈 경로, 예: guest/vendor/services/inspector>
Layer:        <guest | vmm>
Budget:       <허용 줄 수>
Upstream:     pending <링크> | n/a
Remove-when:  <삭제 조건, 예: upstream에 동등 API가 생기면>
Scope-cell:   <docs/scope.md의 해당 행>
```

### 디렉터리
```
third_party/<x>/patches/
├─ series                 # 적용 순서
├─ <project-path>/NNNN-droiduse-<name>.patch
└─ README.md              # 패치별 존재 이유 요약 (lint가 헤더와 동기화 검사)
third_party/aosp/security/    # ASB 백포트 (월간)
```

## 3. 추적 주기
| 대상 | 주기 | 절차 |
|---|---|---|
| wick | wick 태그·커밋마다 (wick이 Firecracker를 따라감) | `tools/rebase/wick.sh` → 패치 refresh → 빌드 → conformance + 부팅 테스트 |
| AOSP | 반기: Google은 2026년부터 AOSP 소스를 Q2/Q4 연 2회 공개 | `tools/rebase/aosp.sh` → `git am -3` → 마커 기준 충돌 리포트 → 빌드 → 부팅 → ADP E2E → CTS |
| ASB 보안 패치 | 매월 (AOSP 반영 시점과 어긋날 수 있음) | `third_party/aosp/security/`에 백포트 → 빌드 → 회귀 → 패치 릴리스 |
| GKI | GKI 릴리스 빌드마다 | KMI 검사 → vendor 모듈 재빌드 → 부팅 |
| rust-vmm, gfxstream, rutabaga 등 | Renovate 자동 PR | 테스트 통과 시 머지 |

## 4. rebase 파이프라인 (CI)
1. 새 upstream 버전 감지
2. sync (`repo sync` / submodule 갱신)
3. 패치 시리즈 적용 (`git am -3`)
4. 충돌 리포트 (`droiduse:` 마커, 패치 헤더의 `Hook-for` 기준으로 영향 모듈 표시)
5. 빌드
6. 부팅 테스트 (`tools/infra/boot-test.sh`)
7. ADP E2E + 골든 테스트
8. CTS 서브셋 (릴리스 후보는 전체)
9. `versions.toml` 갱신 PR

## 5. 브랜치
- `main`: 최신 upstream 기준
- `release/<aosp-tag>`: 릴리스 라인. 이전 라인은 ASB 패치만 받으며 정해진 기간 유지

## 6. 버전 표기
릴리스 번들 버전은 `versions.toml` 해시에서 만든다. 번들에는 VMM, 디바이스 백엔드, 게스트 이미지, 커널, `versions.toml`, SBOM, provenance가 들어간다.
