# 06. Guest AOSP (L3)

## 1. Composition principles
- **Pinned AOSP release tag** (`versions.toml [aosp]`) + **hook patches** (`third_party/aosp/patches/`) + **overlay** (`guest/vendor/` → `$AOSP_ROOT/vendor/droiduse`)
- The upstream manifest is used with `repo init` as is. There is no separate manifest repository.
- The release build is a **user build**: adbd off by default (enabled only by host authorization), AVB on, SELinux enforcing.
- The kernel is **the Google-signed GKI prebuilt + vendor modules + config fragments**. The goal is zero kernel patches. If a feature outside the KMI is needed, the design is redone instead of working around it.

## 2. Extension points used before patches
If one of these solves it, no patch is made.
1. BoardConfig, product makefile, `AndroidProducts.mk`
2. RRO (runtime resource overlay)
3. vendor / product partitions
4. `BOARD_SEPOLICY_DIRS`, init `.rc`
5. APEX, privileged apps and permission XML
6. AIDL vendor HAL
7. Kernel: vendor modules (KMI compliant), config fragments

## 3. Initial patch list (hooks only)
| Project | Patch | Hook content | Where the logic lives |
|---|---|---|---|
| frameworks/base | `0001-droiduse-viewroot-inspector-hook` | Calls `DroidInspector` at the `ViewRootImpl` traversal point | `guest/vendor/services/inspector` |
| frameworks/base | `0002-droiduse-wm-activity-event-hook` | WindowManager / ActivityTaskManager event notification | Same as above |
| frameworks/base | `0003-droiduse-webview-debug-policy` | Applies the debugging policy after `WebViewFactory` loads (the policy is a system property) | Same as above |
| frameworks/native | `0001-droiduse-sf-frameid-layer-capture` | Exposes the SurfaceFlinger frame number and layer information, entry point for per-layer capture | `guest/vendor/services/adpd` |
| frameworks/libs/binary_translation | (after v1) berberis frontend | New-file patches and patches modifying existing files are kept separate | That project |

> The exact insertion points (files, functions) are fixed against the pinned AOSP tag when the work starts.

### Design decisions that avoid patches
- **WebView**: Chromium is not forked. A framework hook only turns debugging on.
- **Flutter/RN semantics**: exposed once agent is registered as an accessibility service. No patch needed.
- **Kernel**: GKI prebuilt. Only when unavoidable, a patch against an android-common tag (`third_party/kernel/patches/`).

## 4. Overlay layout (`guest/vendor/`)
```
vendor/droiduse/
├─ AndroidProducts.mk
├─ products/
│  ├─ wick_arm64/      BoardConfig.mk · device.mk · fstab · init.wick.rc · *.idc
│  ├─ wick_x86_64/
│  └─ common/          common product settings, RRO
├─ services/
│  ├─ inspector/       the actual logic called by framework hooks (java + jni)
│  ├─ adpd/            guest ADP daemon (binder ↔ vsock guest_link)
│  ├─ agent/           platform-signed system service (UiAutomation, node actions, accessibility service registration)
│  └─ restore/         VMGenID detection → reinitialization
├─ hals/
│  ├─ graphics/        gfxstream guest (GLES/Vulkan), minigbm, HWC (reports virtio identifiers for frame matching)
│  ├─ keymint/         HAL that talks to the host KeyMint (dedicated vsock port)
│  ├─ health/  sensors/  audio/
├─ nativebridge/       berberis integration settings (after v1)
├─ apps/microg/        privileged install + permission XML (signature spoofing not used)
├─ sepolicy/           dedicated domains for adpd · agent · restore · HALs
├─ avb/                public keys only (private keys are CI secrets / HSM)
└─ protocol/           contracts/guest_link codegen output (synced by CI)
```

## 5. Boot path
1. `host/runtime/instance` produces the `androidboot.*` values (hardware name, slot, verifiedbootstate, etc.) and passes them as `VmSpec.boot.bootconfig`; `host/vmm/boot` reads boot.img v4 + vendor_boot, builds the kernel and ramdisk (+ vendor ramdisk), and attaches the bootconfig unchanged
2. first-stage init mounts the logical partitions of `super` with dm-linear using the fstab in the ramdisk (GPT names from `host/vmm/disk` → `/dev/block/by-name`)
3. AVB: vbmeta chain verification with our own keys, rollback index
4. second-stage init → vendor modules are loaded (virtio-blk/net/vsock/gpu/input/snd/pci, vmgenid)
5. `adpd`, `agent`, `restore` start → adpd reads the first session token from the secret disk attached by the VMM → adpd connects to the `guest_link` host channel (vsock) and sends Hello → it opens the guest channel port (two channels, `docs/03-contracts.md` guest_link)

## 6. Targets
| product | Host | Notes |
|---|---|---|
| `wick_arm64` | macOS arm64, Linux arm64 | **arm64-only** (32-bit apps after v1, through NativeBridge) |
| `wick_x86_64` | Linux x86_64, Windows x86_64 | ARM apps through NativeBridge (berberis arm64→x86_64). Timing is in the open decisions of `docs/13-roadmap.md` |

## 7. Security elements
- SELinux enforcing, a least-privilege domain for every new component
- Only adpd's SELinux domain may use listening vsock sockets (`vsock_socket` `listen`; sepolicy enforces it with a `neverallow` for every other domain). The kernel checks `vsock_socket` permissions per domain and has no per-port labelling for vsock, so this domain rule is what protects the guest channel port. Debug builds only: adbd over vsock is also allowed
- AVB with our own keys + rollback protection
- KeyMint is wrapped by the host secure storage (the guest has no TEE)
- restore hook: kernel RNG reseed, regeneration of ANDROID_ID and other identifiers, keystore reinitialization, network renegotiation
- Control only through the authenticated guest_link. adbd is off by default and is turned on only by host authorization (for CTS/VTS measurement only)

## 8. GMS
- The default is **microG** (privileged app, explicit permissions). Signature spoofing is not used.
- Shipping GMS officially or the Play Integrity strong tier is a licensing and certification matter. A business decision (open decisions in `docs/13-roadmap.md`).
