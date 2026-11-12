# host/vmm/ (L2)

wick 위에 얹는 VMM 확장 crate들. **Android 의미를 모른다.** 상세: `docs/05-vmm-and-devices.md`

| 모듈 | 책임 |
|---|---|
| `boot/` | boot image v4 + vendor_boot 컨테이너 파싱, ramdisk 결합, bootconfig 부착(내용은 불투명). x86_64·aarch64(FDT) |
| `disk/` | GPT 컴포지트 디스크(이름 있는 파티션) + CoW 레이어(base / snapshot / profile) |
| `vmgenid/` | 세대 ID 갱신 |
| `scanout/` | gpu scanout export → 호스트로 프레임 전달 (fast path) |
| `api/` | Firecracker REST API 확장 + runtime용 `Machine` REST 클라이언트 |

구현 계약: `contracts/machine`, `contracts/device`
