# Glossary

| Term | Meaning |
|---|---|
| **ADP** | Android DevTools Protocol. The CDP-style external protocol of Droid Use |
| **adpd** | The ADP daemon inside the guest. Relays binder ↔ vsock (guest_link) |
| **agent (guest)** | Platform-signed system service. UiAutomation, node actions, accessibility service registration |
| **inspector** | Guest logic called by framework hooks. Collects the View and Compose trees |
| **guest_link** | The host ↔ guest vsock contract |
| **frameId** | `{display, vsyncId, sfFrame}`. The reference frame for trees, screenshots and events |
| **FrameChannel** | The frame contract provided by runtime. Exact matching of the guest frameId to the scanout, keeps recent frames |
| **NodeId** | Opaque ID kept for the lifetime of a node |
| **unified tree** | One tree that merges View, accessibility, Compose, WebView and layers |
| **wick** | Cross-platform Firecracker (Linux KVM, macOS HVF, Windows WHP) |
| **ext** | wick's extension mechanism (`vmm::ext`). Our VMM features are `host/vmm/` crates built the same way |
| **overlay** | Our project added to the AOSP tree (`guest/vendor/` → `vendor/droiduse`) |
| **hook patch** | A patch that adds only call sites to upstream code. The logic lives in overlay/ext |
| **vhost-user** | Standard protocol for implementing a virtio device in a separate process |
| **gfxstream** | Rendering API streaming library. Guest GLES/Vulkan → host GPU |
| **rutabaga** | virtio-gpu backend abstraction (connects gfxstream and others) |
| **usernet** | Our user-mode network stack (smoltcp-based NAT) |
| **VMGenID** | VM generation ID. Lets the guest detect that a snapshot was cloned |
| **restore hook** | Guest reinitialization when VMGenID changes (RNG, identifiers, keystore, network) |
| **MemorySource** | SPI that supplies snapshot memory pages. Local file or remote |
| **CPU template** | Pins the CPU features visible to the guest. Snapshot portability |
| **GKI** | Generic Kernel Image. Google-signed kernel + vendor modules |
| **KMI** | Kernel Module Interface. The kernel ABI vendor modules must keep to |
| **berberis** | AOSP's open-source binary translation framework (NativeBridge) |
| **ASB** | Android Security Bulletin. Monthly security patches |
| **SPI** | Core extension-point traits (`contracts/spi`) |
| **labels** | Opaque key-value pairs on instances, sessions and snapshots. The core does not interpret them |
| **node** | Daemon that runs several instances on one machine (`host/entrypoints/node`) |
| **control plane** | The upper tier that manages several nodes (SaaS, outside this repo) |
