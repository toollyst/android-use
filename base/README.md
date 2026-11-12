# base/ (L1)

도메인 지식이 없는 공통 기반.

| 모듈 | 책임 |
|---|---|
| `ipc/` | vsock, unix socket, shm 전송 추상화, 메시지 프레이밍 |
| `trace/` | Perfetto SDK 래퍼, trace ID 생성·전파, `TelemetryExporter` 기본 구현 |
| `config/` | 설정 로딩, 검증 |
| `error/` | 공통 에러 타입, 에러 코드 |
| `platform/` | OS 추상화 (Linux / macOS / Windows) |

금지: Android, 도메인, 테넌트 개념
