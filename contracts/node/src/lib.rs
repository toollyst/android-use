//! node contract: control plane ↔ node daemon.
//!
//! Details: `docs/03-contracts.md` node. The format is Protocol Buffers; the transport is gRPC
//! over TLS (authenticated).
//!
//! - IDL: `proto/droiduse/node/node.proto` (proto root `proto/`, package `droiduse.node`)
//! - Generated code: `build/codegen` (`cargo xtask codegen`) generates only the message types
//!   into `src/generated/`, and they are committed. Service bindings (tonic, etc.) are generated
//!   by the layers that use this contract. Generated code is never written by hand.
//! - proto generates no constants for the gRPC metadata keys, so they are written by hand below
//!   (the same as the header comment of the proto file).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod generated {
    include!("generated/droiduse.node.rs");
}

pub use generated::*;

/// gRPC metadata key: W3C Trace Context `traceparent`. Every call carries it. A missing or
/// invalid `traceparent` makes the receiver start a new trace and drop `tracestate` (W3C Trace
/// Context sections 3.2, 3.3 and 4.3). The same rule as guest_link.
pub const TRACEPARENT_METADATA_KEY: &str = "traceparent";

/// gRPC metadata key: W3C Trace Context `tracestate`. Optional; it goes with `traceparent`.
pub const TRACESTATE_METADATA_KEY: &str = "tracestate";
