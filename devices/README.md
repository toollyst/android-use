# devices/ (L2)

virtio 디바이스 백엔드. 각각 **별도 vhost-user 바이너리**, 각각 샌드박스에서 실행. **Android 지식 금지.**

| 모듈 | 기반 | 비고 |
|---|---|---|
| `gpu/` | rutabaga + gfxstream | Vulkan / MoltenVK(Metal). blob(shm), scanout export, device state |
| `input/` | virtio-input | 멀티터치, 키보드 |
| `snd/` | virtio-snd | 오디오 |
| `net/` | vhost-user-net + usernet(smoltcp) | 플로우 관찰 훅, egress 정책 적용 지점, `EgressUpstream` SPI |

호스트 KeyMint는 virtio 디바이스가 아니고 Android 개념을 다루므로 `runtime/keymint`에 둔다.

공통: device state 저장·복원, fuzz 타깃, conformance 테스트, 트레이스 이벤트
