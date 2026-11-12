# runtime/ (L4): 묶는 계층

vmm + devices + 게스트 이미지를 `Instance`로 조립하고 수명주기를 관리한다. 도메인 기능은 하지 않는다. 상세: `docs/07-runtime.md`

| 모듈 | 책임 |
|---|---|
| `instance/` | VmSpec 생성(bootconfig `androidboot.*` 포함), 디바이스 프로세스 기동·연결, 상태 머신, `FrameChannel`(프레임 매칭·보관) |
| `session/` | 세션 풀, acquire/reset/release |
| `snapshot/` | 골든 스냅샷, restore, uffd 핸들러(MemorySource), 메타데이터 |
| `identity/` | VMGenID, DeviceProfile, Restore.Notify 검증 |
| `keymint/` | 호스트 KeyMint TA (별도 jailed 프로세스, `contracts/keymint`) |
| `jail/` | Linux jailer / macOS seatbelt / Windows AppContainer |
| `resources/` | 리소스 회계, 한도, 이벤트 |
| `stores/` | SPI 기본 구현 (로컬 Snapshot/Image/Profile Store, 파일 MemorySource) |

구현 계약: `contracts/instance`. `contracts/keymint`(호스트 쪽). 사용 계약: `contracts/machine`, `contracts/device`, `contracts/guest_link`, `contracts/spi`
