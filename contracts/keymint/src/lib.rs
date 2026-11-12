//! keymint contract: guest KeyMint HAL ↔ `host/runtime/keymint`.
//!
//! Details: `docs/03-contracts.md` keymint. Uses a dedicated vsock port.
//!
//! - The payload is exactly the kmr wire format of the pinned AOSP tag. It is defined upstream
//!   and not defined again here.
//! - We define only the framing and the authentication. Re-authentication after restore is the
//!   same as for guest_link.
//! - The authentication token is an opaque secret (`bytes`), as in guest_link.
//!
//! The framing (length and type markers, size limits) and the shape of the authentication
//! messages are not in the documentation yet. So this crate is empty. No types are made before
//! the shapes are fixed.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
