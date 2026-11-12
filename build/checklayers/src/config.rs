//! Loading and validating `LAYERS.toml` and the per-directory `LAYER.toml` files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::words::split_words;
use crate::{Error, Rule, Violation};

/// Name of the layer that holds upstream code. Its crates are not workspace members
/// (root `Cargo.toml` excludes it), so the "every crate is a member" check skips it.
pub const UPSTREAM_LAYER: &str = "third_party";

/// Name of the test layer. `LAYERS.toml` gives it no `special` marker; the rule
/// "product code must not depend on testing" comes from its comment in `LAYERS.toml`,
/// `testing/LAYER.toml` and `docs/02-architecture.md` §2 rule 4.
pub const TESTING_LAYER: &str = "testing";

/// Top-level directories that hold repo tooling. They are not layers: crates there are not
/// subject to the layer rules, and no layer crate may depend on them.
pub const TOOLING_ROOTS: &[&str] = &["build", "tools"];

/// Location used for violations found in `LAYERS.toml`.
const LAYERS_TOML: &str = "LAYERS.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    /// Usable by every layer; depends on no other layer.
    Contracts,
    /// Depends only on contracts; no product code depends on it.
    Clients,
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    /// Repo-relative, normalized.
    pub path: PathBuf,
    pub level: i64,
    pub special: Option<Special>,
}

/// One parsed `LAYER.toml`.
#[derive(Debug, Clone)]
pub struct DirConfig {
    /// Repo-relative path of the `LAYER.toml` file itself.
    pub file: PathBuf,
    /// Repo-relative directory the file governs.
    pub dir: PathBuf,
    /// `None` = key absent (no narrowing). `Some` = only these target layers ("*" = any).
    pub allow: Option<Vec<String>>,
    /// Validated `allow_siblings`: first-level modules of the layer that every module of
    /// the layer may depend on. Only ever non-empty in a layer-root `LAYER.toml`.
    pub allow_siblings: Vec<String>,
    pub forbid_concepts: Vec<String>,
}

/// `LAYERS.toml [forbidden]`: key is a layer name or "*", value the forbidden concepts.
pub type Forbidden = BTreeMap<String, Vec<String>>;

/// The parsed `LAYERS.toml`. The exception lists are raw here; [`Config::validate_globals`]
/// checks them once every `LAYER.toml` (and so every forbidden concept) is known.
#[derive(Debug)]
pub struct LayersFile {
    pub layers: Vec<Layer>,
    pub forbidden: Forbidden,
    /// `[forbidden_exceptions]`: concept -> words exempt from its prefix match.
    pub forbidden_exceptions: BTreeMap<String, Vec<String>>,
    /// `[contracts] allow_external`: registry crates contracts crates may depend on.
    pub contracts_allow_external: Vec<String>,
}

#[derive(Debug)]
pub struct Config {
    pub layers: Vec<Layer>,
    pub forbidden: Forbidden,
    pub dirs: Vec<DirConfig>,
    /// Validated `[forbidden_exceptions]`: concept key (see [`concept_key`]) -> exact words.
    pub concept_exceptions: BTreeMap<String, BTreeSet<String>>,
    /// Validated `[contracts] allow_external`.
    pub contracts_allow_external: BTreeSet<String>,
    raw_exceptions: BTreeMap<String, Vec<String>>,
    raw_allow_external: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayers {
    layer: Vec<RawLayer>,
    #[serde(default)]
    forbidden: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    forbidden_exceptions: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    contracts: RawContracts,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayer {
    name: String,
    path: String,
    level: i64,
    special: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawContracts {
    #[serde(default)]
    allow_external: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayerToml {
    layer: RawLayerSection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLayerSection {
    name: Option<String>,
    allow: Option<Vec<String>>,
    allow_siblings: Option<Vec<String>>,
    #[serde(default)]
    forbid_concepts: Vec<String>,
}

/// Normalized form of a forbidden concept: its words joined by one space
/// (`package_name`, `PackageName` -> "package name"). Empty if it has no word.
pub fn concept_key(raw: &str) -> String {
    split_words(raw).join(" ")
}

/// Normalizes a repo-relative path written in a config file ("./a/b/" -> "a/b").
/// Returns `None` for absolute paths or paths that leave the repo.
fn normalize_rel(p: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in Path::new(p).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// A plain crate name: ASCII letters, digits, `-`, `_`. No wildcards.
fn is_crate_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parses `LAYERS.toml`. Structural problems are fatal: without a valid layer table no
/// other check is meaningful.
pub fn load_layers(root: &Path) -> Result<LayersFile, Error> {
    let file = root.join(LAYERS_TOML);
    let text = fs::read_to_string(&file)
        .map_err(|e| Error(format!("cannot read {}: {e}", file.display())))?;
    let raw: RawLayers = toml::from_str(&text).map_err(|e| Error(format!("LAYERS.toml: {e}")))?;

    let mut layers: Vec<Layer> = Vec::new();
    for l in raw.layer {
        if layers.iter().any(|x| x.name == l.name) {
            return Err(Error(format!(
                "LAYERS.toml: duplicate layer name `{}`",
                l.name
            )));
        }
        let path = normalize_rel(&l.path).ok_or_else(|| {
            Error(format!(
                "LAYERS.toml: layer `{}` has invalid path `{}` (must be repo-relative)",
                l.name, l.path
            ))
        })?;
        let special = match l.special.as_deref() {
            None => None,
            Some("contracts") => Some(Special::Contracts),
            Some("clients") => Some(Special::Clients),
            Some(other) => {
                return Err(Error(format!(
                    "LAYERS.toml: layer `{}` has unknown special `{other}`",
                    l.name
                )));
            }
        };
        layers.push(Layer {
            name: l.name,
            path,
            level: l.level,
            special,
        });
    }
    for a in &layers {
        for b in &layers {
            if a.name != b.name && a.path.starts_with(&b.path) {
                return Err(Error(format!(
                    "LAYERS.toml: layer `{}` ({}) is nested inside layer `{}` ({}); nested layers are not supported",
                    a.name,
                    a.path.display(),
                    b.name,
                    b.path.display()
                )));
            }
        }
    }
    for key in raw.forbidden.keys() {
        if key != "*" && !layers.iter().any(|l| &l.name == key) {
            return Err(Error(format!(
                "LAYERS.toml: [forbidden] key `{key}` is neither \"*\" nor a layer name"
            )));
        }
    }
    Ok(LayersFile {
        layers,
        forbidden: raw.forbidden,
        forbidden_exceptions: raw.forbidden_exceptions,
        contracts_allow_external: raw.contracts.allow_external,
    })
}

impl Config {
    pub fn new(file: LayersFile) -> Config {
        Config {
            layers: file.layers,
            forbidden: file.forbidden,
            dirs: Vec::new(),
            concept_exceptions: BTreeMap::new(),
            contracts_allow_external: BTreeSet::new(),
            raw_exceptions: file.forbidden_exceptions,
            raw_allow_external: file.contracts_allow_external,
        }
    }

    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// The layer containing a repo-relative path, if any.
    pub fn layer_of(&self, rel: &Path) -> Option<&Layer> {
        self.layers.iter().find(|l| rel.starts_with(&l.path))
    }

    /// `LAYER.toml` files governing `dir` (repo-relative directory), outermost first.
    pub fn chain(&self, dir: &Path) -> Vec<&DirConfig> {
        let mut v: Vec<&DirConfig> = self
            .dirs
            .iter()
            .filter(|d| dir.starts_with(&d.dir))
            .collect();
        v.sort_by_key(|d| d.dir.components().count());
        v
    }

    /// The layer-root `LAYER.toml` of `layer`, if it exists and parsed.
    pub fn layer_root_config(&self, layer: &Layer) -> Option<&DirConfig> {
        self.dirs.iter().find(|d| d.dir == layer.path)
    }

    /// Parses every `LAYER.toml` found by the walker and validates it against the global rules.
    /// Problems are reported as violations; a file that cannot be parsed is skipped after
    /// reporting (its narrowing would otherwise be silently lost, so the run still fails).
    pub fn load_dirs(&mut self, root: &Path, files: &[PathBuf], out: &mut Vec<Violation>) {
        for rel in files {
            let loc = rel.display().to_string();
            let Some(layer) = self.layer_of(rel).cloned() else {
                continue;
            };
            let dir = rel.parent().map(Path::to_path_buf).unwrap_or_default();
            let text = match fs::read_to_string(root.join(rel)) {
                Ok(t) => t,
                Err(e) => {
                    out.push(Violation::new(
                        Rule::ConfigInvalid,
                        loc,
                        format!("cannot read: {e}"),
                    ));
                    continue;
                }
            };
            let raw: RawLayerToml = match toml::from_str(&text) {
                Ok(r) => r,
                Err(e) => {
                    out.push(Violation::new(
                        Rule::ConfigInvalid,
                        loc,
                        format!("invalid LAYER.toml: {}", e.message()),
                    ));
                    continue;
                }
            };
            let s = raw.layer;
            if dir == layer.path {
                match &s.name {
                    Some(n) if *n == layer.name => {}
                    Some(n) => out.push(Violation::new(
                        Rule::ConfigInvalid,
                        loc.clone(),
                        format!(
                            "[layer] name = \"{n}\" but LAYERS.toml maps this directory to layer `{}`",
                            layer.name
                        ),
                    )),
                    None => out.push(Violation::new(
                        Rule::ConfigInvalid,
                        loc.clone(),
                        format!("[layer] name missing (expected \"{}\")", layer.name),
                    )),
                }
            }
            if let Some(allow) = &s.allow {
                for entry in allow {
                    if entry == "*" || *entry == layer.name {
                        continue;
                    }
                    let Some(target) = self.layer(entry) else {
                        out.push(Violation::new(
                            Rule::ConfigInvalid,
                            loc.clone(),
                            format!("allow entry `{entry}` is not a layer in LAYERS.toml"),
                        ));
                        continue;
                    };
                    if let Err((_, why)) = crate::deps::global_rule(&layer, target) {
                        out.push(Violation::new(
                            Rule::ConfigWidens,
                            loc.clone(),
                            format!(
                                "allow entry `{entry}` widens the LAYERS.toml rules (LAYER.toml may only narrow): {why}"
                            ),
                        ));
                    }
                }
            }
            let allow_siblings =
                validate_allow_siblings(root, &layer, &dir, s.allow_siblings, &loc, out);
            for c in &s.forbid_concepts {
                if split_words(c).is_empty() {
                    out.push(Violation::new(
                        Rule::ConfigInvalid,
                        loc.clone(),
                        format!("forbid_concepts entry `{c}` contains no word"),
                    ));
                }
            }
            self.dirs.push(DirConfig {
                file: rel.clone(),
                dir,
                allow: s.allow,
                allow_siblings,
                forbid_concepts: s.forbid_concepts,
            });
        }
    }

    /// Validates the `LAYERS.toml` exception lists (`[forbidden_exceptions]`,
    /// `[contracts] allow_external`). Must run after [`Config::load_dirs`]: an exception key
    /// may name a concept declared only in a `LAYER.toml`. Invalid entries are reported and
    /// not applied.
    pub fn validate_globals(&mut self, out: &mut Vec<Violation>) {
        let known: BTreeSet<String> = self
            .forbidden
            .values()
            .flatten()
            .chain(self.dirs.iter().flat_map(|d| d.forbid_concepts.iter()))
            .map(|c| concept_key(c))
            .filter(|k| !k.is_empty())
            .collect();
        let bad = |rule: Rule, msg: String| Violation::new(rule, LAYERS_TOML.into(), msg);

        for (key, entries) in std::mem::take(&mut self.raw_exceptions) {
            let k = concept_key(&key);
            if !known.contains(&k) {
                out.push(bad(
                    Rule::ConfigInvalid,
                    format!(
                        "[forbidden_exceptions] key `{key}` is not a concept forbidden by [forbidden] or any LAYER.toml forbid_concepts"
                    ),
                ));
                continue;
            }
            let last = k.rsplit(' ').next().unwrap_or_default().to_string();
            for e in entries {
                let words = split_words(&e);
                if !e.chars().all(char::is_alphanumeric) || words.len() != 1 {
                    out.push(bad(
                        Rule::ConfigInvalid,
                        format!(
                            "[forbidden_exceptions].\"{key}\" entry `{e}` is not a single word (exceptions are exact single words)"
                        ),
                    ));
                    continue;
                }
                let w = words.into_iter().next().unwrap_or_default();
                if w == last {
                    out.push(bad(
                        Rule::ConfigWidens,
                        format!(
                            "[forbidden_exceptions].\"{key}\" entry `{e}` is the forbidden word itself and would disable the concept"
                        ),
                    ));
                } else if !w.starts_with(&last) {
                    out.push(bad(
                        Rule::ConfigInvalid,
                        format!(
                            "[forbidden_exceptions].\"{key}\" entry `{e}` does not start with `{last}`; only words that extend the concept's last word can be excepted"
                        ),
                    ));
                } else {
                    self.concept_exceptions
                        .entry(k.clone())
                        .or_default()
                        .insert(w);
                }
            }
        }

        for e in std::mem::take(&mut self.raw_allow_external) {
            if is_crate_name(&e) {
                self.contracts_allow_external.insert(e);
            } else {
                out.push(bad(
                    Rule::ConfigInvalid,
                    format!(
                        "[contracts] allow_external entry `{e}` is not a crate name (no wildcards)"
                    ),
                ));
            }
        }
    }
}

/// `allow_siblings` is an exception to the sibling-module rule, so only the layer-root
/// `LAYER.toml` may declare it, and each entry must be an existing first-level module
/// directory of the layer. Returns the valid entries.
fn validate_allow_siblings(
    root: &Path,
    layer: &Layer,
    dir: &Path,
    entries: Option<Vec<String>>,
    loc: &str,
    out: &mut Vec<Violation>,
) -> Vec<String> {
    let Some(entries) = entries else {
        return Vec::new();
    };
    if dir != layer.path {
        out.push(Violation::new(
            Rule::ConfigWidens,
            loc.to_string(),
            format!(
                "allow_siblings is only valid in the layer-root {}/LAYER.toml; a nested LAYER.toml may only narrow",
                layer.path.display()
            ),
        ));
        return Vec::new();
    }
    let mut valid = Vec::new();
    for e in entries {
        let single =
            !e.is_empty() && e != "." && e != ".." && !e.contains(['/', '\\', '*', '?', '[', ']']);
        let is_module = single
            && fs::symlink_metadata(root.join(&layer.path).join(&e)).is_ok_and(|m| m.is_dir());
        if is_module {
            valid.push(e);
        } else {
            out.push(Violation::new(
                Rule::ConfigInvalid,
                loc.to_string(),
                format!(
                    "allow_siblings entry `{e}` is not a first-level module directory of layer `{}` ({})",
                    layer.name,
                    layer.path.display()
                ),
            ));
        }
    }
    valid
}
