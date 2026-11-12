//! Shared contract types.
//!
//! `contracts/` is self-contained (`contracts/LAYER.toml` `allow = []`). Types that several
//! contracts use together (error codes, trace IDs, labels, frameId, etc.) live in this crate,
//! not in another layer. Details: the common rules in `docs/03-contracts.md`.
//!
//! This crate is also visible in `host/vmm` and `host/devices`, through the `machine` and
//! `device` contracts. So no identifier that handles an Android concept goes here
//! (`LAYERS.toml` `[forbidden]`).
//!
//! A type marked `PLACEHOLDER` is one whose name the documentation fixed but whose
//! representation it did not. Its structure and meaning are decided by the project owner.
//! It is not filled in arbitrarily.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;

use futures_core::stream::BoxStream;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Error codes across the contracts.
///
/// The current values are exactly the list in `docs/04-adp.md` §5 "Error codes".
/// `FrameExpired` and `Timeout` of `FrameChannel::get` (`docs/03-contracts.md` instance) are
/// values from this list too.
/// Adding a value is backward compatible; removing one or changing its meaning is a major bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    /// The frame of the requested frameId is not in the ring buffer. No other frame is
    /// substituted. (`FRAME_EXPIRED`)
    FrameExpired,
    /// The target node is gone. (`NODE_GONE`)
    NodeGone,
    /// The target is gone. (`TARGET_GONE`)
    TargetGone,
    /// The action cannot be performed on the target. (`NOT_ACTIONABLE`)
    NotActionable,
    /// Timed out. (`TIMEOUT`)
    Timeout,
    /// Denied by policy. This includes a denial by the `Authorizer` at attach. (`POLICY_DENIED`)
    PolicyDenied,
    /// Credentials are missing or invalid. A message that arrives before authentication is
    /// this too. (`UNAUTHENTICATED`)
    Unauthenticated,
    /// A protocol version that cannot be negotiated. (`UNSUPPORTED_VERSION`)
    UnsupportedVersion,
    /// The message envelope is wrong: a format error, an `id` that collides with a request in
    /// progress, a `sessionId` scope violation, a message the connection state does not allow
    /// (before authentication it is `Unauthenticated`). (`INVALID_REQUEST`)
    InvalidRequest,
    /// Unknown method. (`METHOD_NOT_FOUND`)
    MethodNotFound,
    /// The arguments do not follow the definition: a required value missing, a type or enum
    /// value mismatch, an unknown union `kind`, a selector syntax error or unknown selector
    /// engine, an unknown diff format. (`INVALID_PARAMS`)
    InvalidParams,
    /// The command exists in this protocol version, but this server, or the layer that answers
    /// it, has not built it yet. Not `MethodNotFound`, which means the protocol version has no
    /// such command. (`UNIMPLEMENTED`)
    Unimplemented,
}

/// The error the contract traits return.
///
/// PLACEHOLDER: the documentation uses only `Result<T>` in the trait signatures and does not
/// define the error type. The only documented error codes are the ADP list, so some failures
/// are not on it, such as VM, device and storage failures. Those failures use `code: None`.
/// The error model (code scheme, cause chain, mapping on the wire) is undecided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The documented error code, if one applies. `None` if no code applies.
    pub code: Option<ErrorCode>,
    /// Human-readable description.
    pub message: String,
}

/// Result type of the contract traits.
pub type Result<T, E = Error> = std::result::Result<T, E>;

// ---------------------------------------------------------------------------
// Tracing
// ---------------------------------------------------------------------------

// The single trace that runs through the layers (`docs/02-architecture.md` §5) crosses every
// wire and IPC boundary as W3C Trace Context fields. Details: `trace.rs`.
mod trace;

pub use trace::{
    SpanId, TRACEPARENT, TRACESTATE, TRACESTATE_MAX_MEMBERS, TraceContext, TraceContextError,
    TraceFlags, TraceId, TraceParent, TraceState,
};

// ---------------------------------------------------------------------------
// labels
// ---------------------------------------------------------------------------

/// Opaque key-value pairs of an instance, session or snapshot (`docs/glossary.md` labels).
///
/// The core does not interpret the contents. Concepts such as tenants and plans live in labels
/// and in SPI implementations, not in the core (`docs/01-principles.md` P4).
///
/// `BTreeMap` was chosen to make the order deterministic (snapshot metadata hashes, etc.).
/// Constraints on keys and values (character set, length, reserved prefixes) are undecided.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Labels(pub BTreeMap<String, String>);

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// The reference frame of trees, screenshots and events (`docs/04-adp.md` §3.1).
///
/// `FrameId = { display: u32, vsyncId: i64, sfFrame: u64 }`
///
/// The value comes from the guest. `host/devices` does not handle it (scanouts are exported
/// with virtio-level identifiers, and the `FrameChannel` of `host/runtime/instance` matches the
/// two exactly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameId {
    /// Display number.
    pub display: u32,
    /// App-side Choreographer vsync ID.
    pub vsync_id: i64,
    /// Composition-side SurfaceFlinger frame number.
    pub sf_frame: u64,
}

// ---------------------------------------------------------------------------
// Event streams
// ---------------------------------------------------------------------------

/// Event stream (`Machine::events`, `Instance::events`, `FrameChannel::subscribe`).
///
/// It is a `futures_core::Stream`. Subscribing returns this stream without waiting, and
/// delivery failures such as a connection failure arrive as `Err` items. It can be moved
/// between threads (`Send`).
///
/// Backpressure, handling of slow subscribers, whether the stream continues after an `Err`,
/// and the meaning of its end are undecided.
pub type EventStream<T> = BoxStream<'static, Result<T>>;

/// Compile-only check: `EventStream` is a `Stream` and is `Send` and `Unpin`.
#[cfg(test)]
mod tests {
    use super::*;
    use futures_core::Stream;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    fn assert_stream<S: Stream<Item = Result<u8>> + Send + Unpin>(_: S) {}

    /// Test only: a stream that ends at once.
    struct Ended;
    impl Stream for Ended {
        type Item = Result<u8>;
        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(None)
        }
    }

    #[test]
    fn event_stream_is_send_unpin_stream() {
        let s: EventStream<u8> = Box::pin(Ended);
        assert_stream(s);
    }
}
