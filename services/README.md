# services/ (L5)

도메인별 기능. 각 서비스가 자기 도메인을 끝까지 책임진다. 형제 서비스끼리 직접 의존 금지(`contracts/instance` 채널 경유).

| 모듈 | 도메인 |
|---|---|
| `inspect/` | 통합 트리 병합, 트리에 frameId 부착, 노드 ID, diff (ADP `UI`, `Accessibility`, `Target`) |
| `screen/` | 스크린샷 인코딩, 영역·레이어 지정 (ADP `Screen`). 프레임 매칭·보관은 runtime `FrameChannel` |
| `input/` | 좌표·노드 입력 (ADP `Input`) |
| `app/` | 앱 관리, activity, 파일 (ADP `App`) |
| `network/` | 정책 결정, 플로우·요청 관찰 (ADP `Network`) |
| `log/` | 로그 스트림 (ADP `Log`) |
| `overlay/` | 하이라이트 (ADP `Overlay`) |
| `webview/` | CDP 중계 (ADP `WebView`) |
| `stream/` | 스크린캐스트 인코딩 + `StreamTransport` SPI |
| `record/` | 세션 녹화 + `Recorder` SPI |
