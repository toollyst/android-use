<h1 align="center">Droid Use</h1>

<p align="center">
  <b>Playwright for Android.</b><br>
  An isolated Android runtime and a CDP-style protocol, so that AI agents and tools can read and drive Android apps
  the way they read and drive the web.
</p>

<p align="center">
  <a href="https://github.com/toollyst/android-use/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/toollyst/android-use/actions/workflows/ci.yml/badge.svg"></a>
  <a href="#license"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue"></a>
  <img alt="Status: pre-alpha, milestone M0" src="https://img.shields.io/badge/status-pre--alpha%20(M0)-orange">
</p>

> [!WARNING]
> **Droid Use does not run yet.** This repository is at milestone M0: the layer boundaries exist as versioned
> contracts, and CI enforces the dependency, patch and code-generation rules on four host platforms. There is no
> Android image, no binary and nothing to install. Everything below that describes using Droid Use is the design it is
> being built to, and is marked **planned**. The [roadmap](docs/13-roadmap.md) says what has to pass before each
> part counts as working.

Droid Use runs a self-built Android (AOSP) in a [wick](https://github.com/toollyst/wick) microVM (Firecracker, made
cross-platform) with Rust device backends, and exposes the screen through **ADP, the Android DevTools Protocol**: a
CDP-style protocol of domains, commands and events. ADP sees what Android actually drew, not only what the
accessibility API reports: Views, Compose semantics, WebView DOM (as real CDP) and render layers, merged into one tree
whose nodes keep their IDs and whose every snapshot, diff and screenshot names the exact frame it came from. An
[MCP](https://modelcontextprotocol.io) server is one client of ADP; SDKs, debuggers and other agent frameworks are
others.

It is meant for AI agents that need apps that only exist on a phone, for mobile-agent benchmarks and RL environments
that need a clean, reproducible device per episode, and for app QA that needs selectors and waits that don't flake.

## What we're after

- **Read the screen, not a guess of it.** One tree from Views, accessibility, Compose, WebView and render layers.
  Node IDs stay stable for a node's lifetime. A tree, a diff and a screenshot taken at the same `frameId` describe
  the same frame, or the call fails with `FRAME_EXPIRED`; no other frame is substituted.
- **Nothing to set up inside the app.** Inspection is built into the system image. Apps don't need to be debuggable,
  carry an SDK, or have an accessibility service enabled by hand.
- **A clean Android per task, safely cloned.** Snapshots restore lazily into fresh instances, and every clone gets its
  own identifiers, keys and session token (VMGenID plus a guest restore hook), so two clones never share secrets.
- **The same binary and API on Linux, macOS and Windows.** One microVM stack across KVM, Hypervisor.framework and
  WHP. No QEMU, anywhere.
- **Production security from the first milestone.** A user build with AVB and SELinux enforcing, every virtio device
  in its own sandboxed process, a host-side KeyMint, and authentication on every channel (ADP, the node API, host to
  guest). These are not features added later.
- **An open protocol, not a bundled agent.** ADP is versioned and documented; MCP is one adapter on top. The core is
  Apache-2.0 and complete on its own for a single self-hosted machine.

## Droid Use next to the usual setup

|  | A virtual device or container, plus an accessibility-service agent tool | Droid Use (design target) |
| --- | --- | --- |
| Isolation | A full virtual device, or a container sharing the host kernel | A microVM per instance, each device backend in its own sandboxed process |
| What the agent reads | The accessibility tree and screenshots | Views, accessibility, Compose, WebView (CDP) and render layers in one tree, tied to a `frameId` |
| Setup inside the device | A helper app with its permissions enabled, or ADB | None: inspection ships in the system image |
| Knowing when an action took effect | Left to the tool | Each input says when it resolves: once dispatched, at the next committed frame, or when the UI is idle |
| Fresh state per task | Snapshots where the device supports them | Snapshot restore with a new identity, keys and token for every clone |

## Status

What exists and is checked on every push:

| Piece | State |
| --- | --- |
| Layer contracts: `machine`, `device`, `guest_link`, `keymint`, `instance`, `adp`, `node`, `spi`, plus `common` | Defined and versioned (`0.1.0` each) under [`contracts/`](contracts/). ADP is a CDP-style [`protocol.json`](contracts/adp/protocol.json) with a [selector grammar](contracts/adp/selectors/selector.ebnf) and conformance cases; host-to-guest is gRPC ([`guest_link.proto`](contracts/guest_link/proto/droiduse/guest_link/guest_link.proto)) |
| Layer rules | `cargo xtask check-layers` enforces [`LAYERS.toml`](LAYERS.toml): dependencies only go down, and lower layers may not name upper-layer concepts |
| Upstream patch rules | `tools/patch/lint.sh` checks patch headers, markers and line budgets |
| Generated code | `cargo xtask codegen --check` fails when committed generated code is stale |
| CI | Linux x86_64 and arm64, macOS arm64, Windows x86_64 ([`ci.yml`](.github/workflows/ci.yml)) |

Not written yet: the VMM extensions, device backends, the Android image, the runtime, the ADP server, the MCP server,
the CLI. They arrive in milestone order, and a milestone is done when its gate passes, not on a date:

| Milestone | Gate, in short |
| --- | --- |
| M0 Skeleton | Contracts, CI checks that fail on real violations, a client-to-VMM call path with one trace ID, every v1 end-to-end scenario present (and red) |
| M1 Boot | The same arm64 image boots on macOS (HVF) and Linux (KVM) as a user build with AVB and SELinux enforcing; display, input, network and KeyMint work |
| M2 ADP core | The unified tree with exact `frameId` sync, stable node IDs, tree diffs, node actions, touch input, screenshots by frame, app management, and the MCP `observe`, `act`, `wait_for` and `app_*` tools |
| M3 Snapshots and sessions | Golden snapshots, lazy restore, session acquire, reset and release, zero shared identifiers or keys between clones |
| M4 Full inspection | WebView CDP relay, render-layer trees, Flutter and React Native semantics |
| M5 All platforms | Linux x86_64 and Windows x86_64 pass the same tests |
| M6 v1 | Signed bundles with SBOM and provenance, reproducible builds, a security review |

The full gates and the open decisions are in [docs/13-roadmap.md](docs/13-roadmap.md).

## How it is meant to be used (planned)

ADP is JSON-RPC over an authenticated WebSocket, in CDP's flat-session style. Every instance is a target; displays,
windows, processes and WebViews sit beneath it. The flow the contract defines today:

```jsonc
// Authenticate with "Authorization: Bearer <token>" on the WebSocket upgrade, then negotiate the version.
{ "id": 1, "method": "Connection.hello", "params": { "protocolVersion": "0.1.0" } }
// List the instances this principal may attach to, attach to one, then to one of its displays.
{ "id": 2, "method": "Target.getTargets" }
{ "id": 3, "method": "Target.attachToTarget", "params": { "targetId": "<instance>" } }
{ "id": 4, "sessionId": "<instance session>", "method": "Target.getTargets" }
{ "id": 5, "sessionId": "<instance session>", "method": "Target.attachToTarget", "params": { "targetId": "<display>" } }
// Read the tree, and a screenshot of exactly the frame the tree came from.
{ "id": 6, "sessionId": "<display session>", "method": "UI.getTree", "params": { "sources": ["view", "compose", "a11y"], "depth": -1 } }
{ "id": 6, "result": { "frameId": { "display": 0, "vsyncId": "18233", "sfFrame": "9921" }, "root": { /* Node */ } } }
{ "id": 7, "sessionId": "<display session>", "method": "Screen.captureScreenshot",
  "params": { "frameId": { "display": 0, "vsyncId": "18233", "sfFrame": "9921" }, "format": "png" } }
```

One connection can attach to several instances at once, each through its own session. The protocol's domains,
envelope, error codes and selector syntax are described in [docs/04-adp.md](docs/04-adp.md) and defined in
[`contracts/adp/`](contracts/adp/).

For AI agents, the MCP server will offer tools shaped for language models rather than one tool per ADP command:

| Tool (planned) | What it does |
| --- | --- |
| `observe` | A frame-synchronized screenshot plus a compressed tree of the interactive nodes, with their node IDs |
| `act` | An action on a node ID or a coordinate, returning the result and the new `frameId` |
| `wait_for` | Wait for a selector to appear or disappear, a text, an activity, or network idle, driven by tree-diff events |
| `screenshot` | A capture of a region or layer |
| `app_*`, `session_*` | Install, launch, stop, list and clear apps; reset and snapshot sessions |

The command-line interface, the MCP server's command and configuration, and how a machine is told to start several
instances are **not defined yet**. The architecture reserves a CLI and a node daemon that runs many instances on one
machine, but no command, flag or file format has been specified, so this README doesn't show any.

## Platforms (planned)

| Host | Hypervisor | Milestone |
| --- | --- | --- |
| macOS 15+, Apple silicon | Hypervisor.framework | M1 |
| Linux arm64 | KVM | M1 |
| Linux x86_64 | KVM | M5 |
| Windows x86_64 | WHP | M5 |

The guest image matches the host architecture (arm64 or x86_64). Running arm64-only apps on x86_64 hosts needs a
native bridge; it is currently planned for after v1, and that is still an open decision in the roadmap.

## How it fits together

```text
 clients/            MCP server · SDKs · debuggers              (use contracts/adp only)
     │  ADP: WebSocket JSON-RPC, authenticated
 host/
   entrypoints/      ADP server · node daemon · CLI · desktop
   services/         tree · screen · input · apps · network · logs · overlay · WebView · streaming · recording
   runtime/          assembles VM, devices and guest image into an Instance; sessions, snapshots, identity,
                     frame matching, host KeyMint, sandboxes
   vmm/ devices/     wick microVM extensions · virtio device backends, one sandboxed process each (no Android knowledge)
   base/             transports, tracing, configuration, errors
     │  guest_link: gRPC over vsock, two channels, every call authenticated
 guest/              AOSP overlay: framework hooks, adpd, agent, restore hook, HALs, SELinux policy
 third_party/        pinned upstreams (wick, AOSP, kernel, gfxstream, rutabaga) and our hook-only patches
 contracts/          the only way across a layer boundary; depends on nothing
```

Dependencies only go downward, siblings talk through `contracts/`, and the VMM and device layers know nothing about
Android. Upstream code is never edited in place: a patch adds a hook, and the logic lives in our own layers. One
Perfetto trace follows a request from the MCP client through the host, into the guest and back. Details:
[docs/02-architecture.md](docs/02-architecture.md).

## Repository layout

```text
contracts/       versioned layer contracts (Rust traits, gRPC IDL, the ADP protocol definition)
host/            host code by layer: entrypoints, services, runtime, vmm, devices, base (layer rules only, so far)
guest/           the Android side: AOSP overlay, HALs, guest services (layer rules only, so far)
clients/         ADP clients: MCP, SDKs (layer rules only, so far)
third_party/     upstream pins and patch policy
build/           repository tooling behind cargo xtask: layer checker, patch lint, code generator
tools/           scripts (patch lint)
testing/         where test apps, golden trees, end-to-end, conformance, fuzz and CTS setups go
docs/            vision, principles, architecture, contracts, protocol, security, quality, roadmap
versions.toml    the single pinned set of upstream versions
LAYERS.toml      the layer map that check-layers enforces
```

## Building and checking

You need Rust; [`rust-toolchain.toml`](rust-toolchain.toml) pins the version and rustup picks it up. These are the
checks CI runs:

```sh
cargo --locked xtask check-layers        # layer dependencies and forbidden concepts
bash tools/patch/lint.sh                 # upstream patch headers, markers and budgets
cargo --locked xtask codegen --check     # committed generated code matches the contracts
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

`codegen` downloads the protoc release pinned in [`versions.toml`](versions.toml) and checks its SHA-256 before using
it; it never uses a `protoc` from `PATH`.

## Documentation

The design documents in [`docs/`](docs/) are written in Korean today and are being translated to English. The ones to
start with:

| Document | What's in it |
| --- | --- |
| [docs/00-vision.md](docs/00-vision.md) | Why this exists, use cases, how it compares, non-goals |
| [docs/01-principles.md](docs/01-principles.md) | The rules that don't change |
| [docs/02-architecture.md](docs/02-architecture.md) | Layers, dependency rules, directory layout, tracing |
| [docs/03-contracts.md](docs/03-contracts.md) | Every layer contract |
| [docs/04-adp.md](docs/04-adp.md) | The ADP protocol |
| [docs/10-security.md](docs/10-security.md) | Threat model and security requirements |
| [docs/13-roadmap.md](docs/13-roadmap.md) | Milestones, gates and open decisions |
| [docs/scope.md](docs/scope.md) | Every feature against every layer, with its milestone |

The other documents cover the VMM and devices, the guest, the runtime, upstream and patch policy, the open-core
boundary, quality and testing, and build infrastructure; [docs/glossary.md](docs/glossary.md) defines the terms.

## Security

The design assumes the whole guest is hostile, including its kernel, and that any ADP client is untrusted until it
authenticates. The boundaries and how each one is held are in [docs/10-security.md](docs/10-security.md). There is no
running code to attack yet. A security policy with a private reporting channel will be published before the first
runnable release; until then, please don't put vulnerability details in public issues.

## Contributing

The project is pre-implementation, and contribution guidelines are not written yet. Work follows the rules in
[CLAUDE.md](CLAUDE.md) (for people and coding agents alike): one layer and one contract per change, the end-to-end
test first, and the checks above green before review. Issues about the design are welcome.

## License

Droid Use is licensed under the [Apache License 2.0](LICENSE). Third-party components and their licenses are listed
in [NOTICE](NOTICE). Droid Use is not affiliated with Google, AWS, the Firecracker project or the wick project.
