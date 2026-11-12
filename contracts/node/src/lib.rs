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

// Applies only to the generated code.
mod generated {
    include!("generated/droiduse.node.rs");
}

pub use generated::*;

/// gRPC metadata key: W3C Trace Context `traceparent`. Any call can carry it.
pub const TRACEPARENT_METADATA_KEY: &str = "traceparent";

/// gRPC metadata key: W3C Trace Context `tracestate`. Any call can carry it.
pub const TRACESTATE_METADATA_KEY: &str = "tracestate";
