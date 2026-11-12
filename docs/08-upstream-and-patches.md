# 08. Upstream tracking and patch governance

## 1. Three axes
```
Firecracker tag ─(wick patches: wick's responsibility)─▶ wick ─(third_party/wick/patches)─▶ + host/vmm/ ext ─▶ VMM
AOSP release tag ─(third_party/aosp/patches + security/)─▶ + guest/vendor overlay ─▶ guest image
GKI prebuilt ─(guest/kernel/fragments + modules)─────────────────────────────────▶ kernel + vendor modules
```
The combination is pinned by a single `versions.toml`.

## 2. Patch rules
1. **Only hooks.** E.g. in `ViewRootImpl`, a single call line such as `DroidInspector.onTraversal(this)`. The implementation lives in overlay/ext.
2. **One patch per concern.**
3. **Marker**: a `droiduse:` comment on every modified hunk (the same approach as wick's `wick:` convention).
4. **Required header fields** (template below)
5. **Patch budget**: a hook patch is on the order of tens of lines. Exceeding the budget needs project owner approval. Checked by `tools/patch/lint.sh`.
6. **upstream-first**: general-purpose features are submitted first to wick, Firecracker, rust-vmm or AOSP Gerrit. Once merged, the patch is deleted.
7. **ext/overlay use only the hook interface**: they must not depend directly on upstream internals, so that they do not break on rebase.

### Patch header template
```
From: <author>
Subject: [PATCH] droiduse: <one-line summary>

Purpose:      <why this patch exists>
Hook-for:     <path of our module that it calls, e.g. guest/vendor/services/inspector>
Layer:        <guest | vmm>
Budget:       <allowed line count>
Upstream:     pending <link> | n/a
Remove-when:  <condition for deletion, e.g. when upstream gains an equivalent API>
Scope-cell:   <the matching row of docs/scope.md>
```

### Directory
```
third_party/<x>/patches/
├─ series                 # application order
├─ <project-path>/NNNN-droiduse-<name>.patch
└─ README.md              # summary of why each patch exists (lint checks it is in sync with the headers)
third_party/aosp/security/    # ASB backports (monthly)
```

## 3. Tracking cadence
| Target | Cadence | Procedure |
|---|---|---|
| wick | Every wick tag or commit (wick follows Firecracker) | `tools/rebase/wick.sh` → patch refresh → build → conformance + boot test |
| AOSP | Twice a year: from 2026 Google publishes the AOSP source twice a year, in Q2 and Q4 | `tools/rebase/aosp.sh` → `git am -3` → conflict report based on markers → build → boot → ADP E2E → CTS |
| ASB security patches | Monthly (may be out of step with when AOSP picks them up) | Backport into `third_party/aosp/security/` → build → regression → patch release |
| GKI | Every GKI release build | KMI check → rebuild vendor modules → boot |
| rust-vmm, gfxstream, rutabaga, etc. | Automatic Renovate PRs | Merged when the tests pass |

## 4. Rebase pipeline (CI)
1. Detect a new upstream version
2. sync (`repo sync` / submodule update)
3. Apply the patch series (`git am -3`)
4. Conflict report (affected modules shown based on the `droiduse:` markers and the `Hook-for` field of the patch headers)
5. Build
6. Boot test (`tools/infra/boot-test.sh`)
7. ADP E2E + golden tests
8. CTS subset (the full suite for release candidates)
9. PR updating `versions.toml`

## 5. Branches
- `main`: based on the latest upstream
- `release/<aosp-tag>`: release lines. Older lines receive only ASB patches and are maintained for a set period

## 6. Version labels
The release bundle version is derived from the `versions.toml` hash. A bundle contains the VMM, the device backends, the guest image, the kernel, `versions.toml`, an SBOM and provenance.
