//! adp contract: `host/entrypoints/adp_server` ↔ clients and external clients.
//!
//! Details: `docs/04-adp.md`. A JSON-RPC style protocol like CDP (WebSocket, authenticated).
//!
//! - Definition: `protocol.json`. The same structure as the CDP protocol definition: for each of
//!   `domains[]`, `domain`, `types`, `commands` (`parameters`, `returns`), `events`
//!   (`parameters`), plus `description`, `experimental`, `deprecated`, `optional`, `$ref`,
//!   `enum`, `items`, and CDP's `binary` type (base64, RFC 4648 §4, padded).
//!   Extensions not in CDP:
//!   - Top-level `errors`: the ADP error codes.
//!   - Top-level `envelope`: the Request, Response, Event and Error message types, and the
//!     binary message format `binary` (server→client, client→server `App.uploadChunk`).
//!   - `scope` of commands and events: `connection` | `session` | `both`.
//!   - `targetTypes` of a command: the target types (`Target.TargetType` values) of the sessions
//!     that accept the command. If absent, it is not decided yet (it does not mean every type).
//!   - `format`: `int32`, `uint32` on `integer` (only when the width is fixed), `int64`, `uint64`
//!     on `string` (64-bit integers are JSON decimal strings).
//!   - `discriminator` and `variants` of tagged unions.
//!   - `nullable` on optional properties: an explicit `null` clears the value (three states:
//!     absent, `null`, a value).
//! - `VERSION` is the single source of the version. `protocol.json` holds no version.
//! - The definition holds only what the documentation has fixed down to its shape.
//! - Generated code: `build/codegen` (`cargo xtask codegen`) generates it into `src/generated/`,
//!   and it is committed. Generated code is never written by hand. Everything below is generated
//!   code (one module per domain, `errors`, `envelope`, `wire`, [`PROTOCOL_VERSION`], [`Scope`],
//!   [`Nullable`], [`Command`], [`Event`]).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

// Applies only to the generated code. missing_docs: some fields and types in the definition have
// no description.
#[allow(missing_docs)]
mod generated {
    include!("generated/droiduse.adp.rs");
}

pub use generated::*;
