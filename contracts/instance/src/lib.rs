//! instance contract: services ↔ runtime.
//!
//! "One Android instance" as services see it. Details: `docs/03-contracts.md` instance,
//! `docs/07-runtime.md`.
//!
//! Async methods are declared with `async-trait`. They can be used as trait objects (dyn),
//! assume no particular async runtime, and return `Send` futures. Dropping a future cancels
//! that call.
//!
//! A type marked `PLACEHOLDER` is one only named by the documentation. Its structure and
//! meaning are undecided.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use droiduse_contracts_common::{EventStream, FrameId, Labels, Result};
use droiduse_contracts_spi::SnapshotRef;

// ---------------------------------------------------------------------------
// Instance
// ---------------------------------------------------------------------------

/// One Android instance.
pub trait Instance: Send + Sync {
    /// Instance ID.
    fn id(&self) -> InstanceId;
    /// Opaque labels. The core does not interpret them.
    fn labels(&self) -> &Labels;
    /// Current state.
    fn state(&self) -> InstanceState;
    /// guest_link client.
    fn guest(&self) -> &dyn GuestChannel;
    /// Scanout frames matched to their frameId.
    fn frames(&self) -> &dyn FrameChannel;
    /// Event injection into `host/devices/input`.
    fn input(&self) -> &dyn InputChannel;
    /// Flow observation and policy enforcement for `host/devices/net`.
    fn net(&self) -> &dyn NetObserver;
    /// Instance event stream. Only subscribes and does not wait. Delivery happens on the stream
    /// side.
    fn events(&self) -> EventStream<InstanceEvent>;
}

/// Instance state (exactly the list in `docs/03-contracts.md` instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstanceState {
    /// Being created.
    Creating,
    /// Booting.
    Booting,
    /// Ready for use.
    Ready,
    /// Paused.
    Paused,
    /// Taking a snapshot (entered from `Ready`, back to `Ready` when done, `docs/07-runtime.md`
    /// §2).
    Snapshotting,
    /// Restoring from a snapshot (before `Restore.Notify` is received).
    Restoring,
    /// Stopped.
    Stopped,
    /// Failed. An instance whose lifecycle call was cancelled midway also ends up here. runtime
    /// cleans up the resources.
    Failed,
}

/// Instance ID. PLACEHOLDER: representation undecided (the string only means "an opaque value").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceId(pub String);

/// Instance event. PLACEHOLDER: value set undecided.
#[derive(Debug, Clone)]
pub struct InstanceEvent {}

/// Creates, lists and closes the instances of one host. `host/runtime` implements it over the
/// `MachineFactory` it is given; services and entry points see only this trait.
///
/// **Cancellation**: if `create` or `close` is cancelled midway (future dropped), the instance
/// ends in `Failed` and runtime cleans up the resources, as for [`SessionManager`].
#[async_trait]
pub trait InstanceManager: Send + Sync {
    /// Creates an instance and answers once it is `Ready`. The image and VM size are runtime
    /// settings. On failure no instance remains.
    async fn create(&self, req: CreateInstance) -> Result<Arc<dyn Instance>>;
    /// The current instances, in no particular order.
    fn list(&self) -> Vec<Arc<dyn Instance>>;
    /// The instance `id`, if it exists.
    fn get(&self, id: &InstanceId) -> Option<Arc<dyn Instance>>;
    /// Stops and destroys the instance `id`; it is gone from `list` when this returns `Ok`. An
    /// `id` that names no instance: `TargetGone`.
    async fn close(&self, id: &InstanceId) -> Result<()>;
}

/// What [`InstanceManager::create`] is asked for.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct CreateInstance {
    /// Opaque labels the new instance carries. The core does not interpret them.
    pub labels: Labels,
}

impl CreateInstance {
    /// A request with these labels.
    pub fn new(labels: Labels) -> Self {
        Self { labels }
    }
}

/// guest_link client.
///
/// PLACEHOLDER: methods undecided. They follow the guest_link message definitions (IDL and
/// codegen, format undecided). When async methods are added, they are declared with
/// `#[async_trait]` so that `&dyn` keeps working.
pub trait GuestChannel: Send + Sync {}

/// Event injection into `host/devices/input`. PLACEHOLDER: methods undecided.
pub trait InputChannel: Send + Sync {}

/// Flow observation and policy enforcement for `host/devices/net`. PLACEHOLDER: methods
/// undecided.
pub trait NetObserver: Send + Sync {}

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// Guest frame commits matched exactly to device scanouts. Keeps the most recent N frames.
#[async_trait]
pub trait FrameChannel: Send + Sync {
    /// Frame stream of a display. Only subscribes and does not wait. Delivery happens on the
    /// stream side.
    fn subscribe(&self, display: u32) -> EventStream<Frame>;
    /// Exactly that frame. If it is not matched to a scanout yet, waits until it is matched or
    /// `deadline` passes. `FrameExpired` if it has already been pushed out of the ring buffer,
    /// `Timeout` once `deadline` passes. Never guesses and returns another frame. This is a read
    /// call, so dropping it (cancellation) has no side effects.
    async fn get(&self, id: &FrameId, deadline: Instant) -> Result<Frame>;
    /// Latest frame of a display.
    fn latest(&self, display: u32) -> Result<Frame>;
}

/// One scanout frame.
#[derive(Debug)]
pub struct Frame {
    /// `None` if not matched (not used for requests that name a frameId).
    pub frame_id: Option<FrameId>,
    /// shm reference (no copy).
    pub buffer: FrameBufferRef,
}

/// shm reference to a frame buffer (no copy).
///
/// PLACEHOLDER: representation and lifetime (what happens to this reference when the ring
/// buffer slot is reused) undecided.
#[derive(Debug)]
pub struct FrameBufferRef {}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// Session pool.
///
/// **Cancellation**: if an async method (a lifecycle call) is cancelled midway (future dropped),
/// the target instance ends in `Failed` and runtime cleans up the resources. A half-done
/// instance never shows as `Ready` and is never handed out as a session.
#[async_trait]
pub trait SessionManager: Send + Sync {
    /// Allocates from the snapshot pool.
    async fn acquire(&self, req: SessionRequest) -> Result<Session>;
    /// Resets the session (discards the session's write CoW layer, `docs/07-runtime.md` §5).
    async fn reset(&self, s: &Session) -> Result<()>;
    /// Returns the session.
    async fn release(&self, s: Session) -> Result<()>;
    /// Snapshots the current state of the session as `name`.
    async fn snapshot(&self, s: &Session, name: &str) -> Result<SnapshotRef>;
}

/// Session request. PLACEHOLDER: structure undecided.
#[derive(Debug, Clone)]
pub struct SessionRequest {}

/// An allocated session. PLACEHOLDER: structure undecided.
#[derive(Debug)]
pub struct Session {}

/// Compile-only check: the async traits can be implemented by dummy types, can be used as
/// `Box<dyn …>` and `&dyn …`, and their futures are `Send`.
#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_common::{Error, ErrorCode};

    fn assert_send<T: Send>(_: T) {}
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    fn unimplemented_err() -> Error {
        Error::new(ErrorCode::Unimplemented, "compile-only dummy")
    }

    struct DummySessions;
    #[async_trait]
    impl SessionManager for DummySessions {
        async fn acquire(&self, _req: SessionRequest) -> Result<Session> {
            Err(unimplemented_err())
        }
        async fn reset(&self, _s: &Session) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn release(&self, _s: Session) -> Result<()> {
            Err(unimplemented_err())
        }
        async fn snapshot(&self, _s: &Session, _name: &str) -> Result<SnapshotRef> {
            Err(unimplemented_err())
        }
    }

    struct DummyInstances;
    #[async_trait]
    impl InstanceManager for DummyInstances {
        async fn create(&self, _req: CreateInstance) -> Result<Arc<dyn Instance>> {
            Err(unimplemented_err())
        }
        fn list(&self) -> Vec<Arc<dyn Instance>> {
            Vec::new()
        }
        fn get(&self, _id: &InstanceId) -> Option<Arc<dyn Instance>> {
            None
        }
        async fn close(&self, _id: &InstanceId) -> Result<()> {
            Err(unimplemented_err())
        }
    }

    struct DummyFrames;
    #[async_trait]
    impl FrameChannel for DummyFrames {
        fn subscribe(&self, _display: u32) -> EventStream<Frame> {
            unimplemented!("compile-only dummy")
        }
        async fn get(&self, _id: &FrameId, _deadline: Instant) -> Result<Frame> {
            Err(unimplemented_err())
        }
        fn latest(&self, _display: u32) -> Result<Frame> {
            Err(unimplemented_err())
        }
    }

    /// Every future is `Send` even when called through a trait object.
    fn dyn_futures_are_send(i: &dyn InstanceManager, m: &dyn SessionManager, f: &dyn FrameChannel) {
        let id = InstanceId("i1".to_owned());
        assert_send(i.create(CreateInstance::default()));
        assert_send(i.close(&id));
        let s = Session {};
        assert_send(m.acquire(SessionRequest {}));
        assert_send(m.reset(&s));
        assert_send(m.snapshot(&s, "name"));
        assert_send(m.release(Session {}));
        let id = FrameId {
            display: 0,
            vsync_id: 0,
            sf_frame: 0,
        };
        assert_send(f.get(&id, Instant::now()));
    }

    #[test]
    fn dummy_implements_contract_as_dyn() {
        assert_send_sync::<dyn Instance>();
        assert_send_sync::<dyn FrameChannel>();
        assert_send_sync::<dyn InstanceManager>();
        assert_send_sync::<dyn SessionManager>();
        assert_send_sync::<dyn GuestChannel>();
        assert_send_sync::<dyn InputChannel>();
        assert_send_sync::<dyn NetObserver>();
        let i: Box<dyn InstanceManager> = Box::new(DummyInstances);
        let m: Box<dyn SessionManager> = Box::new(DummySessions);
        let f: Box<dyn FrameChannel> = Box::new(DummyFrames);
        assert!(i.list().is_empty());
        assert!(i.get(&InstanceId("i1".to_owned())).is_none());
        dyn_futures_are_send(i.as_ref(), m.as_ref(), f.as_ref());
    }
}
