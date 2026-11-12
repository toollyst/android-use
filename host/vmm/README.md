# host/vmm/ (L2)

VMM extension crates on top of wick. **They do not know Android meaning.** Details: `docs/05-vmm-and-devices.md`

| Module | Responsibility |
|---|---|
| `boot/` | boot image v4 + vendor_boot container parsing, ramdisk combining, bootconfig attachment (contents are opaque). x86_64 · aarch64 (FDT) |
| `disk/` | GPT composite disk (named partitions) + CoW layers (base / snapshot / profile) |
| `vmgenid/` | Generation ID update |
| `scanout/` | gpu scanout export → frame delivery to the host (fast path) |
| `api/` | Firecracker REST API extensions + the `Machine` REST client for runtime |

Contracts implemented: `contracts/machine`, `contracts/device`
