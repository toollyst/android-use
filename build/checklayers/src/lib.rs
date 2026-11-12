//! `cargo xtask check-layers`: enforces `LAYERS.toml` and the per-directory `LAYER.toml` files.
//!
//! Two checks, both failing the run on any violation:
//!
//! 1. **Dependency rules** on the Cargo workspace graph (`cargo metadata --no-deps`). Every
//!    workspace member is mapped to a layer by the `path` fields of `LAYERS.toml`; every path
//!    dependency is checked against the global rules and then against the `allow` lists of
//!    the `LAYER.toml` files from the layer root down to the crate (each can only narrow).
//! 2. **Forbidden concepts** (`[forbidden]` + `forbid_concepts`) in identifiers, string
//!    literals, file and directory names and dependency crate names, case-insensitive and
//!    word-level. Comments and `*.md` files are not checked.
//!
//! The rules themselves live in `LAYERS.toml`, the `LAYER.toml` files and
//! `docs/02-architecture.md` §2; this crate only enforces them.

mod concepts;
pub mod config;
mod deps;
pub mod lexer;
pub mod words;

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use config::Config;

/// A fatal problem: the check could not run (unreadable `LAYERS.toml`, `cargo metadata`
/// failing, ...). Distinct from violations so callers never mistake it for a clean run.
#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    /// Lower level depends on a higher level.
    Upward,
    /// Two different layers at the same level.
    SameLevel,
    /// `contracts` depends on another layer.
    ContractsSelfContained,
    /// `clients` depends on something other than `contracts`.
    ClientsOnlyContracts,
    /// Product code depends on `clients`.
    DepOnClients,
    /// Anything outside `testing` depends on `testing`.
    DepOnTesting,
    /// Globally allowed, but excluded by a `LAYER.toml` `allow` list.
    NotAllowed,
    /// Sibling modules under a `forbid_siblings = true` directory.
    ForbidSiblings,
    /// Layer crate depends on a repo path that is in no layer (e.g. `build/`).
    UnlayeredTarget,
    /// Layer crate has a path dependency outside the repo.
    OutsideRepo,
    /// Workspace member in no layer and not under a tooling root.
    UnlayeredCrate,
    /// `Cargo.toml` inside a layer that is not a workspace member.
    NotInWorkspace,
    /// Forbidden concept found.
    ForbiddenConcept,
    /// A `LAYER.toml` tries to widen the global rules.
    ConfigWidens,
    /// A `LAYER.toml` is malformed (unknown key, unknown layer, wrong name, ...).
    ConfigInvalid,
    /// A file inside a layer could not be read.
    Unreadable,
}

impl Rule {
    pub fn id(self) -> &'static str {
        match self {
            Rule::Upward => "dep/upward",
            Rule::SameLevel => "dep/same-level",
            Rule::ContractsSelfContained => "dep/contracts-self-contained",
            Rule::ClientsOnlyContracts => "dep/clients-only-contracts",
            Rule::DepOnClients => "dep/on-clients",
            Rule::DepOnTesting => "dep/on-testing",
            Rule::NotAllowed => "dep/layer-toml-allow",
            Rule::ForbidSiblings => "dep/forbid-siblings",
            Rule::UnlayeredTarget => "dep/unlayered-target",
            Rule::OutsideRepo => "dep/outside-repo",
            Rule::UnlayeredCrate => "crate/unlayered",
            Rule::NotInWorkspace => "crate/not-in-workspace",
            Rule::ForbiddenConcept => "concept/forbidden",
            Rule::ConfigWidens => "config/widens",
            Rule::ConfigInvalid => "config/invalid",
            Rule::Unreadable => "io/unreadable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Repo-relative path, optionally `:line`.
    pub location: String,
    pub rule: Rule,
    pub message: String,
}

impl Violation {
    pub fn new(rule: Rule, location: String, message: String) -> Self {
        Violation {
            location,
            rule,
            message,
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.location, self.rule.id(), self.message)
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub crates_checked: usize,
    pub files_scanned: usize,
    /// Files whose content was not checked because it is not UTF-8 text (names still are).
    pub skipped_binary: Vec<String>,
}

#[derive(Debug)]
pub struct Report {
    pub violations: Vec<Violation>,
    pub stats: Stats,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// One file or directory found inside a layer.
pub struct Entry {
    /// Repo-relative path.
    pub rel: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
}

/// Runs every check on the repo at `root`, using `cargo` from `$CARGO` (set when run
/// through cargo) or `PATH`.
pub fn check(root: &Path) -> Result<Report, Error> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    check_with_cargo(root, &cargo)
}

pub fn check_with_cargo(root: &Path, cargo: &OsStr) -> Result<Report, Error> {
    let root = root
        .canonicalize()
        .map_err(|e| Error(format!("cannot resolve repo root {}: {e}", root.display())))?;
    let (layers, forbidden) = config::load_layers(&root)?;
    for (key, list) in &forbidden {
        for c in list {
            if words::split_words(c).is_empty() {
                return Err(Error(format!(
                    "LAYERS.toml: [forbidden].\"{key}\" entry `{c}` contains no word"
                )));
            }
        }
    }
    let mut cfg = Config {
        layers,
        forbidden,
        dirs: Vec::new(),
    };
    let mut violations = Vec::new();
    let mut stats = Stats::default();

    for l in &cfg.layers {
        if !root.join(&l.path).is_dir() {
            violations.push(Violation::new(
                Rule::ConfigInvalid,
                "LAYERS.toml".into(),
                format!(
                    "layer `{}` path `{}` is not a directory",
                    l.name,
                    l.path.display()
                ),
            ));
        }
    }

    let entries = walk(&root, &cfg)?;
    let layer_tomls: Vec<PathBuf> = entries
        .iter()
        .filter(|e| !e.is_dir && e.rel.file_name() == Some(OsStr::new("LAYER.toml")))
        .map(|e| e.rel.clone())
        .collect();
    cfg.load_dirs(&root, &layer_tomls, &mut violations);

    let members = deps::load_members(&root, cargo)?;
    stats.crates_checked = members.len();
    deps::check(&root, &cfg, &members, &mut violations);
    let manifests: Vec<PathBuf> = entries
        .iter()
        .filter(|e| !e.is_dir && e.rel.file_name() == Some(OsStr::new("Cargo.toml")))
        .map(|e| e.rel.clone())
        .collect();
    deps::check_coverage(&cfg, &members, &manifests, &mut violations);

    for e in &entries {
        let Some(layer) = cfg.layer_of(&e.rel) else {
            continue;
        };
        if e.rel == layer.path {
            continue;
        }
        concepts::check_entry(&root, &cfg, layer, e, &mut stats, &mut violations);
    }

    violations.sort();
    violations.dedup();
    Ok(Report { violations, stats })
}

/// Walks every layer directory. `.gitignore` files are honoured (build outputs are not
/// source); hidden files are included; `.git` is skipped; symlinks are not followed.
fn walk(root: &Path, cfg: &Config) -> Result<Vec<Entry>, Error> {
    let layer_paths: Vec<PathBuf> = cfg.layers.iter().map(|l| l.path.clone()).collect();
    let root_owned = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .require_git(false)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(move |e| {
            if e.file_name() == ".git" {
                return false;
            }
            let Ok(rel) = e.path().strip_prefix(&root_owned) else {
                return false;
            };
            // Keep the root, ancestors of layer dirs (e.g. `host/`), and anything inside a layer.
            layer_paths
                .iter()
                .any(|lp| lp.starts_with(rel) || rel.starts_with(lp))
        });
    let mut out = Vec::new();
    for r in builder.build() {
        let e = r.map_err(|e| Error(format!("walking the repo failed: {e}")))?;
        let Ok(rel) = e.path().strip_prefix(root) else {
            continue;
        };
        if cfg.layer_of(rel).is_none() {
            continue;
        }
        let ft = e.file_type();
        out.push(Entry {
            rel: rel.to_path_buf(),
            is_dir: ft.is_some_and(|t| t.is_dir()),
            is_symlink: ft.is_some_and(|t| t.is_symlink()),
        });
    }
    Ok(out)
}
