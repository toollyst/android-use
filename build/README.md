# build/

| 디렉터리 | 내용 |
|---|---|
| `checklayers/` | `LAYERS.toml` + 각 `LAYER.toml` 기반 의존 규칙·금지 개념 검사 (`cargo xtask check-layers`) |
| `patchlint/` | `third_party/<x>/patches/` 패치 lint: 헤더·마커·예산·이름·series·Layer·Hook-for·Scope-cell·README 표 (`cargo xtask patch-lint` = `tools/patch/lint.sh`, 표 생성 `cargo xtask patch-readme`, 정책 `third_party/patches.toml`) |
| `codegen/` | contracts → Rust / Kotlin·Java / C++ / TS / Python 생성 (`cargo xtask codegen [--check]`) |
| `toolchains/` | Rust 툴체인 고정, 크로스 빌드 설정 |
