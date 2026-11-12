# contracts/

The only interface across layer boundaries. Details: `docs/03-contracts.md`

| Directory | Boundary | Form |
|---|---|---|
| `machine/` | runtime ↔ vmm | Rust trait |
| `device/` | vmm ↔ devices | vhost-user + extensions |
| `guest_link/` | runtime, services ↔ guest | IDL → codegen |
| `keymint/` | guest KeyMint HAL ↔ host/runtime/keymint | AOSP kmr wire + framing and authentication |
| `instance/` | services ↔ runtime | Rust trait |
| `adp/` | host/entrypoints/adp_server ↔ clients, external clients | JSON-RPC schema (`docs/04-adp.md`) |
| `node/` | control plane ↔ node | IDL (gRPC) |
| `spi/` | core ↔ extension implementations | Rust trait (`docs/09-oss-and-saas.md`) |
| `codegen/` | None: the generator and its configuration are `build/codegen/` (`codegen.toml`, `cargo xtask codegen`); generated code is in each contract's `src/generated/` | - |
| `compat/` | Fixtures of earlier versions, backward-compatibility tests | - |

Every contract directory must have a `VERSION` (semver). Change procedure: `CLAUDE.md` §4
