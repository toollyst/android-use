# 07. Runtime (L4): the binding layer

runtime **assembles** vmm, devices and the guest image **into one `Instance`** and manages its lifecycle. It does not implement domain features.

## 1. Modules
| Module | Responsibility |
|---|---|
| `host/runtime/instance` | `VmSpec` creation (including the guest bootconfig `androidboot.*`), starting and connecting the device backend processes, the `Instance` implementation, the state machine, frame matching (`FrameChannel`) |
| `host/runtime/session` | Session pool, acquire/reset/release, concurrency and capacity management |
| `host/runtime/snapshot` | Golden snapshot creation, restore, uffd page handler (uses `MemorySource`), snapshot metadata |
| `host/runtime/identity` | VMGenID increment, applying `DeviceProfile`, waiting for and validating the guest's `Restore.Notify` |
| `host/runtime/keymint` | Host KeyMint TA (AOSP Rust kmr). Separate process, runs inside a jail. Keys are wrapped by the host secure storage (Secure Enclave / TPM). Per-instance secrets |
| `host/runtime/jail` | Per-platform sandbox: Linux jailer / macOS seatbelt / Windows AppContainer. Applied separately to the VMM, **each device process** and keymint |
| `host/runtime/resources` | Per-instance accounting of CPU, memory, disk and network usage, limit enforcement, event emission |
| `host/runtime/stores` | Default SPI implementations: local `SnapshotStore`, `ImageStore`, `ProfileStore`, file `MemorySource` |

## 2. Instance state machine
```
Creating → Booting → Ready ⇄ Paused
              │         │
              ▼         ▼
            Failed   Snapshotting → Ready
Restoring → (Restore.Notify received) → Ready
any → Stopped
```

## 3. Snapshots
### Golden snapshot creation
1. Cold boot → `sys.boot_completed` → agent/adpd Ready → (optional) profile preparation work
2. pause → save VM memory + vCPU state + **all device state** (vhost-user device state transfer)
3. Write the metadata (below)

### Metadata (versioned format)
```toml
format_version = 1
wick_commit    = "…"
image_hash     = "…"            # guest image (boot, vendor_boot, super)
cpu_template   = "…"            # required for restoring on another host
protocol       = { guest_link = "0.1.0" }
devices        = ["gpu@rev", "input@rev", "snd@rev", "net@rev"]
disk_layers    = ["base:<hash>", "snapshot:<hash>"]
created_at     = "…"
labels         = {}
```
A restore of an incompatible combination is rejected.

### restore
1. Get a handle from `SnapshotStore` → create a `MemorySource` (local file or remote)
2. uffd lazy restore: `MemorySource.read_at` on page faults, working-set prefetch hints
3. Restore device state
4. **Increment VMGenID** → the guest restore hook runs → confirm that `Restore.Notify` was received
5. Issue a new token per clone (`Auth.Rebind`. The first token went over the secret disk at boot; after every restore it is changed at this step) → `Restore.Notify` received → Ready

## 4. Clone uniqueness (required)
When several instances are started from the same snapshot, the RNG state, ANDROID_ID, keystore keys and TLS sessions are cloned. This is a security incident.
- Host: VMGenID increment, token reissue per clone, per-instance KeyMint secrets
- Guest (`guest/vendor/services/restore`): kernel RNG reseed, regeneration of ANDROID_ID and other identifiers, keystore reinitialization, network renegotiation
- Verification: a "zero duplicate identifiers and keys across cloned sessions" test in `testing/e2e`

## 5. Disk layering
```
[profile layer (rw, keeps user data)]      ← ProfileStore
[snapshot layer (ro, golden state)]        ← SnapshotStore
[base layer (ro, image)]                   ← ImageStore
```
- Session writes go to a temporary CoW layer on top and are discarded on reset.
- Profile retention (login state, etc.) is done by committing the profile layer.

## 6. Resources and events
- Events: instance creation and termination, session acquire/release, snapshot, restore, resource usage (periodic)
- Exported through the `EventSink` SPI. The core does not know billing.
- Every instance and session has `labels` (an opaque map).
