# host/entrypoints/ (L6)

The entry points that tie runtime and services together and actually run on the host, plus the product defaults. No core logic here.

| Module | Content |
|---|---|
| `adp_server/` | ADP WebSocket JSON-RPC server, authentication (`Authenticator`/`Authorizer` SPI) |
| `node/` | Node daemon + `NodeBuilder` (injects SPI implementations). Basis for both self-hosted and SaaS nodes |
| `cli/` | Development and operations CLI |
| `desktop/` | Embedding API for desktop apps |

ADP clients (SDK, MCP) are not here; they are in `clients/`.
