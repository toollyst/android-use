# host/runtime/ (L4): the binding layer

Assembles vmm + devices + the guest image into an `Instance` and manages its lifecycle. Does not implement domain features. Details: `docs/07-runtime.md`

| Module | Responsibility |
|---|---|
| `instance/` | VmSpec creation (including bootconfig `androidboot.*`), starting and connecting device processes, state machine, `FrameChannel` (frame matching and retention) |
| `session/` | Session pool, acquire/reset/release |
| `snapshot/` | Golden snapshots, restore, uffd handler (MemorySource), metadata |
| `identity/` | VMGenID, DeviceProfile, Restore.Notify validation |
| `keymint/` | Host KeyMint TA (separate jailed process, `contracts/keymint`) |
| `jail/` | Linux jailer / macOS seatbelt / Windows AppContainer |
| `resources/` | Resource accounting, limits, events |
| `stores/` | Default SPI implementations (local Snapshot/Image/Profile Store, file MemorySource) |

Contracts implemented: `contracts/instance`. `contracts/keymint` (host side). Contracts used: `contracts/machine`, `contracts/device`, `contracts/guest_link`, `contracts/spi`
