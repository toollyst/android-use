# Scope matrix

> Every feature exists here first. Development is filling cells.
> Status: ☐ not started · ◐ in progress · ☑ done · — not applicable
> When a task is done, update its cell (`CLAUDE.md` §3).

## 1. Platform and execution
| Feature | Contract | guest | vmm / devices | runtime | services | entrypoints · clients | Stage |
|---|---|---|---|---|---|---|---|
| Android boot (boot.img/vendor_boot/bootconfig) | machine | ☐ products, fstab, init | ☐ host/vmm/boot | ☐ instance | — | ☐ cli | M1 |
| GPT composite disk + CoW layers | machine | ☐ fstab | ☐ host/vmm/disk | ☐ stores | — | — | M1 |
| AVB · SELinux enforcing · user build | — | ☐ avb, sepolicy | — | — | — | — | M1 |
| Screen output (gfxstream) | device | ☐ hals/graphics | ☐ host/devices/gpu, vhost-user, shm | ☐ instance | — | — | M1 |
| Coordinate input | device | ☐ idc | ☐ host/devices/input | ☐ InputChannel | ☐ input | ☐ adp, mcp | M1 |
| Audio | device | ☐ hals/audio | ☐ host/devices/snd | ☐ instance | — | — | after v1 |
| Network (usernet) | device | — | ☐ host/devices/net | ☐ instance | ☐ network | — | M1 |
| Egress policy | spi | — | ☐ host/devices/net hooks | ☐ resources | ☐ network | ☐ adp | after v1 |
| KeyMint (host) | keymint | ☐ hals/keymint | — | ☐ keymint, jail | — | — | M1 |
| Sandbox (VMM + per device) | — | — | — | ☐ jail | — | — | M1 |
| Host: macOS arm64 · Linux arm64 | machine, device | ☐ products/wick_arm64 | ☐ wick HVF·KVM, host/devices/gpu(MoltenVK·Vulkan) | ☐ jail(seatbelt·jailer), keymint(Secure Enclave·TPM) | — | — | M1 |
| Host: Linux x86_64 · Windows x86_64 | machine, device | ☐ products/wick_x86_64 | ☐ wick KVM·WHP, host/devices/gpu(Vulkan) | ☐ jail(jailer·AppContainer), keymint(TPM) | — | — | M5 |

## 2. Inspection (ADP)
| Feature | Contract | guest | vmm / devices | runtime | services | entrypoints · clients | Stage |
|---|---|---|---|---|---|---|---|
| View tree | guest_link, adp | ☐ patch 0001 + inspector | — | ☐ GuestChannel | ☐ inspect | ☐ adp, mcp | M2 |
| Accessibility tree | guest_link, adp | ☐ agent | — | — | ☐ inspect | ☐ adp | M2 |
| Compose semantics | guest_link, adp | ☐ inspector | — | — | ☐ inspect | ☐ adp | M2 |
| Unified tree merge | adp | — | — | — | ☐ inspect | ☐ adp | M2 |
| frameId synchronization | guest_link, device, instance | ☐ patch native 0001, hals/graphics | ☐ gpu scanout export, host/vmm/scanout | ☐ FrameChannel | ☐ inspect, screen | ☐ adp | M2 |
| Stable node IDs | adp | ☐ inspector | — | — | ☐ inspect | ☐ adp | M2 |
| Tree diff events | guest_link, adp | ☐ inspector, adpd | — | — | ☐ inspect | ☐ adp | M2 |
| Window · task · IME state | guest_link, adp | ☐ patch 0002 | — | — | ☐ inspect, app | ☐ adp | M2 |
| Node-based actions | guest_link, adp | ☐ agent | — | — | ☐ input | ☐ adp, mcp | M2 |
| Screenshots (for a given frameId) | device, adp | — | ☐ host/vmm/scanout | ☐ FrameChannel | ☐ screen | ☐ adp, mcp | M2 |
| WebView CDP relay | guest_link, adp | ☐ patch 0003 | — | — | ☐ webview | ☐ adp | M4 |
| Flutter / RN semantics | adp | ☐ agent (accessibility registration) | — | — | ☐ inspect | ☐ adp | M4 |
| Layer tree (games, etc.) | guest_link, adp | ☐ patch native 0001 | — | — | ☐ inspect | ☐ adp | M4 |
| Screencast | adp, spi | — | ☐ host/vmm/scanout | — | ☐ stream | ☐ adp | after v1 |
| Overlay highlight | adp | ☐ agent | — | — | ☐ overlay | ☐ adp | after v1 |
| Network observation | adp | ☐ (TLS visibility CA, policy) | ☐ host/devices/net | — | ☐ network | ☐ adp | after v1 |
| Logs · ANR · crashes | guest_link, adp | ☐ adpd | — | — | ☐ log, app | ☐ adp | after v1 |
| Session recording | spi | — | — | — | ☐ record | ☐ adp | after v1 |

## 3. Apps and sessions
| Feature | Contract | guest | vmm / devices | runtime | services | entrypoints · clients | Stage |
|---|---|---|---|---|---|---|---|
| App install · launch · stop · list | guest_link, adp | ☐ adpd | — | — | ☐ app | ☐ adp, mcp | M2 |
| File push/pull | guest_link, adp | ☐ adpd | — | — | ☐ app | ☐ adp | after v1 |
| Session acquire/reset/release | instance, adp | — | — | ☐ session | — | ☐ adp, mcp | M3 |
| Snapshot creation · restore | machine, device | — | ☐ device state, vmgenid | ☐ snapshot | — | ☐ adp | M3 |
| Clone uniqueness | guest_link | ☐ services/restore | ☐ host/vmm/vmgenid | ☐ identity | — | — | M3 |
| Device profiles | spi | ☐ services/restore | — | ☐ identity | — | ☐ node | after v1 |
| Profile (user data) retention | spi | — | ☐ host/vmm/disk | ☐ stores | — | ☐ node | after v1 |

## 4. Compatibility
| Feature | Contract | guest | vmm / devices | runtime | services | entrypoints · clients | Stage |
|---|---|---|---|---|---|---|---|
| NativeBridge (berberis) | — | ☐ nativebridge + patch | — | — | — | — | after v1 |
| microG | — | ☐ apps/microg | — | — | — | — | after v1 |
| adb enablement path for CTS | guest_link, node | ☐ adbd, adpd | — | ☐ instance | — | ☐ node, cli | M1 |
| CTS target pass rate | — | ☐ | ☐ | ☐ | — | — | after v1 |

## 5. Operations and extension
| Feature | Contract | guest | vmm / devices | runtime | services | entrypoints · clients | Stage |
|---|---|---|---|---|---|---|---|
| Single trace (Perfetto) | headers of every contract | ☐ adpd propagation | ☐ | ☐ | ☐ | ☐ | M0 |
| Channel authentication | spi | ☐ guest_link · keymint tokens | — | ☐ | — | ☐ adp_server, node | M1 |
| Re-authentication after cloning | guest_link, keymint | ☐ adpd, services/restore | — | ☐ identity | — | — | M3 |
| Resource accounting events | spi | — | — | ☐ resources | — | ☐ node | after v1 |
| Node API | node | — | — | ☐ | — | ☐ node | after v1 |
| Remote MemorySource | spi | — | — | ☐ snapshot | — | ☐ node builder | after v1 |
| Release bundle · signing · SBOM | — | ☐ | ☐ | — | — | ☐ | M6 |

Stage: `M0`–`M6` = v1 milestones, `after v1` = the stage after v1 (`docs/13-roadmap.md`)
