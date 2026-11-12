# 02. Architecture

## 1. Layers
```
 clients/           mcp · sdk{rust,ts,python}     ADP clients (depend only on contracts/adp)
                       │ ADP (WebSocket JSON-RPC, authenticated)
 host/
   L6 entrypoints/  adp_server · node · cli · desktop                      (= Chromium chrome/)
   L5 services/     inspect · screen · input · app · network · log ·
                    overlay · webview · stream · record                     (= Chromium services/, components/)
   L4 runtime/      ★ assembly layer: Instance lifecycle · session pool · snapshots · uniqueness ·
                    frame matching · host KeyMint · sandbox · resources    (= content/)
   L2 vmm/ devices/ microVM (wick ext) · virtio device backends (no Android knowledge)
   L1 base/         IPC · Perfetto tracing · config · errors · platform abstraction
 guest/        L3   AOSP overlay · HAL · guest services · kernel config
 third_party/  L0   wick · AOSP · GKI · gfxstream · rutabaga (+ patches)
 contracts/         machine · device · guest_link · keymint · instance · adp · node · spi   (= mojom, zero dependencies)
```

| Layer | Responsibility (does only this) | Does not do |
|---|---|---|
| **L0 third_party** | Pinning upstream and patching it | Our logic |
| **L1 base** | Transports (vsock, unix, shm), tracing, config, error types, OS abstraction | Domain knowledge |
| **L2 vmm** | VM create, start, stop, snapshot; boot image v4 container loading (bootconfig is opaque); disk layering; VMGenID; scanout sharing | Android meaning (including interpreting `androidboot.*`), UI |
| **L2 devices** | Correct virtio device implementations (gpu, input, snd, net). Each a separate process | Android knowledge |
| **L3 guest** | Boot, HAL, security (AVB, SELinux), guest services (inspector, adpd, agent, restore) | Host policy |
| **L4 runtime** | Assembles vmm + devices + guest image into an `Instance`. Lifecycle, session pool, snapshots, uniqueness, frame matching (`FrameChannel`), host KeyMint, sandbox, resource accounting, default SPI implementations | Domain features |
| **L5 services** | Owns each domain feature end to end | Transport, product UX |
| **L6 entrypoints** | Host execution entry points (ADP server, node daemon, CLI, desktop) and product defaults | Core logic |
| **clients** | ADP clients (MCP, SDK). Use only `contracts/adp` | Access to host internals |
| **contracts** | Layer boundary contracts. Self-contained | Dependencies on other layers |

## 2. Dependency rules
1. Dependencies go only from upper to lower. (`LAYERS.toml`)
2. No direct dependencies between siblings at the same level. Sibling modules inside the same layer do not depend on each other directly either. `host/services/screen` does not call `host/services/inspect` directly; it goes through a runtime channel (contract). The only exceptions are one-way edges declared in the layer-root `LAYER.toml`.
3. Everyone references `contracts/`, and `contracts/` itself depends on no other layer. External crates are limited to those on the allowlist in `LAYERS.toml`.
4. `testing/` may reference everything, but product code does not depend on `testing/`.
5. `clients/` depends only on `contracts/`, and product code does not depend on `clients/`.

### Enforcement
| Target | Tool |
|---|---|
| Rust crates | `build/checklayers`: checks the dependency graph with `cargo metadata`. A per-directory `LAYER.toml` narrows the allowlist (the same role as Chromium DEPS). Sibling-module exceptions are checked against the one-way edges in the layer-root `LAYER.toml`, and external crates in contracts against the `LAYERS.toml` allowlist |
| AOSP vendor modules | Soong `visibility` property |
| Forbidden concepts | Lint based on `LAYERS.toml [forbidden]` (e.g. no Android concepts in devices) |
| Ownership | `OWNERS`: contracts and patches have separate approval groups |

Every check fails CI.

## 3. Directory structure (monorepo)
```
droid-use/
├─ README.md  CLAUDE.md  AGENTS.md  versions.toml  LAYERS.toml  OWNERS  LICENSE  NOTICE
│
├─ host/                           host code (top → bottom = layer order)
│  ├─ entrypoints/                 L6  adp_server/  node/  cli/  desktop/
│  ├─ services/                    L5  inspect/  screen/  input/  app/  network/  log/  overlay/  webview/  stream/  record/
│  ├─ runtime/                     L4  instance/  session/  snapshot/  identity/  keymint/  jail/  resources/  stores/
│  ├─ vmm/                         L2  boot/  disk/  vmgenid/  scanout/  api/   (wick ext crates)
│  ├─ devices/                     L2  gpu/  input/  snd/  net/   (vhost-user backends, one binary each)
│  └─ base/                        L1  ipc/  trace/  config/  error/  platform/
│
├─ guest/                          L3  guest Android
│  ├─ vendor/                      → synced to $AOSP_ROOT/vendor/droiduse
│  │  ├─ products/{wick_arm64,wick_x86_64,common}/
│  │  ├─ hals/{graphics,keymint,health,sensors,audio}/
│  │  ├─ services/{inspector,adpd,agent,restore}/
│  │  ├─ sepolicy/  avb/  nativebridge/  apps/microg/
│  └─ kernel/{fragments,modules}/
│
├─ clients/                        ADP clients (depend only on contracts/adp)
│  ├─ mcp/
│  └─ sdk/{rust,ts,python}/
│
├─ contracts/                      boundary contracts (zero dependencies)
│  ├─ machine/  device/  guest_link/  keymint/  instance/  adp/  node/  spi/
│  └─ codegen/  compat/
│
├─ third_party/                    L0  upstream + patches
│  ├─ wick/        src/(submodule) patches/
│  ├─ aosp/        manifest pin(versions.toml) · patches/ · security/   (source is an external checkout at $AOSP_ROOT)
│  ├─ kernel/      Kleaf manifest pin · patches/(goal: empty)
│  ├─ gfxstream/   (only when a patch is needed)
│  └─ rutabaga/    (only when a patch is needed)
│
├─ testing/        testapps/{views,compose,webview,flutter,rn,gl}/  e2e/  conformance/  fuzz/  perf/  cts/  golden/
├─ build/          checklayers/  codegen/  toolchains/
├─ tools/          setup/  sync/  patch/  rebase/  bundle/  infra/
└─ docs/
```
- **Start as a monorepo.** Early on the protocols change often and host and guest must change together, so contract changes land in one step.
- **The top-level directories and the layer directories under `host/` = future repository boundaries.** When outside contributors work on only a part, when access rights must be separated, or when a protocol is released on its own, the directory is split off with its history using `git filter-repo --subdirectory-filter`.
- `guest/vendor/` goes into the AOSP tree by **rsync or bind mount**. Soong by default does not accept `Android.bp` under a symlinked directory, so symlinks are not used.

## 4. Runtime diagram
```
  clients/ (MCP · SDK · debugger UI)         control plane (SaaS · self-hosting tools)
        │ ADP: WebSocket JSON-RPC, authenticated       │ Node API: TLS, authenticated
┌───────┼────────────────── Host ──────────────────────┼────────────────────┐
│ entrypoints: adp_server        CLI      node daemon ─┘                     │
│                 │                                                         │
│ services: inspect · screen · input · app · network · log · stream · record│
│                 │  (contracts/instance)                                   │
│ runtime: Instance(FrameChannel) ─ Session pool ─ Snapshot(MemorySource) ─ │
│          Identity ─ KeyMint[jailed] ─ Jail                                │
│      │ (contracts/machine)        │ (contracts/guest_link over vsock)     │
│ vmm: wick + ext [jailed]          │                                       │
│  ├ boot(boot.img/vendor_boot/bootconfig) ├ disk(GPT, CoW layers)          │
│  ├ vmgenid ├ scanout ├ vhost-user frontend(generic) + shm region          │
│      │ (contracts/device = vhost-user)                                    │
│ devices: gpu(rutabaga+gfxstream) · input · snd · net(usernet)             │
│          [each a separate process, each jailed]                           │
└───────────────────────────────────────────────────────────────────────────┘
┌─────────────────────────── Guest (AOSP user build) ───────────────────────┐
│ GKI + vendor modules · AVB · SELinux enforcing                            │
│ app processes (framework hooks) ─┐                                        │
│ system_server hook ──────────────┼ binder ─ adpd ─ vsock ─▶ host          │
│ WebView devtools socket ─────────┘                                        │
│ SurfaceFlinger(frameId, layers) · gfxstream guest · minigbm · HWC         │
│ restore hook(VMGenID) · NativeBridge(berberis) · microG                   │
└───────────────────────────────────────────────────────────────────────────┘
```

## 5. Observability: a single trace across the layers
- The host uses the Perfetto SDK and the guest uses Android's built-in Perfetto. The two are merged on **the same trace ID and time axis**.
- One MCP `act` shows up as one timeline:
  `clients/mcp → host/entrypoints/adp_server → host/services/input → host/devices/input → guest InputDispatcher → app rendering → SurfaceFlinger → host/devices/gpu scanout → host/services/screen`
- The trace ID starts from the ADP request ID and is propagated in the guest_link message headers.

### Symptom → responsible layer
| Symptom | Responsible layer |
|---|---|
| Node bounds are wrong | guest/vendor/services/inspector |
| Tree and screenshot disagree | host/runtime/instance (`FrameChannel` matching) or host/services/inspect (attaching frameId to the tree) |
| Black screen | host/devices/gpu or guest/vendor/hals/graphics (tell them apart with a trace) |
| Input is ignored | host/devices/input or guest idc config |
| Restore is slow | host/runtime/snapshot (MemorySource) or vmm |
| Duplicate IDs or keys across cloned sessions | host/runtime/identity + guest/vendor/services/restore |
| Network blocked | host/services/network (policy) or host/devices/net (transport) |
| MCP response format problem | clients/mcp |
| Boot failure | host/vmm/boot · host/vmm/disk · guest/vendor/products (fstab, init) |
