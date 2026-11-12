# 00. Vision

## In one line
**Playwright/CDP for Android.** A lightweight, isolated, production-grade runtime that lets AI read Android apps structurally, as it reads the web, and operate them precisely.

## Why it is needed
- An agent's action space is in practice confined to the web. Yet many real services (finance, delivery, internal business tools and so on) work **only in an app**.
- Existing approaches are one of two kinds.
  - **Heavy execution environments** (emulators, Cuttlefish-style cloud phones): low density, and a poor fit for local desktop use.
  - **App-level agent tools** (an accessibility service app + ADB): they read only as far as the accessibility API shows, and they need prior setup.
- Droid Use designs **the execution environment and inspection together, at the OS level**.

## Use cases
| Use case | Key requirements |
|---|---|
| AI agents operating apps | Accurate structure reading, node-based actions, deterministic waiting |
| RL environments / benchmarks for mobile agents | Reproducible initial state (snapshots), high density, fast reset |
| App QA automation | Stable selectors, frame synchronization, recording and traces |
| Enterprise agents (e.g. Hopfia Hub integration) | Isolation, egress policy, audit logs |
| Local private execution | A local microVM whose data never leaves the machine |
| Cloud SaaS | Fleet, remote streams, profile retention (separate repo, extended through the SPI) |

## Differentiators (all three are required for v1; without any one of them it becomes similar to what exists)
1. **microVM density + a clone-safe snapshot pool**
2. **Framework-level unified inspection (ADP)**: beyond the accessibility API, down to View, Compose, WebView CDP and render layers. Guarantees frameId synchronization and stable node IDs
3. **Cross-platform local execution**: Linux · macOS · Windows

## Competitive landscape
| | Cuttlefish | redroid | droidrun (Mobilerun) | mobile-use | **Droid Use** |
|---|---|---|---|---|---|
| Category | Virtual device (VM) | Container | Agent framework | Agent framework | VM + inspection protocol |
| Isolation | VM | Shared kernel | Not applicable | Not applicable | microVM + per-device sandbox |
| Host | Mainly Linux | Linux only | Needs a device | Needs a device | Linux · macOS · Windows |
| Graphics | gfxstream (vhost-user-gpu) | Host/guest rendering | Depends on the device | Depends on the device | gfxstream (vhost-user-gpu) |
| Snapshots | Supported (Android 15+) | Container level | None | None | Supported + clone uniqueness (VMGenID) |
| Screen reading | adb · tools | adb · tools | Accessibility service app | Accessibility tree + screenshots | Unified tree from framework hooks + frameId |
| Prior setup | - | Host binder module | App install + manual permission enabling | ADB + helper APK | None (built into the system) |

**Reading the table**
- Cuttlefish runs vhost-user-gpu + gfxstream as a separate process. This supports our direction (devices out of process).
- droidrun and mobile-use are **potential users** rather than competitors. They need a "device", and Droid Use provides that device in a form that is lighter, isolated and can be read more precisely. Adapters can sit on top of ADP.
- The real competitors are cloud phone services built on Cuttlefish/redroid.

## Non-goals
- Supporting apps that need the Play Integrity strong tier (Google licensing and device certification; outside the technical scope)
- Competing on game performance (graphics put correctness and compatibility first)
- Controlling physical devices (could be considered later through an ADP adapter, but outside the core scope)
