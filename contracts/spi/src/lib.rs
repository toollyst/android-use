//! spi contract: core extension points.
//!
//! Details: `docs/09-oss-and-saas.md` §2. Semver here is a **promise of stability**.
//! The core has a default implementation of each SPI (`host/runtime/stores`, etc.); SaaS does
//! not fork the core, it injects implementations of these traits to assemble its own node
//! binary.
//!
//! This crate is also visible in `host/devices` and `host/vmm`, through `host/devices/net`
//! (`EgressUpstream`) and the `machine` contract (`MemorySource`, `SnapshotMeta`). So no
//! identifier that handles an Android concept goes here (`LAYERS.toml` `[forbidden]`). There
//! are no tenant or billing concepts either.
//!
//! Injection (documentation): `MemorySource`, `SnapshotStore`, `StreamTransport` and
//! `EgressUpstream` are in-process (Rust traits); `Authorizer` and `PolicyProvider` also have
//! an out-of-process form (gRPC hooks). This crate defines only the Rust trait side. The
//! serialization format of the gRPC hooks is undecided.
//!
//! Async methods are declared with `async-trait`. Every SPI trait can be injected as a trait
//! object (dyn), assumes no particular async runtime, and returns `Send` futures.
//! Dropping a future cancels that call. Read calls (marked in the method docs) have no side
//! effects when dropped. The cancellation semantics of write calls (`put`, `delete`, `commit`,
//! `connect`, `publish`, `emit`, `Recorder`) are undecided.
//!
//! A type marked `PLACEHOLDER` is one only named by the documentation. Its structure and
//! meaning are undecided.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use bytes::Bytes;
use droiduse_contracts_common::{Labels, Result};
use futures_io::{AsyncRead, AsyncWrite};

// Value types of `Authenticator` and `Authorizer`. Details: `auth.rs`.
mod auth;

pub use auth::{
    Action, AdpCommand, BearerToken, ChannelCtx, CommandScope, Credential, CredentialError,
    Decision, DenyReason, InstanceRef, InstanceResource, InstanceScope, Origin, PeerIdentity,
    Principal, PrincipalId, ReasonCode, ReasonCodeError, Resource, TargetRef, TargetResource,
    Transport,
};

// ---------------------------------------------------------------------------
// Snapshot and image stores
// ---------------------------------------------------------------------------

/// Snapshot store. Core default: the local file system. Used by: `host/runtime/snapshot`.
#[async_trait]
pub trait SnapshotStore: Send + Sync {
    /// Stores a snapshot and returns a reference to it.
    async fn put(&self, meta: &SnapshotMeta, parts: SnapshotParts) -> Result<SnapshotRef>;
    /// Opens a snapshot: metadata + device state + `MemorySource` factory. A read call, so
    /// dropping it (cancellation) has no side effects.
    async fn open(&self, r: &SnapshotRef) -> Result<SnapshotHandle>;
    /// The snapshots matching the selector. A read call, so dropping it (cancellation) has no
    /// side effects.
    async fn list(&self, selector: &LabelSelector) -> Result<Vec<SnapshotRef>>;
    /// Deletes a snapshot.
    async fn delete(&self, r: &SnapshotRef) -> Result<()>;
}

/// Image store (the disk base layer). Core default: the local file system.
///
/// PLACEHOLDER: only named in the `docs/09-oss-and-saas.md` §2 table; there is no draft
/// signature.
pub trait ImageStore: Send + Sync {}

/// Source of snapshot memory pages. A local file or a remote page server (lazy restore).
/// Used by: `host/runtime/snapshot` (uffd), `MachineFactory::restore` of the `machine` contract.
// The documentation draft has no `is_empty`. Adding a method is a contract change, so the lint
// is only marked as expected.
#[expect(
    clippy::len_without_is_empty,
    reason = "exactly the docs/09-oss-and-saas.md §2 draft. Adding is_empty is a contract change, which is the owner's decision"
)]
#[async_trait]
pub trait MemorySource: Send + Sync {
    /// Total length (bytes).
    fn len(&self) -> u64;
    /// Reads exactly `len` bytes from `offset` and returns them in an owned buffer.
    ///
    /// The caller does not lend the buffer, so the future can be dropped to cancel even with
    /// completion-based I/O (io_uring, IOCP). A read call, so dropping it (cancellation) has no
    /// side effects.
    async fn read_at(&self, offset: u64, len: usize) -> Result<Bytes>;
    /// Working-set hint. A read call, so dropping it (cancellation) has no side effects.
    async fn prefetch(&self, ranges: &[std::ops::Range<u64>]);
}

/// Store of the disk profile layer (keeps user data). Used by: runtime.
#[async_trait]
pub trait ProfileStore: Send + Sync {
    /// Opens a profile layer.
    async fn open(&self, id: &ProfileId) -> Result<DiskLayer>;
    /// Commits a profile layer.
    async fn commit(&self, id: &ProfileId, layer: DiskLayer) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Authentication, authorization, policy
// ---------------------------------------------------------------------------

/// Authentication. Used by: `host/entrypoints/adp_server`, `host/entrypoints/node`. Core
/// default: local tokens (`host/runtime/auth`).
///
/// An implementation compares secrets only in constant time (see [`BearerToken`]) and never
/// logs a credential.
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Verifies the credential presented on the channel `ch` and returns the principal.
    ///
    /// A credential that does not authenticate is an `Err` with `ErrorCode::Unauthenticated`.
    /// The caller rejects the channel on every `Err` and on a cancelled call (fail closed). A
    /// read call, so dropping it (cancellation) has no side effects.
    async fn authenticate(&self, cred: &Credential, ch: &ChannelCtx) -> Result<Principal>;
}

/// Authorization. Used by: `host/entrypoints/adp_server`, `host/entrypoints/node`.
///
/// The server calls it once per check: at attach, before every command in a session, and for
/// every target it would list. See [`AdpCommand`] for the checks ADP makes.
#[async_trait]
pub trait Authorizer: Send + Sync {
    /// Decides whether `who` may do `action` on `on`.
    ///
    /// **fail closed**: only [`Decision::Allow`] allows; the caller treats a denial, an `Err` and
    /// a cancelled call (future dropped) alike. A read call, so dropping it (cancellation) has no
    /// side effects.
    async fn authorize(&self, who: &Principal, action: &Action, on: &Resource) -> Result<Decision>;
}

/// Policy supply. Used by: `host/services/network`, `host/runtime/resources`. Core default:
/// static configuration.
#[async_trait]
pub trait PolicyProvider: Send + Sync {
    /// Session policy (egress, resource limits, TLS visibility, etc.). A read call, so dropping
    /// it (cancellation) has no side effects.
    async fn session_policy(&self, labels: &Labels) -> Result<SessionPolicy>;
}

// ---------------------------------------------------------------------------
// Network, profiles, streams
// ---------------------------------------------------------------------------

/// Egress upstream. Used by: `host/devices/net` (usernet). Core default: direct connection.
#[async_trait]
pub trait EgressUpstream: Send + Sync {
    /// Opens a connection to the destination.
    async fn connect(&self, dst: &Destination, flow: &FlowCtx) -> Result<Box<dyn Conn>>;
}

/// Device profile supply (model, locale, time zone, location). Used by:
/// `host/runtime/identity`.
#[async_trait]
pub trait DeviceProfileProvider: Send + Sync {
    /// The profile matching the labels. A read call, so dropping it (cancellation) has no side
    /// effects.
    async fn profile(&self, labels: &Labels) -> Result<DeviceProfile>;
}

/// Screen stream transport. Used by: `host/services/stream`. Core default: WebSocket.
#[async_trait]
pub trait StreamTransport: Send + Sync {
    /// Publishes an encoded track and returns the endpoint to connect to.
    async fn publish(&self, session: &SessionId, track: EncodedTrack) -> Result<StreamEndpoint>;
}

// ---------------------------------------------------------------------------
// Events, telemetry, recording
// ---------------------------------------------------------------------------

/// Event output (lifecycle, resource usage, etc.). Used by: runtime, services. Core default:
/// logs.
#[async_trait]
pub trait EventSink: Send + Sync {
    /// Emits one event.
    async fn emit(&self, ev: &Event) -> Result<()>;
}

/// Telemetry export. Used by: `host/base/trace`. Core default: Perfetto files.
///
/// PLACEHOLDER: only named in the `docs/09-oss-and-saas.md` §2 table; there is no draft
/// signature.
pub trait TelemetryExporter: Send + Sync {}

/// Session recording. Used by: `host/services/record`. Core default: local files.
#[async_trait]
pub trait Recorder: Send + Sync {
    /// Starts a recording.
    async fn begin(&self, session: &SessionId) -> Result<RecordingId>;
    /// Appends an item.
    async fn append(&self, id: &RecordingId, item: RecordItem) -> Result<()>;
    /// Finishes the recording.
    async fn finish(&self, id: &RecordingId) -> Result<RecordingRef>;
}

// ---------------------------------------------------------------------------
// Placeholder types (only named by the documentation)
// ---------------------------------------------------------------------------

/// Snapshot metadata.
///
/// PLACEHOLDER: `docs/03-contracts.md` machine (returned by `Machine::snapshot`) and
/// `docs/09-oss-and-saas.md` §2 (argument of `SnapshotStore::put`) use the same name.
/// It is kept as one type. Candidate fields of the versioned format are in
/// `docs/07-runtime.md` §3.
#[derive(Debug, Clone)]
pub struct SnapshotMeta {}

/// Snapshot parts (memory, vCPU state, device state, etc.). PLACEHOLDER.
#[derive(Debug)]
pub struct SnapshotParts {}

/// Reference to a stored snapshot. PLACEHOLDER: representation undecided (the string only
/// means "an opaque value").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotRef(pub String);

/// An open snapshot: metadata + device state + `MemorySource` factory. PLACEHOLDER.
#[derive(Debug)]
pub struct SnapshotHandle {}

/// labels selector. PLACEHOLDER: syntax undecided.
#[derive(Debug, Clone)]
pub struct LabelSelector {}

/// Profile identifier. PLACEHOLDER: representation undecided.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileId(pub String);

/// One disk CoW layer. PLACEHOLDER.
#[derive(Debug)]
pub struct DiskLayer {}

/// Session policy (egress, resource limits, TLS visibility, etc.). PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct SessionPolicy {}

/// Egress destination. PLACEHOLDER.
#[derive(Debug)]
pub struct Destination {}

/// Egress flow context. PLACEHOLDER.
#[derive(Debug)]
pub struct FlowCtx {}

/// Upstream connection: a bidirectional async byte stream. Closed with `AsyncWrite::poll_close`.
pub trait Conn: AsyncRead + AsyncWrite + Send {}

/// Device profile (model, locale, time zone, location). PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct DeviceProfile {}

/// Session identifier. PLACEHOLDER: representation undecided.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

/// Encoded media track. PLACEHOLDER.
#[derive(Debug)]
pub struct EncodedTrack {}

/// Stream endpoint. PLACEHOLDER.
#[derive(Debug, Clone)]
pub struct StreamEndpoint {}

/// An event emitted through `EventSink`. PLACEHOLDER.
#[derive(Debug)]
pub struct Event {}

/// Recording identifier. PLACEHOLDER: representation undecided.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordingId(pub String);

/// One recording item. PLACEHOLDER.
#[derive(Debug)]
pub struct RecordItem {}

/// Reference to a finished recording. PLACEHOLDER: representation undecided.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordingRef(pub String);

/// Compile-only check: the async SPI traits can be implemented by dummy types, can be used as
/// `Box<dyn …>` and `&dyn …`, and their futures are `Send`.
#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_common::Error;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    fn assert_send<T: Send>(_: T) {}

    fn unimplemented_err() -> Error {
        Error {
            code: None,
            message: String::new(),
        }
    }

    struct Dummy;

    #[async_trait]
    impl SnapshotStore for Dummy {
        async fn put(&self, _meta: &SnapshotMeta, _parts: SnapshotParts) -> Result<SnapshotRef> {
            Err(unimplemented_err())
        }
        async fn open(&self, _r: &SnapshotRef) -> Result<SnapshotHandle> {
            Err(unimplemented_err())
        }
        async fn list(&self, _selector: &LabelSelector) -> Result<Vec<SnapshotRef>> {
            Err(unimplemented_err())
        }
        async fn delete(&self, _r: &SnapshotRef) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl MemorySource for Dummy {
        fn len(&self) -> u64 {
            0
        }
        async fn read_at(&self, _offset: u64, _len: usize) -> Result<Bytes> {
            Err(unimplemented_err())
        }
        async fn prefetch(&self, _ranges: &[std::ops::Range<u64>]) {}
    }
    #[async_trait]
    impl ProfileStore for Dummy {
        async fn open(&self, _id: &ProfileId) -> Result<DiskLayer> {
            Err(unimplemented_err())
        }
        async fn commit(&self, _id: &ProfileId, _layer: DiskLayer) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl Authenticator for Dummy {
        async fn authenticate(&self, _cred: &Credential, _ch: &ChannelCtx) -> Result<Principal> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl Authorizer for Dummy {
        async fn authorize(
            &self,
            _who: &Principal,
            _action: &Action,
            _on: &Resource,
        ) -> Result<Decision> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl PolicyProvider for Dummy {
        async fn session_policy(&self, _labels: &Labels) -> Result<SessionPolicy> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl EgressUpstream for Dummy {
        async fn connect(&self, _dst: &Destination, _flow: &FlowCtx) -> Result<Box<dyn Conn>> {
            Ok(Box::new(DummyConn))
        }
    }
    #[async_trait]
    impl DeviceProfileProvider for Dummy {
        async fn profile(&self, _labels: &Labels) -> Result<DeviceProfile> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl StreamTransport for Dummy {
        async fn publish(
            &self,
            _session: &SessionId,
            _track: EncodedTrack,
        ) -> Result<StreamEndpoint> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl EventSink for Dummy {
        async fn emit(&self, _ev: &Event) -> Result<()> {
            Err(unimplemented_err())
        }
    }
    #[async_trait]
    impl Recorder for Dummy {
        async fn begin(&self, _session: &SessionId) -> Result<RecordingId> {
            Err(unimplemented_err())
        }
        async fn append(&self, _id: &RecordingId, _item: RecordItem) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn finish(&self, _id: &RecordingId) -> Result<RecordingRef> {
            Err(unimplemented_err())
        }
    }

    /// Test only: a connection that is always at EOF.
    struct DummyConn;
    impl AsyncRead for DummyConn {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Ok(0))
        }
    }
    impl AsyncWrite for DummyConn {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Ok(0))
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }
    impl Conn for DummyConn {}

    /// Every future is `Send` even when injected and called through trait objects.
    #[allow(clippy::too_many_arguments)]
    fn dyn_futures_are_send(
        store: &dyn SnapshotStore,
        mem: &dyn MemorySource,
        profiles: &dyn ProfileStore,
        authn: &dyn Authenticator,
        authz: &dyn Authorizer,
        policy: &dyn PolicyProvider,
        egress: &dyn EgressUpstream,
        device: &dyn DeviceProfileProvider,
        transport: &dyn StreamTransport,
        sink: &dyn EventSink,
        recorder: &dyn Recorder,
    ) {
        let snap = SnapshotRef(String::new());
        let profile = ProfileId(String::new());
        let session = SessionId(String::new());
        let rec = RecordingId(String::new());
        let labels = Labels::default();
        let ranges: [std::ops::Range<u64>; 0] = [];

        assert_send(store.put(&SnapshotMeta {}, SnapshotParts {}));
        assert_send(store.open(&snap));
        assert_send(store.list(&LabelSelector {}));
        assert_send(store.delete(&snap));
        assert_send(mem.read_at(0, 1));
        assert_send(mem.prefetch(&ranges));
        assert_send(profiles.open(&profile));
        assert_send(profiles.commit(&profile, DiskLayer {}));
        let cred = Credential::Bearer(BearerToken::new("t".to_owned()).expect("b64token"));
        let ch = ChannelCtx::new(Transport::UnixSocket);
        let who = Principal::new(PrincipalId(String::new()), InstanceScope::All);
        let action = Action::Adp(AdpCommand::new(
            "Target.getTargets",
            CommandScope::Connection,
        ));
        assert_send(authn.authenticate(&cred, &ch));
        assert_send(authz.authorize(&who, &action, &Resource::Server));
        assert_send(policy.session_policy(&labels));
        assert_send(egress.connect(&Destination {}, &FlowCtx {}));
        assert_send(device.profile(&labels));
        assert_send(transport.publish(&session, EncodedTrack {}));
        assert_send(sink.emit(&Event {}));
        assert_send(recorder.begin(&session));
        assert_send(recorder.append(&rec, RecordItem {}));
        assert_send(recorder.finish(&rec));
    }

    #[test]
    fn dummy_implements_contract_as_dyn() {
        let d = Dummy;
        dyn_futures_are_send(&d, &d, &d, &d, &d, &d, &d, &d, &d, &d, &d);
        let boxed: Vec<Box<dyn SnapshotStore>> = vec![Box::new(Dummy)];
        assert_send(boxed);
        let conn: Box<dyn Conn> = Box::new(DummyConn);
        assert_send(conn);
    }
}
