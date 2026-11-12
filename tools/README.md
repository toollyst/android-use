# tools/

| 디렉터리 | 내용 |
|---|---|
| `setup/` | 첫 체크아웃: submodule, wick setup, 우리 패치 적용, 툴체인 |
| `sync/` | `aosp-sync.sh` (repo init/sync + `guest/vendor` rsync/bind mount + 패치 적용), `kernel-sync.sh` |
| `patch/` | `apply.sh`, `refresh.sh`, `lint.sh` (헤더 템플릿·`droiduse:` 마커·예산과 오버런 승인·파일 이름·series·Layer·Hook-for·Scope-cell. 정책: `third_party/patches.toml`, 구현: `build/patchlint`) |
| `rebase/` | `wick.sh`, `aosp.sh`, `kernel.sh` |
| `bundle/` | 릴리스 번들 (VMM, 디바이스, 이미지, 커널, versions.toml, SBOM, provenance). |
| `infra/` | GCE 인스턴스 스크립트, `boot-test.sh`, `cts.sh`, CI 러너 설정 |

모든 스크립트는 `versions.toml`을 읽어 버전을 결정한다.
