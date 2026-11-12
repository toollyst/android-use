# 04. ADP: Android DevTools Protocol

> The core of the product. A protocol that lets AI (or any client) read and operate an app's screen **as precisely as CDP**.
> MCP is one of the clients (adapters) on top of ADP. The protocol must not be tied to the shape of LLM tools, so that SDKs, debuggers, QA tools and other agent frameworks (droidrun, mobile-use, etc.) use the same protocol.

## 1. Design goals
1. **Completeness**: expose as much of the structure of what is drawn on screen as possible, beyond what the accessibility API shows.
2. **Consistency**: guarantee that trees, screenshots and events refer to **the same frame**.
3. **Stability**: a node ID is kept for the node's lifetime. Selectors and actions do not waver.
4. **No app cooperation needed**: works regardless of debuggable status, app installation or permission settings (built into the system).
5. **General-purpose protocol**: CDP style (domains, commands, events). Language-independent SDKs.

## 2. Data sources
| Source | What it provides | Where it is implemented |
|---|---|---|
| **View tree** | class, resource-id, text, contentDescription, bounds, visibility, alpha, z-order, enabled/focused/selected/checked, scroll state | AOSP patch: `ViewRootImpl` traversal hook → `guest/vendor/services/inspector` |
| **Accessibility tree** | semantics (role, actions, state) | `guest/vendor/services/agent` (UiAutomation) |
| **Compose** | semantics tree, node bounds | inspector reads the semantics owner of `AndroidComposeView`. Falls back to the accessibility tree on failure |
| **WebView** | **real CDP (the full DOM)** | AOSP patch: the debugging policy is enabled in a `WebViewFactory` hook → ADP relays it as a CDP target (no Chromium fork) |
| **Flutter / React Native** | semantics tree | once agent is registered as an accessibility service, the app exposes its semantics (no patch needed) |
| **Games · custom renderers** | layer structure and pixels | AOSP patch: SurfaceFlinger layer information and per-layer capture |
| **Windows · tasks** | focused app, activity stack, dialogs, IME, toasts, notifications, system UI | AOSP patch: WindowManager/ActivityTaskManager event hooks |
| **Pixels** | full screen, region, per layer | host scanout (`host/devices/gpu` → `host/vmm/scanout`) + SurfaceFlinger layer capture |
| **Network** | per-app flows, (when policy allows) requests and responses | `host/devices/net` observation + `host/services/network` policy. TLS visibility injects a per-session CA into the system store (on/off by policy) |
| **Logs · crashes** | logcat, ANR, tombstone | adpd |

## 3. Core guarantees
### 3.1 Frame synchronization
- Every tree snapshot, diff, screenshot and event carries a `frameId`.
- `FrameId = { display: u32, vsyncId: i64, sfFrame: u64 }`
  - `vsyncId`: Choreographer vsync ID (app side)
  - `sfFrame`: SurfaceFlinger frame number (composition side)
- The host (`FrameChannel` in `host/runtime/instance`) keeps the scanout of the last N frames in a ring buffer. `Screen.captureScreenshot({frameId})` returns **exactly that frame** or the `FRAME_EXPIRED` error. It never guesses and returns another frame.
- runtime's `FrameChannel` matches the guest frameId and the host scanout **exactly** by the virtio-level identifier. It does not estimate, and a frame that was not matched is not used for requests that name a frameId. `host/services/inspect` and `host/services/screen` use the same matching result.

### 3.2 Stable node IDs
- `NodeId` is an opaque string. The same View / Compose node / accessibility node keeps the same ID while it is alive.
- When a node disappears, its ID is not reused. (Corresponds to CDP `backendNodeId`)
- Actions can target a node ID. If the target is gone: `NODE_GONE`.

### 3.3 Coordinate system
- All bounds are converted to **screen coordinates in physical display pixels**.
- Rotation, density, system insets, multi-window and display scale are taken into account.
- Input coordinates use the same coordinate system.

### 3.4 Unified tree
- View, accessibility, Compose, WebView DOM and layers are merged into one tree.
- Every node has a `source` field: `view | a11y | compose | webview | layer`
- Merge rules:
  1. The View tree is the skeleton
  2. Accessibility nodes are merged into their source View as semantic attributes (added as virtual nodes when the source View is unknown)
  3. Compose semantics nodes are attached as children under `AndroidComposeView`
  4. WebView nodes are attached under their View as a `webview` subtree (summary); the full DOM is accessed through the `WebView` domain
  5. Layers with no counterpart in the tree (game surfaces, etc.) are attached as `layer` nodes

## 4. Message format
JSON-RPC style, like CDP. The definition file is `contracts/adp/protocol.json`.
- `id` is an integer (below 2^53) and does not collide among requests in flight on a connection.
- `sessionId` is absent on connection-level calls (`Connection.*`, `Target.*` outside a session) and is mandatory on every message to a session.
- An error is `{ "id", "error": { "code": "<error code name>", "message", "data"? } }`. `data.reason`, when present, names the component and operation that answered (`<component>#<operation>`, for example `host/vmm/api#create`); it is for diagnosis, a client decides by `code`. The codes are the error code list below (the same code space as `ErrorCode` in `contracts/common`).
- 64-bit integers (`vsyncId`, `sfFrame`, etc.) are sent as decimal strings in JSON; integers of 32 bits or less are sent as numbers.
- In a binary WebSocket message, the first 8 bytes (a big-endian unsigned integer) are the `id` of the request the message belongs to, and the rest is the content. The server sends it before the response with the same `id`. The client sends exactly one after each `App.uploadChunk` request, and the server responds after receiving it (also when it rejects). A client binary message with no matching request closes the connection with status code 1002 (RFC 6455 §7.4.1).
- Authentication: the `Authorization: Bearer` header of the WebSocket upgrade (RFC 6750), or a first message `Connection.auth`. Before authentication every other message is rejected with `UNAUTHENTICATED`. Tokens in the URL query string are not accepted. Both paths are verified by the `Authenticator` SPI. Authentication happens once per connection. `Connection.auth` on an authenticated connection is `INVALID_REQUEST`; to change credentials, open a new connection.
- Right after authentication, `Connection.hello` must be sent once, before any other command. Any other command before it, and a second `hello`, is `INVALID_REQUEST`.
- A request may carry W3C Trace Context `traceparent` and `tracestate`. The server treats them as untrusted, validates the format and ignores an invalid value (Trace Context §3.2, §3.3); the request is not rejected for it. Continuing the client's trace or starting a new trace with a link is a server setting; default: a new trace with a link.
- An upgrade whose `Authorization` header does not authenticate gets HTTP 401 with `WWW-Authenticate: Bearer error="invalid_token"` (RFC 6750 §3.1). A failed `Connection.auth` closes the connection with status 1008 (RFC 6455 §7.4.1). So does a version mismatch at `hello`, after `UNSUPPORTED_VERSION` with `error.data.protocolVersion` (the server's version); there is no retry on the same connection.
- Authorization is checked at attach and again before every command in a session. A denial answers that command with `POLICY_DENIED` and then ends the session with `detachedFromTarget` reason `authorizationRevoked`.
- The target type a session command accepts is its `targetTypes` in `protocol.json`; in the session of another target type it gets `INVALID_REQUEST`. Tree, selector, node action, touch, key, text and screenshot commands belong to a display target's session; `App.*` (including uploads) and `Screen.getDisplays` to an instance target's session.
- A webview target's session carries CDP messages unchanged (CDP flat-session mode). How the ADP envelope rules apply inside it is not defined yet.
- Screen-changing input commands (node actions, touch, key and text input) choose when they resolve with `resolveOn` (`dispatched` | `frameCommitted` | `idle`, default `frameCommitted`) and return the latest committed `frameId` at that point (none for `dispatched`). The wait for `frameCommitted` and `idle` is bounded by `timeoutMs`; the command succeeds either way, and `conditionMet` in the result is `true` when the awaited condition was reached and `false` when `timeoutMs` elapsed first. The idle criterion and the default `timeoutMs` are server settings.
- Node clicks (`click`, `longClick`) take `method`: `accessibility` (`AccessibilityNodeInfo` `ACTION_CLICK` / `ACTION_LONG_CLICK` only), `touch` (a synthesized touch at the node's centre) or `auto` (accessibility when the node exposes the action, touch otherwise); default `auto`. The result reports the method used.
- Touch input follows the `android.view.MotionEvent` model (`ACTION_DOWN`, `ACTION_POINTER_DOWN`, `ACTION_MOVE`, `ACTION_POINTER_UP`, `ACTION_UP`, `ACTION_CANCEL`); `pointers` lists every pointer that is down.
- `Node.role` is a WAI-ARIA 1.2 role name assigned by the table `contracts/adp/roles/role-map.json`; a node without an entry has no role. `Node.actions` lists the node's `AccessibilityNodeInfo` actions: standard actions by constant name, custom actions by id and label.
- The `ops` of `UI.treeMutated` are node-ID-based operations (`insert {parentId, previousSiblingId?, node}`, `update {nodeId, changes}`, `remove {nodeId}`, `move {nodeId, parentId, previousSiblingId?}`). They are applied in array order, and a parent comes before its children. `update` carries only the changed fields, and `null` clears an optional field. If the client's current frame is not `baseFrameId`, it fetches the tree again with `getTree`. The ops are not applied to a different base frame. The diff format is extended through `DiffFormat` (currently only `nodeOps`).
- A screenshot chooses its delivery with `transfer` (`binary` | `base64`, default `binary`). Without `frameId` it is the last committed frame of that display, and the captured `frameId` is always returned.
- Server settings and their defaults:
  - Idle criterion: no accessibility event for 500 ms (`UiAutomation.waitForIdle` with the quiet time androidx.test.uiautomator `QueryController` uses). Default `timeoutMs`: 10 000 ms (uiautomator `Configurator` wait-for-idle timeout).
  - Long-click margin over the guest's long-press timeout (`method` `touch`): default 100 ms.
  - Maximum upload size: default 4 GiB − 1 byte. `maxChunkSize`: default 1 MiB. Upload idle expiry: default 60 s. Unfinished uploads per session: default 4.
  - Maximum WebSocket message size: default 16 MiB (larger: close with status 1009, RFC 6455 §7.4.1). It is at least `maxChunkSize` + 8 bytes.
  - Keepalive: a Ping (RFC 6455 §5.5.2) every 30 s by default; the connection is closed after 2 unanswered Pings by default.
  - Authentication deadline for the first-message path: default 10 s (then close with status 1008).
  - Failed-authentication rate limit: default 10 failures per 60 s per peer address (upgrade: HTTP 429, RFC 6585 §4, with `Retry-After`, RFC 9110 §10.2.3; `Connection.auth`: close with status 1008).
- APK upload: `App.createUpload(size)` declares the size first (the server's maximum size is a setting; above it, `INVALID_PARAMS` before any bytes are received). Chunks no larger than the `maxChunkSize` in the result (a server setting) are sent as an `App.uploadChunk(uploadId)` request + a client binary message with that request's `id`, and each chunk gets a response. If the SHA-256 does not match at `App.finishUpload(uploadId, sha256)`, the result is `INVALID_PARAMS`. `install` points to a finished upload with `{kind: "upload", uploadId}` and consumes it. A split-APK install names all its uploads with `{kind: "uploads", uploadIds}`. An install failure is `NOT_ACTIONABLE` with `data {status, legacyStatus}` from `PackageInstaller`. An upload belongs to the connection and session that created it and is single-use; it is discarded when the connection drops, the session ends, or the idle expiry (a server setting) passes.
```jsonc
// request
{ "id": 7, "sessionId": "s_abc", "method": "UI.getTree", "params": { "sources": ["view","compose","a11y"], "depth": -1 } }
// response
{ "id": 7, "result": { "frameId": { "display": 0, "vsyncId": "18233", "sfFrame": "9921" }, "root": { /* Node */ } } }
// event
{ "method": "UI.treeMutated", "sessionId": "s_abc", "params": { "baseFrameId": {...}, "frameId": {...}, "ops": [ /* insert/remove/update */ ] } }
```

### Core types
```ts
type Node = {
  id: NodeId; source: "view"|"a11y"|"compose"|"webview"|"layer";
  windowId: string; package?: string;
  className?: string; resourceId?: string; role?: string;
  text?: string; contentDescription?: string; hint?: string;
  bounds: Rect;                       // screen pixels
  visible: boolean; alpha?: number; z?: number;
  enabled?: boolean; focused?: boolean; selected?: boolean; checked?: boolean;
  clickable?: boolean; longClickable?: boolean; scrollable?: boolean; editable?: boolean;
  actions?: string[];                 // actions that can be performed
  webviewTargetId?: string;           // CDP target reference for a WebView
  children?: Node[];
};
type Rect = { x: number; y: number; w: number; h: number };
```

## 5. Domains
| Domain | Commands | Events |
|---|---|---|
| `Connection` | `auth(token)`, `hello(protocolVersion)` | - |
| `Target` | `createTarget` → `targetId` (a new instance, from server defaults), `closeTarget(targetId)`, `getTargets` (instances, displays, windows, processes, WebViews; only targets the principal can attach to), `attachToTarget(targetId)` → `sessionId`, `detachFromTarget(sessionId)` | `targetCreated/Destroyed`, `detachedFromTarget(sessionId, reason)` |
| `UI` | `getTree(sources, depth, windowId?)`, `querySelector(selector, frameId?)` / `querySelectorAll(selector, frameId?)` → `{frameId, nodeIds}`, `getNode(nodeId)` → `{frameId, node}`, `describe(id)` | `treeMutated`, `windowChanged`, `focusChanged`, `imeChanged` |
| `Accessibility` | `getFullAXTree`, `performAction(nodeId, action, arguments?)` (`AccessibilityNodeInfo` actions and `ACTION_ARGUMENT_*` arguments, `resolveOn`) | - |
| `Input` | Touch: `dispatchTouch(type, pointers, actionPointerId?)` (MotionEvent actions) / key: `dispatchKey(type, keyCode)` (`KEYCODE_*` names) / text: `insertText(text)` / node: `click(nodeId, method?)`, `longClick(nodeId, method?)`, `setText`, `scrollTo`, `scrollBy`, `swipe` (all with `resolveOn`) | - |
| `Screen` | `captureScreenshot(region?, layer?, frameId?, format, quality?, transfer?)`, `startScreencast`, `stopScreencast`, `getDisplays` | `frameCommitted`, `screencastFrame` |
| `App` | `createUpload(size)`, `uploadChunk(uploadId)`, `finishUpload(uploadId, sha256)`, `install(source)`, `uninstall`, `launch(packageName, activity?, intent?)`, `stop`, `clearData`, `list` → `PackageInfo[]`, `getActivityStack`, `grantPermission`, `pushFile`, `pullFile` | `activityLifecycle`, `anr`, `crash`, `notificationPosted` |
| `Network` | `enable`, `getResponseBody`, `setPolicy` | `requestWillBeSent`, `responseReceived`, `flowOpened/Closed` |
| `Log` | `enable(filters)` | `entryAdded` |
| `Overlay` | `highlightNode`, `hideHighlight` | - |
| `WebView` | `listTargets`, `attach(targetId)` → proxy to the inner CDP session | CDP events relayed unchanged |
| `Session` | `snapshot(name)`, `reset`, `getInfo` | `restored` |

### Selectors
```
resource-id:com.app:id/login_button
text:"Log in"          text~:"Log"        (substring match)
role:button[enabled]
class:android.widget.EditText >> nth(0)
xpath://*[@resourceId="..."]
```
- The grammar and semantics are in `contracts/adp/selectors/selector.ebnf`, and the cases an implementation must pass are in `contracts/adp/selectors/conformance.json`.
- `engine:body` parts are chained with `>>`, and `nth(N)` picks one. A chained selector searches relative to the previous result (Playwright selector chaining). The set of engines is open, and an unknown engine is `INVALID_PARAMS`.
- Attributes are the ADP `Node` fields as they are. `text~` is a case-sensitive substring match. `xpath` is XPath 1.0, evaluated over the XML representation of the unified tree (element name `node`, attributes = `Node` fields).

### Error codes
| Code | When |
|---|---|
| `FRAME_EXPIRED` | The frame for the requested frameId is not in the ring buffer |
| `NODE_GONE` | The target node is gone |
| `TARGET_GONE` | The target is gone |
| `NOT_ACTIONABLE` | The action cannot be performed on the target |
| `TIMEOUT` | Timed out |
| `POLICY_DENIED` | Denied by policy, including an Authorizer denial at attach or at a command |
| `UNAUTHENTICATED` | Credentials are missing or invalid, or the message came before authentication |
| `UNSUPPORTED_VERSION` | There is no protocol version that can be negotiated |
| `INVALID_REQUEST` | The envelope is wrong: malformed, an `id` that collides with a request in flight, a `sessionId` scope violation, a `sessionId` not on the connection, a session of a target type that does not accept it, a message the connection state does not allow (a command before `hello`, a second `hello`, `auth` on an authenticated connection, a concurrent second `uploadChunk` for one upload, a `createUpload` beyond the per-session upload limit) |
| `METHOD_NOT_FOUND` | Unknown method |
| `INVALID_PARAMS` | The arguments do not follow the definition: a required value is missing, a type or enum value does not match, an unknown union `kind`, a selector syntax error or unknown engine, an unknown diff format |
| `UNIMPLEMENTED` | The command exists in the protocol version, but the server, or the layer that answers it, has not built it yet |

## 6. Transport path
```
app process (inspector hook) ─┐
system_server hook ───────────┼─ binder ─ adpd (dedicated SELinux domain)
WebView devtools socket ──────┘            │ guest_link (vsock, version negotiation, session token)
                                           ▼
          host/services/inspect · screen · input · app · network · log · webview
                                           │ (contracts/instance)
          host/entrypoints/adp_server ── WebSocket JSON-RPC (authenticated) ── clients/(MCP · SDK) · debugger UI
          host/vmm/scanout ── pixels straight from host memory (no guest round trip)
```

## 7. Performance targets (initial values, adjusted as measured)
| Metric | Definition |
|---|---|
| Tree update latency | Guest frame commit → `UI.treeMutated` received |
| Screenshot latency | Request → PNG/WebP response (scanout path) |
| Input→screen | `Input.*` request → `frameCommitted` of the resulting frame |
| Full tree size | JSON size for a typical app screen (before and after compression) |

## 8. MCP adapter (clients/mcp)
Exposes ADP in a summarized form that is easy for an LLM to use.

| Tool | Behaviour |
|---|---|
| `observe` | Frame-synchronized screenshot + compressed unified tree (focused on interactive nodes, with node IDs) |
| `act` | `{nodeId | x,y, action, args}` → result + new frameId |
| `wait_for` | Condition (selector appears/disappears, text, activity, network idle) → based on tree diff events |
| `screenshot` | Capture of a given region or layer |
| `app_*` | install / launch / stop / list / clear |
| `session_*` | reset / snapshot |
| `network_*`, `log_*` | Queries and filters |

Compression rules: remove invisible nodes, fold meaningless containers, normalize text, one line per node as `[id] role "text" (bounds)`.

## 9. Versioning
- `contracts/adp/VERSION` (semver). Negotiated at `Connection.hello`, right after authentication. Compatible when the major is the same and, for 0.x, the minor is the same too (semver caret rule). Otherwise `UNSUPPORTED_VERSION`.
- Only additions of domains and fields are allowed as backward compatible. Removal or a change of meaning is a major bump.
