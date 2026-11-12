# wick 패치 시리즈

`third_party/wick/src`(submodule) 위에 적용한다. 적용 순서는 `series`.

| 패치 | Purpose | Hook-for | Upstream |
|---|---|---|---|
| (예정) ext 디바이스 등록 hook | 우리 crate의 디바이스·API 등록 | host/vmm/* | wick 예정 |
| (예정) 부트 페이로드 hook | host/vmm/boot가 kernel/initrd/cmdline 공급 | host/vmm/boot | wick 예정 |
| (예정) 범용 vhost-user frontend | device type 파라미터화, device state | host/devices/* | wick / Firecracker 예정 |
| (예정) virtio shm region | gfxstream blob | host/devices/gpu | wick / Firecracker 예정 |
| (예정) VMGenID 플랫폼 지원 | macOS·Windows | host/vmm/vmgenid | wick 예정 |

> 착수 전에 wick `vmm::ext` API를 확인하고, ext로 해결되는 항목은 이 목록에서 뺀다.
