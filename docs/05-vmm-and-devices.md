# 05. VMM and devices (L2)

## 1. Relationship with wick
- wick = a project that keeps upstream Firecracker as a submodule and supports Linux, macOS and Windows with a small patch series and per-OS platform backends (Apache-2.0).
  - Linux: upstream Firecracker (KVM)
  - macOS (Apple silicon, 15+): Hypervisor.framework. SMP, virtio-blk, vsock, virtio-net (vmnet), PCI/MSI-X, snapshots, userfaultfd and vhost-user work. aarch64 guests only
  - Windows (x86_64): WHP. Builds and CI work, but at the time of writing no guest had booted on real hardware yet
- wick keeps its own features separate in `crates/ext` (`vmm::ext`). **We add one more layer on top of wick in the same way.**
```
Firecracker tag ─(wick patches: wick's responsibility)─▶ wick commit ─(third_party/wick/patches: ours)─▶ + host/vmm/ crates ─▶ VMM binary
```

## 2. wick patches (conservative)
`third_party/wick/patches/`. Principle: **only extension points and general-purpose features**. General-purpose features are submitted to wick as an upstream PR first.

| Patch | Content | Upstream candidate |
|---|---|---|
| ext device registration hook | The point where our crates register devices and APIs (not needed if the wick ext API is enough) | wick |
| Boot payload hook | The point where the ext loader supplies kernel/initrd/cmdline before the kernel is loaded | wick |
| Generic vhost-user frontend | Parameterizes upstream's block-only implementation by device type. feature/protocol negotiation, device state transfer | wick / Firecracker |
| virtio shared memory region | Exposes the shm region through a PCI BAR (gfxstream blob) | wick / Firecracker |
| VMGenID platform support | Makes upstream VMGenID work on the macOS and Windows backends | wick |

> Before starting, check wick's actual `vmm::ext` API and remove from the patch list every item that ext solves.

## 3. host/vmm/ ext crates (aggressive)
| crate | Responsibility |
|---|---|
| `host/vmm/boot` | Parses the boot image v4 + vendor_boot **container format** (header, kernel, ramdisk, vendor ramdisk), combines ramdisks, attaches the **bootconfig** section. It only receives the bootconfig contents as opaque key=value (producing the `androidboot.*` values is `host/runtime/instance`'s job). Supports both x86_64 (cmdline/boot params) and aarch64 (FDT) |
| `host/vmm/disk` | **GPT composite disk** (named partitions such as super, userdata, metadata, misc → `/dev/block/by-name`) + **CoW layering** (base ro / snapshot / profile) |
| `host/vmm/vmgenid` | Generation ID update API, automatic increment on restore |
| `host/vmm/scanout` | Fast path that delivers the gpu backend's scanout export to the host (the ring buffer and frameId matching are `FrameChannel` in `host/runtime/instance`) |
| `host/vmm/api` | Firecracker REST API extensions (boot image v4 boot spec, disk layers, generation ID, etc.) + the `Machine` REST client for runtime. No Android meaning (the spec name is the format name, `BootImageV4`) |

## 4. Device backends (host/devices/)
All are built as **separate vhost-user backend binaries**, and each runs in a `host/runtime/jail` sandbox. No Android knowledge.

| Device | Based on | Notes |
|---|---|---|
| `host/devices/gpu` | rust-vmm `vhost-device-gpu` family + rutabaga + **gfxstream** | Host renderer: Linux Vulkan / macOS MoltenVK (Metal) / Windows Vulkan. blob resources (shm region), scanout export, device state serialization. SwiftShader is only a fallback for hosts without a GPU, not the default path |
| `host/devices/input` | virtio-input | Multi-touch (MT protocol B), keyboard. The host injects events directly |
| `host/devices/snd` | rust-vmm `vhost-device-sound` family | Compatibility for apps that need audio |
| `host/devices/net` | vhost-user-net + **usernet** (smoltcp-based NAT) | Works without root or entitlements. Flow observation hooks, the egress policy enforcement point (policy decisions are in host/services/network; the upstream is replaced through the SPI `EgressUpstream`) |

> The host KeyMint is not a virtio device and deals with Android concepts, so it lives in `host/runtime/keymint`, not in `host/devices/` (`docs/07-runtime.md`).

### Common requirements
- Device state save and restore support (included in snapshots)
- Fuzzing targets for untrusted guest input (`testing/fuzz/`)
- Conformance tests (`testing/conformance/`)
- Trace event emission (`host/base/trace`)

## 5. Per-platform notes
| Host | Hypervisor | Guest | Graphics | Sandbox |
|---|---|---|---|---|
| Linux x86_64 / arm64 | KVM | Same architecture | Vulkan | Firecracker jailer |
| macOS arm64 | Hypervisor.framework | arm64 (**arm64-only image**; Apple silicon does not support AArch32) | MoltenVK → Metal | seatbelt / App Sandbox |
| Windows x86_64 | WHP | x86_64 | Vulkan | AppContainer |
