# 10. Security

## 1. Threat model
- **Untrusted**: the entire guest (apps, actions the AI performed, the guest kernel included), external ADP clients (before authentication)
- **Protected**: the host, other sessions, host keys and secrets, other tenants (SaaS)
- Assumption: the guest can send malicious virtio input and guest_link messages at any time.

## 2. Isolation
| Target | Method |
|---|---|
| Guest ↔ host | microVM (KVM / HVF / WHP) |
| VMM process | `host/runtime/jail`: Linux jailer, macOS seatbelt/App Sandbox, Windows AppContainer |
| Device backends | **Each a separate process, each in a least-privilege sandbox.** The gpu, with the largest attack surface, gets the strongest one |
| Host KeyMint | Separate process, dedicated sandbox, no access beyond the host secure storage (`host/runtime/keymint`) |
| Session ↔ session | A process set per instance, separate disk CoW layers, separate tokens |
| Network | usernet blocks by default + allowlist policy, per-session egress logging |

## 3. Guest security
- user build, adbd off by default (enabled only by host authorization, for CTS/VTS only)
- AVB with our own keys, rollback index
- SELinux enforcing, a dedicated domain for every new component
- KeyMint runs on the host (`host/runtime/keymint`, a separate jailed process); keys are wrapped by the host secure storage (Secure Enclave / TPM)
- Clone uniqueness: VMGenID + restore hook (`docs/07-runtime.md` §4)

## 4. Channel security
- Every channel is authenticated (ADP, Node API, streams, guest_link session token)
- guest_link processes a message only after version negotiation, the message size limit and schema validation. The size limit is a host and guest setting per channel, and its default is the gRPC default (4 MiB)
- guest_link channels: two gRPC channels on two vsock ports. On the host channel the host is the server (guest → host services); on the guest channel adpd is the server (host → guest services). Every call carries W3C `traceparent`
- guest_link guest channel port: only adpd's SELinux domain may use listening vsock sockets; every other domain is denied `vsock_socket` `listen` by a sepolicy `neverallow`. SELinux has no per-port labelling for vsock (the kernel checks `name_bind` only for inet sockets), so the rule is per domain. Debug builds only: adbd over vsock is also allowed
- guest_link authentication: every call on both channels carries the session token in binary metadata (`droiduse-session-token-bin`); that is the authentication, and there is no separate authentication call. No call is accepted without it; a missing or invalid token gets gRPC `UNAUTHENTICATED`
- guest_link first token: the VMM attaches a small dedicated secret disk holding the token, and only adpd's SELinux domain can read that disk. `Auth.Rebind` changes the token after every restore
- ADP authentication: the `Authorization: Bearer` header of the upgrade or a first message `Connection.auth`. Every message before authentication is rejected. Tokens in the URL query string are not accepted. Authentication happens once per connection, and changing credentials needs a new connection. Rate limiting of failed authentication is a server setting and is on by default
- ADP authorization: checked at attach and again before every command in a session. A denial answers the command with `POLICY_DENIED` and ends the session (`detachedFromTarget`, reason `authorizationRevoked`); an Authorizer failure counts as a denial
- ADP version mismatch: `UNSUPPORTED_VERSION`, then the connection is closed with status 1008; no retry on the same connection
- ADP targets: `getTargets` and target events show only targets the principal can attach to. If authorization fails, the target is not shown (fail closed)
- ADP `traceparent`: the value a client sends is untrusted input. Its format is validated, and by default a new trace is started that links the client trace
- TLS visibility (per-session CA injection) is turned on only by policy, and sessions with it on are marked in the metadata
- Re-authentication after cloning: right after restore, every clone gets a new token and the previous token is revoked

## 4.1 Exposure inside the guest
- Other apps inside the guest are not trusted either.
- Paths opened for inspection and debugging (WebView debugging, inspector hooks, agent, adbd, the TLS visibility CA) must be usable only by adpd and the host, are turned on only by policy, and do not break AVB.

## 5. Fuzzing (always on in CI)
- virtio queue parsers (all devices)
- `host/vmm/boot` image parsers, `host/vmm/disk` GPT/layer parsers
- usernet packet handling
- guest_link decoder, keymint framing decoder, ADP decoder
- Snapshot metadata parser
- Frame matching input (identifiers sent by the guest are not trusted and are validated)

## 6. Keys and secrets
- AVB signing keys, platform signing keys: CI secrets / HSM. Only public keys in the repository
- Key rotation runbook (`docs/runbooks/`)
- Release bundles: signature, SBOM, provenance

## 7. Supply chain
- Pinned dependencies (`Cargo.lock`, `versions.toml`)
- Reproducible builds as a goal
- Renovate updates are merged after the tests pass
