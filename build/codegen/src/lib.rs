//! `cargo xtask codegen [--check]`: generates the contract code committed under
//! `contracts/<x>/src/generated/` and checks that the committed code is up to date.
//!
//! - Protocol Buffers contracts (`[[proto]]` in `build/codegen/codegen.toml`): the pinned
//!   official protoc (versions.toml `[protoc]`, verified by sha256, [`protoc`]) compiles every
//!   `.proto` under the proto root to a FileDescriptorSet, and prost-build turns it into
//!   message types ([`proto`]). No service bindings.
//! - ADP (`[adp]`): `protocol.json` is validated and turned into serde types ([`adp`]).
//!
//! Every generated file starts with a two-line header: the source and the sha256 of the body.
//! `--check` regenerates in memory and compares with the committed files (line endings
//! normalized): a differing file is *hand-edited* when its body no longer matches its own
//! hash, else *stale*; expected files that do not exist are *missing*; any other file in a
//! generated directory is an *orphan*. Writing replaces stale files and removes orphans.

pub mod adp;
pub mod config;
pub mod output;
pub mod proto;
pub mod protoc;

use std::fmt;
use std::path::Path;

pub use output::{Rule, Violation};

/// The generator config, relative to the repo root.
pub const CONFIG_PATH: &str = "build/codegen/codegen.toml";

/// What stopped a run. Every error maps to exit status 2 (the check could not run).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// `build/codegen/codegen.toml` is missing or invalid, or a path in it is not allowed.
    Config,
    /// A source or output file could not be read or written.
    Io,
    /// The IDL (protocol.json, VERSION, .proto) is invalid.
    Definition,
    /// versions.toml `[protoc]` is missing or invalid, or has no entry for this host.
    ProtocPin,
    /// A protoc archive or binary does not match the pin. Never used, never cached.
    ProtocUnverified,
    /// protoc could not be obtained or run.
    Protoc,
    /// prost-build or the Rust output stage failed.
    Generate,
}

#[derive(Debug, Clone)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Write the generated files (and remove orphans).
    Write,
    /// Compare only; nothing is written.
    Check,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub mode: Mode,
    /// Run one target (`[[proto]] name` or `adp`) instead of all.
    pub only: Option<String>,
}

#[derive(Debug, Default)]
pub struct Report {
    /// `--check`: what is out of date. Empty after a write.
    pub violations: Vec<Violation>,
    /// Number of generated files compared or written.
    pub files_checked: usize,
    /// Write: repo-relative paths of files created or changed.
    pub written: Vec<String>,
    /// Write: repo-relative paths of orphan files and directories removed.
    pub removed: Vec<String>,
}

/// Runs the generator over the repo at `root`.
pub fn run(root: &Path, opts: &Options) -> Result<Report, Error> {
    let cfg = config::load(root)?;
    if let Some(only) = &opts.only
        && !cfg.targets().any(|t| t == only)
    {
        return Err(Error::new(
            ErrorKind::Config,
            format!("{CONFIG_PATH}: no target named `{only}`"),
        ));
    }
    let wanted = |name: &str| opts.only.as_deref().is_none_or(|o| o == name);

    let mut dirs = Vec::new();
    let protos: Vec<&config::Proto> = cfg.proto.iter().filter(|p| wanted(&p.name)).collect();
    if !protos.is_empty() {
        let protoc = protoc::acquire(root)?;
        for p in protos {
            dirs.push(proto::generate(root, p, &protoc)?);
        }
    }
    if let Some(a) = &cfg.adp
        && wanted("adp")
    {
        dirs.push(adp::generate_target(root, a)?);
    }
    output::apply(root, &dirs, opts.mode)
}
