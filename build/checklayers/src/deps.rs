//! Crate dependency rules, checked on the `cargo metadata` graph.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::config::{
    Config, DirConfig, Layer, Special, TESTING_LAYER, TOOLING_ROOTS, UPSTREAM_LAYER,
};
use crate::{Error, Rule, Violation, concepts};

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    id: String,
    manifest_path: PathBuf,
    dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    /// The real package name (not the `rename`d alias).
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    path: Option<PathBuf>,
    /// `None` for path dependencies, else e.g. `registry+https://...` or `git+https://...`.
    source: Option<String>,
}

/// `cargo metadata` source id of crates.io (whatever protocol is used to reach it).
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// A workspace member, located in the repo.
pub struct Member {
    pub name: String,
    /// Repo-relative crate directory.
    pub dir: PathBuf,
    deps: Vec<Dependency>,
}

/// Where a repo-relative path sits in the layer model.
enum Place<'a> {
    Layer(&'a Layer),
    Tooling,
    Unlayered,
}

fn place<'a>(cfg: &'a Config, rel: &Path) -> Place<'a> {
    if let Some(l) = cfg.layer_of(rel) {
        Place::Layer(l)
    } else if TOOLING_ROOTS.iter().any(|t| rel.starts_with(t)) {
        Place::Tooling
    } else {
        Place::Unlayered
    }
}

/// Repo-relative form of an absolute path, resolving symlinks where the path exists
/// (macOS `/tmp` -> `/private/tmp`).
pub fn relativize(root: &Path, p: &Path) -> Option<PathBuf> {
    let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    p.strip_prefix(root).ok().map(Path::to_path_buf)
}

/// Runs `cargo metadata --no-deps` on the workspace at `root` (canonical).
pub fn load_members(root: &Path, cargo: &OsStr) -> Result<Vec<Member>, Error> {
    let manifest = root.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(Error(format!(
            "{} not found: the dependency rules are checked on the Cargo workspace",
            manifest.display()
        )));
    }
    let out = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .map_err(|e| Error(format!("cannot run `cargo metadata`: {e}")))?;
    if !out.status.success() {
        return Err(Error(format!(
            "`cargo metadata` failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let md: Metadata = serde_json::from_slice(&out.stdout)
        .map_err(|e| Error(format!("cannot parse `cargo metadata` output: {e}")))?;
    let members: BTreeSet<&str> = md.workspace_members.iter().map(String::as_str).collect();
    let mut v = Vec::new();
    for p in md.packages {
        if !members.contains(p.id.as_str()) {
            continue;
        }
        let dir = p
            .manifest_path
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let dir = relativize(root, &dir).ok_or_else(|| {
            Error(format!(
                "workspace member `{}` at {} is outside the repo root",
                p.name,
                dir.display()
            ))
        })?;
        v.push(Member {
            name: p.name,
            dir,
            deps: p.dependencies,
        });
    }
    v.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(v)
}

/// The rules from `LAYERS.toml` for a dependency between two *different* layers,
/// before any `LAYER.toml` narrowing. `Err((rule, reason))` if forbidden.
pub fn global_rule(from: &Layer, to: &Layer) -> Result<(), (Rule, String)> {
    debug_assert_ne!(from.name, to.name);
    if to.name == TESTING_LAYER {
        return Err((
            Rule::DepOnTesting,
            "no code outside testing may depend on testing".into(),
        ));
    }
    if from.name == TESTING_LAYER {
        return Ok(());
    }
    if from.special == Some(Special::Contracts) {
        return Err((
            Rule::ContractsSelfContained,
            "contracts is self-contained and depends on no other layer".into(),
        ));
    }
    if from.special == Some(Special::Clients) && to.special != Some(Special::Contracts) {
        return Err((
            Rule::ClientsOnlyContracts,
            "clients may depend only on contracts".into(),
        ));
    }
    if to.special == Some(Special::Clients) {
        return Err((
            Rule::DepOnClients,
            "no product code may depend on clients".into(),
        ));
    }
    if to.special == Some(Special::Contracts) {
        return Ok(());
    }
    if from.level < to.level {
        return Err((
            Rule::Upward,
            format!(
                "level {} may not depend on higher level {}",
                from.level, to.level
            ),
        ));
    }
    if from.level == to.level {
        return Err((
            Rule::SameLevel,
            format!(
                "`{}` and `{}` are siblings at level {}; cross through contracts",
                from.name, to.name, from.level
            ),
        ));
    }
    Ok(())
}

/// First path component of `p` below `base` ("" if `p == base`).
fn module_under(p: &Path, base: &Path) -> String {
    p.strip_prefix(base)
        .ok()
        .and_then(|r| r.components().next())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn dep_kind(d: &Dependency) -> &'static str {
    match d.kind.as_deref() {
        Some("dev") => "dev-dependency",
        Some("build") => "build-dependency",
        _ => "dependency",
    }
}

pub fn check(root: &Path, cfg: &Config, members: &[Member], out: &mut Vec<Violation>) {
    for m in members {
        let loc = m.dir.join("Cargo.toml").display().to_string();
        let src = match place(cfg, &m.dir) {
            Place::Layer(l) => l,
            Place::Tooling => continue,
            Place::Unlayered => {
                out.push(Violation::new(
                    Rule::UnlayeredCrate,
                    loc,
                    format!(
                        "crate `{}` is in no layer of LAYERS.toml and not under a tooling root ({})",
                        m.name,
                        TOOLING_ROOTS.join(", ")
                    ),
                ));
                continue;
            }
        };
        let chain = cfg.chain(&m.dir);
        let concepts = concepts::concepts_for(cfg, src, &chain);
        for d in &m.deps {
            let kind = dep_kind(d);
            // Forbidden concepts in dependency crate names (registry, git and path deps alike).
            let mut names = vec![d.name.as_str()];
            if let Some(r) = &d.rename {
                names.push(r.as_str());
            }
            for n in names {
                for hit in concepts::matches(&concepts, n) {
                    out.push(Violation::new(
                        Rule::ForbiddenConcept,
                        loc.clone(),
                        format!(
                            "\"{}\" in {kind} crate name `{n}` (forbidden here by {})",
                            hit.concept, hit.sources
                        ),
                    ));
                }
            }

            let Some(dpath) = &d.path else {
                if src.special == Some(Special::Contracts) {
                    check_contracts_external(cfg, d, &loc, out);
                }
                continue;
            };
            let Some(drel) = relativize(root, dpath) else {
                out.push(Violation::new(
                    Rule::OutsideRepo,
                    loc.clone(),
                    format!(
                        "{kind} `{}` ({}): path dependency outside the repo cannot be placed in a layer",
                        d.name,
                        dpath.display()
                    ),
                ));
                continue;
            };
            let dloc = format!("{kind} `{}` ({})", d.name, drel.display());
            let dst = match place(cfg, &drel) {
                Place::Layer(l) => l,
                Place::Tooling | Place::Unlayered => {
                    out.push(Violation::new(
                        Rule::UnlayeredTarget,
                        loc.clone(),
                        format!(
                            "{dloc}: `{}` is not in any layer; layer code may depend only on layers",
                            drel.display()
                        ),
                    ));
                    continue;
                }
            };
            let edge = Edge {
                src,
                src_dir: &m.dir,
                dst,
                dst_dir: &drel,
                loc: &loc,
                dloc: &dloc,
            };
            check_edge(cfg, &edge, &chain, out);
        }
    }
}

/// A non-path dependency of a contracts crate: allowed only for a crates.io crate whose
/// real package name is in `LAYERS.toml [contracts] allow_external`.
fn check_contracts_external(cfg: &Config, d: &Dependency, loc: &str, out: &mut Vec<Violation>) {
    let kind = dep_kind(d);
    let source = d.source.as_deref().unwrap_or("unknown source");
    let listed = cfg.contracts_allow_external.contains(&d.name);
    let why = if !listed {
        "is not in LAYERS.toml [contracts] allow_external".to_string()
    } else if d.source.as_deref() != Some(CRATES_IO) {
        format!("comes from {source}; allow_external covers crates.io crates only")
    } else {
        return;
    };
    out.push(Violation::new(
        Rule::ContractsExternal,
        loc.to_string(),
        format!("{kind} on external crate `{}` ({source}) {why}", d.name),
    ));
}

/// One path dependency from a layer crate to a layer crate.
struct Edge<'a> {
    src: &'a Layer,
    src_dir: &'a Path,
    dst: &'a Layer,
    dst_dir: &'a Path,
    /// Reported location (the depending crate's Cargo.toml).
    loc: &'a str,
    /// Human-readable description of the dependency.
    dloc: &'a str,
}

fn check_edge(cfg: &Config, edge: &Edge<'_>, chain: &[&DirConfig], out: &mut Vec<Violation>) {
    let Edge {
        src,
        src_dir,
        dst,
        dst_dir,
        loc,
        dloc,
    } = *edge;
    if src.name == dst.name {
        // Same layer: only within one first-level module, or to an `allow_siblings` module.
        let (a, b) = (
            module_under(src_dir, &src.path),
            module_under(dst_dir, &src.path),
        );
        if a == b {
            return;
        }
        let root_cfg = cfg.layer_root_config(src);
        if !b.is_empty() && root_cfg.is_some_and(|c| c.allow_siblings.contains(&b)) {
            return;
        }
        let show = |m: &str| {
            if m.is_empty() {
                "(layer root)".to_string()
            } else {
                m.to_string()
            }
        };
        out.push(Violation::new(
            Rule::SiblingModule,
            loc.to_string(),
            format!(
                "{dloc}: `{}` -> `{}` are sibling modules of layer `{}` ({}) and may not depend on each other directly (`{}` is not in allow_siblings of {}/LAYER.toml)",
                show(&a),
                show(&b),
                src.name,
                src.path.display(),
                show(&b),
                src.path.display()
            ),
        ));
        return;
    }
    if let Err((rule, why)) = global_rule(src, dst) {
        out.push(Violation::new(
            rule,
            loc.to_string(),
            format!("{dloc}: `{}` -> `{}`: {why}", src.name, dst.name),
        ));
        return;
    }
    for c in chain {
        let Some(allow) = &c.allow else { continue };
        if !allow.iter().any(|a| a == "*" || *a == dst.name) {
            out.push(Violation::new(
                Rule::NotAllowed,
                loc.to_string(),
                format!(
                    "{dloc}: `{}` -> `{}` is not in the allow list of {} ({:?})",
                    src.name,
                    dst.name,
                    c.file.display(),
                    allow
                ),
            ));
        }
    }
}

/// `Cargo.toml` files inside a layer that are not workspace members escape every rule above.
pub fn check_coverage(
    cfg: &Config,
    members: &[Member],
    manifests: &[PathBuf],
    out: &mut Vec<Violation>,
) {
    let member_dirs: BTreeSet<&Path> = members.iter().map(|m| m.dir.as_path()).collect();
    for rel in manifests {
        let Some(layer) = cfg.layer_of(rel) else {
            continue;
        };
        if layer.name == UPSTREAM_LAYER {
            continue;
        }
        let dir = rel.parent().unwrap_or(Path::new(""));
        if !member_dirs.contains(dir) {
            out.push(Violation::new(
                Rule::NotInWorkspace,
                rel.display().to_string(),
                "crate is not a member of the root Cargo workspace, so its dependencies are not checked; add it to [workspace] members".into(),
            ));
        }
    }
}
