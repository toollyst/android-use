//! `third_party/patches.toml`: the patch-lint policy.
//!
//! Every key is optional; an omitted key takes the quality-first default below. Unknown keys
//! and malformed values are configuration errors (the lint does not run, exit 2).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::{Error, LAYERS, UPSTREAM_ROOT};

/// Repo-relative path of the policy file.
pub const CONFIG_FILE: &str = "third_party/patches.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub budget: Budget,
    pub header: Header,
    pub marker: Marker,
    pub rules: Rules,
    pub readme: Readme,
    /// Per `third_party/<x>`: its `Layer:`, `series` format and optional budget cap.
    pub upstream: BTreeMap<String, Upstream>,
}

impl Default for Config {
    fn default() -> Self {
        let up = |layer: &str, series| Upstream {
            layer: layer.into(),
            series,
            cap: None,
        };
        Config {
            budget: Budget::default(),
            header: Header::default(),
            marker: Marker::default(),
            rules: Rules::default(),
            readme: Readme::default(),
            upstream: BTreeMap::from([
                ("wick".into(), up("vmm", SeriesFormat::File)),
                ("aosp".into(), up("guest", SeriesFormat::ProjectFile)),
                ("kernel".into(), up("guest", SeriesFormat::File)),
            ]),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Budget {
    /// Largest `Budget:` a patch may declare without an approved overrun.
    pub cap: u32,
    /// What counts toward a patch's changed lines.
    pub count: Count,
    /// How a `Budget:` above the cap can be approved.
    pub overrun: Overrun,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            cap: 30,
            count: Count::AddedRemoved,
            overrun: Overrun::Owners,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Count {
    #[serde(rename = "added+removed")]
    AddedRemoved,
    #[serde(rename = "added")]
    Added,
    #[serde(rename = "max")]
    Max,
}

impl Count {
    pub fn name(self) -> &'static str {
        match self {
            Count::AddedRemoved => "added+removed",
            Count::Added => "added",
            Count::Max => "max(added, removed)",
        }
    }

    pub fn apply(self, added: usize, removed: usize) -> usize {
        match self {
            Count::AddedRemoved => added + removed,
            Count::Added => added,
            Count::Max => added.max(removed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Overrun {
    /// `Budget-approved-by: <login>` naming an owner of the patch in OWNERS.
    #[serde(rename = "owners")]
    Owners,
    /// No `Budget:` above the cap at all.
    #[serde(rename = "forbidden")]
    Forbidden,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Header {
    /// Also accept `[PATCH n/m]`, `[PATCH vN]`, `[PATCH vN n/m]` before `droiduse: `.
    pub numbered_subject: bool,
    /// `From:` must be one valid mailbox (`Name <user@host>` or `user@host`).
    pub from_mailbox: bool,
}

impl Default for Header {
    fn default() -> Self {
        Header {
            numbered_subject: false,
            from_mailbox: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Marker {
    /// Hunks that add no line (removal-only hunks, file deletions).
    pub removal_only: RemovalOnly,
}

impl Default for Marker {
    fn default() -> Self {
        Marker {
            removal_only: RemovalOnly::Fail,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum RemovalOnly {
    /// They cannot carry a marker, so they fail.
    #[serde(rename = "fail")]
    Fail,
    /// They need no marker.
    #[serde(rename = "allow")]
    Allow,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Rules {
    /// File name `NNNN-droiduse-<name>.patch` (`<name>`: lowercase kebab-case).
    pub file_name: bool,
    /// `series` lists every patch exactly once and nothing else.
    pub series: bool,
    /// `Layer:` equals `[upstream.<x>].layer`.
    pub layer_matches_upstream: bool,
    /// `Hook-for:` exists in the repo.
    pub hook_for_exists: bool,
    /// `Scope-cell:` equals the feature cell of a table row in `docs/scope.md`.
    pub scope_cell_exists: bool,
    /// The generated patch table in each `patches/README.md` is up to date.
    pub readme: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Readme {
    /// Columns of the generated patch table, in order. Must include `patch`.
    pub columns: Vec<Column>,
}

impl Default for Readme {
    fn default() -> Self {
        Readme {
            columns: vec![
                Column::Patch,
                Column::Summary,
                Column::Purpose,
                Column::HookFor,
                Column::Layer,
                Column::Budget,
                Column::Upstream,
                Column::RemoveWhen,
                Column::ScopeCell,
            ],
        }
    }
}

/// A column of the generated patch table: the patch file or one header field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Column {
    Patch,
    Summary,
    Purpose,
    HookFor,
    Layer,
    Budget,
    Upstream,
    RemoveWhen,
    ScopeCell,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            file_name: true,
            series: true,
            layer_matches_upstream: true,
            hook_for_exists: true,
            scope_cell_exists: true,
            readme: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upstream {
    /// `Layer:` value patches of this upstream must declare.
    pub layer: String,
    /// Format of `third_party/<x>/patches/series`.
    pub series: SeriesFormat,
    /// Overrides `[budget].cap` for this upstream.
    pub cap: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SeriesFormat {
    /// One `<patch-file>` per line (wick).
    #[serde(rename = "file")]
    File,
    /// `<project-path> <patch-file>` per line, the file under `<project-path>/` (aosp).
    #[serde(rename = "project-file")]
    ProjectFile,
}

impl Config {
    /// The budget cap for patches of upstream `x`.
    pub fn cap(&self, x: &str) -> u32 {
        self.upstream
            .get(x)
            .and_then(|u| u.cap)
            .unwrap_or(self.budget.cap)
    }
}

/// Loads and validates [`CONFIG_FILE`]. A missing file is an error: the policy is explicit.
pub fn load(root: &Path) -> Result<Config, Error> {
    let path = root.join(CONFIG_FILE);
    let text =
        fs::read_to_string(&path).map_err(|e| Error(format!("cannot read {CONFIG_FILE}: {e}")))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| Error(format!("{CONFIG_FILE}: {e}")))?;
    let bad = |m: String| Err(Error(format!("{CONFIG_FILE}: {m}")));
    if cfg.budget.cap == 0 {
        return bad("[budget] cap must be at least 1".into());
    }
    let cols = &cfg.readme.columns;
    if !cols.contains(&Column::Patch) {
        return bad("[readme] columns must include \"patch\"".into());
    }
    if (1..cols.len()).any(|i| cols[..i].contains(&cols[i])) {
        return bad("[readme] columns lists a column twice".into());
    }
    for (x, u) in &cfg.upstream {
        let dir = root.join(UPSTREAM_ROOT).join(x);
        if x.contains(['/', '\\']) || !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
            return bad(format!(
                "[upstream.{x}]: `{UPSTREAM_ROOT}/{x}` is not a directory"
            ));
        }
        if !LAYERS.contains(&u.layer.as_str()) {
            return bad(format!(
                "[upstream.{x}] layer must be one of {}, found `{}`",
                LAYERS.join(" | "),
                u.layer
            ));
        }
        if u.cap == Some(0) {
            return bad(format!("[upstream.{x}] cap must be at least 1"));
        }
    }
    Ok(cfg)
}
