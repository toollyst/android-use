# build/

| Directory | Contents |
|---|---|
| `checklayers/` | Dependency-rule and forbidden-concept checks based on `LAYERS.toml` + each `LAYER.toml` (`cargo xtask check-layers`) |
| `patchlint/` | Lint for the patches in `third_party/<x>/patches/`: header, markers, budget, name, series, Layer, Hook-for, Scope-cell, README table (`cargo xtask patch-lint` = `tools/patch/lint.sh`, table generation `cargo xtask patch-readme`, policy `third_party/patches.toml`) |
| `codegen/` | Generation from contracts → Rust / Kotlin, Java / C++ / TS / Python (`cargo xtask codegen [--check]`). Rust only for now: guest_link and node message types (prost-build), adp serde types. Configuration `build/codegen/codegen.toml`, protoc pin `versions.toml [protoc]` |
| `toolchains/` | Rust toolchain pin, cross-build configuration |
