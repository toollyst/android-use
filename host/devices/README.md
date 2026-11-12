# host/devices/ (L2)

virtio device backends. Each is a **separate vhost-user binary**, each runs in a sandbox. **No Android knowledge.**

| Module | Based on | Notes |
|---|---|---|
| `gpu/` | rutabaga + gfxstream | Vulkan / MoltenVK (Metal). blob (shm), scanout export, device state |
| `input/` | virtio-input | Multi-touch, keyboard |
| `snd/` | virtio-snd | Audio |
| `net/` | vhost-user-net + usernet (smoltcp) | Flow observation hooks, egress policy enforcement point, `EgressUpstream` SPI |

The host KeyMint is not a virtio device and deals with Android concepts, so it lives in `host/runtime/keymint`.

Common: device state save and restore, fuzz targets, conformance tests, trace events
