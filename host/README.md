# host/

Code that runs on the host (Linux · macOS · Windows). Listed top to bottom in layer order; dependencies go only from top → bottom.

| Directory | Layer | Role |
|---|---|---|
| `entrypoints/` | L6 | ADP server, node daemon, CLI, desktop: execution entry points |
| `services/` | L5 | Domain features (tree, screen, input, apps, network …) |
| `runtime/` | L4 | Assembles VM, devices and guest image into an instance; lifecycle and snapshots |
| `vmm/` | L2 | microVM (wick extension). Does not know Android |
| `devices/` | L2 | virtio device backends (each a separate process). Do not know Android |
| `base/` | L1 | Common utilities with no domain knowledge |

The guest (L3) is in `guest/`, boundary contracts in `contracts/`, ADP clients in `clients/`.
