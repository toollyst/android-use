# build/

| 디렉터리 | 내용 |
|---|---|
| `checklayers/` | `LAYERS.toml` + 각 `LAYER.toml` 기반 의존 규칙·금지 개념 검사 (`cargo xtask check-layers`) |
| `codegen/` | contracts → Rust / Kotlin·Java / C++ / TS / Python 생성 (`cargo xtask codegen [--check]`) |
| `toolchains/` | Rust 툴체인 고정, 크로스 빌드 설정 |
