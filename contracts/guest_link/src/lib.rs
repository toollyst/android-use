//! guest_link contract: host (runtime, services) ↔ guest (adpd) vsock protocol.
//!
//! Details: `docs/03-contracts.md` guest_link. The format is Protocol Buffers; the transport is
//! gRPC over vsock.
//!
//! - IDL: `proto/droiduse/guest_link/guest_link.proto` (proto root `proto/`, package
//!   `droiduse.guest_link`)
//! - Generated code: `build/codegen` (`cargo xtask codegen`) generates only the message types
//!   into `src/generated/`, and they are committed. Service bindings (tonic, etc.) are generated
//!   by the layers that use this contract. Generated code is never written by hand.
//! - proto generates no constants for the gRPC metadata keys, so they are written by hand below
//!   (the same as the header comment of the proto file).
//!
//! This contract faces the guest, so it may handle Android concepts. `host/vmm` and
//! `host/devices` do not depend on this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Applies only to the generated code.
mod generated {
    include!("generated/droiduse.guest_link.rs");
}

pub use generated::*;

/// gRPC metadata key: W3C Trace Context `traceparent`. Any call can carry it.
pub const TRACEPARENT_METADATA_KEY: &str = "traceparent";

/// gRPC metadata key: W3C Trace Context `tracestate`. Any call can carry it.
pub const TRACESTATE_METADATA_KEY: &str = "tracestate";

/// gRPC metadata key: session token (opaque secret, binary metadata). Every call on both
/// channels carries it, `HelloService.Hello` included; it is the authentication, and a missing
/// or wrong token is gRPC `UNAUTHENTICATED`. The first token reaches the guest on the secret
/// disk; `AuthService.Rebind` rotates it after every restore (its call carries the previous
/// token).
pub const SESSION_TOKEN_METADATA_KEY: &str = "droiduse-session-token-bin";
