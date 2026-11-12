# host/services/ (L5)

Per-domain features. Each service owns its domain end to end. Sibling services do not depend on each other directly (they go through the `contracts/instance` channels).

| Module | Domain |
|---|---|
| `inspect/` | Unified tree merge, attaching frameId to the tree, node IDs, diff (ADP `UI`, `Accessibility`, `Target`) |
| `screen/` | Screenshot encoding, region and layer selection (ADP `Screen`). Frame matching and retention are runtime's `FrameChannel` |
| `input/` | Coordinate and node input (ADP `Input`) |
| `app/` | App management, activities, files (ADP `App`) |
| `network/` | Policy decisions, flow and request observation (ADP `Network`) |
| `log/` | Log streams (ADP `Log`) |
| `overlay/` | Highlighting (ADP `Overlay`) |
| `webview/` | CDP relay (ADP `WebView`) |
| `stream/` | Screencast encoding + `StreamTransport` SPI |
| `record/` | Session recording + `Recorder` SPI |
