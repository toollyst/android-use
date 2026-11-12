# Hand-off (2026-11-12)

State of the work for whoever continues, human or agent. Read `CLAUDE.md` and
`.claude/rules/*.md` first; this file only says where things stand. Delete or
rewrite it once it is stale.

## How the work is run
- The main session supervises ("mother"): at most two subagents (Opus, high
  effort) do all implementation; the main session verifies their branches with
  the commands below, merges to `main`, pushes, reports to the owner in Korean,
  and asks the owner for decisions. See `.claude/rules/orchestration.md` and
  `.claude/rules/decisions.md`.
- Repository text is English only (`.claude/rules/language.md`). Most existing
  docs and comments are still Korean; see "Documentation overhaul".
- Remote: `origin` = github.com/toollyst/android-use (public). Commits are
  authored as `dev-chamomile` (repo-local git config on the original machine:
  set `user.name`/`user.email` to that account's noreply address on a new
  machine before committing).
- Subagents had been split as: A = build tooling (`build/**`, `tools/**`, CI),
  B = contracts and docs. Their conversations do not transfer to another
  machine; start fresh agents and brief them from this file.

## Verification before every merge
```sh
cargo --locked xtask check-layers
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
bash tools/patch/lint.sh
cargo --locked xtask codegen --check     # downloads the pinned protoc into a cache outside the repo
buf build contracts/guest_link/proto && buf build contracts/node/proto   # local only, not in CI
```

## What exists on main
- Workspace, toolchain pin (1.98.1), CI on Linux x86_64/arm64, macOS arm64,
  Windows x86_64 (`.github/workflows/ci.yml`).
- `cargo xtask check-layers`: layer dependencies, directional sibling edges
  (`[allow_siblings]` in a layer-root `LAYER.toml`), forbidden concepts with
  prefix matching, contracts external-crate allowlist (`LAYERS.toml [contracts]`).
- `cargo xtask patch-lint` / `tools/patch/lint.sh`: policy in
  `third_party/patches.toml`; patch README tables generated from headers.
- `cargo xtask codegen [--check]`: prost messages for guest_link and node,
  serde types for ADP from `contracts/adp/protocol.json`; config in
  `build/codegen/codegen.toml`; protoc 36.2 pinned by sha256 in `versions.toml`.
- Contracts (all 0.1.0; the set at the M0 gate becomes the baseline):
  common, machine, device, instance, spi (Rust traits, async at I/O boundaries
  via `async-trait`), adp (CDP-style definition + selector grammar and
  conformance corpus + role map), guest_link and node (protobuf; gRPC services
  defined, bindings are generated outside contracts), keymint (empty: framing
  undefined).
- No product code yet: `host/`, `guest/`, `clients/`, `testing/` hold only
  `LAYER.toml` and README files.

## CI status
- Linux x86_64, Linux arm64, macOS arm64: green on main `13f6fc9`.
- Windows: the CRLF failure is fixed (`.gitattributes`). The next failure was
  one test, `lint_sh_passes_the_exit_code_through` (it picked up WSL's
  `System32\bash.exe`); the fix is in this push and is **unverified until the
  CI run on this push finishes**. With `--no-fail-fast` now set, this run is
  also the first time the remaining Windows test targets report.
- First thing to do next session: check the latest run
  (`gh run list --repo toollyst/android-use --limit 3`).

## Branches pushed besides main
- `docs/readme-draft`: English rewrite of the root `README.md` in the style of
  github.com/toollyst/sandbox (one commit on top of an older main; only
  `README.md` differs). **Not merged: the owner has not reviewed it.** Show it
  to the owner, then merge or revise.

## M0 gate (docs/13-roadmap.md §3): where it stands
1. Eight contracts with versions: present.
2. CI fails on real violations: binary-level exit-code tests exist
   (`build/xtask/tests/exit_codes.rs`); green on three OSes, Windows pending.
   Not in CI yet: `contracts/compat` back-compat checks, `buf` checks (new work,
   needs owner approval).
3. Call path `clients/mcp → host/entrypoints/adp_server → … → host/vmm` with
   unimplemented responses and one trace id: nothing exists. Proposed split
   (one layer + one contract each): T1 contracts/common trace context type;
   T2 host/vmm stub Machine; T3 host/runtime stub Instance; T4 host/services
   input + screen stubs; T5 host/entrypoints/adp_server (WebSocket JSON-RPC,
   authentication, unimplemented results); T6 clients/mcp; T7 testing/e2e
   harness. T1 first; then one agent T2→T3→T4 and the other T5→T6; T7 once T5
   exists. Blocked on the trace-context decision below.
4. All v1 E2E scenarios exist and are red: `testing/e2e` does not exist.
   Scenario set = docs/scope.md M1–M6 rows + roadmap gates. Each scenario should
   fail on a named UNIMPLEMENTED reason, not be skipped.
5. `versions.toml` pins still TBD: `wick.commit`, kernel GKI branch/build,
   `gfxstream.rev`, `rutabaga.rev`, `images.*`.

## Decisions the owner already made (do not re-ask)
- Product focus now: local CLI managing several Android instances on one
  machine + MCP. The node contract (multi-machine control plane) stays a
  minimal extension point.
- ADP: CDP-style Target model (instance → display/window/webview sessions);
  auth by upgrade header or first-message `Connection.auth`; mandatory
  `Connection.hello`; string error codes; 64-bit integers as decimal strings;
  per-request `resolveOn` with one `timeoutMs` and `conditionMet`; click
  `method` = accessibility | touch | auto (default auto); screenshots binary
  frame or base64 per request; touch = Android MotionEvent model, keys =
  Android key codes; APK install via upload (size up front, chunk acks,
  SHA-256), source is an extensible union; tree diff = node-id ops, one format,
  extensible; selectors = extensible engines with EBNF + conformance corpus;
  roles = WAI-ARIA names via `contracts/adp/roles/role-map.json`.
- guest_link: gRPC over vsock, two channels on two ports; every call carries
  the session token (binary metadata) and W3C `traceparent`; no Auth RPC,
  Rebind rotates the token; initial token on a dedicated secret disk readable
  only by the guest agent's SELinux domain; only that domain may listen on
  vsock (adbd exception in debug builds only).
- Contracts may use external crates only from the allowlist; use established
  crates and standards instead of inventing; ask before adding a runtime or a
  server stack to a contract.
- Settings instead of questions wherever a setting suffices; when several
  variants would multiply implementation, build the best one and keep it
  extensible.
- AOSP tag `android-17.0.0_r1`. Infra: see docs/12, docs/13 §4.

## Open questions for the owner (ask in question form, recommendation first)
Structure / contracts:
- Trace context across in-process traits: (a) explicit parameter on every trait
  method, (b) ambient `tracing`/OpenTelemetry context, (c) ambient in-process +
  explicit W3C fields at every wire/IPC boundary. Recommended: (c). Blocks T1.
- ErrorCode exists twice (contracts/common and generated ADP enum; a test keeps
  them equal): keep both, re-export, or generate common's from protocol.json.
- Should generated code validate envelope invariants (exactly one of
  result/error, id range)?
- Java `float` values typed as JSON number: recommended rule = INVALID_PARAMS
  outside the finite f32 range, round-to-nearest-even otherwise.
- Key injection path: (a) virtio-input like touch (no metaState/repeatCount
  parameters), (b) guest-side `InputManager.injectInputEvent`. Recommended: (a).
- Window/task/IME model: `AccessibilityWindowInfo` vs
  `WindowManager.LayoutParams.TYPE_*`; neither verified field by field yet.
- `App.stop` (needed for M2): recommended `{packageName}` with force-stop
  semantics on the instance target.
- How CTS reaches adbd on the release image: recommended adbd on a local socket
  relayed by the guest agent over a new guest_link stream.
- Reconnection order after restore: recommended guest reconnects host channel
  → Rebind with the old token → host opens guest channel with the new token;
  fix docs/07 to Rebind → Restore.Notify → Ready.
- guest_link shapes still undefined: TreeSnapshot (recommended: raw per-source
  trees, merged by services/inspect), Frame.Committed (`scanout_ref` waits for
  the device contract; `layers[]` depends on screenshot layer identity),
  Restore.Notify (`generation` = 16-byte VMGenID recommended).
- Deferred by the owner until wick integration is further along: how the
  secret disk appears in the machine contract and whether Rebind rewrites it
  (row in docs/13 §6). Analysis so far: a generic GPT partition name is the
  cleanest identification; host rewrites the backing after Rebind.
- Screenshot layer identity; Session domain (snapshot/reset/restored); pullFile
  (recommended: upload pattern reversed); full ErrorCode → gRPC status table;
  what `Node.className` holds (recommended: accessibility class name); touch
  occlusion for click.
- Choices the contracts agent made inside decisions, awaiting confirmation:
  wrong target type → INVALID_REQUEST; detach removes the subtree children
  first; upload edge cases (1002 on uncorrelated binary frame, etc.);
  `querySelector` returns `nodeIds` with at most one entry; `auto` falls back
  to touch only when the action is not exposed; `ActionArguments` keys are the
  Java constant names; `displayId` uint32; strict role map leaves EditText,
  TextView, ImageView without a role; selector `[s=false]` matches an absent
  boolean; XPath view omits bounds/actions; insert without previousSiblingId =
  first child.
- Still undecided from earlier: GMS, TLS visibility default, ARM apps on x86_64
  hosts in v1 (docs/13 §6); Tokyo C4A quota; whether to create the build server
  and start the AOSP sync.

Open source / documentation:
- Naming: repository `android-use` vs product/crates `droid-use`/`droiduse`.
  Other projects already use both names (an "Android Use" library, a PyPI
  `droid-use`). Registries not checked yet.
- Whether CLAUDE.md, AGENTS.md and `.claude/rules/` stay public as they are.
  Recommended: keep the engineering rules as an English contributor guide; move
  owner-workflow rules (orchestration, ask-the-owner) out of the public repo.
- "Hopfia Hub" mention in docs/00 (recommended: generalize); private-repo
  layout in docs/09 §4 and competitor table in docs/00; GCP machine types,
  regions and prices in docs/12 and docs/13 §4 (recommended: keep requirements,
  move prices out).
- Enable GitHub private vulnerability reporting and publish SECURITY.md
  (recommended: now). OWNERS vs CODEOWNERS (recommended: CODEOWNERS). DCO vs
  CLA vs none (recommended: DCO). Dependabot vs Renovate. Start CHANGELOG now.
- Per-directory GPL-2.0 notice for `third_party/kernel/patches`.
- Budget-overrun approval CI job (compare PR approvers with
  `Budget-approved-by:`) and branch protection: not implemented yet.

## Documentation overhaul (owner request: read like github.com/toollyst/sandbox)
Stage 1 done: reference analysis and README draft (`docs/readme-draft`).
Proposed next stages, after the owner approves the README direction:
- 2a, translate in place, no renames, two agents with disjoint files. Agent 1:
  `docs/glossary.md` first (shared term list), then docs/*, scope, runbooks,
  layer READMEs, new community files (CONTRIBUTING, SECURITY, CODE_OF_CONDUCT,
  GOVERNANCE, MAINTAINERS, CHANGELOG, issue/PR templates). Agent 2: comments
  and test names in `build/**`, all config files, `ci.yml`, then contract
  comments, IDL descriptions and protos, then regenerate.
- 2b, restructure in one mechanical commit: `docs/design/`, `docs/protocol/`,
  `docs/developers/`, top-level roadmap/scope/glossary. About 116 references in
  47 tracked files point at the current `docs/NN-*` paths (CI, Cargo.toml,
  checker sources and tests, contract sources, IDL, generated files, CLAUDE.md
  §0/§2/§6/§8) and must be rewritten by script; keep CLAUDE.md's reading order
  working.
- 2c, GitHub settings: description, topics, private vulnerability reporting.
- No install/quickstart/benchmark content until something runs; no CLI examples
  until a CLI surface is defined (it is not defined anywhere yet).
