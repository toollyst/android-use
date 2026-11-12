# third_party/ (L0)

upstream 코드와 그 패치만 둔다. **우리 로직은 여기에 두지 않는다.**

| 디렉터리 | 내용 |
|---|---|
| `wick/` | `src/` = wick submodule (`versions.toml [wick]`), `patches/` = 우리 패치 시리즈 |
| `aosp/` | 소스는 외부 체크아웃(`$AOSP_ROOT`, 태그는 `versions.toml [aosp]`). `patches/` = hook 패치, `security/` = ASB 백포트 |
| `kernel/` | GKI 고정 정보는 `versions.toml [kernel]`. `patches/` = 목표는 비어 있음 |
| `gfxstream/`, `rutabaga/` | 패치가 필요해질 때만 생성 |

패치 규칙: `docs/08-upstream-and-patches.md`
