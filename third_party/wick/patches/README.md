# wick patch series

Applied on top of `third_party/wick/src` (submodule). The application order is `series`.

## Patches

This table is generated from the patch headers (`cargo xtask patch-readme`). Do not edit it by hand.

<!-- BEGIN GENERATED: patch table from the patch headers. Run `cargo xtask patch-readme`. -->
_패치 없음._
<!-- END GENERATED: patch table -->

## Plan (not patches yet)

| Patch | Purpose | Hook-for | Upstream |
|---|---|---|---|
| (planned) ext device registration hook | Device and API registration for our crates | host/vmm/* | wick, planned |
| (planned) boot payload hook | host/vmm/boot supplies kernel/initrd/cmdline | host/vmm/boot | wick, planned |
| (planned) generic vhost-user frontend | Device type parameterization, device state | host/devices/* | wick / Firecracker, planned |
| (planned) virtio shm region | gfxstream blob | host/devices/gpu | wick / Firecracker, planned |
| (planned) VMGenID platform support | macOS · Windows | host/vmm/vmgenid | wick, planned |

> Before starting, check the wick `vmm::ext` API and remove from this list every item that ext solves.
