//! host/runtime/auth: the core default `Authenticator` and `Authorizer` (`contracts/spi`).
//!
//! One server token. The token file holds the token in plain text; it is created when absent
//! (256 bits from the OS CSPRNG, base64url without padding, which is an RFC 6750 `b64token`).
//! Whoever can read the file is the owner. The [`LocalTokenAuthenticator`] keeps only the
//! SHA-256 digest of the token and compares digests in constant time. Its principal is the owner
//! ([`OWNER`], scope [`InstanceScope::All`]); the [`OwnerAuthorizer`] allows everything for that
//! principal and denies everything else.
//!
//! File protection:
//! - Unix: the file is created with mode 0600 (its directory, when created, with 0700), and an
//!   existing file that the group or others may access is refused.
//! - Windows: no ACL is set; the file inherits the ACL of its directory. Under the per-user
//!   profile (the default location) that is the user, SYSTEM and Administrators.
//!
//! Not built yet: per-instance tokens, minting, revocation, peer-credential checks.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use async_trait::async_trait;
use base64::Engine;
use droiduse_contracts_common::{Error, ErrorCode, Result};
use droiduse_contracts_spi::{
    Action, Authenticator, Authorizer, BearerToken, ChannelCtx, Credential, Decision, DenyReason,
    InstanceScope, Principal, PrincipalId, Resource,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// The component name in `Error::reason`.
const COMPONENT: &str = "host/runtime/auth";

/// The principal id of the token holder.
pub const OWNER: &str = "owner";

/// Bytes of randomness in a created token (256 bits).
const TOKEN_BYTES: usize = 32;

/// A token file larger than this is not a token file.
const MAX_TOKEN_FILE_LEN: u64 = 4096;

/// Authenticates the one server token.
pub struct LocalTokenAuthenticator {
    digest: [u8; 32],
}

impl std::fmt::Debug for LocalTokenAuthenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalTokenAuthenticator")
    }
}

impl LocalTokenAuthenticator {
    /// Reads the token from `path`, creating the file with a new token when it does not exist.
    /// The error never contains the token.
    pub fn from_token_file(path: &Path) -> io::Result<Self> {
        let token = match read_token_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => match create_token_file(path) {
                // Another process created it first: use that one.
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => read_token_file(path)?,
                other => other?,
            },
            other => other?,
        };
        Ok(Self {
            digest: digest(token.expose_secret()),
        })
    }
}

fn digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

fn read_token_file(path: &Path) -> io::Result<BearerToken> {
    let file = File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(invalid("the token file is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(invalid(
                "the token file is accessible by the group or others; it must have mode 0600",
            ));
        }
    }
    if meta.len() > MAX_TOKEN_FILE_LEN {
        return Err(invalid("the token file is too large"));
    }
    let mut text = String::new();
    file.take(MAX_TOKEN_FILE_LEN).read_to_string(&mut text)?;
    let token = text.trim_end_matches(['\n', '\r']).to_owned();
    BearerToken::new(token)
        .map_err(|_| invalid("the token file does not hold an RFC 6750 b64token"))
}

fn create_token_file(path: &Path) -> io::Result<BearerToken> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir)?;
    }
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(token.as_bytes())?;
    file.sync_all()?;
    BearerToken::new(token).map_err(|_| invalid("the created token is not a b64token"))
}

fn unauthenticated(message: &str) -> Error {
    Error::new(ErrorCode::Unauthenticated, message).with_reason(COMPONENT, "authenticate")
}

#[async_trait]
impl Authenticator for LocalTokenAuthenticator {
    async fn authenticate(&self, cred: &Credential, ch: &ChannelCtx) -> Result<Principal> {
        // No browser origin is allowed yet (fail closed).
        if ch.origin.is_some() {
            return Err(unauthenticated("browser origins are not accepted"));
        }
        let token = match cred {
            Credential::Bearer(token) => token,
            _ => return Err(unauthenticated("unsupported credential")),
        };
        if bool::from(digest(token.expose_secret()).ct_eq(&self.digest)) {
            Ok(Principal::new(
                PrincipalId(OWNER.to_owned()),
                InstanceScope::All,
            ))
        } else {
            Err(unauthenticated("the token does not authenticate"))
        }
    }
}

/// Allows everything for the owner principal; denies everything else (fail closed).
#[derive(Debug, Default, Clone, Copy)]
pub struct OwnerAuthorizer;

#[async_trait]
impl Authorizer for OwnerAuthorizer {
    async fn authorize(
        &self,
        who: &Principal,
        _action: &Action,
        _on: &Resource,
    ) -> Result<Decision> {
        if who.id.0 == OWNER && who.scope == InstanceScope::All {
            Ok(Decision::Allow)
        } else {
            Ok(Decision::Deny(DenyReason::Unspecified))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use droiduse_contracts_spi::{AdpCommand, CommandScope, Origin, Transport};
    use std::net::{Ipv4Addr, SocketAddr};

    fn ch() -> ChannelCtx {
        ChannelCtx::new(Transport::Tcp {
            peer: SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        })
    }

    fn bearer(token: &str) -> Credential {
        Credential::Bearer(BearerToken::new(token.to_owned()).unwrap())
    }

    #[tokio::test]
    async fn creates_a_token_file_and_authenticates_its_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("token");
        let authn = LocalTokenAuthenticator::from_token_file(&path).unwrap();
        let token = fs::read_to_string(&path).unwrap();
        assert_eq!(token.len(), 43, "32 bytes in base64url without padding");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let p = authn.authenticate(&bearer(&token), &ch()).await.unwrap();
        assert_eq!(p.id, PrincipalId(OWNER.to_owned()));
        assert_eq!(p.scope, InstanceScope::All);

        // Loading again keeps the token.
        let again = LocalTokenAuthenticator::from_token_file(&path).unwrap();
        assert!(again.authenticate(&bearer(&token), &ch()).await.is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), token);
    }

    #[tokio::test]
    async fn a_wrong_token_or_an_origin_does_not_authenticate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        let authn = LocalTokenAuthenticator::from_token_file(&path).unwrap();
        let token = fs::read_to_string(&path).unwrap();

        let e = authn
            .authenticate(&bearer("wrong"), &ch())
            .await
            .unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::Unauthenticated));
        assert_eq!(e.reason.as_deref(), Some("host/runtime/auth#authenticate"));
        assert!(!e.to_string().contains("wrong"));

        let browser = ch().with_origin(Origin("https://example.com".to_owned()));
        let e = authn
            .authenticate(&bearer(&token), &browser)
            .await
            .unwrap_err();
        assert_eq!(e.code, Some(ErrorCode::Unauthenticated));
    }

    #[test]
    fn reads_a_token_with_a_trailing_newline_and_refuses_a_bad_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap().write_all(b"abc\n").unwrap();
        assert!(LocalTokenAuthenticator::from_token_file(&path).is_ok());

        fs::write(&path, "not a token").unwrap();
        let e = LocalTokenAuthenticator::from_token_file(&path).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(!e.to_string().contains("not a token"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_token_file_others_can_read() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        LocalTokenAuthenticator::from_token_file(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let e = LocalTokenAuthenticator::from_token_file(&path).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn the_authorizer_allows_only_the_owner() {
        let action = Action::Adp(AdpCommand::new(
            "Target.createTarget",
            CommandScope::Connection,
        ));
        let owner = Principal::new(PrincipalId(OWNER.to_owned()), InstanceScope::All);
        let d = OwnerAuthorizer
            .authorize(&owner, &action, &Resource::Server)
            .await
            .unwrap();
        assert!(d.is_allowed());

        let other = Principal::new(PrincipalId("someone".to_owned()), InstanceScope::All);
        let d = OwnerAuthorizer
            .authorize(&other, &action, &Resource::Server)
            .await
            .unwrap();
        assert!(!d.is_allowed());
        let limited = Principal::new(
            PrincipalId(OWNER.to_owned()),
            InstanceScope::Only(Default::default()),
        );
        let d = OwnerAuthorizer
            .authorize(&limited, &action, &Resource::Server)
            .await
            .unwrap();
        assert!(!d.is_allowed());
    }
}
