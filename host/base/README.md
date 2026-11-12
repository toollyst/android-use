# host/base/ (L1)

Common foundation with no domain knowledge.

| Module | Responsibility |
|---|---|
| `ipc/` | Transport abstraction for vsock, unix socket and shm, message framing |
| `trace/` | Perfetto SDK wrapper, trace ID creation and propagation, default `TelemetryExporter` implementation |
| `config/` | Config loading, validation |
| `error/` | Common error types, error codes |
| `platform/` | OS abstraction (Linux / macOS / Windows) |

Forbidden: Android, domain and tenant concepts
