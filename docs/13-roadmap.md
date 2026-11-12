# 13. Roadmap

## 1. Working principles
- **Milestones are defined only by gates.** Each milestone has only a goal and verifiable completion criteria, no duration. The gate must pass before moving on to the next milestone.
- **Gates are never passed by lowering the quality bar.** Production quality (P6) is the default from the first milestone on. user build, AVB, SELinux enforcing, per-process sandboxes and channel authentication are not features added later.
- **Scope → contract → E2E, in that order.** A feature is in `docs/scope.md` first, the contract is made first, and the E2E tests are added first. The red tests are the remaining work.
- **Layers progress in parallel against the contracts.** Concrete tasks are set by code and instructions.

## 2. Definition of v1
v1 is the first release that meets all three differentiators in the README at production quality.
1. **microVM density and clone-safe snapshots**: a clean session is provided immediately, and clones never share identifiers or keys.
2. **CDP-grade inspection**: View, accessibility, Compose, WebView (CDP), render layers and Flutter/RN semantics are merged into one tree. Exact frameId synchronization and stable node IDs are guaranteed.
3. **Cross-platform local execution**: the same binaries and the same API work on Linux (x86_64 · arm64), macOS (arm64) and Windows (x86_64).

v1 = the state in which all the M0–M6 gates below have passed.

## 3. Milestones

### M0. Skeleton
The layers and contracts exist as code, CI catches violations, and the call path from the client to the VMM is connected end to end.

Gate:
- The eight contracts (machine, device, guest_link, keymint, instance, adp, node, spi) exist with versions
- The checks for layer dependencies, forbidden concepts, patch rules and codegen freshness actually fail CI on violating cases
- The `clients/mcp` → `host/entrypoints/adp_server` → … → `host/vmm` path is connected (responses unimplemented). One trace ID runs through the whole path
- All v1 E2E scenarios exist, and all are red
- The upstream combination in `versions.toml` is pinned

Verification environment: local Mac + GitHub Actions. Once the AOSP tag is pinned, the build server is created and the AOSP sync is started.

### M1. Production-quality boot: macOS arm64 + Linux arm64
The same arm64 guest image boots with the production security configuration on both hosts (HVF, KVM), and screen, input and network work.

Gate:
- `boot_completed` is reached with a user build, AVB and SELinux enforcing, and guest_link authentication is established
- gfxstream screen output, virtio-input input, usernet networking and the host KeyMint work
- The VMM, each device process and KeyMint run in their own sandboxes
- The boot tests pass on both hosts, and the boot metrics are measured and recorded
- Device conformance and fuzzing run continuously in CI. CTS/VTS are measured on the release image and the baseline is recorded

Verification environment: build server (image builds); everyday boot iterations on the Mac (HVF); gate verification on the Mac + C4A bare metal (KVM).

### M2. ADP core
AI and clients read and operate an app's screen accurately, frame by frame.

Gate:
- A unified tree merging View, accessibility and Compose. The frameIds of trees, screenshots and events match exactly
- Node IDs are kept for the node's lifetime, and tree diff events are emitted
- Window, task and IME state, node-based actions, coordinate input, screenshots for a given frameId and app management work
- MCP `observe` / `act` / `wait_for` / `app_*` work
- The ADP golden tests (tree, bounds, frameId agreement, node ID stability, action results) pass for the views and compose test apps
- Tree update, screenshot and input→screen latencies are measured and recorded

Verification environment: the same as M1. The golden tests normally run on the Mac, and gate verification is done on both hosts.

### M3. Snapshots and sessions
A clean Android is provided immediately for every task, and clones are independent of each other.

Gate:
- Golden snapshot creation, lazy restore, device state restore and disk CoW layers work
- Session acquire / reset / release work
- Zero duplicate identifiers, keys and tokens across cloned sessions
- Restoring an incompatible snapshot is rejected
- Restore time and density are measured and recorded

Verification environment: the same as M1. The density metric is measured on C4A bare metal.

### M4. Inspection complete
The full v1 inspection scope comes into one tree.

Gate:
- WebView CDP relay, the render layer tree (games, custom renderers) and Flutter/RN semantics are in the unified tree
- The ADP golden tests pass for all test apps (views, compose, webview, flutter, rn, gl)
- The security regression tests for the exposure inside the guest (WebView debugging, inspector, agent) pass

Verification environment: the same as M1.

### M5. Platforms complete
M1–M4 hold on every v1 host with the same binaries and the same API.

Gate:
- Boots with the production security configuration on Linux x86_64 (gfxstream Vulkan path) and Windows x86_64 (WHP)
- The AppContainer sandbox and TPM key wrapping work on Windows
- Parts that need upstream work, such as wick WHP guest boot, are solved by contributing upstream first (upstream-first)
- The same E2E and ADP golden tests pass on every host of the boot matrix

Verification environment: the M1 environment plus Linux x86_64 (GCE x86; the GPU path on the local PC with Linux) and Windows (local PC). The cost of additional machines is estimated when the work starts.

### M6. v1 release
A production 1.0 that can be shipped.

Gate:
- Release bundles carry a signature, an SBOM and provenance, and the build is reproducible
- The Apache-2.0 core alone is complete as a self-hosted single node (P9)
- The security review is done, and the operations runbooks are in place
- CTS/VTS have zero regressions against the baseline

Verification environment: the full boot matrix (`docs/11-quality-and-testing.md` §4).

## 4. Verification environment and cost baseline
### How it is run
| Machine | Role | When it is on |
|---|---|---|
| Local Mac (Apple silicon) | Everyday development, arm64 guest boot and test iterations (HVF) | Always |
| Build server: GCE n2-standard-96 **Spot** (Seoul) + Local SSD | AOSP, kernel and host builds | Only while building. Turned off when the work is done |
| C4A bare metal: c4a-highmem-96-metal (Tokyo) | Linux arm64 (KVM) gate verification, density metrics | **Only for gate verification**. Turned off right after |

- Gate verification is never skipped or replaced by a single host because of cost. Cost is managed by the time machines stay on.

### Cost baseline (2026-11 GCP Billing Catalog prices, assuming USD 1 = KRW 1,400)
| Item | Unit price | KRW |
|---|---|---|
| Build server (Spot, including Local SSD) | $1.55/h | about KRW 2,200/h |
| C4A bare metal (on demand) | $7.27/h | about KRW 10,000/h |
| Disks (500GB source + 200GB C4A boot) | about $3/day | about KRW 4,000/day (charged even when the VMs are off) |

| Daily usage pattern | Daily cost |
|---|---|
| Build server for 6–8 hours | **about KRW 20,000** |
| The above + 1 hour of C4A gate verification | about KRW 30,000 |

- Prices can change. Look them up again when the machine setup changes.
- GCP can reclaim the Spot build server. If it is reclaimed, bring it up again and continue the build (the source disk remains).

## 5. After v1
| Stage | Scope |
|---|---|
| ADP extensions | Network (observation, policy), Log · ANR · crash, Overlay, screencast, session recording, file push/pull |
| Compatibility | NativeBridge (berberis), microG, audio, CTS/VTS target pass rate |
| Node and SaaS readiness | Node API v1, SPI stabilization, resource accounting events, remote MemorySource, device and user profiles |
| Release operations | OTA (A/B delta), upstream rebase automation |

## 6. Open decisions
| Item | Options | Notes |
|---|---|---|
| GMS | microG / Google partnership | Apps that need Play Integrity strong are explicitly excluded from the product scope |
| TLS visibility default | off (proposed) | Turned on only by policy |
| Running ARM apps on x86_64 hosts | In v1 / after v1 | Running arm64-only apps on an x86_64 host needs NativeBridge (berberis). Currently classified as after v1 |
| guest_link secret token disk | How the disk appears in the machine contract (`VmSpec`) / whether `Auth.Rebind` also rewrites the disk to the new token | Decided once the wick integration is further along. The guest_link proto keeps an OPEN note |
