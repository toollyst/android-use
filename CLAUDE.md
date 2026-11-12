# CLAUDE.md: working rules for coding agents

This file holds the rules for every coding agent (and person) working in this repository.
When a judgment that departs from these rules is needed, or new work comes up during a task, stop **before writing code**, ask the maintainers directly and get their approval.
When presenting options, recommend only by the quality, correctness and security of the final product. Implementation cost, schedule and workaround convenience are not grounds for a recommendation.

---

## 0. Read first (in order)
1. `docs/01-principles.md`: invariant principles
2. `docs/02-architecture.md`: layers and dependency rules
3. `docs/03-contracts.md`: boundary contracts
4. The `README.md` and `LAYER.toml` of the layer you are working on
5. `docs/scope.md`: which cell you are filling now
6. `docs/04-adp.md` for ADP-related work

## 1. Absolute rules
1. **No QEMU.** QEMU code, binaries and runtime dependencies are not brought in in any form.
2. **No workarounds.** Temporary paths that "just make it work" (turning AVB off, SELinux permissive, adb-dependent control, pinned software rendering, etc.) are never made the default path. If something is debug-only, put it behind the `debug` build flag and state it in the docs.
3. **Layer dependencies go only from top to bottom.** Sibling modules in the same layer do not depend on each other directly. The only exceptions are one-way edges declared in the layer-root `LAYER.toml`. Boundaries are crossed only through `contracts/`. (`LAYERS.toml`, `build/checklayers`)
4. **`host/vmm/` and `host/devices/` do not know Android.** An Android concept (package, Activity, View, etc.) in L2 is a violation.
5. **The core does not know tenants.** Concepts such as tenants, billing and plans do not exist in the core. If needed, solve it with `labels` (an opaque map) and the SPI. SaaS code does not go into this repository.
6. **Every channel is authenticated.** Never assume "it is localhost, so it is trusted".
7. **Upstream code is never modified directly.** Change it only through patches in `third_party/<x>/patches/`, and put only hook points in the patches. The logic lives in our layers (ext/overlay).

## 2. Deciding where code goes
```
Does upstream behaviour have to change?
 ├─ yes → only the hook point, in third_party/<x>/patches/ (follow the budget and header rules)
 │        └─ the actual logic goes in one of the places below
 └─ no ↓
VM/virtio device level?                     → host/vmm/ or host/devices/   (no Android knowledge)
Runs inside the guest (Android)?            → guest/
Instance assembly, lifecycle, snapshots?    → host/runtime/
Domain feature (tree, screen, input, app…)? → host/services/<domain>/
Host execution entry point (ADP server, node, CLI, desktop)? → host/entrypoints/
ADP client (MCP, SDK)?                      → clients/   (depends only on contracts/adp)
Interface shared by several layers?         → contracts/
Common utility with no domain knowledge?    → host/base/
```
If two or more places look right, a contract is missing. Propose the contract first.

## 3. Unit of work
- The docs (`docs/`) record only what is abstract and clear, such as the roadmap, the architecture and the direction. Concrete tasks are set by code or by the maintainers' instructions.
- One task = **one layer + one contract**. If several layers have to change at once, split it into the contract change → the implementation in each layer.
- When a task is done, update the status of its cell in `docs/scope.md`.
- Before implementing, add the expected behaviour to the E2E tests (`testing/e2e/`) first. The red tests are the remaining work.

## 4. Contract change procedure
1. Modify the schema/trait in `contracts/<name>/` and bump the semver.
2. If backward compatibility breaks, add a fixture of the previous version to `contracts/compat/` and write down the migration policy.
3. Approval by the OWNERS of the layers on both sides of the contract is required. (`OWNERS`)
4. Update the codegen output (`build/codegen`).

## 5. Patch procedure
1. Follow the header template in `docs/08-upstream-and-patches.md`.
2. Add a `droiduse:` marker comment to every modified hunk.
3. Exceeding the patch budget (line count) needs maintainer approval.
4. For a general-purpose feature, open the upstream PR first (upstream-first). Leave the link in the patch header.
5. Pass `tools/patch/lint.sh`.

## 6. Verification (always, before finishing a task)
> The commands below are created in M0 (skeleton). If one does not exist, creating it is the first part of the task.
```sh
cargo xtask check-layers        # layer dependency rules
cargo fmt --check && cargo clippy --workspace -- -D warnings
cargo test --workspace
tools/patch/lint.sh             # patch header, markers, budget
cargo xtask codegen --check     # contract codegen is up to date
testing/e2e/run.sh <target>     # E2E for the affected area
```
For guest changes, check through `tools/sync/aosp-sync.sh` → AOSP build → `tools/infra/boot-test.sh`.

## 7. Commits / PRs
- Commit title: `<layer>/<module>: <summary>` (e.g. `host/services/inspect: merge compose semantics into unified tree`)
- Contract change PRs and implementation PRs are separate.
- PR body: the `scope.md` cells that changed, the affected layers, the verification results, trace screenshots (for performance work).

## 8. When stuck
- If you do not know where a problem is, look at a Perfetto trace first (`docs/02-architecture.md` §5, symptom → layer table).
- For anything undecided, check "Open decisions" in `docs/13-roadmap.md`; do not decide it yourself, ask the maintainers.
