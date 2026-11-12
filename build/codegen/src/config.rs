//! `build/codegen/codegen.toml`. Unknown keys are errors; every path is repo-relative,
//! written with `/`, without `.`/`..` components, and every output directory is a
//! `contracts/<x>/src/generated` directory of its own.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::adp::{IntegerDefault, IntegerWidth, Settings};
use crate::{CONFIG_PATH, Error, ErrorKind};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub proto: Vec<Proto>,
    #[serde(default)]
    pub adp: Option<Adp>,
}

/// One Protocol Buffers contract.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proto {
    /// Target name (`--only`), e.g. `guest_link`.
    pub name: String,
    /// The proto root (`-I`); every `.proto` below it is compiled.
    pub root: String,
    /// The generated directory.
    pub out: String,
    /// prost-build `Config::bytes` paths.
    #[serde(default)]
    pub bytes: Vec<String>,
    /// prost-build `Config::btree_map` paths.
    #[serde(default)]
    pub btree_map: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adp {
    /// protocol.json.
    pub definition: String,
    /// The VERSION file (single source of the protocol version).
    pub version: String,
    /// The generated directory.
    pub out: String,
    #[serde(default = "default_integer")]
    pub integer_without_format: IntegerDefault,
    #[serde(default = "default_true")]
    pub deny_unknown_fields: bool,
    /// Per-property width of an `integer` without `format`.
    #[serde(default)]
    pub integer: BTreeMap<String, IntegerWidth>,
    /// The string enum whose values `targetTypes` lists.
    #[serde(default = "default_target_type")]
    pub target_type: String,
}

fn default_target_type() -> String {
    Settings::default().target_type
}

fn default_integer() -> IntegerDefault {
    Settings::default().integer_without_format
}

fn default_true() -> bool {
    true
}

impl Adp {
    pub fn settings(&self) -> Settings {
        Settings {
            integer_without_format: self.integer_without_format,
            deny_unknown_fields: self.deny_unknown_fields,
            integer: self.integer.clone(),
            target_type: self.target_type.clone(),
        }
    }
}

impl Config {
    /// Target names in run order: the proto targets, then `adp`.
    pub fn targets(&self) -> impl Iterator<Item = &str> {
        self.proto
            .iter()
            .map(|p| p.name.as_str())
            .chain(self.adp.as_ref().map(|_| "adp"))
    }
}

fn err(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::Config, format!("{CONFIG_PATH}: {}", msg.into()))
}

/// Loads and validates the config of the repo at `root`.
pub fn load(root: &Path) -> Result<Config, Error> {
    let path = root.join(CONFIG_PATH);
    let text = fs::read_to_string(&path).map_err(|e| err(format!("cannot read: {e}")))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| err(e.to_string()))?;
    validate(&cfg)?;
    Ok(cfg)
}

/// A repo-relative path: `/`-separated, non-empty components, no `.`/`..`, not absolute.
pub(crate) fn check_rel(key: &str, p: &str) -> Result<(), Error> {
    let ok = !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..");
    if ok {
        Ok(())
    } else {
        Err(err(format!(
            "{key} = {p:?}: must be a repo-relative path with `/` separators and no `.`/`..`"
        )))
    }
}

fn check_out(key: &str, p: &str) -> Result<(), Error> {
    check_rel(key, p)?;
    let parts: Vec<&str> = p.split('/').collect();
    if parts.len() == 4 && parts[0] == "contracts" && parts[2] == "src" && parts[3] == "generated" {
        Ok(())
    } else {
        Err(err(format!(
            "{key} = {p:?}: an output directory must be contracts/<x>/src/generated"
        )))
    }
}

fn validate(cfg: &Config) -> Result<(), Error> {
    let mut names = BTreeSet::new();
    let mut outs = BTreeSet::new();
    for p in &cfg.proto {
        let key = format!("[[proto]] {:?}", p.name);
        if p.name.is_empty() || p.name == "adp" || !names.insert(p.name.clone()) {
            return Err(err(format!(
                "{key}: target names must be unique, non-empty and not `adp`"
            )));
        }
        check_rel(&format!("{key} root"), &p.root)?;
        check_out(&format!("{key} out"), &p.out)?;
        if !outs.insert(p.out.clone()) {
            return Err(err(format!(
                "{key}: output directory {} is used twice",
                p.out
            )));
        }
        for path in p.bytes.iter().chain(&p.btree_map) {
            if !(path == "." || path.starts_with('.')) {
                return Err(err(format!(
                    "{key}: prost path {path:?} must be fully qualified (start with `.`)"
                )));
            }
        }
    }
    if let Some(a) = &cfg.adp {
        check_rel("[adp] definition", &a.definition)?;
        check_rel("[adp] version", &a.version)?;
        check_out("[adp] out", &a.out)?;
        if !outs.insert(a.out.clone()) {
            return Err(err(format!(
                "[adp]: output directory {} is used twice",
                a.out
            )));
        }
    }
    if cfg.proto.is_empty() && cfg.adp.is_none() {
        return Err(err("no targets"));
    }
    Ok(())
}
