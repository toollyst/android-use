# 01. Invariant principles

These principles take precedence over features. Changing them needs an explicit decision by the project owner.

## P1. No QEMU and no workaround paths
- The runtime is **wick (Firecracker) + Rust device backends + AOSP we build ourselves**, and nothing else. QEMU code, binaries and runtime dependencies are not brought in in any form.
- gfxstream (a standalone AOSP library) and berberis (AOSP's own binary translator) are unrelated to QEMU and are allowed.
- Workarounds for development convenience (AVB off, SELinux permissive, adb-dependent control, pinned software rendering) can never become the default path. adb is only for CTS/VTS measurement and is not used as an ADP or MCP control path.
- **Widening the supported scope in stages** (e.g. v1 on one platform) is not a workaround. The quality bar is not lowered.

## P2. Pin upstream, patch conservatively, extend aggressively
- The same principle on all three axes: Firecracker→wick, AOSP release tags, the GKI kernel
- Patches add **only hook points**. The logic lives in our layers (ext, overlay).
- General-purpose features go upstream first. Once merged upstream, our patch is deleted.

## P3. Boundaries only through contracts
- Chromium-style layers. Dependencies go only from top to bottom, and siblings go through contracts. runtime (L4) is the layer that ties things together.
- Contracts have semver, backward-compatibility tests, and approval by the OWNERS of both sides.

## P4. Lower layers do not know upper-layer concepts
- `host/vmm/` and `host/devices/` do not know Android. They handle only generic virtio, so they can be reused as a general-purpose sandbox.
- The core does not know tenants or billing. It is extended with `labels` and the SPI.

## P5. Devices are separate processes
- gpu, input, snd and net are split out as vhost-user backends, each running in a sandbox.
- The host KeyMint also runs as a separate process in a separate sandbox (`host/runtime/keymint`).

## P6. Production quality is the default
- user build (adbd off by default, enabled only by host authorization), AVB, SELinux enforcing
- Device and VMM sandboxes, host KeyMint
- Snapshot clone uniqueness (VMGenID + guest restore hook)
- Fuzzing at untrusted input boundaries
- CTS/VTS used as the compatibility metric. Measured **on the release image as is**

## P7. Make it measurable
- A single trace (Perfetto) across the layers
- Quality is defined by metrics: CTS pass rate, boot and restore time, input→screen latency, tree update latency, density

## P8. Fix the scope first, then fill the cells
- Every feature exists first in the matrix (feature × layer) in `docs/scope.md`.
- Development is "filling cells", not "adding features". Contracts come first, and the layers are implemented in parallel.

## P9. The open core is complete on its own
- The Apache-2.0 core alone must fully run a self-hosted single node.
- SaaS is built without forking the core, only with SPI implementations and a control plane.

## P10. Decide by production quality
- Every choice is judged by the quality, correctness and security of the final product. Implementation cost and schedule are not selection criteria.

## P11. Docs give direction; tasks come from code and instructions
- Docs record only what is abstract and clear, such as the roadmap, the architecture and the direction.
- Concrete tasks are set by code or by the project owner's instructions. New work that comes up during a task is done only with permission.
