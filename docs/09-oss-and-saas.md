# 09. Open core and SaaS extension

## 1. Boundary
```
[droid-use-cloud: separate private repository]
   control plane: tenants · authentication · billing · scheduler · fleet · dashboard
   storage (GCS, etc.) · WebRTC gateway · proxy · recording archive
           │  contracts/node (Node API)  +  injected contracts/spi implementations
[droid-use: this repository, Apache-2.0]
   host/entrypoints/node: node daemon that runs several instances on one machine
   runtime / services / devices / vmm / guest
```
- **Core**: everything that happens inside one machine. It is complete as a self-hosted single node.
- **SaaS**: work that spans several machines and several tenants. It does not fork the core; it only **replaces and adds**.
- SaaS code does not go into this repository. The core does not know tenant or billing concepts.
- License: **Apache-2.0** (the same as wick and Firecracker). The competitive edge is built in the operations tier (fleet, snapshot distribution speed, profiles, observability, dashboard).

## 2. SPI (contracts/spi)
| SPI | Used in | Core default implementation | Example SaaS implementation |
|---|---|---|---|
| `SnapshotStore`, `ImageStore` | host/runtime/snapshot | Local filesystem | Object storage + regional cache |
| `MemorySource` | host/runtime/snapshot (uffd) | Local file | Remote page server (lazy restore) |
| `ProfileStore` | runtime (disk profile layer) | Local | Per-user login state retention |
| `Authenticator`, `Authorizer` | host/entrypoints/adp_server, host/entrypoints/node | Local tokens | API keys, OIDC, tenant permissions |
| `PolicyProvider` | host/services/network, host/runtime/resources | Static config | Dynamic policy per tenant and plan |
| `EgressUpstream` | host/devices/net (usernet) | Direct connection | Proxy chain, region selection |
| `DeviceProfileProvider` | host/runtime/identity | Default profile | Device model, locale, time zone and location profiles |
| `StreamTransport` | host/services/stream | WebSocket | WebRTC |
| `EventSink` | runtime, services | Log | Metering, audit log |
| `TelemetryExporter` | host/base/trace | Perfetto file | Central OTel collection |
| `Recorder` | host/services/record | Local file | Archive + replay |

### Draft signatures
```rust
// Traits with async methods use #[async_trait]: they can be injected as dyn and the futures are Send
pub trait SnapshotStore: Send + Sync {
    async fn put(&self, meta: &SnapshotMeta, parts: SnapshotParts) -> Result<SnapshotRef>;
    async fn open(&self, r: &SnapshotRef) -> Result<SnapshotHandle>;   // metadata + device state + MemorySource factory
    async fn list(&self, selector: &LabelSelector) -> Result<Vec<SnapshotRef>>;
    async fn delete(&self, r: &SnapshotRef) -> Result<()>;
}
pub trait MemorySource: Send + Sync {
    fn len(&self) -> u64;
    async fn read_at(&self, offset: u64, len: usize) -> Result<Bytes>;   // owned buffer (bytes::Bytes)
    async fn prefetch(&self, ranges: &[std::ops::Range<u64>]);          // working-set hint
}
pub trait ProfileStore: Send + Sync {
    async fn open(&self, id: &ProfileId) -> Result<DiskLayer>;
    async fn commit(&self, id: &ProfileId, layer: DiskLayer) -> Result<()>;
}
pub trait Authenticator: Send + Sync {
    async fn authenticate(&self, cred: &Credential, ch: &ChannelCtx) -> Result<Principal>;
}
pub trait Authorizer: Send + Sync {
    async fn authorize(&self, who: &Principal, action: &Action, on: &Resource) -> Result<Decision>;   // Err or cancellation is a denial (fail closed)
}
pub trait PolicyProvider: Send + Sync {
    async fn session_policy(&self, labels: &Labels) -> Result<SessionPolicy>;   // egress, resource limits, TLS visibility, etc.
}
pub trait EgressUpstream: Send + Sync {
    async fn connect(&self, dst: &Destination, flow: &FlowCtx) -> Result<Box<dyn Conn>>;
}
pub trait Conn: AsyncRead + AsyncWrite + Send {}   // futures_io
pub trait DeviceProfileProvider: Send + Sync {
    async fn profile(&self, labels: &Labels) -> Result<DeviceProfile>;
}
pub trait StreamTransport: Send + Sync {
    async fn publish(&self, session: &SessionId, track: EncodedTrack) -> Result<StreamEndpoint>;
}
pub trait EventSink: Send + Sync { async fn emit(&self, ev: &Event) -> Result<()>; }
pub trait Recorder: Send + Sync {
    async fn begin(&self, session: &SessionId) -> Result<RecordingId>;
    async fn append(&self, id: &RecordingId, item: RecordItem) -> Result<()>;
    async fn finish(&self, id: &RecordingId) -> Result<RecordingRef>;
}
```

### Injection
- **in-process (Rust trait)**: performance-critical ones (`MemorySource`, `SnapshotStore`, `StreamTransport`, `EgressUpstream`). The SaaS repository takes the core crates as dependencies and **assembles its own node binary**. No dynamic loading.
- **out-of-process (gRPC hooks)**: control decisions (`Authorizer`, `PolicyProvider`). No language constraint.
- The core's `host/entrypoints/node` exposes a **builder API** that accepts SPI implementations.
```rust
NodeBuilder::new(config)
    .snapshot_store(my_store)
    .memory_source_factory(my_remote_pages)
    .authenticator(my_auth)
    .event_sink(my_metering)
    .build()?
    .run()
```

## 3. What must be in the design now (expensive to fix later)
1. **Disk layering**: base / snapshot / profile CoW (`host/vmm/disk`)
2. **The `MemorySource` abstraction**: restore does not read local files directly
3. **Pinned CPU template** and the template and image hashes in the snapshot metadata
4. **Authentication on every channel**: ADP, Node API, streams, guest_link
5. **Opaque `labels`**: on all instances, sessions and snapshots
6. **Resource accounting events**: the basis for metering
7. **Versioned snapshot format**: incompatible restores are rejected

## 4. SaaS repository layout (reference only, not created in this repository)
```
droid-use-cloud/
├─ node/            node binary assembled from the core crates + SPI implementations
├─ control-plane/   tenants · authentication · scheduler · billing · public API
├─ gateway/         WebRTC · ADP remote proxy
├─ storage/         snapshots · images · profiles · recordings
└─ dashboard/       live view · replay · trace viewer
```
