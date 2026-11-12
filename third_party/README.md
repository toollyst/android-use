# third_party/ (L0)

Holds only upstream code and its patches. **Our logic does not go here.**

| Directory | Content |
|---|---|
| `wick/` | `src/` = wick submodule (`versions.toml [wick]`), `patches/` = our patch series |
| `aosp/` | The source is an external checkout (`$AOSP_ROOT`, tag in `versions.toml [aosp]`). `patches/` = hook patches, `security/` = ASB backports |
| `kernel/` | GKI pin information in `versions.toml [kernel]`. `patches/` = the goal is empty |
| `gfxstream/`, `rutabaga/` | Created only when a patch becomes necessary |

Patch rules: `docs/08-upstream-and-patches.md`
