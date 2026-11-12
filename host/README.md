# host/

호스트(Linux · macOS · Windows)에서 도는 코드. 위에서 아래로 레이어 순서이고, 의존은 위 → 아래로만 한다.

| 디렉터리 | 레이어 | 역할 |
|---|---|---|
| `entrypoints/` | L6 | ADP 서버, node 데몬, CLI, desktop: 실행 진입점 |
| `services/` | L5 | 도메인 기능 (트리, 화면, 입력, 앱, 네트워크 …) |
| `runtime/` | L4 | VM·디바이스·게스트 이미지를 인스턴스로 조립, 수명주기·스냅샷 |
| `vmm/` | L2 | microVM (wick 확장). Android를 모른다 |
| `devices/` | L2 | virtio 디바이스 백엔드 (각각 별도 프로세스). Android를 모른다 |
| `base/` | L1 | 도메인 지식 없는 공통 유틸 |

게스트(L3)는 `guest/`, 경계 계약은 `contracts/`, ADP 클라이언트는 `clients/`.
