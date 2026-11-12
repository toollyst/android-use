# contracts/

레이어 경계의 유일한 인터페이스. 상세: `docs/03-contracts.md`

| 디렉터리 | 경계 | 형식 |
|---|---|---|
| `machine/` | runtime ↔ vmm | Rust trait |
| `device/` | vmm ↔ devices | vhost-user + 확장 |
| `guest_link/` | runtime·services ↔ guest | IDL → codegen |
| `keymint/` | guest KeyMint HAL ↔ host/runtime/keymint | AOSP kmr wire + 프레이밍·인증 |
| `instance/` | services ↔ runtime | Rust trait |
| `adp/` | host/entrypoints/adp_server ↔ clients·외부 클라이언트 | JSON-RPC 스키마 (`docs/04-adp.md`) |
| `node/` | control plane ↔ node | IDL(gRPC) |
| `spi/` | 코어 ↔ 확장 구현 | Rust trait (`docs/09-oss-and-saas.md`) |
| `codegen/` | 생성기 설정 | - |
| `compat/` | 이전 버전 픽스처, 하위호환 테스트 | - |

각 계약 디렉터리에는 `VERSION`(semver)이 있어야 한다. 변경 절차: `CLAUDE.md` §4
