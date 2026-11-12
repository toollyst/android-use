# tools/

| Directory | Contents |
|---|---|
| `setup/` | First checkout: submodules, wick setup, applying our patches, toolchain |
| `sync/` | `aosp-sync.sh` (repo init/sync + `guest/vendor` rsync/bind mount + applying patches), `kernel-sync.sh` |
| `patch/` | `apply.sh`, `refresh.sh`, `lint.sh` (header template, `droiduse:` markers, budget and overrun approval, file name, series, Layer, Hook-for, Scope-cell, README table up to date. Policy: `third_party/patches.toml`, implementation: `build/patchlint`. README table generation: `cargo xtask patch-readme`) |
| `rebase/` | `wick.sh`, `aosp.sh`, `kernel.sh` |
| `bundle/` | Release bundle (VMM, devices, images, kernel, versions.toml, SBOM, provenance). |
| `infra/` | GCE instance scripts, `boot-test.sh`, `cts.sh`, CI runner configuration |

Every script reads `versions.toml` to decide versions.
