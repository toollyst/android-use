# 07. 런타임 (L4): 묶는 계층

runtime은 vmm, devices, 게스트 이미지를 **`Instance` 하나로 조립**하고 수명주기를 관리한다. 도메인 기능은 하지 않는다.

## 1. 모듈
| 모듈 | 책임 |
|---|---|
| `host/runtime/instance` | `VmSpec` 생성(게스트 bootconfig `androidboot.*` 포함), 디바이스 백엔드 프로세스 기동·연결, `Instance` 구현, 상태 머신, 프레임 매칭(`FrameChannel`) |
| `host/runtime/session` | 세션 풀, acquire/reset/release, 동시성·용량 관리 |
| `host/runtime/snapshot` | 골든 스냅샷 생성, restore, uffd 페이지 핸들러(`MemorySource` 사용), 스냅샷 메타데이터 |
| `host/runtime/identity` | VMGenID 증가, `DeviceProfile` 적용, 게스트 `Restore.Notify` 대기·검증 |
| `host/runtime/keymint` | 호스트 KeyMint TA (AOSP Rust kmr). 별도 프로세스, jail 안에서 실행. 키는 호스트 보안 저장소(Secure Enclave / TPM)로 래핑. 인스턴스별 비밀 |
| `host/runtime/jail` | 플랫폼별 샌드박스: Linux jailer / macOS seatbelt / Windows AppContainer. VMM, **각 디바이스 프로세스**, keymint마다 별도 적용 |
| `host/runtime/resources` | 인스턴스별 CPU·메모리·디스크·네트워크 사용량 회계, 한도 적용, 이벤트 발행 |
| `host/runtime/stores` | SPI 기본 구현: 로컬 `SnapshotStore`, `ImageStore`, `ProfileStore`, 파일 `MemorySource` |

## 2. 인스턴스 상태 머신
```
Creating → Booting → Ready ⇄ Paused
              │         │
              ▼         ▼
            Failed   Snapshotting → Ready
Restoring → (Restore.Notify 수신) → Ready
any → Stopped
```

## 3. 스냅샷
### 골든 스냅샷 생성
1. 콜드 부팅 → `sys.boot_completed` → agent/adpd Ready → (선택) 프로필 준비 작업
2. pause → VM 메모리 + vCPU 상태 + **모든 디바이스 state**(vhost-user device state 전송) 저장
3. 메타데이터 기록 (아래)

### 메타데이터 (버전 있는 포맷)
```toml
format_version = 1
wick_commit    = "…"
image_hash     = "…"            # 게스트 이미지(boot, vendor_boot, super)
cpu_template   = "…"            # 다른 호스트 복원을 위해 필수
protocol       = { guest_link = "0.1.0" }
devices        = ["gpu@rev", "input@rev", "snd@rev", "net@rev"]
disk_layers    = ["base:<hash>", "snapshot:<hash>"]
created_at     = "…"
labels         = {}
```
호환되지 않는 조합의 restore는 거부한다.

### restore
1. `SnapshotStore`에서 핸들 획득 → `MemorySource` 생성 (로컬 파일 또는 원격)
2. uffd lazy restore: 페이지 폴트 시 `MemorySource.read_at`, 워킹셋 prefetch 힌트
3. 디바이스 state 복원
4. **VMGenID 증가** → 게스트 restore hook 실행 → `Restore.Notify` 수신 확인
5. 복제본별 새 토큰 발급(`Auth.Rebind`) → `Restore.Notify` 수신 → Ready

## 4. 복제 고유성 (필수)
같은 스냅샷에서 여러 인스턴스를 띄우면 RNG 상태, ANDROID_ID, keystore 키, TLS 세션이 복제된다. 이는 보안 사고다.
- 호스트: VMGenID 증가, 복제본별 토큰 재발급, 인스턴스별 KeyMint 비밀
- 게스트(`guest/vendor/services/restore`): 커널 RNG reseed, ANDROID_ID·기타 식별자 재생성, keystore 재초기화, 네트워크 재협상
- 검증: `testing/e2e`에 "복제 세션 간 식별자·키 중복 0" 테스트

## 5. 디스크 레이어링
```
[profile 레이어 (rw, 사용자 데이터 보존)]   ← ProfileStore
[snapshot 레이어 (ro, 골든 상태)]          ← SnapshotStore
[base 레이어 (ro, 이미지)]                 ← ImageStore
```
- 세션 쓰기는 최상단 임시 CoW 레이어로 가고, reset 시 폐기한다.
- 프로필 보존(로그인 상태 등)은 profile 레이어 commit으로 한다.

## 6. 리소스와 이벤트
- 이벤트: 인스턴스 생성·종료, 세션 acquire/release, 스냅샷, restore, 리소스 사용량(주기적)
- `EventSink` SPI로 내보낸다. 코어는 과금을 모른다.
- 모든 인스턴스와 세션은 `labels`(불투명 map)를 가진다.
