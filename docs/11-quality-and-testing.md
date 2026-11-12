# 11. Quality and testing

## 1. Quality metrics
| Metric | Measurement |
|---|---|
| Compatibility | CTS/VTS pass rate per module. Zero regressions. Measured **on the release image as is** |
| Cold boot | VM creation → `sys.boot_completed` → guest_link Ready |
| restore | Request → session Ready (including `Restore.Notify`) |
| Input→screen | `Input.*` → `frameCommitted` of the resulting frame |
| Tree update | Guest frame commit → `UI.treeMutated` received |
| Screenshot | Request → response |
| Density | Concurrent sessions per host, memory per session |
| Uniqueness | Number of duplicate identifiers and keys across cloned sessions (always 0) |

Every metric is measured in `testing/perf/`, and its trend is stored.

## 2. Test tiers
| Tier | Location | Cadence |
|---|---|---|
| Unit | Each crate | Every PR |
| Layer rules · patch lint · codegen check | `build/`, `tools/patch/` | Every PR |
| Contract backward compatibility | `contracts/compat/` | Every PR |
| virtio conformance | `testing/conformance/` | Every PR (when the relevant paths change) |
| fuzz | `testing/fuzz/` | Always on, nightly |
| Boot matrix | `tools/infra/boot-test.sh` | On guest or vmm changes + nightly |
| **ADP golden** | `testing/golden/` + `testing/testapps/` | Every PR (on guest or services changes) |
| E2E (MCP and ADP level) | `testing/e2e/` | Every PR |
| CTS/VTS (release image) | `testing/cts/` | Nightly subset, full suite for releases |
| Security regression | `testing/e2e/security/` | Nightly |

## 3. ADP golden tests (the densest, since this is the core of the product)
- `testing/testapps/`: `views`, `compose`, `webview`, `flutter`, `rn`, `gl` (custom renderer)
- Expected values are stored per screen:
  - Unified tree (normalized JSON)
  - Bounds of key nodes (with tolerance)
  - frameId agreement: are the tree and the screenshot the same frame
  - Node ID stability: does the same node keep the same ID after scrolling and animation
  - Action results: node click → expected screen transition

## 4. Boot matrix
| Host | Guest | Machine |
|---|---|---|
| Linux x86_64 | wick_x86_64 | GCE (nested or bare metal), local PC (Linux) |
| Linux arm64 | wick_arm64 | GCE C4A bare metal |
| macOS arm64 | wick_arm64 | Physical Mac |
| Windows x86_64 | wick_x86_64 | Local PC (Windows), once wick boots on Windows |

## 5. CI setup
- **Free GitHub Actions**: Rust build and tests, lint, patch lint, layer check, codegen check, short fuzz runs
- **self-hosted**: AOSP build, KVM boot tests, macOS boot tests (physical Mac), GPU path tests, CTS
- The AOSP build runs only on changes to `guest/`, `third_party/aosp/` or `versions.toml`. The resulting images are cached and reused by the host-side CI

## 6. Measurement image
Every test runs on the release image. Only CTS/VTS temporarily turn adbd on, by host authorization.

## 7. E2E-first principle
Before implementing a feature, add the expected behaviour to `testing/e2e/` first. From M0 on, with the full MCP → VMM path connected, the red tests become the list of remaining work.
