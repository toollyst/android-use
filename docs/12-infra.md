# 12. 빌드·테스트 인프라

## 1. 장비
| 장비 | 역할 |
|---|---|
| **GCE 빌드 서버** (n2-standard-96 **Spot**, 서울 + Local SSD) | AOSP·커널·호스트 빌드, CTS shard. `out/`은 Local SSD, 소스 캐시는 persistent disk, ccache 사용. 빌드할 때만 켠다 |
| **GCE C4A bare metal** (예: `c4a-highmem-96-metal`, Axion arm64) | (도쿄) Linux arm64(KVM 네이티브) **게이트 검증**과 밀도·성능 지표 측정. **게이트 검증할 때만 켠다** |
| **GCE x86** (Intel VM nested 또는 bare metal) | x86_64 게스트 부팅 테스트, 성능 지표는 bare metal |
| **로컬 Windows PC → Linux 듀얼부팅** (별도 SSD 권장) | 네이티브 GPU 드라이버 + bare metal KVM으로 **gfxstream 호스트 GPU 경로(Linux) 검증**. GCE에서 GPU+KVM 조합이 어려운 부분을 메움 |
| 같은 PC의 **Windows** | wick Windows(WHP) 첫 부팅 이후 네이티브 Windows 검증 장비 |
| **Mac (Apple silicon, macOS 15+)** | **평소 arm64 게스트 부팅·테스트 반복(HVF)**, Metal(MoltenVK), seatbelt, Secure Enclave, 게이트·v1 릴리스 검증. GCE에 Mac 인스턴스는 없음 |

## 2. 주의
- **WSL2는 보조용**: KVM은 nested로 되지만, GPU가 DirectX 반가상화 + Vulkan-on-D3D12 변환 레이어를 거치므로 gfxstream 검증에 부적합하다. 상용 타깃 어디에도 해당하지 않는다.
- **GCE의 GPU + KVM 조합**은 머신 시리즈별로 확인이 필요하다. 안 되면 GPU 경로는 로컬 Linux PC와 Mac에서 검증한다.
- **비용**: 빌드 서버는 Spot으로 빌드할 때만, C4A bare metal은 게이트 검증할 때만 켠다. CI가 필요할 때 띄우고 끝나면 내린다. CTS shard는 Spot VM. 비용 기준은 `docs/13-roadmap.md` §4.

## 3. M0에 필요한 인프라
1. 빌드 서버 생성 → AOSP 릴리스 태그 `repo sync` 시작 (수 시간 소요, 가장 먼저)
2. C4A bare metal에 wick Linux 빌드 + 부팅 테스트 하네스 (`tools/infra/boot-test.sh`)
3. 이미지 캐시 버킷과 CI 러너 등록
4. (M5용) 로컬 PC Linux 듀얼부팅 + GPU 드라이버 + KVM 확인

## 4. tools/infra
```
tools/infra/
├─ gce/            인스턴스 생성·삭제 스크립트 (빌드 서버, C4A, x86)
├─ boot-test.sh    이미지 → wick 부팅 → boot_completed → guest_link Hello 확인
├─ cts.sh          CTS shard 실행·결과 수집
└─ runners/        self-hosted CI 러너 설정
```
