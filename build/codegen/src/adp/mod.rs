//! ADP target: `contracts/adp/protocol.json` → serde types.
//!
//! Mapping (all names from the definition, nothing guessed):
//! - one module per domain (`snake_case` of the domain name); root modules `errors`
//!   (`ErrorCode` from `errors[]`), `envelope` (the `envelope` types; `$ref` to them is
//!   `envelope.<Type>`) and `wire` (string-carried values); root items `PROTOCOL_VERSION`
//!   (from `VERSION`), `Scope`, `Nullable<T>`, and the traits `Command` (implemented by
//!   `<Command>Params`: `Returns`, `METHOD`, `SCOPE`, and `TARGET_TYPES` from `targetTypes`
//!   when the definition has the `target_type` enum; `None` = not fixed yet) and `Event`
//!   (by `<Event>Event`);
//! - object types → structs with `#[serde(rename_all = "camelCase")]` (an explicit `rename`
//!   only where a name does not round-trip), `object` without `properties` → any JSON object
//!   (`serde_json::Map`), tagged unions (`discriminator` + `variants`, keyed by `kind`) →
//!   internally tagged enums (unknown kinds are errors), other types →
//!   `#[serde(transparent)]` newtypes, string enums → enums;
//! - inline enums → `<Type><Property>`; commands → `<Command>Params` / `<Command>Returns`;
//!   events → `<Event>Event`;
//! - `optional` → `Option<T>`, omitted when `None`; `optional` + `nullable` → `Nullable<T>`
//!   (absent / `null` / value);
//! - `integer` with `format` `int32`/`uint32` → `i32`/`u32`; `string` with `format`
//!   `int64`/`uint64` → `i64`/`u64` carried as canonical decimal strings; `number` → `f64`;
//!   `binary` → `Vec<u8>` carried as base64; `integer` without `format` → the
//!   `integer_without_format` setting (or a per-property override in `[adp.integer]`);
//! - a `$ref` that embeds a type in itself (not through an array) is boxed.
//!
//! The definition is validated first (unknown keys, value types, duplicates, `$ref`
//! resolution, type/format/shape rules, Rust names); every problem is reported.

mod emit;
mod model;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::config::Adp;
use crate::output::{self, GeneratedDir, lf};
use crate::{Error, ErrorKind};

/// The generated file in the adp output directory.
pub const FILE: &str = "droiduse.adp.rs";

/// Rust type of an `integer` without `format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegerDefault {
    I64,
    I32,
    U32,
    /// Fail generation.
    Reject,
}

/// Rust type of one `integer` property without `format` (`[adp.integer]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegerWidth {
    I64,
    I32,
    U32,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub integer_without_format: IntegerDefault,
    pub deny_unknown_fields: bool,
    /// Property key (`UI.getTree.parameters.depth`) → width.
    pub integer: BTreeMap<String, IntegerWidth>,
    /// The string enum (`<Domain>.<Type>`) whose values `targetTypes` lists.
    pub target_type: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            integer_without_format: IntegerDefault::I64,
            deny_unknown_fields: true,
            integer: BTreeMap::new(),
            target_type: "Target.TargetType".to_string(),
        }
    }
}

/// One problem in the definition (or in the settings that apply to it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// `adp/json`, `adp/version`, `adp/unknown-key`, `adp/missing-key`, `adp/type`,
    /// `adp/duplicate`, `adp/unresolved-ref`, `adp/shape`, `adp/format`,
    /// `adp/integer-width`, `adp/name`, `adp/setting`.
    pub rule: &'static str,
    pub location: String,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.location, self.rule, self.message)
    }
}

pub(crate) fn problem(
    rule: &'static str,
    location: impl Into<String>,
    message: impl Into<String>,
) -> Problem {
    Problem {
        rule,
        location: location.into(),
        message: message.into(),
    }
}

/// The protocol version: one `MAJOR.MINOR.PATCH` line (decimal, no leading zeros).
fn parse_version(text: &str) -> Result<String, Problem> {
    let v = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    let parts: Vec<&str> = v.split('.').collect();
    let num = |p: &str| {
        !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p == "0" || !p.starts_with('0'))
    };
    if parts.len() == 3 && parts.iter().all(|p| num(p)) {
        Ok(v.to_string())
    } else {
        Err(problem(
            "adp/version",
            "VERSION",
            format!("{v:?} is not one MAJOR.MINOR.PATCH line"),
        ))
    }
}

/// Validates `definition` (protocol.json text) and generates the Rust body (no header).
pub fn generate(
    definition: &str,
    version: &str,
    settings: &Settings,
) -> Result<String, Vec<Problem>> {
    let version = parse_version(version).map_err(|p| vec![p])?;
    let value: serde_json::Value = serde_json::from_str(definition)
        .map_err(|e| vec![problem("adp/json", "protocol.json", e.to_string())])?;
    let def = model::build(&value, settings)?;
    Ok(emit::emit(&def, &version, settings))
}

/// Generates the `[adp]` target of the repo at `root`.
pub fn generate_target(root: &Path, a: &Adp) -> Result<GeneratedDir, Error> {
    let read = |rel: &str| {
        fs::read_to_string(root.join(rel))
            .map(|t| lf(&t))
            .map_err(|e| Error::new(ErrorKind::Io, format!("cannot read {rel}: {e}")))
    };
    let definition = read(&a.definition)?;
    let version = read(&a.version)?;
    let body = generate(&definition, &version, &a.settings()).map_err(|ps| {
        let mut msg = format!("{}: {} problem(s):", a.definition, ps.len());
        for p in &ps {
            msg.push_str(&format!("\n  {p}"));
        }
        Error::new(ErrorKind::Definition, msg)
    })?;
    let source = format!("{} and {}", a.definition, a.version);
    let mut files = BTreeMap::new();
    files.insert(FILE.to_string(), output::file(&source, &body));
    Ok(GeneratedDir {
        dir: a.out.clone(),
        files,
    })
}
