# 03. Contracts (contracts/)

Layer boundaries are crossed **only through the contracts in this document**. Contracts play the same role as Chromium's mojom.

## Common rules
- Every contract exists as a schema/trait in `contracts/<name>/` and has a `VERSION` (semver).
- contracts do not depend on other layers. Common types (error codes, trace IDs, etc.) also live inside contracts.
- A backward-incompatible change needs a major bump + a `contracts/compat/` fixture + a migration document.
- A change PR needs approval by the OWNERS of the layers on both sides.
- Serialization contracts (guest_link, adp, node) generate Rust / Kotlin·Java / C++ / TypeScript / Python code with `build/codegen`.
- Every channel is authenticated. The trace context crosses every wire and IPC boundary explicitly as W3C Trace Context `traceparent` and `tracestate` fields or metadata (types in `contracts/common`); inside a process it is ambient.
- If an asynchronous lifecycle call (VM create, start, snapshot, restore, session acquire, etc.) is cancelled midway (future drop), that instance ends in `Failed` and runtime takes care of cleaning up resources (a half-built VM never appears as `Ready`); cancelling a read call has no side effects.

## Contract list
| Contract | Boundary | Format | Transport |
|---|---|---|---|
| `machine` | runtime ↔ vmm | Rust trait + extended REST API | in-process / unix socket |
| `device` | vmm ↔ devices | vhost-user standard + extensions | unix socket + shm |
| `guest_link` | runtime·services ↔ guest | IDL → codegen | gRPC over vsock (two channels) |
| `keymint` | guest KeyMint HAL ↔ host/runtime/keymint | AOSP kmr wire payload + our framing and authentication | dedicated vsock port |
| `instance` | services ↔ runtime | Rust trait | in-process |
| `adp` | host/entrypoints/adp_server ↔ clients·external clients | CDP-style JSON-RPC schema | WebSocket |
| `node` | control plane ↔ node daemon | IDL (gRPC) | TLS |
| `spi` | core ↔ extension implementations | Rust trait (+ some gRPC hooks) | in-process / gRPC |

---

## machine
Definition and control of one VM. No Android concepts.
```rust
struct VmSpec {
    vcpus: u8,
    memory_mib: u32,
    cpu_template: CpuTemplateRef,          // required for snapshot portability
    boot: BootSpec,                        // Linux{kernel,initrd,cmdline} | BootImageV4{boot,vendor_boot,bootconfig}
                                           // bootconfig is an opaque key=value list. vmm does not interpret its contents
                                           // (the androidboot.* values are produced by host/runtime/instance)
    disks: Vec<CompositeDiskSpec>,         // GPT + CoW layers (base ro / snapshot / profile)
    vsock: VsockSpec,
    net: Vec<NetSpec>,                     // vhost-user-net backend socket
    vhost_user: Vec<VhostUserDeviceSpec>,  // {kind, socket, shm_regions}
    vmgenid: bool,
    labels: Labels,                        // opaque map, the core does not interpret it
}

// The creation path is separate from instance control so both can be used as trait objects (dyn).
#[async_trait]
trait MachineFactory: Send + Sync {
    async fn create(&self, spec: VmSpec) -> Result<Box<dyn Machine>>;
    async fn restore(&self, spec: VmSpec, meta: SnapshotMeta, mem: Box<dyn MemorySource>) -> Result<Box<dyn Machine>>;
}

#[async_trait]
trait Machine: Send + Sync {
    async fn start(&self) -> Result<()>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    async fn snapshot(&self, out: SnapshotSink) -> Result<SnapshotMeta>;
    async fn bump_generation(&self) -> Result<()>;   // VMGenID update
    async fn destroy(self: Box<Self>) -> Result<()>;
    fn events(&self) -> EventStream<VmEvent>;        // EventStream<T> = BoxStream<'static, Result<T>>
}
```
- The VMM is a separate process inside a jail. On the runtime side, the REST client in `host/vmm/api` implements this trait (the in-process implementation is for tests).

## device
- The baseline is the **vhost-user standard protocol**. Devices are implemented as vhost-user backends.
- Extensions:
  - Device state transfer of the `SET_DEVICE_STATE_FD` family (device state included in snapshots)
  - virtio shared memory regions (for gfxstream blob resources)
  - **scanout export** (gpu only): the shm fd of the current scanout buffer + a frame commit notification (including a virtio-level identifier). `host/runtime/instance` receives it and exposes it as `FrameChannel`.
- devices do not know Android. They do not put the guest frameId in the frame metadata (that comes over guest_link). `FrameChannel` in `host/runtime/instance` matches the two sides **exactly** by the virtio-level identifier. No estimated matching is done.

## guest_link
The vsock protocol between the host and the guest (adpd).
```
Hello { protocol_version, build_fingerprint, capabilities[] }   // version negotiation is mandatory
Auth.Rebind { prev_token, vmgenid } → { session_token }       // once right after restore, a new token per clone

// Inspection
Inspect.Subscribe { sources[], displays[], throttle_ms }
Inspect.Snapshot  → TreeSnapshot { frame_id, windows[], nodes[] }
Inspect.Diff      ← TreeDiff { base_frame_id, frame_id, ops[] }     // event
Inspect.Node      { node_id } → Node

// Frames
Frame.Committed   ← { frame_id{display, vsync_id, sf_frame}, scanout_ref, ts_ns, layers[] }  // event. scanout_ref = virtio identifier for matching

// Actions (node-based; coordinate input goes through host/devices/input)
Action.Perform    { node_id, action, args } → ActionResult

// Apps and system
App.Install / Launch / Stop / ClearData / List / ActivityStack
App.Lifecycle     ← event (activity, anr, crash)
Log.Stream        { filters } ← LogEntry stream
WebView.Targets   → [ { target_id, pkg, url } ]
WebView.Attach    { target_id } → CDP proxy stream

// Debug (host authorization only, for CTS/VTS)
Debug.SetAdb      { enabled }

// Restore
Restore.Notify    ← { generation, completed_steps[] }   // notification that guest reinitialization finished after a VMGenID change
```
- gRPC over vsock, with two gRPC channels on two vsock ports. On the host channel the host is the server and offers the guest → host services (Hello, Rebind, and future guest-initiated services). On the guest channel adpd is the server and offers the host → guest services (Inspect, WebView, Debug, ...); only adpd's SELinux domain may listen on vsock. The port numbers are settings; defaults: host channel 7100, guest channel 7101. The message size limit is set per channel.
- There is no separate authentication call: the session token every call carries is the authentication. `Auth.Rebind` rotates it.
- Errors are gRPC status codes. A failure with a droiduse `ErrorCode` also carries `google.rpc.ErrorInfo` (reason = the `ErrorCode` name, domain `droiduse.guest_link`) in the `grpc-status-details-bin` trailer. A version mismatch at Hello is `FAILED_PRECONDITION` with reason `UNSUPPORTED_VERSION`.
- Every call on both channels carries the session token in the metadata `droiduse-session-token-bin` and the trace in W3C Trace Context (`traceparent`, `tracestate`) metadata. A missing or wrong token is `UNAUTHENTICATED`. The token is an opaque secret (bytes); `vmgenid` is 16 bytes.
- The first session token reaches the guest through a dedicated secret disk attached by the VMM, and only adpd's SELinux domain can read that disk. `Auth.Rebind` changes the token after every restore.
- The KeyMint HAL channel uses a **dedicated vsock port separate from guest_link**, and its contract is `keymint` (below).

## keymint
The channel between the guest KeyMint HAL and the host `host/runtime/keymint`.
- The payload uses the kmr wire format of the pinned AOSP tag as is; we define only the framing and authentication.
- Re-authentication after restore is the same as for guest_link.

## instance
"One Android instance" as services see it.
```rust
trait Instance: Send + Sync {
    fn id(&self) -> InstanceId;
    fn labels(&self) -> &Labels;
    fn state(&self) -> InstanceState;          // Creating | Booting | Ready | Paused | Snapshotting | Restoring | Stopped | Failed
    fn guest(&self) -> &dyn GuestChannel;      // guest_link client
    fn frames(&self) -> &dyn FrameChannel;     // scanout frames matched to a frameId
    fn input(&self) -> &dyn InputChannel;      // event injection into host/devices/input
    fn net(&self) -> &dyn NetObserver;         // flow observation and policy enforcement in host/devices/net
    fn events(&self) -> EventStream<InstanceEvent>;
}

// Create, list and close; host/runtime implements it over an injected MachineFactory
#[async_trait]
trait InstanceManager: Send + Sync {
    async fn create(&self, req: CreateInstance) -> Result<Arc<dyn Instance>>;   // answers once Ready; image and VM size are runtime settings
    fn list(&self) -> Vec<Arc<dyn Instance>>;
    fn get(&self, id: &InstanceId) -> Option<Arc<dyn Instance>>;
    async fn close(&self, id: &InstanceId) -> Result<()>;                       // unknown id: TargetGone
}
struct CreateInstance { labels: Labels }

// Exact matching of guest frame commits + device scanout, keeps the last N frames
#[async_trait]
trait FrameChannel: Send + Sync {
    fn subscribe(&self, display: u32) -> EventStream<Frame>;
    async fn get(&self, id: &FrameId, deadline: Instant) -> Result<Frame>;
        // Waits until matched. FrameExpired if already evicted, Timeout once the deadline passes. Never guesses and returns another frame
    fn latest(&self, display: u32) -> Result<Frame>;
}
struct Frame {
    frame_id: Option<FrameId>,   // None if not matched (not used for requests that name a frameId)
    buffer: FrameBufferRef,      // shm reference (no copy)
}

#[async_trait]
trait SessionManager: Send + Sync {
    async fn acquire(&self, req: SessionRequest) -> Result<Session>;   // allocated from the snapshot pool
    async fn reset(&self, s: &Session) -> Result<()>;
    async fn release(&self, s: Session) -> Result<()>;
    async fn snapshot(&self, s: &Session, name: &str) -> Result<SnapshotRef>;
}
```

## adp
The protocol for external clients. Details in `docs/04-adp.md`.

## node
The API through which a control plane (SaaS or a self-hosting tool) controls the node daemon.
```
Node.Info        → { node_id, host_os, arch, capacity{vcpu, mem, gpu}, versions }
Node.Health      → { status, instances, load }
Instance.Create  { image_ref, snapshot_ref?, profile_ref?, device_profile?, policy?, labels } → InstanceId
Instance.Destroy / Get / List(label_selector)
Session.Acquire / Reset / Release
Snapshot.Export  { instance_id } → SnapshotRef (to the SnapshotStore)
Snapshot.Import  { snapshot_ref }
Events.Stream    ← lifecycle and resource usage events (basis for metering)
Adp.Endpoint     { session_id } → { url, token }   // issues ADP connection details
Debug.SetAdb     { instance_id, enabled }         // for CTS/VTS. Not exposed to ADP or MCP
```

## spi
Core extension points. Details in `docs/09-oss-and-saas.md`. **Stability is promised** through semver.
