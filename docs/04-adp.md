# 04. ADP: Android DevTools Protocol

> 제품의 핵심. AI(또는 어떤 클라이언트든)가 앱 화면을 **CDP 수준으로 정밀하게** 읽고 조작하게 하는 프로토콜.
> MCP는 ADP 위의 클라이언트(어댑터) 중 하나다. 프로토콜을 LLM 도구 형태에 묶지 않아야 SDK, 디버거, QA 도구, 다른 에이전트 프레임워크(droidrun, mobile-use 등)가 같은 프로토콜을 쓴다.

## 1. 설계 목표
1. **완전성**: 접근성 API가 보여주는 범위를 넘어, 화면에 그려진 것의 구조를 최대한 노출한다.
2. **일관성**: 트리, 스크린샷, 이벤트가 **같은 프레임** 기준임을 보장한다.
3. **안정성**: 노드 ID는 노드 수명 동안 유지된다. 셀렉터와 액션이 흔들리지 않는다.
4. **앱 협조 불필요**: debuggable 여부, 앱 설치, 권한 설정과 무관하게 동작한다 (시스템 내장).
5. **범용 프로토콜**: CDP 스타일(도메인, 커맨드, 이벤트). 언어 무관 SDK.

## 2. 데이터 소스
| 소스 | 얻는 것 | 구현 위치 |
|---|---|---|
| **View 트리** | 클래스, resource-id, text, contentDescription, bounds, visibility, alpha, z-order, enabled/focused/selected/checked, 스크롤 상태 | AOSP 패치: `ViewRootImpl` traversal hook → `guest/vendor/services/inspector` |
| **접근성 트리** | 시맨틱(role, actions, 상태) | `guest/vendor/services/agent` (UiAutomation) |
| **Compose** | 시맨틱 트리, 노드 bounds | inspector가 `AndroidComposeView`의 semantics owner를 읽음. 실패 시 접근성 트리 fallback |
| **WebView** | **실제 CDP (DOM 전체)** | AOSP 패치: `WebViewFactory` hook에서 디버깅 정책 활성화 → ADP가 CDP 타깃으로 중계 (Chromium 포크 없음) |
| **Flutter / React Native** | 시맨틱 트리 | agent가 접근성 서비스로 등록되면 앱이 시맨틱을 노출 (패치 불필요) |
| **게임·커스텀 렌더러** | 레이어 구조와 픽셀 | AOSP 패치: SurfaceFlinger 레이어 정보·레이어별 캡처 |
| **윈도우·태스크** | 포커스 앱, activity 스택, 다이얼로그, IME, 토스트, 알림, 시스템 UI | AOSP 패치: WindowManager/ActivityTaskManager 이벤트 hook |
| **픽셀** | 전체 화면, 영역, 레이어 단위 | 호스트 scanout(`host/devices/gpu` → `host/vmm/scanout`) + SurfaceFlinger 레이어 캡처 |
| **네트워크** | 앱별 플로우, (정책 허용 시) 요청·응답 | `host/devices/net` 관찰 + `host/services/network` 정책. TLS 가시성은 세션별 CA를 시스템 저장소에 주입 (정책으로 on/off) |
| **로그·크래시** | logcat, ANR, tombstone | adpd |

## 3. 핵심 보장
### 3.1 프레임 동기화
- 모든 트리 스냅샷, diff, 스크린샷, 이벤트에 `frameId`를 붙인다.
- `FrameId = { display: u32, vsyncId: i64, sfFrame: u64 }`
  - `vsyncId`: Choreographer vsync ID (앱 쪽)
  - `sfFrame`: SurfaceFlinger 프레임 번호 (합성 쪽)
- 호스트(`host/runtime/instance`의 `FrameChannel`)는 최근 N 프레임의 scanout을 링 버퍼로 유지한다. `Screen.captureScreenshot({frameId})`는 **정확히 그 프레임**을 주거나 `FRAME_EXPIRED` 에러를 낸다. 추측해서 다른 프레임을 주지 않는다.
- 게스트 frameId와 호스트 scanout은 runtime의 `FrameChannel`이 virtio 수준 식별자로 **정확히** 매칭한다. 추정하지 않으며, 매칭되지 않은 프레임은 frameId 지정 요청에 쓰지 않는다. `host/services/inspect`와 `host/services/screen`은 같은 매칭 결과를 쓴다.

### 3.2 안정적인 노드 ID
- `NodeId`는 불투명 문자열. 같은 View/Compose 노드/접근성 노드는 살아 있는 동안 같은 ID를 유지한다.
- 노드가 사라지면 ID는 재사용하지 않는다. (CDP `backendNodeId`에 해당)
- 액션은 노드 ID로 지정할 수 있다. 대상이 사라졌으면 `NODE_GONE`.

### 3.3 좌표계
- 모든 bounds는 **디스플레이 물리 픽셀 기준 화면 좌표**로 변환해서 준다.
- 회전, 밀도, 시스템 인셋, 멀티 윈도우, 디스플레이 스케일을 반영한다.
- 입력 좌표도 같은 좌표계를 쓴다.

### 3.4 통합 트리
- View, 접근성, Compose, WebView DOM, 레이어를 하나의 트리로 병합한다.
- 모든 노드에 `source` 필드: `view | a11y | compose | webview | layer`
- 병합 규칙:
  1. View 트리가 뼈대
  2. 접근성 노드는 원본 View에 시맨틱 속성으로 합쳐진다 (원본 View를 알 수 없으면 가상 노드로 추가)
  3. Compose 시맨틱 노드는 `AndroidComposeView` 아래 자식으로 붙는다
  4. WebView 노드는 해당 View 아래에 `webview` 서브트리(요약)로 붙고, 전체 DOM은 `WebView` 도메인으로 접근한다
  5. 트리에 대응되지 않는 레이어(게임 surface 등)는 `layer` 노드로 붙는다

## 4. 메시지 형식
CDP와 같은 JSON-RPC 스타일. 정의 파일은 `contracts/adp/protocol.json`이다.
- `id`는 정수(2^53 미만)이고 연결 안에서 진행 중인 요청끼리 겹치지 않는다.
- `sessionId`는 연결 수준 호출(`Connection.*`, 세션 밖의 `Target.*`)에서는 없고, 세션으로 가는 모든 메시지에는 반드시 있다.
- 에러는 `{ "id", "error": { "code": "<에러 코드 이름>", "message", "data"? } }`이다. 코드는 아래 에러 코드 목록(`contracts/common` `ErrorCode`와 같은 코드 공간)이다.
- 64비트 정수(`vsyncId`, `sfFrame` 등)는 JSON에서 10진 문자열로, 32비트 이하 정수는 숫자로 보낸다.
- 바이너리 WebSocket 메시지는 앞 8바이트(big-endian 부호 없는 정수)가 그 메시지가 속한 요청의 `id`이고 나머지가 내용이다. 서버는 같은 `id`의 응답보다 먼저 보낸다. 클라이언트는 `App.uploadChunk` 요청마다 그 뒤에 정확히 하나를 보내고, 서버는 그것을 받은 뒤에 응답한다(거부할 때도). 대응하는 요청이 없는 클라이언트 바이너리 메시지는 연결을 상태 코드 1002(RFC 6455 §7.4.1)로 닫는다.
- 인증: WebSocket 업그레이드의 `Authorization: Bearer` 헤더(RFC 6750), 또는 첫 메시지 `Connection.auth`. 인증 전에는 다른 모든 메시지를 `UNAUTHENTICATED`로 거부한다. URL 쿼리 문자열의 토큰은 받지 않는다. 두 경로 모두 `Authenticator` SPI로 검증한다. 인증은 연결당 한 번이다. 인증된 연결의 `Connection.auth`는 `INVALID_REQUEST`이고, 자격 증명을 바꾸려면 새로 연결한다.
- 인증 직후 다른 어떤 커맨드보다 먼저 `Connection.hello`를 한 번 보내야 한다. 그 전의 다른 커맨드와 두 번째 `hello`는 `INVALID_REQUEST`다.
- A request may carry W3C Trace Context `traceparent` and `tracestate`. The server treats them as untrusted, validates the format and ignores an invalid value (Trace Context §3.2, §3.3); the request is not rejected for it. Continuing the client's trace or starting a new trace with a link is a server setting; default: a new trace with a link.
- An upgrade whose `Authorization` header does not authenticate gets HTTP 401 with `WWW-Authenticate: Bearer error="invalid_token"` (RFC 6750 §3.1). A failed `Connection.auth` closes the connection with status 1008 (RFC 6455 §7.4.1). So does a version mismatch at `hello`, after `UNSUPPORTED_VERSION` with `error.data.protocolVersion` (the server's version); there is no retry on the same connection.
- Authorization is checked at attach and again before every command in a session. A denial answers that command with `POLICY_DENIED` and then ends the session with `detachedFromTarget` reason `authorizationRevoked`.
- The target type a session command accepts is its `targetTypes` in `protocol.json`; in the session of another target type it gets `INVALID_REQUEST`. Tree, selector, node action, touch, key, text and screenshot commands belong to a display target's session; `App.*` (including uploads) and `Screen.getDisplays` to an instance target's session.
- A webview target's session carries CDP messages unchanged (CDP flat-session mode). How the ADP envelope rules apply inside it is not defined yet.
- Screen-changing input commands (node actions, touch, key and text input) choose when they resolve with `resolveOn` (`dispatched` | `frameCommitted` | `idle`, default `frameCommitted`) and return the latest committed `frameId` at that point (none for `dispatched`). The wait for `frameCommitted` and `idle` is bounded by `timeoutMs`; the command succeeds either way, and `conditionMet` in the result is `true` when the awaited condition was reached and `false` when `timeoutMs` elapsed first. The idle criterion and the default `timeoutMs` are server settings.
- Node clicks (`click`, `longClick`) take `method`: `accessibility` (`AccessibilityNodeInfo` `ACTION_CLICK` / `ACTION_LONG_CLICK` only), `touch` (a synthesized touch at the node's centre) or `auto` (accessibility when the node exposes the action, touch otherwise); default `auto`. The result reports the method used.
- Touch input follows the `android.view.MotionEvent` model (`ACTION_DOWN`, `ACTION_POINTER_DOWN`, `ACTION_MOVE`, `ACTION_POINTER_UP`, `ACTION_UP`, `ACTION_CANCEL`); `pointers` lists every pointer that is down.
- `Node.role` is a WAI-ARIA 1.2 role name assigned by the table `contracts/adp/roles/role-map.json`; a node without an entry has no role. `Node.actions` lists the node's `AccessibilityNodeInfo` actions: standard actions by constant name, custom actions by id and label.
- `UI.treeMutated`의 `ops`는 노드 ID 기반 연산(`insert {parentId, previousSiblingId?, node}`, `update {nodeId, changes}`, `remove {nodeId}`, `move {nodeId, parentId, previousSiblingId?}`)이다. 배열 순서대로 적용하고 부모가 자식보다 먼저 온다. `update`는 바뀐 필드만 싣고 `null`은 선택 필드를 지운다. 클라이언트의 현재 프레임이 `baseFrameId`가 아니면 `getTree`로 다시 받는다. 다른 기준 프레임에 적용하지 않는다. diff 형식은 `DiffFormat`(현재 `nodeOps` 하나)으로 확장한다.
- 스크린샷은 `transfer`(`binary` | `base64`, 기본 `binary`)로 전달 방식을 고른다. `frameId`가 없으면 그 디스플레이의 마지막 커밋 프레임이고, 캡처한 `frameId`는 항상 돌려준다.
- Server settings and their defaults:
  - Idle criterion: no accessibility event for 500 ms (`UiAutomation.waitForIdle` with the quiet time androidx.test.uiautomator `QueryController` uses). Default `timeoutMs`: 10 000 ms (uiautomator `Configurator` wait-for-idle timeout).
  - Long-click margin over the guest's long-press timeout (`method` `touch`): default 100 ms.
  - Maximum upload size: default 4 GiB − 1 byte. `maxChunkSize`: default 1 MiB. Upload idle expiry: default 60 s. Unfinished uploads per session: default 4.
  - Maximum WebSocket message size: default 16 MiB (larger: close with status 1009, RFC 6455 §7.4.1). It is at least `maxChunkSize` + 8 bytes.
  - Keepalive: a Ping (RFC 6455 §5.5.2) every 30 s by default; the connection is closed after 2 unanswered Pings by default.
  - Authentication deadline for the first-message path: default 10 s (then close with status 1008).
  - Failed-authentication rate limit: default 10 failures per 60 s per peer address (upgrade: HTTP 429, RFC 6585 §4, with `Retry-After`, RFC 9110 §10.2.3; `Connection.auth`: close with status 1008).
- APK 업로드: `App.createUpload(size)`로 크기를 먼저 밝힌다(서버 최대 크기는 설정, 넘으면 바이트를 받기 전에 `INVALID_PARAMS`). 결과의 `maxChunkSize`(서버 설정) 이하 조각을 `App.uploadChunk(uploadId)` 요청 + 그 요청 `id`의 클라이언트 바이너리 메시지로 보내고 조각마다 응답을 받는다. `App.finishUpload(uploadId, sha256)`에서 SHA-256이 맞지 않으면 `INVALID_PARAMS`다. `install`은 `{kind: "upload", uploadId}`로 끝난 업로드를 가리키고 그것을 소비한다. A split-APK install names all its uploads with `{kind: "uploads", uploadIds}`. An install failure is `NOT_ACTIONABLE` with `data {status, legacyStatus}` from `PackageInstaller`. 업로드는 만든 연결·세션에 속하고 1회용이며, 연결이 끊기거나 세션이 끝나거나 유휴 만료(서버 설정)가 지나면 버린다.
```jsonc
// 요청
{ "id": 7, "sessionId": "s_abc", "method": "UI.getTree", "params": { "sources": ["view","compose","a11y"], "depth": -1 } }
// 응답
{ "id": 7, "result": { "frameId": { "display": 0, "vsyncId": "18233", "sfFrame": "9921" }, "root": { /* Node */ } } }
// 이벤트
{ "method": "UI.treeMutated", "sessionId": "s_abc", "params": { "baseFrameId": {...}, "frameId": {...}, "ops": [ /* insert/remove/update */ ] } }
```

### 핵심 타입
```ts
type Node = {
  id: NodeId; source: "view"|"a11y"|"compose"|"webview"|"layer";
  windowId: string; package?: string;
  className?: string; resourceId?: string; role?: string;
  text?: string; contentDescription?: string; hint?: string;
  bounds: Rect;                       // 화면 픽셀
  visible: boolean; alpha?: number; z?: number;
  enabled?: boolean; focused?: boolean; selected?: boolean; checked?: boolean;
  clickable?: boolean; longClickable?: boolean; scrollable?: boolean; editable?: boolean;
  actions?: string[];                 // 수행 가능한 액션
  webviewTargetId?: string;           // WebView면 CDP 타깃 참조
  children?: Node[];
};
type Rect = { x: number; y: number; w: number; h: number };
```

## 5. 도메인
| 도메인 | 커맨드 | 이벤트 |
|---|---|---|
| `Connection` | `auth(token)`, `hello(protocolVersion)` | - |
| `Target` | `getTargets` (인스턴스, 디스플레이, 윈도우, 프로세스, WebView. 주체가 attach할 수 있는 타깃만), `attachToTarget(targetId)` → `sessionId`, `detachFromTarget(sessionId)` | `targetCreated/Destroyed`, `detachedFromTarget(sessionId, reason)` |
| `UI` | `getTree(sources, depth, windowId?)`, `querySelector(selector, frameId?)` / `querySelectorAll(selector, frameId?)` → `{frameId, nodeIds}`, `getNode(nodeId)` → `{frameId, node}`, `describe(id)` | `treeMutated`, `windowChanged`, `focusChanged`, `imeChanged` |
| `Accessibility` | `getFullAXTree`, `performAction(nodeId, action, arguments?)` (`AccessibilityNodeInfo` actions and `ACTION_ARGUMENT_*` arguments, `resolveOn`) | - |
| `Input` | Touch: `dispatchTouch(type, pointers, actionPointerId?)` (MotionEvent actions) / key: `dispatchKey(type, keyCode)` (`KEYCODE_*` names) / text: `insertText(text)` / node: `click(nodeId, method?)`, `longClick(nodeId, method?)`, `setText`, `scrollTo`, `scrollBy`, `swipe` (all with `resolveOn`) | - |
| `Screen` | `captureScreenshot(region?, layer?, frameId?, format, quality?, transfer?)`, `startScreencast`, `stopScreencast`, `getDisplays` | `frameCommitted`, `screencastFrame` |
| `App` | `createUpload(size)`, `uploadChunk(uploadId)`, `finishUpload(uploadId, sha256)`, `install(source)`, `uninstall`, `launch(packageName, activity?, intent?)`, `stop`, `clearData`, `list` → `PackageInfo[]`, `getActivityStack`, `grantPermission`, `pushFile`, `pullFile` | `activityLifecycle`, `anr`, `crash`, `notificationPosted` |
| `Network` | `enable`, `getResponseBody`, `setPolicy` | `requestWillBeSent`, `responseReceived`, `flowOpened/Closed` |
| `Log` | `enable(filters)` | `entryAdded` |
| `Overlay` | `highlightNode`, `hideHighlight` | - |
| `WebView` | `listTargets`, `attach(targetId)` → 내부 CDP 세션 프록시 | CDP 이벤트 그대로 중계 |
| `Session` | `snapshot(name)`, `reset`, `getInfo` | `restored` |

### 셀렉터
```
resource-id:com.app:id/login_button
text:"로그인"          text~:"로그"        (부분 일치)
role:button[enabled]
class:android.widget.EditText >> nth(0)
xpath://*[@resourceId="..."]
```
- 문법과 의미는 `contracts/adp/selectors/selector.ebnf`, 구현이 통과해야 하는 사례는 `contracts/adp/selectors/conformance.json`이다.
- `엔진:본문`을 `>>`로 잇고 `nth(N)`으로 고른다. 이어 붙인 셀렉터는 앞 결과를 기준으로 찾는다(Playwright 셀렉터 체이닝). 엔진은 열린 집합이고 모르는 엔진은 `INVALID_PARAMS`다.
- 속성은 ADP `Node` 필드 그대로다. `text~`는 대소문자를 구분하는 부분 일치다. `xpath`는 XPath 1.0이고 통합 트리의 XML 표현(요소 이름 `node`, 속성 = `Node` 필드) 위에서 평가한다.

### 에러 코드
| 코드 | 언제 |
|---|---|
| `FRAME_EXPIRED` | 요청한 frameId의 프레임이 링 버퍼에 없다 |
| `NODE_GONE` | 대상 노드가 사라졌다 |
| `TARGET_GONE` | 대상 타깃이 사라졌다 |
| `NOT_ACTIONABLE` | 대상에 액션을 수행할 수 없다 |
| `TIMEOUT` | 시간 초과 |
| `POLICY_DENIED` | Denied by policy, including an Authorizer denial at attach or at a command |
| `UNAUTHENTICATED` | 자격 증명이 없거나 유효하지 않다, 또는 인증 전 메시지 |
| `UNSUPPORTED_VERSION` | 협상할 수 있는 프로토콜 버전이 없다 |
| `INVALID_REQUEST` | 봉투가 잘못됐다: 형식 오류, 진행 중인 요청과 겹치는 `id`, `sessionId` 범위 위반, 연결에 없는 `sessionId`, 받지 않는 타깃 종류의 세션, 연결 상태가 허용하지 않는 메시지(`hello` 전 커맨드, 두 번째 `hello`, 인증된 연결의 `auth`, 한 업로드에 동시 둘째 `uploadChunk`, a `createUpload` beyond the per-session upload limit) |
| `METHOD_NOT_FOUND` | 알 수 없는 메서드 |
| `INVALID_PARAMS` | 인자가 정의를 따르지 않는다: 필수 값 누락, 타입·열거값 불일치, 알 수 없는 union `kind`, 셀렉터 문법 오류·알 수 없는 엔진, 알 수 없는 diff 형식 |

## 6. 전송 경로
```
앱 프로세스(inspector hook) ─┐
system_server hook ─────────┼─ binder ─ adpd (SELinux 전용 도메인)
WebView devtools 소켓 ──────┘            │ guest_link (vsock, 버전 협상, 세션 토큰)
                                         ▼
          host/services/inspect · screen · input · app · network · log · webview
                                         │ (contracts/instance)
          host/entrypoints/adp_server ── WebSocket JSON-RPC (인증) ── clients/(MCP · SDK) · 디버거 UI
          host/vmm/scanout ── 호스트 메모리에서 바로 픽셀 (게스트 왕복 없음)
```

## 7. 성능 목표 (초기값, 측정하며 조정)
| 지표 | 정의 |
|---|---|
| 트리 갱신 지연 | 게스트 프레임 커밋 → `UI.treeMutated` 수신 |
| 스크린샷 지연 | 요청 → PNG/WebP 응답 (scanout 경로) |
| 입력→화면 반영 | `Input.*` 요청 → 결과 프레임 `frameCommitted` |
| 전체 트리 크기 | 일반 앱 화면 기준 JSON 크기 (압축 전후) |

## 8. MCP 어댑터 (clients/mcp)
ADP를 LLM이 쓰기 좋게 요약한 형태로 노출한다.

| 도구 | 동작 |
|---|---|
| `observe` | 프레임 동기화된 스크린샷 + 압축 통합 트리(상호작용 가능한 노드 중심, 노드 ID 포함) |
| `act` | `{nodeId | x,y, action, args}` → 결과 + 새 frameId |
| `wait_for` | 조건(셀렉터 등장/소멸, 텍스트, activity, 네트워크 idle) → 트리 diff 이벤트 기반 |
| `screenshot` | 영역·레이어 지정 캡처 |
| `app_*` | install / launch / stop / list / clear |
| `session_*` | reset / snapshot |
| `network_*`, `log_*` | 조회와 필터 |

압축 규칙: 보이지 않는 노드 제거, 의미 없는 컨테이너 접기, 텍스트 정규화, 노드마다 `[id] role "text" (bounds)` 한 줄 표현.

## 9. 버전 관리
- `contracts/adp/VERSION` (semver). 인증 다음 `Connection.hello`에서 협상한다. major가 같고 0.x면 minor도 같아야 호환이다(semver caret 규칙). 아니면 `UNSUPPORTED_VERSION`.
- 도메인과 필드는 추가만 하위호환으로 허용한다. 제거나 의미 변경은 major 증가.
