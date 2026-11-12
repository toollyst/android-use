//! Value types of the `Authenticator` and `Authorizer` SPIs.
//!
//! Flow (`docs/10-security.md` §4, `docs/04-adp.md` §4):
//! 1. A listener accepts a channel and describes it as a [`ChannelCtx`]: the transport, the peer
//!    address or the peer's OS identity when the transport knows it, and the browser `Origin`
//!    when one was sent. Localhost is never trusted on its own: the context only informs the
//!    `Authenticator`, it does not authenticate anything.
//! 2. The presented secret becomes a [`Credential`]. `Authenticator::authenticate` checks it in
//!    that channel context and returns the [`Principal`]: a stable non-secret id, the
//!    [`InstanceScope`] it was granted, and opaque labels.
//! 3. For every check (attach, every command in a session, every target listed by
//!    `Target.getTargets` or a target event) the server calls `Authorizer::authorize` once, with
//!    the [`Action`] (the command) and the [`Resource`] it addresses. The [`Decision`] is
//!    [`Decision::Allow`] or a [`Decision::Deny`] with a machine-readable [`DenyReason`].
//!
//! The core knows only two scopes: an owner principal that may touch every instance on the
//! server, and principals limited to a set of instances (the per-instance tokens the CLI mints
//! for AI agents). Anything else an extension needs to decide
//! on, such as an organisation it groups principals and instances by, lives in the opaque
//! [`Labels`] of the [`Principal`] and of the [`InstanceResource`]; the core does not interpret
//! them (`docs/01-principles.md` P4).
//!
//! Every type that the core may need to grow is `#[non_exhaustive]`. Code outside this crate
//! that matches on [`Decision`] must treat an unknown variant as a denial; the simplest way is
//! [`Decision::is_allowed`].

use std::collections::BTreeSet;
use std::fmt;
use std::net::SocketAddr;

use droiduse_contracts_common::Labels;

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// A credential presented on a channel.
///
/// Its `Debug` output never contains the secret, and it implements neither `Display`, `Clone`,
/// `PartialEq` nor `Hash` (see [`BearerToken`]).
#[derive(Debug)]
#[non_exhaustive]
pub enum Credential {
    /// An OAuth 2.0 bearer token (RFC 6750). ADP: the `Authorization: Bearer` header of the
    /// WebSocket upgrade or the `token` of `Connection.auth`; both carry the same string.
    Bearer(BearerToken),
}

/// The secret of a bearer token, in RFC 6750 §2.1 `b64token` syntax.
///
/// - `Debug` prints `BearerToken(<redacted>)`. There is no `Display` and no `serde`
///   implementation, so the secret cannot reach a log or a message by formatting.
/// - No `Clone`: the secret is not copied around; the `Authenticator` borrows it.
/// - No `PartialEq` and no `Hash`. Comparing a presented secret with a stored one through `==`
///   stops at the first differing byte and leaks its position through timing. The comparison
///   belongs in the `Authenticator` implementation, not in this contract: it derives the stored
///   form from the presented secret (the core default keeps only a SHA-256 digest of each token)
///   and compares the two with a proven constant-time primitive.
/// - The only way to read the secret is [`BearerToken::expose_secret`], so every read is easy
///   to find in review.
///
/// OPEN: the buffer is not zeroed on drop. Doing that reliably needs a crate outside the
/// contracts allowlist (`zeroize`); this crate forbids `unsafe` and does not imitate it.
pub struct BearerToken(String);

impl BearerToken {
    /// Wraps a token string after checking that it is an RFC 6750 §2.1 `b64token`:
    /// `1*( ALPHA / DIGIT / "-" / "." / "_" / "~" / "+" / "/" ) *"="`.
    ///
    /// The `Bearer` scheme and the whitespace after it are not part of the token; taking them off
    /// the header is the listener's job. The error does not contain the rejected value.
    pub fn new(token: String) -> Result<Self, CredentialError> {
        validate_b64token(&token)?;
        Ok(Self(token))
    }

    /// The secret. Only for the `Authenticator` that verifies it; never for logs or messages.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for BearerToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BearerToken(<redacted>)")
    }
}

impl TryFrom<String> for BearerToken {
    type Error = CredentialError;

    fn try_from(token: String) -> Result<Self, Self::Error> {
        Self::new(token)
    }
}

/// RFC 6750 §2.1 `b64token`.
fn validate_b64token(token: &str) -> Result<(), CredentialError> {
    let body = token.trim_end_matches('=');
    if body.is_empty() {
        return Err(CredentialError::Empty);
    }
    let token_char =
        |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'+' | b'/');
    if body.bytes().all(token_char) {
        Ok(())
    } else {
        Err(CredentialError::Syntax)
    }
}

/// Why a credential value was rejected before it reached the `Authenticator`. It never contains
/// the value. On the wire this is an authentication failure like any other (ADP:
/// `UNAUTHENTICATED`, and `invalid_token` on the upgrade, RFC 6750 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CredentialError {
    /// The token is empty, or consists of padding (`=`) only.
    Empty,
    /// The token contains a character outside the `b64token` set, or `=` before the end.
    Syntax,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "the bearer token is empty",
            Self::Syntax => "the bearer token is not in RFC 6750 b64token syntax",
        })
    }
}

impl std::error::Error for CredentialError {}

// ---------------------------------------------------------------------------
// Channel context
// ---------------------------------------------------------------------------

/// The channel a credential arrived on, as the listener observed it.
///
/// Facts the listener does not know are absent (`None`); an `Authenticator` that needs one of
/// them treats its absence as a failure (fail closed). The values come from the OS (transport,
/// peer address, peer identity) or from the client (`origin`); the latter is untrusted.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChannelCtx {
    /// The transport, with the peer's network address where the transport has one.
    pub transport: Transport,
    /// The OS identity of the peer process, when the listener could obtain it from the OS (the
    /// peer credentials of a local socket or pipe).
    pub peer_identity: Option<PeerIdentity>,
    /// The browser `Origin` header of the WebSocket upgrade, when one was sent.
    pub origin: Option<Origin>,
}

impl ChannelCtx {
    /// A context with only the transport known.
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            peer_identity: None,
            origin: None,
        }
    }

    /// Sets the peer's OS identity.
    pub fn with_peer_identity(mut self, identity: PeerIdentity) -> Self {
        self.peer_identity = Some(identity);
        self
    }

    /// Sets the browser `Origin`.
    pub fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = Some(origin);
        self
    }
}

/// The transport of a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Transport {
    /// A Unix domain socket (the default local listener: a per-user socket).
    UnixSocket,
    /// A Windows named pipe (the default local listener on Windows: a per-user pipe).
    NamedPipe,
    /// TCP. A loopback address is not trusted by itself.
    Tcp {
        /// The peer's address and port.
        peer: SocketAddr,
    },
}

/// The OS identity of the peer process, as the OS reported it to the listener.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PeerIdentity {
    /// A POSIX user ID (the peer credentials of a Unix domain socket).
    Unix {
        /// The peer's effective user ID.
        uid: u32,
    },
    /// A Windows security identifier, in its string form (`S-1-…`; MS-DTYP §2.4.2.1).
    Windows {
        /// The SID of the peer process's user.
        sid: String,
    },
}

/// A browser `Origin` header value (RFC 6454 §7), exactly as received.
///
/// Untrusted input. An allowlist compares it as a whole string; `null` is the opaque origin.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin(pub String);

// ---------------------------------------------------------------------------
// Principal
// ---------------------------------------------------------------------------

/// An authenticated principal: who is acting, and what it was granted at authentication.
///
/// `scope` is what the credential granted when the connection was authenticated. Revocation
/// happens later, so an `Authorizer` that supports it decides by the live state it keeps for
/// `id`, never by `scope` alone; the server keeps calling it for every command, so a revocation
/// takes effect at the next command (`docs/10-security.md` §4).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Principal {
    /// Stable identifier of the principal.
    pub id: PrincipalId,
    /// The instances the principal may touch.
    pub scope: InstanceScope,
    /// Opaque key-value pairs an extension attaches. The core does not interpret them.
    pub labels: Labels,
}

impl Principal {
    /// A principal with no labels.
    pub fn new(id: PrincipalId, scope: InstanceScope) -> Self {
        Self {
            id,
            scope,
            labels: Labels::default(),
        }
    }

    /// Sets the labels.
    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }
}

/// Identifier of a principal. Opaque to the core; it appears in logs, traces and audit events,
/// so it is never the credential, nor derived from it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrincipalId(pub String);

impl fmt::Display for PrincipalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The instances a principal may touch.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstanceScope {
    /// Every instance on this server, including ones created later (the owner token).
    All,
    /// Only these instances (a per-instance token). An empty set touches no instance.
    Only(BTreeSet<InstanceRef>),
}

impl InstanceScope {
    /// Whether `instance` is inside the scope. A membership test only: whether an action on that
    /// instance is allowed is still the `Authorizer`'s decision.
    pub fn includes(&self, instance: &InstanceRef) -> bool {
        match self {
            Self::All => true,
            Self::Only(set) => set.contains(instance),
        }
    }
}

// ---------------------------------------------------------------------------
// Action and resource
// ---------------------------------------------------------------------------

/// What the principal is trying to do. One value per protocol call, so that one
/// `Authorizer::authorize` call covers one command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// An ADP command.
    Adp(AdpCommand),
}

/// An ADP command, named as in `contracts/adp/protocol.json`.
///
/// The server authorizes only commands that exist in the negotiated protocol version (an
/// unknown method is `METHOD_NOT_FOUND` before authorization), so `method` is always a defined
/// `Domain.command`. The checks the server makes:
/// - attach: `Target.attachToTarget`, on the target being attached to;
/// - listing: `Target.attachToTarget` again, once per target `Target.getTargets` or a target
///   event would show (only targets the principal can attach to are shown);
/// - every command in a session: that command, on the session's target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AdpCommand {
    /// The method, `Domain.command` (for example `Input.click`).
    pub method: String,
    /// How the command was sent.
    pub scope: CommandScope,
}

impl AdpCommand {
    /// An ADP command.
    pub fn new(method: impl Into<String>, scope: CommandScope) -> Self {
        Self {
            method: method.into(),
            scope,
        }
    }
}

/// How an ADP command was sent (the `scope` of commands in `contracts/adp/protocol.json`; a
/// `both` command is one or the other, depending on the request).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CommandScope {
    /// Without a `sessionId`.
    Connection,
    /// In a session (with a `sessionId`).
    Session,
}

/// What the action addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Resource {
    /// The server itself, not one instance (for example `Target.getTargets` without a session).
    Server,
    /// One instance (ADP: the instance target, or a command in an instance target's session).
    Instance(InstanceResource),
    /// A target beneath an instance (ADP: a display, window, process or webview target, or a
    /// command in its session).
    Target(TargetResource),
}

impl Resource {
    /// The instance the resource belongs to; `None` for [`Resource::Server`].
    pub fn instance(&self) -> Option<&InstanceResource> {
        match self {
            Self::Server => None,
            Self::Instance(instance) => Some(instance),
            Self::Target(target) => Some(&target.instance),
        }
    }
}

/// An instance as the `Authorizer` sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstanceResource {
    /// The instance.
    pub id: InstanceRef,
    /// The instance's labels, so that an `Authorizer` can decide by them without looking the
    /// instance up.
    pub labels: Labels,
}

impl InstanceResource {
    /// An instance resource.
    pub fn new(id: InstanceRef, labels: Labels) -> Self {
        Self { id, labels }
    }
}

/// A target beneath an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TargetResource {
    /// The instance the target sits beneath.
    pub instance: InstanceResource,
    /// The target.
    pub target: TargetRef,
}

impl TargetResource {
    /// A target resource.
    pub fn new(instance: InstanceResource, target: TargetRef) -> Self {
        Self { instance, target }
    }
}

/// Identifier of an instance: the same opaque value as `InstanceId` of the `instance` contract
/// and `instance_id` of the `node` contract. PLACEHOLDER: representation undecided, as there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceRef(pub String);

/// Identifier of an ADP target (`Target.TargetID`). Opaque.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetRef(pub String);

// ---------------------------------------------------------------------------
// Decision
// ---------------------------------------------------------------------------

/// An authorization decision. Fails closed: the default is a denial, and only
/// [`Decision::Allow`] allows.
///
/// The server also counts an `Err` from the `Authorizer`, and a cancelled call, as a denial. In
/// ADP a denial answers the command with `POLICY_DENIED`; a denial in a session then ends the
/// session (`detachedFromTarget`, reason `authorizationRevoked`).
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Decision {
    /// Allowed.
    Allow,
    /// Denied, with the reason.
    Deny(DenyReason),
}

impl Decision {
    /// Whether the decision allows the action. `false` for every denial and for every variant a
    /// later contract version adds, so callers outside this crate stay fail-closed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

impl Default for Decision {
    /// A denial with [`DenyReason::Unspecified`].
    fn default() -> Self {
        Self::Deny(DenyReason::Unspecified)
    }
}

/// Why an action was denied, machine-readable. [`DenyReason::code`] gives the reason as an
/// `UPPER_SNAKE_CASE` string (the `google.rpc.ErrorInfo` `reason` convention).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DenyReason {
    /// No reason given: nothing allowed the action. The default. (`UNSPECIFIED`)
    #[default]
    Unspecified,
    /// The resource is outside the principal's [`InstanceScope`]. (`OUT_OF_SCOPE`)
    OutOfScope,
    /// The principal may touch the resource but not perform this action on it.
    /// (`ACTION_NOT_PERMITTED`)
    ActionNotPermitted,
    /// The principal's grant was revoked, or expired, after authentication. (`REVOKED`)
    Revoked,
    /// A reason an extension defines.
    Other(ReasonCode),
}

impl DenyReason {
    /// Codes of the reasons this contract defines. [`ReasonCode`] rejects them, so every code
    /// has one meaning.
    pub const DEFINED_CODES: &'static [&'static str] = &[
        "UNSPECIFIED",
        "OUT_OF_SCOPE",
        "ACTION_NOT_PERMITTED",
        "REVOKED",
    ];

    /// The reason as an `UPPER_SNAKE_CASE` code.
    pub fn code(&self) -> &str {
        match self {
            Self::Unspecified => "UNSPECIFIED",
            Self::OutOfScope => "OUT_OF_SCOPE",
            Self::ActionNotPermitted => "ACTION_NOT_PERMITTED",
            Self::Revoked => "REVOKED",
            Self::Other(code) => code.as_str(),
        }
    }
}

/// A deny reason code an extension defines: `UPPER_SNAKE_CASE`, matching
/// `[A-Z][A-Z0-9_]+[A-Z0-9]`, at most 63 characters (the `google.rpc.ErrorInfo` `reason`
/// convention), and not one of [`DenyReason::DEFINED_CODES`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReasonCode(String);

impl ReasonCode {
    /// Longest code.
    pub const MAX_LEN: usize = 63;

    /// Checks and wraps a code.
    pub fn new(code: impl Into<String>) -> Result<Self, ReasonCodeError> {
        let code = code.into();
        let bytes = code.as_bytes();
        let well_formed = (3..=Self::MAX_LEN).contains(&bytes.len())
            && bytes[0].is_ascii_uppercase()
            && bytes
                .iter()
                .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && bytes[bytes.len() - 1] != b'_';
        if !well_formed {
            return Err(ReasonCodeError::Syntax);
        }
        if DenyReason::DEFINED_CODES.contains(&code.as_str()) {
            return Err(ReasonCodeError::Defined);
        }
        Ok(Self(code))
    }

    /// The code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a [`ReasonCode`] was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReasonCodeError {
    /// Not `[A-Z][A-Z0-9_]+[A-Z0-9]` of at most 63 characters.
    Syntax,
    /// One of [`DenyReason::DEFINED_CODES`]; use that variant instead.
    Defined,
}

impl fmt::Display for ReasonCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Syntax => {
                "a reason code is UPPER_SNAKE_CASE ([A-Z][A-Z0-9_]+[A-Z0-9], at most 63 characters)"
            }
            Self::Defined => {
                "the reason code is defined by the contract; use its DenyReason variant"
            }
        })
    }
}

impl std::error::Error for ReasonCodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "s3cr3t-T0ken_value~+/==";

    fn token() -> BearerToken {
        BearerToken::new(SECRET.to_owned()).expect("valid b64token")
    }

    /// Compile-time check that `T` implements none of the traits that would let a secret leak or
    /// be compared in variable time (the technique of `static_assertions::assert_not_impl_any`).
    macro_rules! assert_not_impl {
        ($t:ty: $($tr:path),+ $(,)?) => {
            $(
                const _: fn() = || {
                    trait AmbiguousIfImpl<A> {
                        fn some_item() {}
                    }
                    impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
                    #[allow(dead_code)]
                    struct Invalid;
                    impl<T: ?Sized + $tr> AmbiguousIfImpl<Invalid> for T {}
                    let _ = <$t as AmbiguousIfImpl<_>>::some_item;
                };
            )+
        };
    }

    assert_not_impl!(BearerToken: Clone, PartialEq, std::hash::Hash, std::fmt::Display);
    assert_not_impl!(Credential: Clone, PartialEq, std::hash::Hash, std::fmt::Display);

    #[test]
    fn debug_redacts_the_secret() {
        let cred = Credential::Bearer(token());
        for rendered in [
            format!("{:?}", token()),
            format!("{:#?}", token()),
            format!("{cred:?}"),
            format!("{cred:#?}"),
        ] {
            assert!(!rendered.contains("s3cr3t"), "leaked: {rendered}");
            assert!(rendered.contains("<redacted>"), "not marked: {rendered}");
        }
        assert_eq!(format!("{cred:?}"), "Bearer(BearerToken(<redacted>))");
    }

    #[test]
    fn expose_secret_returns_the_token() {
        assert_eq!(token().expose_secret(), SECRET);
    }

    #[test]
    fn bearer_token_accepts_b64token() {
        for ok in ["a", "Z9", "-._~+/", "abc=", "abc==", "A-b.C_d~E+f/G"] {
            assert!(BearerToken::new(ok.to_owned()).is_ok(), "{ok}");
        }
    }

    #[test]
    fn bearer_token_rejects_other_syntax() {
        for (bad, err) in [
            ("", CredentialError::Empty),
            ("=", CredentialError::Empty),
            ("==", CredentialError::Empty),
            ("a=b", CredentialError::Syntax),
            ("a b", CredentialError::Syntax),
            ("Bearer abc", CredentialError::Syntax),
            ("abc\n", CredentialError::Syntax),
            ("tök", CredentialError::Syntax),
            ("a\"b", CredentialError::Syntax),
        ] {
            assert_eq!(
                BearerToken::new(bad.to_owned()).unwrap_err(),
                err,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn credential_error_does_not_echo_the_value() {
        let err = BearerToken::try_from("s3cr3t value".to_owned()).unwrap_err();
        assert!(!format!("{err} {err:?}").contains("s3cr3t"));
    }

    #[test]
    fn channel_ctx_builder_sets_only_what_is_known() {
        let peer: SocketAddr = "127.0.0.1:50000".parse().expect("address");
        let local = ChannelCtx::new(Transport::UnixSocket);
        assert_eq!(local.peer_identity, None);
        assert_eq!(local.origin, None);

        let tcp = ChannelCtx::new(Transport::Tcp { peer })
            .with_peer_identity(PeerIdentity::Unix { uid: 501 })
            .with_origin(Origin("http://localhost:3000".to_owned()));
        assert_eq!(tcp.transport, Transport::Tcp { peer });
        assert_eq!(tcp.peer_identity, Some(PeerIdentity::Unix { uid: 501 }));
        assert_eq!(tcp.origin, Some(Origin("http://localhost:3000".to_owned())));
    }

    #[test]
    fn instance_scope_membership() {
        let a = InstanceRef("a".to_owned());
        let b = InstanceRef("b".to_owned());
        assert!(InstanceScope::All.includes(&a));
        let only_a = InstanceScope::Only(BTreeSet::from([a.clone()]));
        assert!(only_a.includes(&a));
        assert!(!only_a.includes(&b));
        assert!(!InstanceScope::Only(BTreeSet::new()).includes(&a));
    }

    #[test]
    fn resource_names_its_instance() {
        let inst = InstanceResource::new(InstanceRef("i1".to_owned()), Labels::default());
        assert_eq!(Resource::Server.instance(), None);
        assert_eq!(Resource::Instance(inst.clone()).instance(), Some(&inst));
        let target = TargetResource::new(inst.clone(), TargetRef("t1".to_owned()));
        assert_eq!(Resource::Target(target).instance(), Some(&inst));
    }

    #[test]
    fn principal_defaults_to_no_labels() {
        let p = Principal::new(PrincipalId("owner".to_owned()), InstanceScope::All);
        assert_eq!(p.labels, Labels::default());
        assert_eq!(p.id.to_string(), "owner");
    }

    #[test]
    fn adp_command_carries_method_and_scope() {
        let action = Action::Adp(AdpCommand::new("Input.click", CommandScope::Session));
        let Action::Adp(cmd) = &action;
        assert_eq!(cmd.method, "Input.click");
        assert_eq!(cmd.scope, CommandScope::Session);
    }

    #[test]
    fn decision_fails_closed_by_default() {
        assert_eq!(Decision::default(), Decision::Deny(DenyReason::Unspecified));
        assert!(!Decision::default().is_allowed());
        assert!(!Decision::Deny(DenyReason::Revoked).is_allowed());
        assert!(Decision::Allow.is_allowed());
        assert_eq!(DenyReason::default(), DenyReason::Unspecified);
    }

    #[test]
    fn deny_reason_codes_are_distinct_and_defined() {
        let defined = [
            DenyReason::Unspecified,
            DenyReason::OutOfScope,
            DenyReason::ActionNotPermitted,
            DenyReason::Revoked,
        ];
        let codes: Vec<&str> = defined.iter().map(DenyReason::code).collect();
        assert_eq!(codes, DenyReason::DEFINED_CODES);
        let unique: BTreeSet<&str> = codes.iter().copied().collect();
        assert_eq!(unique.len(), codes.len());
        let custom = ReasonCode::new("RATE_LIMITED").expect("valid code");
        assert_eq!(DenyReason::Other(custom).code(), "RATE_LIMITED");
    }

    #[test]
    fn reason_code_syntax() {
        for ok in ["ABC", "A1B", "OUT_OF_HOURS", "X_9", &"A".repeat(63)] {
            assert!(ReasonCode::new(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "AB",
            "abc",
            "Abc",
            "_ABC",
            "1ABC",
            "ABC_",
            "AB-C",
            "AB C",
            &"A".repeat(64),
        ] {
            assert_eq!(
                ReasonCode::new(bad).unwrap_err(),
                ReasonCodeError::Syntax,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn reason_code_rejects_defined_codes() {
        for code in DenyReason::DEFINED_CODES {
            assert_eq!(
                ReasonCode::new(*code).unwrap_err(),
                ReasonCodeError::Defined,
                "{code}"
            );
        }
    }
}
