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
CDP와 같은 JSON-RPC 스타일.
```jsonc
// 요청
{ "id": 7, "sessionId": "s_abc", "method": "UI.getTree", "params": { "sources": ["view","compose","a11y"], "depth": -1 } }
// 응답
{ "id": 7, "result": { "frameId": { "display": 0, "vsyncId": 18233, "sfFrame": 9921 }, "root": { /* Node */ } } }
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
| `Target` | `getTargets` (디스플레이, 윈도우, 프로세스, WebView), `attachToTarget` | `targetCreated/Destroyed` |
| `UI` | `getTree(sources, depth, windowId?)`, `querySelector(selector)`, `querySelectorAll`, `getNode(id)`, `describe(id)` | `treeMutated`, `windowChanged`, `focusChanged`, `imeChanged` |
| `Accessibility` | `getFullAXTree`, `performAction(nodeId, action, args)` | - |
| `Input` | 좌표: `dispatchTouch`, `dispatchKey`, `insertText` / 노드: `click`, `longClick`, `setText`, `scrollTo`, `scrollBy`, `swipe` | - |
| `Screen` | `captureScreenshot(region?, layer?, frameId?, format)`, `startScreencast`, `stopScreencast`, `getDisplays` | `frameCommitted`, `screencastFrame` |
| `App` | `install`, `uninstall`, `launch(pkg, activity?, intent?)`, `stop`, `clearData`, `list`, `getActivityStack`, `grantPermission`, `pushFile`, `pullFile` | `activityLifecycle`, `anr`, `crash`, `notificationPosted` |
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

### 에러 코드
`FRAME_EXPIRED`, `NODE_GONE`, `TARGET_GONE`, `NOT_ACTIONABLE`, `TIMEOUT`, `POLICY_DENIED`, `UNAUTHENTICATED`, `UNSUPPORTED_VERSION`

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
- `contracts/adp/VERSION` (semver). `Hello` 단계에서 협상한다.
- 도메인과 필드는 추가만 하위호환으로 허용한다. 제거나 의미 변경은 major 증가.
